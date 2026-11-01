--!strict
local active, done, remembered, walking, point = false, false, false, false, 1
local waiting = ""
-- Pass the near parasol, then continue to the far garden parasol.
local route = {
    {x=2,z=5.5}, {x=-1,z=5.5}, {x=-4.5,z=5},
    {x=-8.5,z=7}, {x=-11.5,z=8}, {x=-13.6,z=9.7},
}
local function guidance(api)
    if done then
        api.message("THE SHADE KEEPER", {
            "The crab has reached the far striped parasol.",
            "Beach favor complete. Your Hub progress is saved.",
            "The Hub door waits back by the stilt house.",
        })
    elseif active then
        api.message("BEACH / WALK WITH THE CRAB", {
            "Follow the crab west, past the first striped parasol.",
            "Its destination is the far garden parasol by the rocks.",
            "Keep one to six metres away, on the sand beside it.",
            "Too close or too far? It waits; return beside it to resume.",
            "It must walk the whole route. Reaching the shade alone won't win.",
        })
    elseif remembered then
        api.message("THE SHADE KEEPER", {
            "Your Beach favor is already saved in the Hub.",
            "Visits start fresh. Speak to the crab for an optional walk.",
            "The Hub door waits back by the stilt house.",
        })
    else
        api.message("THE SHADE KEEPER", {
            "A little crab waits in the sun beside me, near the water.",
            "Speak to it with E, then walk west alongside it.",
            "Pass the first parasol to reach the far garden parasol.",
            "Keep one to six metres away. Nothing needs to be carried.",
            "It waits if crowded or left behind. F1: help at any time.",
        })
    end
end
return {
    start = function(api)
        remembered = api.completed("beach")
        if remembered then
            api.prompt("beach_quest_crab", "Walk with the crab again / optional")
            api.prompt("beach_keeper", "Beach favor already saved / optional walk")
        end
    end,
    talk = guidance,
    hint = function(api)
        if not active and not done and not remembered then
            api.message("BEACH", {
                "The shade keeper stands west of the stilt house.",
                "The small crab beside the keeper needs a walk to shade.",
                "Speak to the crab with E to start; F1 will guide you.",
            })
        else guidance(api) end
    end,
    crab = function(api)
        if done then
            api.message("THE LITTLE CRAB", {
                "Click-click. A contented sound from the shade.",
                "Beach favor complete. You may return to the Hub.",
            })
        elseif active then guidance(api)
        else
            active, waiting = true, ""
            api.prompt("beach_quest_crab", "Escort crab / one to six metres of room")
            api.message("THE LITTLE CRAB", {
                "Click? Click! It turns toward the far garden parasol.",
                "Walk beside it, one to six metres away on the sand.",
                "It will wait for you. No timer, carrying or throwing.",
            })
        end
    end,
    update = function(api)
        if not active then return end
        local crab = api.actor("beach_quest_crab")
        local player = api.player()
        local dx, dz = player.x-crab.x, player.z-crab.z
        local distance2 = dx*dx+dz*dz
        local reason = if distance2 < 1 or (waiting == "close" and distance2 < 1.44)
            then "close"
            elseif distance2 > 36 or (waiting == "far" and distance2 > 30.25)
                or math.abs(player.y-crab.y) > 1.5 then "far"
            else ""
        if reason ~= "" then
            if walking then
                api.navigate("beach_quest_crab", crab.x, crab.y, crab.z)
                walking = false
            end
            if waiting ~= reason then
                waiting = reason
                api.message("BEACH / CRAB WAITING", {
                    if reason == "close" then "Give the crab a metre of space."
                        else "Come back within six metres, on the sand beside it.",
                    "It will continue from here; your walk is not lost.",
                })
            end
            return
        end
        waiting = ""
        local goal = route[point]
        if not walking then
            api.navigate("beach_quest_crab", goal.x, 0, goal.z)
            walking = true
        end
        dx, dz = crab.x-goal.x, crab.z-goal.z
        -- Navigation coalesces goals within 25 cm. Arrival must include that
        -- band so pausing just short of a waypoint cannot strand the escort.
        if dx*dx+dz*dz < .28*.28 then
            if point < #route then
                point += 1
                walking = false
            else
                active, done = false, true
                api.animate("beach_quest_crab", "idle", true)
                api.prompt("beach_quest_crab", "Crab safe in the shade / favor complete")
                api.prompt("beach_keeper", "Beach favor complete / Hub remembers")
                if not remembered then api.complete() end
                remembered = true
                api.animate("beach_keeper", "pose_arms_up", true)
                api.message("THE SHADE KEEPER", {
                    "Look! It has settled beneath the far garden parasol.",
                    "Beach favor complete. The Hub will remember.",
                    "Return through the Hub door beside the stilt house.",
                })
            end
        end
    end,
}
