--!strict
-- Closing Time: map-local shift state. Reset recreates this module and the world.
local returned = { ball = false, gift = false, plush = false }
local off = { arcade = false, party = false, bowling = false }
local completed = false
local function refresh(api)
    local ready = returned.ball and returned.gift and returned.plush
        and off.arcade and off.party and off.bowling
    api.set_state("front_door", "ready", ready)
end
local function mark(api, event, present)
    if completed then return end
    local key = ({ return_ball = "ball", return_gift = "gift", return_plush = "plush" })[event.subject]
    if key == nil then return end
    returned[key] = present
    api.set_state("front_door", key .. "_returned", present)
    refresh(api)
    api.notice(key .. (present and " returned" or " outstanding"))
end
local function shut(api, key, light, sound)
    if completed or off[key] then return end
    off[key] = true
    api.light(light, false)
    api.sound(sound, false)
    api.set_state("front_door", key .. "_off", true)
    api.sound("switch_click", true)
    refresh(api)
    api.notice(key .. " power off")
end
return {
    start = function(api, event)
        api.notice("Closing Time: the last shift")
    end,
    returned = function(api, event) mark(api, event, true) end,
    removed = function(api, event) mark(api, event, false) end,
    arcade_off = function(api, event)
        shut(api, "arcade", "arcade_lamp_a", "arcade_hum")
    end,
    party_off = function(api, event)
        shut(api, "party", "party_lamp", "party_hum")
    end,
    bowling_off = function(api, event)
        shut(api, "bowling", "bowling_lamp_a", "bowling_hum")
    end,
    finish = function(api, event)
        if completed then return end
        completed = true
        api.set_state("front_door", "won", true)
        api.door("front_door", "close")
        api.door("front_door", "lock")
        api.sound("lobby_hum", false)
        api.sound("lock_click", true)
        api.notice("CLOSING TIME COMPLETE - Good night.")
    end,
    restart = function(api, event)
        api.physics("closing_items", false)
        api.reset()
    end,
}
