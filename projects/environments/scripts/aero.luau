--!strict
local done = false
local valves = { false, false, false }
local ids = { "aero_valve_blue", "aero_valve_white", "aero_valve_lime" }
local names = { "BLUE", "WHITE", "LIME" }
local amounts = { 1, 2, 4 }
local function pressure() return (if valves[1] then 1 else 0) + (if valves[2] then 2 else 0) + (if valves[3] then 4 else 0) end
local function prompts(api)
    for index, id in ids do
        local state = if valves[index] then "OPEN" else "CLOSED"
        local action = if done then "complete" elseif valves[index] then "E to close" else "E to open"
        api.prompt(id, names[index] .. " / " .. tostring(amounts[index]) .. " / " .. state .. " - " .. action)
    end
    api.prompt("aero_gardener", if done then "Fountain restored / speak" else "Fountain: " .. tostring(pressure()) .. "/5 measures / speak to test")
end
local function reward(api)
    api.enable("aero_fountain_mist", true)
    api.sound("aero_fountain_song", true)
    api.animate("aero_gardener", "pose_arms_up", true)
    if not api.exists("aero_reward_fish") then
        api.spawn("aero_fish_point", "aero_reward_fish")
    end
end
local function talk(api)
    if done then
        api.message("THE GLASS GARDENER", {"Water, air, and one very happy fish.", "Aero favor complete. Return using the Hub elevator when ready."})
    else
        api.message("THE GLASS GARDENER", {
            "Restore the fountain with exactly FIVE measures.",
            "Go around the glass divider's right end to the NORTH wall.",
            "BLUE adds 1, WHITE adds 2, LIME adds 4 while open.",
            "E opens/closes a control. Its prompt shows the state.",
            "Current pressure: " .. tostring(pressure()) .. "/5.",
            "Speak to me at five to release the fish. F1 helps.",
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
        prompts(api)
    end,
    talk = function(api)
        if not done and pressure() == 5 then
            done = true
            api.complete()
            reward(api)
            prompts(api)
            api.message("THE GLASS GARDENER", {"Five measures. Listen to the fountain sing!", "Aero favor complete. The Hub will remember.", "Return using the Hub elevator when ready."})
        else talk(api) end
    end,
    valve = function(api, event)
        if done then return end
        local index = ({ aero_valve_blue=1, aero_valve_white=2, aero_valve_lime=3 })[event.subject]
        if index == nil then return end
        valves[index] = not valves[index]
        api.toggle_animation(event.subject, "toggle")
        prompts(api)
        api.message("FOUNTAIN PRESSURE", {
            names[index] .. " is " .. (if valves[index] then "OPEN." else "CLOSED."),
            tostring(pressure()) .. "/5 measures. Open controls add their numbers together.",
            if pressure() == 5 then "Exactly five! Speak to the gardener to release the fish." else "BLUE: 1. WHITE: 2. LIME: 4. E toggles each control.",
        })
    end,
    hint = talk,
}
