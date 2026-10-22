--!strict
local active, done, point = false, false, 1
local marks = { {x=8,z=-7}, {x=13,z=-10}, {x=9,z=-13} }
local function talk(api)
    if done then
        api.message("THE ICE COACH", {"A fine figure across the pond.", "Slow feet, brave heart. Remember that."})
    else
        point, active = 1, true
        api.message("THE ICE COACH", {
            "Will you trace a winter figure for me?",
            "Pass the three lanterns ON THE ICE, in order:",
            "Near left, far right, then far left. Return to me.",
            "Stay on the pond between lanterns. No time limit.",
            "Stepping ashore resets the figure. Speak to try again.",
        })
    end
end
return {
    start = function(api) done = api.completed("winter") end,
    talk = function(api)
        if active and point == 4 then
            done, active = true, false
            api.complete()
            api.animate("winter_coach", "pose_arms_up", true)
            api.message("THE ICE COACH", {"You drew the whole figure, and came back safely.", "Winter favor complete. The Hub will remember."})
        else talk(api) end
    end,
    hint = function(api)
        if active then
            api.message("THE ICE COACH", {"Next mark: " .. tostring(point) .. " of 3.", "Near left, far right, far left; then return to the coach.", "The ice is slippery. There is no hurry."})
        else talk(api) end
    end,
    update = function(api)
        if not active or done or point == 4 then return end
        local player = api.player()
        local on_ice = player.x > 6.8 and player.x < 15.3
            and player.z > -15.3 and player.z < -4.7
            and player.y > -.22 and player.y < -.08
        if point > 1 and not on_ice then
            active, point = false, 1
            api.message("THE ICE COACH", {"That figure reached the bank a little early.", "Come back to me for a fresh start."})
            return
        end
        local mark = marks[point]
        local dx, dz = player.x-mark.x, player.z-mark.z
        if on_ice and dx*dx+dz*dz < .8*.8 then
            point += 1
            api.sound("winter_quest_bell", true)
            if point == 4 then
                api.message("THE ICE COACH", {"Three lanterns passed. Come back to me on the bank!"})
            end
        end
    end,
}
