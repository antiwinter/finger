use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::types::*;
use crate::platform::Platform;
use crate::lua_rt::LuaBot;
use crate::logger;

/// Build the on_error callback for a bot instance.
/// The VM fires this with formatted traceback lines; we log and disable the entry.
fn make_on_error(
    id: String,
    state: Arc<Mutex<Vec<BotEntry>>>,
) -> Arc<dyn Fn(Vec<String>) + Send + Sync> {
    Arc::new(move |lines: Vec<String>| {
        // First line is the header; the rest are indented continuation lines.
        // Log as a single message so the logger emits one timestamped entry
        // followed by prefix-free indented continuation lines.
        let mut msg = String::from("runtime error:");
        for line in &lines {
            msg.push('\n');
            msg.push_str("  ");
            msg.push_str(line);
        }
        logger::error_p(&id, &msg);
        let mut entries = state.lock().unwrap();
        if let Some(entry) = entries.iter_mut()
            .find(|e| e.instances.iter().any(|i| i.id == id))
        {
            entry.enabled = false;
            if let Some(inst) = entry.instances.iter_mut().find(|i| i.id == id) {
                inst.error = lines.into_iter().next();
                inst.status = String::new();
            }
        }
    })
}

/// Recursively find all directories containing `main.lua` under `dir`.
pub fn find_bot_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !name.starts_with('.') && name != "node_modules" {
                let main_lua = path.join("main.lua");
                if main_lua.is_file() {
                    results.push(main_lua);
                } else {
                    results.extend(find_bot_dirs(&path));
                }
            }
        }
    }
    results
}

/// Derive bot name from main.lua path: bots/wow-rally-hk/main.lua -> wow-rally-hk
pub fn derive_bot_name(path: &Path, root: &Path) -> String {
    // path is e.g. bots/wow-rally-hk/main.lua, we want the parent dir relative to root
    let bot_dir = path.parent().unwrap_or(path);
    let rel = bot_dir.strip_prefix(root).unwrap_or(bot_dir);
    rel.to_string_lossy()
        .replace('\\', "/")
        .to_string()
}

/// Load all bots from a directory, returning BotEntry list.
pub fn load_bots(bots_dir: &Path) -> Vec<BotEntry> {
    let files = find_bot_dirs(bots_dir);
    let mut entries = Vec::new();

    for path in files {
        let name = derive_bot_name(&path, bots_dir);
        match LuaBot::load_meta(&path) {
            Ok((pattern, description)) => {
                entries.push(BotEntry {
                    name,
                    window_pattern: pattern,
                    description,
                    enabled: false,
                    instances: Vec::new(),
                    error: None,
                    script_path: path,
                });
            }
            Err(e) => {
                logger::error(&format!("failed to load bot {}: {}", name, e));
            }
        }
    }

    entries
}

/// Scan for windows matching each bot's pattern, populate instances.
pub fn scan_instances(entries: &mut Vec<BotEntry>, platform: &dyn Platform) {
    for entry in entries.iter_mut() {
        let windows = platform.get_instances(&entry.window_pattern);
        entry.instances.clear();
        for (wid, title) in windows {
            entry.instances.push(Instance::new(&entry.name, wid, title));
        }
    }
}

/// Drain pending commands. Returns false on Quit.
fn process_commands(
    cmd_rx: &mpsc::Receiver<Command>,
    state: &Arc<Mutex<Vec<BotEntry>>>,
    orch_state: &Mutex<OrchestratorState>,
    platform: &dyn Platform,
    bots: &mut HashMap<String, LuaBot>,
    cooldowns: &mut HashMap<String, Instant>,
) -> bool {
    while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
            Command::Quit => {
                logger::info("shutting down");
                // Stop all bots
                for (_, mut bot) in bots.drain() {
                    bot.stop().ok();
                }
                cooldowns.clear();
                *orch_state.lock().unwrap() = OrchestratorState::Stopped;
                return false;
            }
            Command::Toggle(idx) => {
                let mut entries = state.lock().unwrap();

                // Rescan windows, remove dead, add new
                for entry in entries.iter_mut() {
                    let wins = platform.get_instances(&entry.window_pattern);
                    entry.instances.retain(|i| {
                        let alive = wins.iter().any(|(w, _)| *w == i.window_id);
                        if !alive {
                            if let Some(mut b) = bots.remove(&i.id) { b.stop().ok(); }
                            cooldowns.remove(&i.id);
                        }
                        alive
                    });
                    for (wid, title) in &wins {
                        if !entry.instances.iter().any(|i| i.window_id == *wid) {
                            entry.instances.push(Instance::new(&entry.name, *wid, title.clone()));
                        }
                    }
                }

                let Some(entry) = entries.get_mut(idx) else { continue };
                logger::info(&format!("enable {}: {}", entry.name, entry.enabled));

                let is_running = *orch_state.lock().unwrap() == OrchestratorState::Running;

                if entry.enabled && is_running {
                    for inst in &entry.instances {
                        if bots.contains_key(&inst.id) {
                            bots.get(&inst.id).unwrap().reset().ok();
                        } else {
                            match LuaBot::new(
                                &entry.script_path, &inst.id,
                                platform.create_window(&entry.window_pattern, inst.window_id),
                                make_on_error(inst.id.clone(), Arc::clone(&state)),
                            ) {
                                Ok(bot) => { bots.insert(inst.id.clone(), bot); }
                                Err(_) => {} // on_error already fired inside lua_rt
                            }
                        }
                    }
                } else if !entry.enabled {
                    // Stop bots for disabled entry
                    for inst in &entry.instances {
                        if let Some(mut b) = bots.remove(&inst.id) { b.stop().ok(); }
                        cooldowns.remove(&inst.id);
                    }
                }
            }
            Command::StartStop => {
                let current = *orch_state.lock().unwrap();
                match current {
                    OrchestratorState::Stopping => {
                        // TUI already set Stopping; teardown happens in main loop
                        logger::info("orchestrator stopping...");
                    }
                    OrchestratorState::Running => {
                        // TUI set Running (was Stopped → start)
                        logger::info("orchestrator started");
                        // Create bots for all enabled entries
                        let entries = state.lock().unwrap();
                        for entry in entries.iter() {
                            if !entry.enabled { continue; }
                            for inst in &entry.instances {
                                if !bots.contains_key(&inst.id) {
                                    match LuaBot::new(
                                        &entry.script_path, &inst.id,
                                        platform.create_window(&entry.window_pattern, inst.window_id),
                                        make_on_error(inst.id.clone(), Arc::clone(&state)),
                                    ) {
                                        Ok(bot) => { bots.insert(inst.id.clone(), bot); }
                                        Err(_) => {} // on_error already fired inside lua_rt
                                    }
                                }
                            }
                        }
                    }
                    OrchestratorState::Stopped => {}
                }
            }
            Command::Restart(idx) => {
                let is_running = *orch_state.lock().unwrap() == OrchestratorState::Running;
                if !is_running { continue; }

                let entries = state.lock().unwrap();
                let Some(entry) = entries.get(idx) else { continue };
                if !entry.enabled { continue; }

                logger::info(&format!("restarting bot {}", entry.name));
                for inst in &entry.instances {
                    if let Some(mut b) = bots.remove(&inst.id) {
                        b.stop().ok();
                    }
                    cooldowns.remove(&inst.id);
                    match LuaBot::new(
                        &entry.script_path, &inst.id,
                        platform.create_window(&entry.window_pattern, inst.window_id),
                        make_on_error(inst.id.clone(), Arc::clone(&state)),
                    ) {
                        Ok(bot) => { bots.insert(inst.id.clone(), bot); }
                        Err(_) => {} // on_error already fired inside lua_rt
                    }
                }
            }
        }
    }
    true
}

/// Main orchestration loop. Runs on a background thread.
pub fn orchestrate(
    state: Arc<Mutex<Vec<BotEntry>>>,
    orch_state: Arc<Mutex<OrchestratorState>>,
    platform: Box<dyn Platform>,
    _bots_dir: PathBuf,
    cmd_rx: mpsc::Receiver<Command>,
) {
    let mut bots: HashMap<String, LuaBot> = HashMap::new();
    let mut cooldowns: HashMap<String, Instant> = HashMap::new();

    loop {
        if !process_commands(&cmd_rx, &state, &orch_state, platform.as_ref(), &mut bots, &mut cooldowns) {
            return;
        }

        // Skip tick processing when stopped
        let current = *orch_state.lock().unwrap();
        if current == OrchestratorState::Stopping {
            // Graceful stop: tear down all bots, then transition to Stopped
            for (_, mut bot) in bots.drain() {
                bot.stop().ok();
            }
            cooldowns.clear();
            *orch_state.lock().unwrap() = OrchestratorState::Stopped;
            logger::info("orchestrator stopped");
            continue;
        }
        if current != OrchestratorState::Running {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Collect ready instances
        let ready: Vec<String> = {
            let entries = state.lock().unwrap();
            entries.iter()
                .filter(|e| e.enabled)
                .flat_map(|e| e.instances.iter())
                .filter(|i| {
                    bots.contains_key(&i.id)
                        && cooldowns.get(&i.id).map_or(true, |t| Instant::now() >= *t)
                })
                .map(|i| i.id.clone())
                .collect()
        };

        for id in &ready {
            // Stay responsive: check commands between each tick
            if !process_commands(&cmd_rx, &state, &orch_state, platform.as_ref(), &mut bots, &mut cooldowns) {
                return;
            }

            // If orchestrator was stopped mid-tick, break out
            if *orch_state.lock().unwrap() != OrchestratorState::Running {
                break;
            }

            let Some(bot) = bots.get_mut(id) else { continue };
            bot.set_active(true);
            bot.activate();
            std::thread::sleep(Duration::from_millis(1000));

            let tick_result = bot.tick();
            let status = if tick_result.is_ok() { bot.get_status().ok() } else { None };
            bot.set_active(false);

            if let Ok(s) = tick_result {
                let cd = s.unwrap_or(5.0);
                let cd = if cd.is_finite() && cd >= 0.0 { cd } else { 5.0 };
                if cd > 60.0 {
                     logger::info(&format!("next return for {}: {}", id, cd));
                }
                cooldowns.insert(id.clone(), Instant::now() + Duration::from_secs_f64(cd));
                let mut entries = state.lock().unwrap();
                if let Some(inst) = entries.iter_mut()
                    .flat_map(|e| e.instances.iter_mut())
                    .find(|i| i.id == *id)
                {
                    inst.status = status.unwrap_or_default();
                    inst.error = None;
                }
            }
            // On Err: on_error fired inside lua_rt; entry.enabled already set to false.
            // The post-tick sweep below handles cleanup via the natural disable path.
        }

        // Sweep bots whose entry was disabled (e.g. by a runtime error via on_error).
        // This is the single removal path — the same one Toggle-disable uses.
        let disabled_ids: Vec<String> = {
            let entries = state.lock().unwrap();
            bots.keys()
                .filter(|id| !entries.iter()
                    .any(|e| e.enabled && e.instances.iter().any(|i| &i.id == *id)))
                .cloned()
                .collect()
        };
        for id in disabled_ids {
            if let Some(mut bot) = bots.remove(&id) { bot.stop().ok(); }
            cooldowns.remove(&id);
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
