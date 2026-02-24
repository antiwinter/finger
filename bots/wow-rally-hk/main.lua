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
local DONE, WAIT_RALLY, WAIT_HK = 0, 1, 2

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
        if c.st == tst then
            return c
        end
    end
end

local function hint()
    local h = win:decodev2() or ''
    -- h is like 'rally~名字~1234~2134~0'
    local parts = {}
    for part in h:gmatch("[^~]+") do
        parts[#parts + 1] = part
    end
    return {
        hint = parts[1],
        name = parts[2],
        zone = tonumber(parts[3]),
        cd = tonumber(parts[4]),
        onFlight = tonumber(parts[5])
    }
end

-- ── helpers ──────────────────────────────────────────────
local function logout()
    win:type("/logout")
    win:tap("enter")
    F.sleep(6)
end

local function fly()
    win:type("=-========")
    -- next alway test_hk, so don't need sleep 10 here
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
    if h.hint == "hk" then
        F.log("got hk for", h.name)
        set_state(DONE, h)
        return
    end
    set_state(WAIT_HK, h)
end

local function switch_next()
    -- use cases: current char may got a buff, need logout, pick a char to login
    -- 2. if any waiting for hk, switch to watcher
    -- 3. if any waiting for rally, switch to it or add new entry

    local c = pick(WAIT_HK)
    if c then
        switch_char(0)
        return
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

    -- change char or anti-afk
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
            F.log("got rally for", h.name)
            set_state(WAIT_HK, h)
            fly()
            return 240 -- need 4 mins to fly to booty bay
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
        local s = "|"
        for i, c in ipairs(chars) do
            s = s .. (i == pos and "*" or "") .. (c.name or "?") .. ":" .. c.st .. "|"
        end
        return s
    end
}
