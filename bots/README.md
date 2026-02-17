# Writing a Bot

A bot is a folder under `bots/` with a `main.lua` entry point.
`main.lua` returns a table with metadata and callback functions.

```
bots/
  my-bot/
    main.lua        -- entry point (required)
    helpers.lua     -- optional, loaded via require("helpers")
```

The orchestrator discovers bots by scanning for `main.lua` files.
The folder name becomes the bot name: `bots/my-bot/main.lua` -> `my-bot`.
Nesting works: `bots/game/clicker/main.lua` -> `game/clicker`.

## Minimal bot

The smallest possible bot. Only `window_pattern`, `description`, and `tick` are required.

```lua
-- bots/my-bot/main.lua

return {
    window_pattern = "Notepad|notepad",   -- regex matched against window titles
    description = "Clicks the center every 2 seconds",

    tick = function()
        -- called repeatedly while the bot is running
        -- return cooldown in milliseconds before next tick
        -- return nil (or nothing) for the default 5000ms
        return 2000
    end,
}
```

`window_pattern` is a `|`-separated regex. The orchestrator finds all windows
whose title matches and creates one bot instance per window.

## Using the window

`start(win)` receives the window handle. Stash it in an upvalue -- you'll
need it in `tick()`.

```lua
local win = nil

return {
    window_pattern = "Notepad|notepad",
    description = "Types hello",

    start = function(w)
        win = w
    end,

    tick = function()
        win:tap("enter")           -- press and release a key
        win:type("hello world")    -- type a string
        win:click(0.5, 0.5)        -- click at (50%, 50%) of the window
        return 3000
    end,
}
```

### Window methods

| Method | Args | Description |
|--------|------|-------------|
| `win:click(x, y)` | `x`, `y`: 0.0-1.0 | Click at a position relative to the window |
| `win:tap(key)` | key name string | Press and release a key |
| `win:type(text)` | text string | Type a string of characters |
| `win:decodev2()` | none | Read an overlay hint from the window (returns string or nil) |

All window methods are only valid during a tick. The orchestrator activates
the window before each tick and deactivates it after. Calls outside this
window (e.g. from a coroutine that outlives the tick) are silently dropped
with a warning.

## Globals

The `F` table is available everywhere:

| Function | Description |
|----------|-------------|
| `F.sleep(seconds)` | Sleep with small random jitter added |
| `F.log(message)` | Log a message, auto-prefixed with the bot name |

```lua
tick = function()
    win:tap("1")
    F.sleep(1.5)       -- wait ~1.5 seconds (plus jitter)
    win:tap("2")
    F.log("combo done")
    return 5000
end,
```

## State and get_status

Use upvalues for state. `get_status()` returns a string shown in the TUI.

```lua
local win = nil
local count = 0

return {
    window_pattern = "Calculator|calc",
    description = "Counts clicks",

    start = function(w)
        win = w
        count = 0
    end,

    tick = function()
        win:click(0.5, 0.5)
        count = count + 1
        return 1000
    end,

    get_status = function()
        return string.format("clicked %d times", count)
    end,
}
```

## reset

`reset()` is called when the user presses `r` in the TUI. Use it to
re-initialize state without a full stop/start cycle. The VM and the window
handle stay alive -- just reset your variables.

```lua
    reset = function()
        count = 0
        phase = "idle"
    end,
```

## start and stop

| Callback | When | Use for |
|----------|------|---------|
| `start(win)` | Orchestrator starts or bot is toggled on | Stash the window handle, initialize state |
| `stop()` | Orchestrator stops or bot is toggled off | Clean up (if needed) |

`start` receives the window handle. `stop` receives nothing.
Both are optional -- if you don't need setup/teardown, omit them.

```lua
local win = nil
local log_file = nil

return {
    window_pattern = "MyApp",
    description = "Bot with setup and teardown",

    start = function(w)
        win = w
        log_file = io.open("run.log", "a")
    end,

    tick = function()
        local h = win:decodev2()
        if h then
            log_file:write(h .. "\n")
        end
        return 2000
    end,

    stop = function()
        if log_file then
            log_file:close()
            log_file = nil
        end
    end,
}
```

## Multi-file bots

`require()` resolves relative to the bot's folder. Split logic into modules
as your bot grows.

```
bots/
  my-bot/
    main.lua
    combat.lua
    navigation.lua
```

```lua
-- bots/my-bot/combat.lua
local M = {}

function M.attack(win)
    win:tap("1")
    F.sleep(0.5)
    win:tap("2")
end

return M
```

```lua
-- bots/my-bot/main.lua
local combat = require("combat")

local win = nil

return {
    window_pattern = "MyGame",
    description = "Multi-file bot",

    start = function(w) win = w end,

    tick = function()
        combat.attack(win)
        return 3000
    end,
}
```

Subdirectories work too -- `require("utils.math")` loads `utils/math.lua`.

## Lifecycle summary

```
discover     main.lua is loaded once to read window_pattern, description, tick
             (the VM is thrown away -- no side effects here)

start        orchestrator starts -> start(win) called on each instance
  |
  v
tick loop    activate window -> tick() -> deactivate window -> wait cooldown
  |              ^
  |   (r key)    |
  +-- reset() ---+
  |
  v
stop         orchestrator stops -> stop() called on each instance
```

## Complete example

```lua
-- bots/example-clicker/main.lua
local win = nil
local phase = "clicking"
local click_count = 0

return {
    window_pattern = "Target App|target",
    description = "Example clicker with phases",

    start = function(w)
        win = w
        phase = "clicking"
        click_count = 0
        F.log("started")
    end,

    tick = function()
        if phase == "clicking" then
            win:click(0.5, 0.5)
            click_count = click_count + 1
            if click_count >= 10 then
                phase = "reading"
            end
            return 500
        elseif phase == "reading" then
            local hint = win:decodev2()
            if hint then
                F.log("got hint: " .. hint)
                phase = "clicking"
                click_count = 0
            end
            return 1000
        end
    end,

    get_status = function()
        if phase == "clicking" then
            return string.format("clicking %d/10", click_count)
        else
            return "reading hint"
        end
    end,

    reset = function()
        phase = "clicking"
        click_count = 0
    end,

    stop = function()
        F.log("stopped")
    end,
}
```
