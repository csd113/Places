--!strict
-- Jun walks every metre; the last authored performance backs into the stool.
local phase, waypoint, elapsed = "hiding", 1, 0
local route = { {x=-12.5,z=-36.5}, {x=-13.5,z=-35.2},
    {x=-15.5,z=-34.7}, {x=-16.6067,z=-35.2918} }
local function navigate(api)
    local goal = route[waypoint]
    api.navigate("hollow_missing_friend", goal.x, 0, goal.z)
end
local function group(api)
    if api.completed("lantern_hollow") then
        api.message("THE CAMPFIRE CIRCLE", {"Seven friends, seven seats. Jun loves another round.", "We remember who brought our friend home."})
    elseif phase == "hiding" then
        api.message("THE CAMPFIRE CIRCLE", {
            "Count the stumps. One of our friends is missing.",
            "Jun is playing hide-and-seek beyond the east trees.",
            "Look northeast of the fire, behind the tall conifer.",
            "Speak to Jun. Keep the path to the empty stool clear.",
        })
    else
        api.message("THE CAMPFIRE CIRCLE", {"Jun is on the way! Make room beside the empty stool.", "We will count everyone in once our friend is seated."})
    end
end
return {
    start = function(api)
        if api.completed("lantern_hollow") then
            phase = "returning"
            navigate(api)
        end
    end,
    group = group,
    hint = group,
    friend = function(api)
        if phase == "hiding" then
            phase = "returning"
            navigate(api)
            api.message("JUN", {"Found me! I could hear them laughing from here.", "I will walk back. Leave me a little room by the fire."})
        elseif phase == "seated" then
            api.message("JUN", {"Best hiding place. Better company.", "Thank you for coming to find me."})
        else
            api.message("JUN", {"The empty stump is mine. I know the way.", "You can walk alongside me, if you like."})
        end
    end,
    update = function(api, event)
        if phase == "returning" then
            elapsed += event.dt
            local actor, goal = api.actor("hollow_missing_friend"), route[waypoint]
            local dx, dz = actor.x-goal.x, actor.z-goal.z
            if dx*dx+dz*dz <= .025 then
                elapsed = 0
                if waypoint < #route then
                    waypoint += 1
                    navigate(api)
                else
                    phase = "sitting"
                    api.set_state("hollow_missing_friend", "seating", true)
                end
            elseif elapsed > 12 then
                elapsed = 0
                navigate(api)
            end
        elseif phase == "sitting" and api.state("hollow_missing_friend", "seated") then
            phase = "seated"
            if not api.completed("lantern_hollow") then
                api.complete()
                api.message("THE CAMPFIRE CIRCLE", {"One, two, three... seven. Everyone is here.", "Lantern Hollow favor complete. The Hub will remember."})
            end
        end
    end,
}
