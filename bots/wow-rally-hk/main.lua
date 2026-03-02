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
local ERR, DONE, WAIT_RALLY, WAIT_HEARTH, WAIT_HK, WATCHER = -1, 0, 1, 2, 3, 4
local SW, BB = 1453, 1434 -- zone IDs for Stormwind and Booty Bay

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
    for i, c in ipairs(chars) do
        if c.st == tst and -- also check not on cd
        (c.st ~= WAIT_HEARTH or c.cd < os.time()) then
            return c
        end
    end
end

local function hint()
    local h = win:decodev2()
    if not h then
        return {}
    end
    -- h is {[0]=raw, [1]=hint, [2]=name, [3]=zone, [4]=cd, [5]=onFlight}
    F.log("raw hint", h[1], h[2], h[3], h[4], h[5])
    return {
        hint = h[1],
        name = h[2],
        zone = tonumber(h[3]),
        cd = tonumber(h[4]),
        onFlight = tonumber(h[5])
    }
end

-- ── helpers ──────────────────────────────────────────────
local function logout()
    win:tap("enter")
    win:type("/logout")
    win:tap("enter")
    F.sleep(26)
end

local function do_hearth_or_fly()
    -- use item:6948
    win:type("=-====")
    F.sleep(20)
end

local function switch_char(target)

    if target == pos and -- refuse same char switch
    os.time() < last_login + 1000 then
        return
    end

    local c = chars[target] or {
        id = 0
    }

    F.log("switch char", c.id, c.name, c.st)
    logout()
    local dir = target > pos and "up" or "down"
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
    if h.hint == "hk" then
        F.log("got hk for", h.name)
        set_state(DONE, h)
    elseif h.zone ~= BB then
        set_state(ERR, h)
    else
        set_state(WAIT_HK, h)
    end
end

local function pick_next()
    -- use cases: current char may got a buff, need logout, pick a char to login
    -- 1. check hearth cd
    -- 2. if any waiting for hk, switch to watcher
    -- 3. if any waiting for rally, switch to it or add new entry

    local c = pick(WAIT_HEARTH)
    if c then
        return c.id
    end

    c = pick(WAIT_HK)
    if c then
        return 0
    end

    c = pick(WAIT_RALLY)
    if not c then
        local n = #chars + 1
        chars[n] = {
            id = n,
            st = WAIT_RALLY
        }
        c = chars[n]
    end

    return c.id
end

local fsm = {
    [WAIT_RALLY] = function(h)
        if h.hint == 'rally' then
            F.log("got rally for", h.name)
            do_hearth_or_fly()
            h = hint() -- read hint again
            if h.onFlight == 1 then
                set_state(WAIT_HK, h)
                F.log("fly to bb", h.name)
                return 240 -- return after 4min
            elseif (h.cd or 0) > 0 then
                h.cd = os.time() + h.cd
                set_state(WAIT_HEARTH, h)
            else -- not on cd, not flying, cannot get to BB
                set_state(ERR, h)
            end
        end
    end,
    [WAIT_HEARTH] = function(h)
        do_hearth_or_fly()
        test_hk()
    end,
    [WAIT_HK] = function(h)
        test_hk()
    end,
    [WATCHER] = function(h)
        if h.hint == 'hkpre' then
            local c = pick(WAIT_HK)
            if c then
                switch_char(c.id)
                test_hk()
            end
        end
    end,
    [ERR] = function(h)
        F.log("shouldn't entry", ERR, h.name, h.zone, pos)
    end,
    [DONE] = function(h)
        F.log("done for", h.name)
    end
}

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
        local st = chars[pos] and chars[pos].st or WATCHER
        local n = fsm[st](hint())
        if not n then 
            switch_char(pick_next())
        end
        return n
    end,

    get_status = function()
        local s = "|"
        for i, c in ipairs(chars) do
            s = s .. (i == pos and "*" or "") .. (c.name or "?") .. ":" .. c.st .. "|"
        end
        return s
    end
}
