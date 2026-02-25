use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use mlua::prelude::*;

use crate::types::*;
use crate::platform::WindowHandle;
use crate::hint;
use crate::sleep;
use crate::logger;

/// Wrapper around a WindowHandle for Lua userdata.
struct LuaWindow {
    inner: Rc<RefCell<Box<dyn WindowHandle>>>,
    active: Rc<Cell<bool>>,
}

impl LuaUserData for LuaWindow {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("click", |_, this, (x_ratio, y_ratio): (f64, f64)| {
            if !this.active.get() {
                logger::warn("dropped win:click — window not active");
                return Ok(());
            }
            this.inner.borrow_mut().click_relative(x_ratio, y_ratio);
            Ok(())
        });

        methods.add_method("tap", |_, this, key: String| {
            if !this.active.get() {
                logger::warn("dropped win:tap — window not active");
                return Ok(());
            }
            this.inner.borrow_mut().tap(&key);
            Ok(())
        });

        methods.add_method("type", |_, this, text: String| {
            if !this.active.get() {
                logger::warn("dropped win:type — window not active");
                return Ok(());
            }
            this.inner.borrow_mut().type_text(&text);
            Ok(())
        });

        methods.add_method("decodev2", |lua, this, ()| {
            if !this.active.get() {
                logger::warn("dropped win:decodev2 — window not active");
                return Ok(LuaNil);
            }
            let rect = Some(CaptureRect { l: 0, t: 0, w: 320, h: 80 });
            let capture = this.inner.borrow_mut().capture(rect);
            match capture {
                Some(cap) => match hint::decode_hint_v2(&cap) {
                    Some(segments) => {
                        let table = lua.create_table()?;
                        for (i, seg) in segments.iter().enumerate() {
                            table.set(i as i64, lua.create_string(seg.as_bytes())?)?;
                        }
                        Ok(LuaValue::Table(table))
                    },
                    None => Ok(LuaNil),
                },
                None => Ok(LuaNil),
            }
        });
    }
}

/// A loaded Lua bot instance, owning its own Lua VM.
pub struct LuaBot {
    lua: Lua,
    bot_key: LuaRegistryKey,
    win: Rc<RefCell<Box<dyn WindowHandle>>>,
    active: Rc<Cell<bool>>,
    on_error: Arc<dyn Fn(Vec<String>) + Send + Sync>,
}

/// Format an mlua runtime error: strip `[string "…"]` wrappers and
/// absolute path prefixes before `bots/`, returning one line per traceback line.
fn format_mlua_error(e: &mlua::Error) -> Vec<String> {
    e.to_string()
        .lines()
        .map(|line| {
            // mlua prefixes the message line with "runtime error: " — strip it
            let line = line.strip_prefix("runtime error: ").unwrap_or(line);
            let mut out = String::new();
            let mut rest = line;
            while let Some(s) = rest.find("[string \"") {
                out.push_str(&rest[..s]);
                rest = &rest[s + 9..];
                if let Some(end) = rest.find("\"]" ) {
                    let path = &rest[..end];
                    let short = path.find("bots/").map_or(path, |i| &path[i + 5..]);
                    out.push_str(short);
                    rest = &rest[end + 2..];
                } else {
                    out.push_str(rest);
                    rest = "";
                }
            }
            out.push_str(rest);
            out.trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// Build the Lua chunk name for a script path.
/// Uses `@`-prefix so Lua treats it as a filename (no `[string ""]` wrapper,
/// no Lua-side truncation). Path is shortened to the part after `bots/`.
fn chunk_name(path: &Path) -> String {
    let s = path.to_string_lossy();
    let short = s.find("bots/").map_or(s.as_ref(), |i| &s[i + 5..]);
    format!("@{}", short)
}

/// Helper to convert mlua::Error -> anyhow::Error
fn lua_err(e: mlua::Error) -> anyhow::Error {
    anyhow!("{}", e)
}

impl LuaBot {
    /// Load a bot script just to extract metadata (window_pattern, description).
    /// Does NOT call start(). Used during bot discovery.
    pub fn load_meta(path: &Path) -> Result<(String, String)> {
        let lua = Lua::new();
        register_globals(&lua, "").map_err(lua_err)?;

        // Set package.path so require() finds modules in the bot's directory
        if let Some(bot_dir) = path.parent() {
            let dir_str = bot_dir.to_string_lossy();
            let pkg: LuaTable = lua.globals().get("package").map_err(lua_err)?;
            pkg.set("path", format!("{}/?.lua;{}/?/init.lua", dir_str, dir_str)).map_err(lua_err)?;
        }

        let code = std::fs::read_to_string(path)?;
        let table: LuaTable = lua
            .load(&code)
            .set_name(chunk_name(path))
            .eval()
            .map_err(lua_err)?;

        let pattern: String = table.get("window_pattern").map_err(lua_err)?;
        let description: String = table.get("description").map_err(lua_err)?;

        // Validate tick exists
        let _: LuaFunction = table.get("tick").map_err(lua_err)?;

        Ok((pattern, description))
    }

    /// Create a new LuaBot, load the script, and call start(win).
    pub fn new(
        script_path: &Path,
        instance_id: &str,
        win_handle: Box<dyn WindowHandle>,
        on_error: Arc<dyn Fn(Vec<String>) + Send + Sync>,
    ) -> Result<Self> {
        let lua = Lua::new();
        register_globals(&lua, instance_id).map_err(lua_err)?;

        // Set package.path so require() finds modules in the bot's directory
        if let Some(bot_dir) = script_path.parent() {
            let dir_str = bot_dir.to_string_lossy();
            let pkg: LuaTable = lua.globals().get("package").map_err(lua_err)?;
            pkg.set("path", format!("{}/?.lua;{}/?/init.lua", dir_str, dir_str)).map_err(lua_err)?;
        }

        let code = std::fs::read_to_string(script_path)?;
        let on_err = Arc::clone(&on_error);
        let table: LuaTable = lua
            .load(&code)
            .set_name(chunk_name(script_path))
            .eval()
            .map_err(|e| { on_err(format_mlua_error(&e)); lua_err(e) })?;

        let bot_key = lua.create_registry_value(table.clone()).map_err(lua_err)?;

        let win = Rc::new(RefCell::new(win_handle));
        let active = Rc::new(Cell::new(false));

        // Create win userdata and call start(win)
        let win_ud = lua.create_userdata(LuaWindow {
            inner: Rc::clone(&win),
            active: Rc::clone(&active),
        }).map_err(lua_err)?;

        if let Ok(start_fn) = table.get::<LuaFunction>("start") {
            let on_err = Arc::clone(&on_error);
            start_fn.call::<()>(win_ud)
                .map_err(|e| { on_err(format_mlua_error(&e)); lua_err(e) })?;
        }

        Ok(Self { lua, bot_key, win, active, on_error })
    }

    /// Call tick() -> Option<cooldown_ms>. Fires on_error on runtime failure.
    pub fn tick(&self) -> Result<Option<u64>> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        let tick_fn: LuaFunction = table.get("tick").map_err(lua_err)?;
        let result: LuaValue = tick_fn.call(())
            .map_err(|e| { (self.on_error)(format_mlua_error(&e)); lua_err(e) })?;
        match result {
            LuaValue::Integer(ms) => Ok(Some(ms as u64)),
            LuaValue::Number(ms) => Ok(Some(ms as u64)),
            _ => Ok(None),
        }
    }

    /// Call get_status() -> String
    pub fn get_status(&self) -> Result<String> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        match table.get::<LuaFunction>("get_status") {
            Ok(f) => {
                let s: String = f.call(()).map_err(lua_err)?;
                Ok(s)
            }
            Err(_) => Ok(String::new()),
        }
    }

    /// Call reset()
    pub fn reset(&self) -> Result<()> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        if let Ok(f) = table.get::<LuaFunction>("reset") {
            f.call::<()>(())
                .map_err(|e| { (self.on_error)(format_mlua_error(&e)); lua_err(e) })?;
        }
        Ok(())
    }

    /// Call stop()
    pub fn stop(&mut self) -> Result<()> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        if let Ok(f) = table.get::<LuaFunction>("stop") {
            f.call::<()>(())
                .map_err(|e| { (self.on_error)(format_mlua_error(&e)); lua_err(e) })?;
        }
        Ok(())
    }

    /// Activate the window (bring to foreground).
    pub fn activate(&self) {
        self.win.borrow_mut().activate();
    }

    /// Set whether the window is currently active (controls whether win actions are allowed).
    pub fn set_active(&self, active: bool) {
        self.active.set(active);
    }
}

/// Register the F.* global table into a Lua state.
fn register_globals(lua: &Lua, tag: &str) -> mlua::Result<()> {
    let f_table = lua.create_table()?;

    // F.sleep(seconds)
    let sleep_fn = lua.create_function(|_, secs: f64| {
        sleep::sleep_jitter(secs);
        Ok(())
    })?;
    f_table.set("sleep", sleep_fn)?;

    // F.log(msg) — auto-prefixed with tag from script folder name (blue)
    let tag = tag.to_string();
    if !tag.is_empty() {
        logger::register_prefix(&tag, logger::COLOR_BLUE);
    }
    let log_fn = lua.create_function(move |_, msg: String| {
        if tag.is_empty() {
            logger::info(&msg);
        } else {
            logger::info_p(&tag, &msg);
        }
        Ok(())
    })?;
    f_table.set("log", log_fn)?;

    lua.globals().set("F", f_table)?;
    Ok(())
}
