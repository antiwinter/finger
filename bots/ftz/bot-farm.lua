-- bots/ftz/bot-farm.lua
-- Simple auto-clicker bot for "向僵尸开炮" (Fire the Zombies)

local win = nil

-- Click positions as [x_ratio, y_ratio, cooldown_seconds]
local positions = {
    { 0.85, 0.75, 3 },   -- right1
    { 0.50, 0.80, 5 },   -- start
    { 0.50, 0.50, 3 },   -- done
    { 0.30, 0.70, 3 },   -- tower
}

local timers = {}
local current = 1

return {
    window_pattern = "僵尸|zombie",
    description = "Auto-clicker for Fire the Zombies",

    start = function(w)
        win = w
        local now = os.clock()
        for i = 1, #positions do
            timers[i] = now
        end
        current = 1
    end,

    tick = function()
        local now = os.clock()
        local pos = positions[current]
        if now >= timers[current] then
            win:click(pos[1], pos[2])
            timers[current] = now + pos[3]
        end
        current = current % #positions + 1
        return 700
    end,

    get_status = function()
        return string.format("pos %d/%d", current, #positions)
    end,

    reset = function()
        local now = os.clock()
        for i = 1, #positions do
            timers[i] = now
        end
        current = 1
    end,

    stop = function() end,
}
