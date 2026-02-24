-- bots/wow/bot-rally-hk.lua
-- Rally+HK buff coordination bot for WoW Classic Era
--
-- Character layout (bottom to top):
--   0 = watcher (Booty Bay, yell detector)
--   1 = char1 (city), 2 = char2 (city), ...
--
-- chars[id] states:
--   0 polling for rally hint
--   1 hearth on CD, timestamp when ready
--   2 hearth'd to BB, awaiting HK buff
--   3 got HK this cycle
local win = nil
local chars = {} -- {[id] = state}
local current_pos = 1 -- which char slot we're on (0 = watcher)
local last_login = 0 -- anti-AFK timer (0 = trigger on first tick)

-- ── helpers ──────────────────────────────────────────────

local function logout()
    win:type("/logout")
    win:tap("enter")
    F.sleep(6)
end

local function anti_afk()
    if os.time() - last_login >= 300 then
        F.log("anti-AFK: logging out")
        logout()
        win:tap("enter") -- login
        last_login = os.time()
    end
end

local function safe_switch_char(target)
    local diff = current_pos - target
    if diff == 0 then
        return
    end

    logout()
    local dir = diff > 0 and "down" or "up"
    for i = 1, math.abs(diff) do
        win:tap(dir)
    end
    win:tap("enter")
    last_login = os.time()
    current_pos = target
end

local function hint()
    return win:decodev2()
end

local function test_hk()
    F.sleep(45)
    if hint() == "hk" then
        F.log("got hk on char " .. id)
        chars[id] = "done"
    else
        F.log("no hk on char " .. id .. ", will retry")
    end
    -- we don't know if id+1 has got any buff yet, so we always switch back to watcher
    -- and determine what to do on next tick
    safe_switch_char(0)
end

local function do_hearth()
    win:tap("=")
    chars[current_pos] = "waiting_hk"
    test_hk()
end

local function reset()
    chars = {
        [1] = "waiting_rally"
    }
    current_pos = 1
    last_login = 0
end

-- ── bot interface ────────────────────────────────────────

reset()
return {
    window_pattern = "World of Warcraft|wow|魔兽世界",
    description = "Rally+HK buff coordination",

    reset = reset,
    start = function(w)
        win = w
    end,
    stop = function()
    end,
    tick = function()
        local now = os.time()

        for id, st in ipairs(chars) do
            if type(st) == "number" and st <= now then
                safe_switch_char(id)
                do_hearth()
            end
        end

        for id, st in pairs(chars) do

            if st == "waiting_hk" then
                safe_switch_char(0)
                if hint() == "hkpre" then
                    F.log("hkpre detected, switching to char " .. id)

                end
                return
            end

            if st == "waiting_rally" then
                -- Move to this char to poll its rally hint
                safe_switch_char(id)
                local r = string.match(hint() or '', "^rally(%d+)$")
                if not r then
                    return
                end
                local cd = tonumber(r)
                -- discover next char
                chars[id + 1] = chars[id + 1] or "waiting_rally"

                if cd == 0 then
                    F.log("rally0 on char " .. id .. ", hearthing")
                    do_hearth()
                else
                    F.log("rally" .. cd .. " on char " .. id .. ", storing cd")
                    chars[id] = os.time() + cd
                    safe_switch_char(id + 1)
                end
                return
            end
        end

        anti_afk()
    end,

    get_status = function()
        return current_pos .. '-> ' .. table.concat(caars, '|')
    end
}
