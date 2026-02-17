-- bots/wow/bot-rally-hk.lua
-- Rally+HK buff coordination bot for WoW Classic Era

local win = nil
local state = 0       -- 0 = waiting rally, 1 = waiting hk yell
local count = 0       -- character position counter
local last_login = 0  -- anti-AFK timer (0 = trigger on first tick)

local function logout()
    -- F.log("[Rally] logout")
    F.sleep(0.5)
    win:tap("enter")
    F.sleep(0.2)
    win:type("/logout")
    win:tap("enter")
    F.sleep(6)
    -- F.log("[Rally] logout complete")
end

local function switch_char(n, key)
    F.log("switch char " .. n .. " " .. key)
    logout()
    for i = 1, n do
        win:tap(key)
    end
    win:tap("enter")
    last_login = os.time()
end

local function hint()
    return win:decodev2()
end

local function try_final()
    local h = hint()
    if h == "hk" then
        F.log("got hk")
        count = count + 1
        state = 0
        switch_char(1, "up")
    else
        switch_char(count + 1, "down")
    end
end

return {
    window_pattern = "World of Warcraft|wow|魔兽世界",
    description = "Rally+HK buff coordination",

    start = function(w)
        win = w
        last_login = 0
    end,

    tick = function()
        if state == 0 then
            local h = hint()
            if h == "rally" then
                F.log("got rally signal")
                win:tap("=")
                F.sleep(1)
                win:tap("=")
                F.sleep(1)
                win:tap("=")
                F.sleep(30)
                state = 1
                try_final()
            end
        elseif state == 1 then
            local h = hint()
            if h == "hkpre" then
                F.log("zandalar yelled")
                switch_char(count + 1, "up")
                F.sleep(45)
                try_final()
            end
        end

        -- Auto re-login every 20 minutes to avoid AFK kick
        local now = os.time()
        if last_login == 0 or now - last_login > 20 * 60 then
            logout()
            win:tap("enter")
            last_login = os.time()
            F.log("auto re-login")
        end
    end,

    get_status = function()
        if state == 0 then return "waiting rally"
        elseif state == 1 then return "waiting hk yell"
        else return "unknown" end
    end,

    reset = function()
        state = 0
        count = 0
        last_login = 0
    end,

    stop = function() end,
}
