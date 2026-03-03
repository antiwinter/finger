## Plan: Cooperative `F.sleep` via Lua Coroutines

**TL;DR**: Replace the blocking `thread::sleep` inside `F.sleep` with a Lua coroutine yield. Each bot's `tick()` runs inside a coroutine. When `F.sleep(secs)` is called, it yields a "wake-after" timestamp back to the orchestrator, which then moves on to other bots and resumes this coroutine after the specified time. `F.delay(ms, jitter?)` remains a blocking `thread::sleep` for short, precise waits where holding the thread is intentional. No bot code changes required — existing `F.sleep(45)` calls automatically become cooperative.

**Steps**

1. **Change `LuaBot::tick` from direct call to coroutine-based** in `crates/core/src/lua_rt.rs`.
   - On first `tick()` call, wrap the bot's `tick` function in a `coroutine.create()` and store the `LuaThread` in a new field `suspended: Option<LuaRegistryKey>` on `LuaBot`.
   - `tick()` becomes `resume()`: calls `coroutine.resume(co)`. If the coroutine yields `("sleep", wake_time)`, store it and return `TickResult::Sleeping(wake_instant)`. If it returns normally, return `TickResult::Done(Option<f64>)` (the cooldown).
   - Add a new enum:
     ```rust
     enum TickResult {
         Done(Option<f64>),          // tick() returned; value is cooldown
         Sleeping(Instant),          // yielded; resume after this instant
     }
     ```

2. **Redefine `F.sleep(secs, jitter?)` to yield instead of blocking** in `register_globals()`.
   - `F.sleep = function(secs, p) coroutine.yield("sleep", secs, p or 0) end` — registered as a Lua chunk, not a Rust closure. This makes it a yieldable call (Rust closures can't yield across in mlua without `async` feature).
   - `F.sleep(10)` — yield for 10 seconds, no jitter.
   - `F.sleep(10, 0.3)` — yield for 10 seconds ±30% jitter.
   - When the orchestrator calls `coroutine.resume(co)`, this yield bubbles up with `("sleep", secs, jitter_pct)`. The orchestrator computes the actual sleep duration with jitter before setting the wake time.

3. **Add `F.delay(ms, jitter?)` as a blocking sleep** in `register_globals()`.
   - `F.delay(ms)` → `sleep::ms(ms)` — exact millisecond blocking sleep.
   - `F.delay(ms, 0.3)` → `sleep::jittered_ms(ms, 0.3)` — blocking with ±30% jitter.
   - This is for short precise waits (e.g. between keystrokes) where you don't want to yield control.

4. **Update the orchestrator tick loop** in `orchestrate()`.
   - Change the per-instance state from a simple cooldown timestamp to a richer model:
     ```rust
     enum BotState {
         Ready,                    // eligible for ticking
         Cooldown(Instant),        // returned from tick(), waiting
         Suspended(Instant),       // yielded F.sleep(), resume after this time
     }
     ```
   - In the main loop, collect bots that are `Ready` or `Suspended(past)`.
   - For `Ready` bots: activate window → `bot.tick()` → handle `TickResult`.
   - For `Suspended(past)` bots: activate window → `bot.resume()` → handle `TickResult`. The window must be re-activated before resuming because the coroutine may call `win:tap()` etc. after the sleep.
   - On `TickResult::Sleeping(wake)`: set state to `Suspended(wake)`, **deactivate window**, move to next bot immediately (no blocking).
   - On `TickResult::Done(cd)`: set state to `Cooldown(now + cd)`, deactivate window.
   - The 1000ms activation sleep (`thread::sleep(Duration::from_millis(1000))`) still happens before each activate — it's needed for the OS to bring the window to front.

5. **Handle the `active` flag correctly across yields** in `orchestrator.rs`.
   - When a coroutine yields (F.sleep), call `bot.set_active(false)` so `win:*` calls during the sleep period are safely dropped.
   - When resuming a suspended coroutine, call `bot.set_active(true)` + `bot.activate()` + wait 1000ms, just like a fresh tick.

6. **Remove `F.sleep_jittered`** entirely from `register_globals()` and `lua_rt.rs`.
   - `F.sleep(secs, p)` now covers both cases: no jitter when `p` is omitted, ±p jitter when provided.
   - Clean up the Rust-side `sleep_jittered_fn` registration.

7. **Update `bots/README.md`** to document the new semantics:
   - `F.sleep(secs)` — yields control back to orchestrator, bot resumes after `secs` seconds. Other bots can tick during this time.
   - `F.sleep(secs, 0.3)` — same, but with ±30% jitter on the sleep duration.
   - `F.delay(ms)` — blocks the thread for exact `ms` milliseconds. Use for short pauses between actions (e.g. `F.delay(100)` between keystrokes).
   - `F.delay(ms, 0.3)` — blocks with ±30% jitter.

**Verification**
- Run `cargo build` — ensures compile.
- Test with `wow-rally-hk` bot: the `F.sleep(45)` call should yield, allowing `ftz-farm` to tick during those 45 seconds. Observe in TUI logs that both bots tick interleaved.
- Test `F.delay(100)` still blocks precisely (no yield).
- Edge case: a coroutine that errors mid-sleep should be caught by the existing `on_error` callback and disable the entry.

**Decisions**
- Coroutine-based yield (Lua-side `coroutine.yield`) over mlua `async` feature: simpler, no tokio dependency, mlua async requires `Send` bounds that conflict with `Rc<RefCell<>>` in `LuaWindow`.
- `F.sleep` defined as a Lua function (not Rust closure) to enable yielding: Rust-registered closures are non-yieldable in standard mlua; a Lua-side wrapper calling `coroutine.yield` avoids this.
- `F.delay` in milliseconds (not seconds) to distinguish semantics clearly: `F.sleep` = cooperative/seconds, `F.delay` = blocking/milliseconds.
- Keep the 1000ms window-activation wait before each resume — without it, window actions right after sleep would fire before the window is foregrounded.
