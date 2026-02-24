-- bots/wow/bot-rally-hk.lua
-- Rally+HK buff coordination bot for WoW Classic Era
--
-- Character layout (bottom to top):
--   0 = watcher (Booty Bay, yell detector)
--   1 = char1 (city), 2 = char2 (city), ...
local win = nil
local chars = {} -- {[id] = state}
local pos = 1 -- which char slot we're on (0 = watcher)
local last_login = 0 -- anti-AFK timer (0 = trigger on first tick)
local DONE, WAIT_RALLY, WAIT_HEARTH, WAIT_HK = 0, 1, 2, 3

local function reset()
    chars = {
        [1] = {
            id = 1,
            st = WAIT_RALLY
        }
    }
    pos = 1
    last_login = 0
end
reset()

local set_state = function(st, h)
    chars[pos] = {
        id = pos,
        st = st,
        cd = h.cd,
        name = h.name,
        zone = h.zone
    }
end

local function pick(tst)
    local cc = nil
    for i, c in ipairs(chars) do
        if tst and c.st == tst then
            return c
        elseif not cc or c.st > cc.st then
            if c.st ~= WAIT_HEARTH or c.cd < os.time() then
                cc = c
            end
        end
    end

    return cc
end

local function hint()
    local h = win:decodev2()
    if not h then
        return nil
    end
    -- FIXME: h is some like 'rally~名字~1234~2134~0'
    return {
        hint = '',
        name = '',
        zone = 1234,
        cd = 2134,
        onFlight = 0
    }
end

-- ── helpers ──────────────────────────────────────────────
local function logout()
    win:type("/logout")
    win:tap("enter")
    F.sleep(6)
end

local function do_hearth()
    win:type("=-====")
    -- next alway test_hk, so don't need sleep 10 here
end

local function switch_char(target)
    logout()
    local dir = target > pos and "down" or "up"
    for i = 1, math.abs(target - pos) do
        win:tap(dir)
    end
    win:tap("enter")
    last_login = os.time()
    pos = target
end

local function test_hk()
    F.sleep(45)
    local h = hint()
    if h and h.hint == "hk" then
        F.log("got hk for", h.name)
        set_state(DONE, h)
        return
    end
    set_state(WAIT_HK, h)
end

local function switch_next()
    -- use cases: current char may got a buff, need logout, pick a char to login
    -- 1. check hearth cd
    -- 2. if any waiting for hk, switch to watcher
    -- 3. if any waiting for rally, switch to it or add new entry

    local c = pick(WAIT_HEARTH)
    if c then
        F.log("switching to char", c.id, "for hearth cd", c.cd)
        switch_char(c.id)
        do_hearth()
        test_hk()
        switch_next()
        return
    end

    local c = pick()
    if not c then
        chars[#chars + 1] = {
            id = #chars + 1,
            st = WAIT_RALLY
        }
        c = chars[#chars]
    end

    if c.id == pos and -- several mininutes
    os.time() < last_login + 1000 then
        return
    end
    -- change char or anti-afk
    F.log("switch to", c.id, c.name, c.st)
    switch_char(c.id)
end

-- ── bot interface ────────────────────────────────────────
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
        local h = hint()

        if h.hint == 'rally' then
            if h.cd == 0 then
                F.log("got rally for", h.name)
                do_hearth()
                test_hk()
            else
                h.cd = os.time() + h.cd
                set_state(WAIT_HEARTH, h)
            end
        elseif h.hint == 'hkpre' then
            local c = pick(WAIT_HK)
            if c then
                switch_char(c.id)
                test_hk()
            end
        end

        switch_next()
    end,

    get_status = function()
        local parts = {}
        for i, c in ipairs(chars) do
            parts[#parts + 1] = c.id .. ":" .. c.st
        end
        return pos .. "-> " .. table.concat(parts, "|")
    end
}
