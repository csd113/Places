--!strict
-- Jun walks every metre; the last authored performance backs into the stool.
local phase, waypoint, elapsed = "hiding", 1, 0
local route = { {x=-10.5,z=-24.5}, {x=-11.5,z=-23.2},
    {x=-13.5,z=-22.7}, {x=-14.6067,z=-23.2918} }
local function navigate(api)
    local goal = route[waypoint]
    api.navigate("editor_prop_202", goal.x, 0, goal.z)
end
local function group(api)
    if phase == "seated" then
        api.message("THE CAMPFIRE CIRCLE", {"Seven friends, seven seats. Everyone is here.", "Lantern Hollow favor complete. Return through the Hub door when ready."})
    elseif phase == "hiding" then
        api.message("THE CAMPFIRE CIRCLE", {
            "Jun is playing hide-and-seek. One stump is empty.",
            "Look NORTHEAST of the fire, past the large boulder.",
            "A cool blue lantern points to the tall conifer.",
            "Look at the tree's far edge. F is your flashlight.",
            "Speak to Jun, then leave room for the walk home.",
        })
    elseif phase == "sitting" then
        api.message("THE CAMPFIRE CIRCLE", {"Jun has reached the empty stump and is sitting down.", "Give our friend a moment; the favor completes once everyone is seated."})
    else
        api.message("THE CAMPFIRE CIRCLE", {"Jun is walking back from the northeast tree.", "Stand to one side of the path and the empty stump.", "If our friend is stuck, speak to Jun to ask them to try the path again."})
    end
end
return {
    start = function(api)
        if api.completed("lantern_hollow") then
            phase = "returning"
            navigate(api)
            api.prompt("editor_prop_202", "Jun is returning to the fire / speak")
        end
    end,
    group = group,
    hint = group,
    friend = function(api)
        if phase == "hiding" then
            phase = "returning"
            elapsed = 0
            navigate(api)
            api.prompt("editor_prop_202", "Jun is returning to the fire / speak")
            api.message("JUN", {"Found me! I could hear them laughing from here.", "I will walk back to the empty stump by the fire.", "Walk alongside me and leave a little room on the path."})
        elseif phase == "seated" then
            api.message("JUN", {"Best hiding place. Better company.", "Lantern Hollow favor complete. Thank you for bringing me home."})
        elseif phase == "returning" then
            elapsed = 0
            navigate(api)
            api.message("JUN", {"The empty stump is mine. I am trying the path again.", "Stand to one side so I can get past; I will walk every step."})
        else
            api.message("JUN", {"Here is my stump. Just sitting down!"})
        end
    end,
    update = function(api, event)
        if phase == "returning" then
            elapsed += event.dt
            local actor, goal = api.actor("editor_prop_202"), route[waypoint]
            local dx, dz = actor.x-goal.x, actor.z-goal.z
            if dx*dx+dz*dz <= .025 then
                elapsed = 0
                if waypoint < #route then
                    waypoint += 1
                    navigate(api)
                else
                    phase = "sitting"
                    api.set_state("editor_prop_202", "seating", true)
                end
            elseif elapsed > 6 then
                elapsed = 0
                navigate(api)
            end
        elseif phase == "sitting" and api.state("editor_prop_202", "seated") then
            phase = "seated"
            api.prompt("editor_prop_202", "Jun is home / speak")
            if not api.completed("lantern_hollow") then
                api.complete()
                api.message("THE CAMPFIRE CIRCLE", {"One, two, three... seven. Everyone is here.", "Lantern Hollow favor complete. The Hub will remember.", "Return through the Hub door when ready."})
            end
        end
    end,
}
