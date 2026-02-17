use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

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
}

impl LuaUserData for LuaWindow {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("activate", |_, this, ()| {
            this.inner.borrow_mut().activate();
            Ok(())
        });

        methods.add_method("click", |_, this, (x_ratio, y_ratio): (f64, f64)| {
            this.inner.borrow_mut().click_relative(x_ratio, y_ratio);
            Ok(())
        });

        methods.add_method("tap", |_, this, key: String| {
            this.inner.borrow_mut().tap(&key);
            Ok(())
        });

        methods.add_method("type", |_, this, text: String| {
            this.inner.borrow_mut().type_text(&text);
            Ok(())
        });

        methods.add_method("decodev2", |lua, this, ()| {
            let rect = Some(CaptureRect { l: 0, t: 0, w: 150, h: 80 });
            let capture = this.inner.borrow_mut().capture(rect);
            match capture {
                Some(cap) => match hint::decode_hint_v2(&cap) {
                    Some(s) => Ok(LuaValue::String(lua.create_string(&s)?)),
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
        let code = std::fs::read_to_string(path)?;
        let table: LuaTable = lua
            .load(&code)
            .set_name(path.to_string_lossy())
            .eval()
            .map_err(lua_err)?;

        let pattern: String = table.get("window_pattern").map_err(lua_err)?;
        let description: String = table.get("description").map_err(lua_err)?;

        // Validate tick exists
        let _: LuaFunction = table.get("tick").map_err(lua_err)?;

        Ok((pattern, description))
    }

    /// Create a new LuaBot, load the script, and call start(win).
    pub fn new(script_path: &Path, win_handle: Box<dyn WindowHandle>) -> Result<Self> {
        let lua = Lua::new();

        // Derive log tag from parent folder name (e.g. wow/bot-rally-hk.lua -> "wow")
        let tag = script_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        register_globals(&lua, &tag).map_err(lua_err)?;

        let code = std::fs::read_to_string(script_path)?;
        let table: LuaTable = lua
            .load(&code)
            .set_name(script_path.to_string_lossy())
            .eval()
            .map_err(lua_err)?;

        let bot_key = lua.create_registry_value(table.clone()).map_err(lua_err)?;

        let win = Rc::new(RefCell::new(win_handle));

        // Create win userdata and call start(win)
        let win_ud = lua.create_userdata(LuaWindow {
            inner: Rc::clone(&win),
        }).map_err(lua_err)?;

        if let Ok(start_fn) = table.get::<LuaFunction>("start") {
            start_fn.call::<()>(win_ud).map_err(lua_err)?;
        }

        Ok(Self { lua, bot_key, win })
    }

    /// Call tick() -> Option<cooldown_ms>
    pub fn tick(&self) -> Result<Option<u64>> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        let tick_fn: LuaFunction = table.get("tick").map_err(lua_err)?;
        let result: LuaValue = tick_fn.call(()).map_err(lua_err)?;
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
            f.call::<()>(()).map_err(lua_err)?;
        }
        Ok(())
    }

    /// Call stop()
    pub fn stop(&mut self) -> Result<()> {
        let table: LuaTable = self.lua.registry_value(&self.bot_key).map_err(lua_err)?;
        if let Ok(f) = table.get::<LuaFunction>("stop") {
            f.call::<()>(()).map_err(lua_err)?;
        }
        Ok(())
    }

    /// Activate the window (bring to foreground).
    pub fn activate(&self) {
        self.win.borrow_mut().activate();
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
