use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::types::*;
use crate::platform::Platform;
use crate::lua_rt::LuaBot;
use crate::logger;

/// Recursively find all bot-*.lua files under `dir`.
pub fn find_bot_files(dir: &Path) -> Vec<PathBuf> {
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
                results.extend(find_bot_files(&path));
            }
        } else if let Some(name) = path.file_name() {
            let name = name.to_string_lossy();
            if name.starts_with("bot-") && name.ends_with(".lua") {
                results.push(path);
            }
        }
    }
    results
}

/// Derive bot name from path: bots/wow/bot-rally-hk.lua -> wow/rally-hk
pub fn derive_bot_name(path: &Path, root: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy()
        .replace("bot-", "")
        .replace(".lua", "")
        .replace('\\', "/")
        .to_string()
}

/// Load all bots from a directory, returning BotEntry list.
pub fn load_bots(bots_dir: &Path) -> Vec<BotEntry> {
    let files = find_bot_files(bots_dir);
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

/// Main orchestration loop. Runs on the main thread.
pub fn orchestrate(
    state: Arc<Mutex<Vec<BotEntry>>>,
    platform: Box<dyn Platform>,
    _bots_dir: PathBuf,
    cmd_rx: mpsc::Receiver<Command>,
) {
    // LuaBot instances keyed by instance id
    let mut lua_bots: std::collections::HashMap<String, LuaBot> = std::collections::HashMap::new();

    loop {
        // Check for commands (non-blocking)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Command::Toggle(idx) => {
                    let mut entries = state.lock().unwrap();

                    // Rescan windows for all entries (discover new, remove dead)
                    for entry in entries.iter_mut() {
                        let windows = platform.get_instances(&entry.window_pattern);

                        // Remove instances whose windows no longer exist
                        entry.instances.retain(|inst| {
                            let exists = windows.iter().any(|(wid, _)| *wid == inst.window_id);
                            if !exists {
                                if let Some(mut bot) = lua_bots.remove(&inst.id) {
                                    bot.stop().ok();
                                }
                            }
                            exists
                        });

                        // Add new instances for newly discovered windows
                        for (wid, title) in &windows {
                            if !entry.instances.iter().any(|i| i.window_id == *wid) {
                                entry.instances.push(Instance::new(&entry.name, *wid, title.clone()));
                            }
                        }
                    }

                    if let Some(entry) = entries.get_mut(idx) {
                        entry.enabled = !entry.enabled;
                        let name = entry.name.clone();
                        let enabled = entry.enabled;

                        logger::info(&format!("enable {}: {}", name, enabled));

                        if enabled {
                            // Create LuaBot for each instance
                            for inst in &entry.instances {
                                if lua_bots.contains_key(&inst.id) {
                                    continue;
                                }
                                match LuaBot::new(
                                    &entry.script_path,
                                    platform.create_window(&entry.window_pattern, inst.window_id),
                                ) {
                                    Ok(bot) => {
                                        lua_bots.insert(inst.id.clone(), bot);
                                    }
                                    Err(e) => {
                                        logger::error(&format!("  failed to start {}: {}", inst.id, e));
                                    }
                                }
                            }
                        } else {
                            for inst in &entry.instances {
                                if let Some(mut bot) = lua_bots.remove(&inst.id) {
                                    bot.stop().ok();
                                }
                            }
                        }
                    }
                }
                Command::Quit => {
                    logger::info("shutting down");
                    return;
                }
            }
        }

        // Tick enabled bots
        {
            let mut entries = state.lock().unwrap();
            for entry in entries.iter_mut() {
                if !entry.enabled {
                    continue;
                }
                for inst in entry.instances.iter_mut() {
                    if Instant::now() < inst.next_tick {
                        // Still update status while waiting for cooldown
                        if let Some(lua_bot) = lua_bots.get(&inst.id) {
                            inst.status = lua_bot.get_status().unwrap_or_else(|_| "waiting".to_string());
                        }
                        continue;
                    }
                    if let Some(lua_bot) = lua_bots.get(&inst.id) {
                        // Activate window
                        lua_bot.activate();

                        // Small delay after activation
                        std::thread::sleep(Duration::from_millis(200));

                        // Tick
                        match lua_bot.tick() {
                            Ok(cooldown_ms) => {
                                let cd = cooldown_ms.unwrap_or(5000);
                                inst.next_tick = Instant::now() + Duration::from_millis(cd);
                            }
                            Err(e) => {
                                inst.error = Some(format!("{}", e));
                                logger::error(&format!("tick error {}: {}", inst.id, e));
                            }
                        }

                        // Update status
                        match lua_bot.get_status() {
                            Ok(s) => inst.status = s,
                            Err(_) => {}
                        }
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
