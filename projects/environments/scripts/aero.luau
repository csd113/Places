--!strict
local active, done = false, false
local valves = { false, false, false }
local function pressure() return (if valves[1] then 1 else 0) + (if valves[2] then 2 else 0) + (if valves[3] then 4 else 0) end
local function reward(api)
    api.enable("aero_fountain_mist", true)
    api.sound("aero_fountain_song", true)
    api.animate("aero_gardener", "pose_arms_up", true)
    api.spawn("aero_fish_point", "aero_reward_fish")
end
local function talk(api)
    if done then
        api.message("THE GLASS GARDENER", {"Water, air, and one very happy fish.", "The garden is breathing again."})
    else
        active = true
        api.message("THE GLASS GARDENER", {
            "The fountain needs FIVE measures of air.",
            "Blue sends one, white sends two, lime sends four.",
            "Each handle opens or closes its own pipe.",
            "Set the handles, then speak to me to release the fish.",
            "Current pressure: " .. tostring(pressure()) .. " of 5.",
        })
    end
end
return {
    start = function(api)
        done = api.completed("aero")
        if done then
            valves = { true, false, true }
            api.toggle_animation("aero_valve_blue", "toggle")
            api.toggle_animation("aero_valve_lime", "toggle")
            reward(api)
        end
    end,
    talk = function(api)
        if active and not done and pressure() == 5 then
            done = true
            api.complete()
            reward(api)
            api.message("THE GLASS GARDENER", {"Five measures. Listen to the fountain sing!", "Aero favor complete. The Hub will remember."})
        else talk(api) end
    end,
    valve = function(api, event)
        if done then return end
        local index = ({ aero_valve_blue=1, aero_valve_white=2, aero_valve_lime=3 })[event.subject]
        if index == nil then return end
        valves[index] = not valves[index]
        api.toggle_animation(event.subject, "toggle")
        api.message("FOUNTAIN PRESSURE", {tostring(pressure()) .. " measures. The gardener asks for five.", "Blue: 1. White: 2. Lime: 4. Speak to the gardener to test."})
    end,
    hint = talk,
}
