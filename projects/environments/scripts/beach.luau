--!strict
local active, done, remembered, walking, point = false, false, false, false, 1
local route = { {x=2,z=5}, {x=1,z=6.0}, {x=-.5,z=6.0}, {x=-1.85,z=6.3} }
local function talk(api)
    if done then
        api.message("THE SHADE KEEPER", {"A small shadow can be a very large kindness.", "That crab still remembers your company."})
        return
    end
    api.message("THE SHADE KEEPER", {
        "A little crab is stranded in the sun by the water.",
        "Speak to it, then walk with it toward the striped parasol.",
        "Keep within six metres, but give it a metre of room.",
        "It stops if crowded or left behind. Nothing to carry.",
    })
end
return {
    start = function(api)
        done = api.completed("beach")
        remembered, active = done, done
    end,
    talk = talk,
    hint = function(api) api.message("BEACH", { "The shade keeper waits on the sand west of the stilt house.", "The little crab needs company on its walk to the parasol." }) end,
    crab = function(api)
        if done and not active then
            api.message("THE LITTLE CRAB", {"Click-click. A contented sound from the shade."})
            return
        end
        active = true
        api.message("THE LITTLE CRAB", {"Click? Click! It turns toward the striped parasol.", "Walk beside it, with a little breathing room."})
    end,
    update = function(api)
        if not active then return end
        local crab = api.actor("beach_quest_crab")
        local player = api.player()
        local dx, dz = player.x-crab.x, player.z-crab.z
        local near = dx*dx+dz*dz >= 1.0 and dx*dx+dz*dz <= 36
        if not near then
            if walking then
                api.navigate("beach_quest_crab", crab.x, crab.y, crab.z)
                walking = false
            end
            return
        end
        local goal = route[point]
        if not walking then
            api.navigate("beach_quest_crab", goal.x, 0, goal.z)
            walking = true
        end
        dx, dz = crab.x-goal.x, crab.z-goal.z
        if dx*dx+dz*dz < .04 then
            if point < #route then
                point += 1
                walking = false
            else
                active, done = false, true
                api.animate("beach_quest_crab", "idle", true)
                if not remembered then api.complete() end
                api.animate("beach_keeper", "pose_arms_up", true)
                if remembered then
                    api.message("THE SHADE KEEPER", {"Back beneath the parasol. It remembers the way now."})
                else
                    api.message("THE SHADE KEEPER", {"Look! It has settled beneath the parasol.", "Beach favor complete. The Hub will remember."})
                end
            end
        end
    end,
}
