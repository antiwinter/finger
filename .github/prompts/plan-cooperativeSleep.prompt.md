## Plan: Cooperative F.sleep via Lua Coroutines (v2)

**TL;DR**: Wrap each bot's `tick()` in a Lua coroutine (`create_thread` + `resume`). When `F.sleep(secs)` is called, it yields instead of blocking. The orchestrator sees the returned seconds, sets a cooldown, and moves on. On the next wakeup, `tick()` resumes the parked coroutine instead of creating a new one. From the orchestrator's perspective, there is no distinction between "cooldown-ready" and "suspended" — both flow through the same `tick()` call and the same cooldown map.

**Changes** — only `lua_rt.rs` and one line in `orchestrator.rs`.

### 1. `lua_rt.rs` — wrap tick in coroutine

Add one field to `LuaBot`:

```rust
suspended: Option<LuaRegistryKey>,  // parked coroutine, if any
```

Rewrite `tick(&mut self) -> Result<Option<f64>>` (**same return type** as before):

```
tick():
    if self.suspended exists:
        take the stored key → registry_value → co.resume(())
    else:
        create_thread(tick_fn) → co.resume(())

    match thread.status():
        Resumable →                          // F.sleep yielded
            store thread in self.suspended
            return Ok(Some(yielded_secs))    // orchestrator treats as cooldown
        Finished →                           // tick() returned normally
            return Ok(Some(cd)) or Ok(None), same as before
```

`stop()`: drop stored coroutine before calling Lua `stop()`.

### 2. `lua_rt.rs` — F.sleep/F.sleep_jittered yield instead of blocking

Redefine as **Lua functions** (Rust closures can't yield in mlua). Placed after `lua.globals().set("F", f_table)` so `F` exists:

```lua
F.sleep = function(secs) coroutine.yield(secs) end
F.sleep_jittered = function(secs, p)
    p = p or 0.3
    coroutine.yield(secs * (1 + (math.random() * 2 - 1) * p))
end
```

Remove the Rust `sleep_fn` and `sleep_jittered_fn` closures.

### 3. `orchestrator.rs` — one-line change

`bots.get(id)` → `bots.get_mut(id)` because `tick()` takes `&mut self`.

Everything else unchanged: cooldowns map, ready collection, activate/deactivate, status update, error handling. The elegant flow is:

```
cooldown expired → bot.tick()
    internally: suspended? → resume : create_thread
    → returns Option<f64>
→ set cooldown → next bot
```

**NOT changed:**
- No new enums (`TickResult`, `BotSchedule`)
- No changes to `types.rs` or `sleep.rs`
- No new `F.delay` function, no removal of `F.sleep_jittered`
- No changes to bot Lua code
- Orchestrator flow stays identical
