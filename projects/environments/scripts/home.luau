--!strict
-- A local, resettable favour. Ownership confirmation precedes dialogue/motion.
local phase = "waiting"
local pending_frames = 0
local waypoint = 1
local route = {
    { x = 1.5, z = 4.3 },
    { x = -1.5, z = 4.3 },
    { x = -1.5, z = 1.5 },
    { x = -8.0, z = 2.2 },
}

local function navigate(api)
    local goal = route[waypoint]
    api.navigate("home_caretaker", goal.x, 0, goal.z)
end

return {
    talk = function(api)
        if phase == "departing" or phase == "hidden_wait" then
            api.message("THE CARETAKER", { "Thank you. That is the one.", "I will put it away." })
            return
        end
        if phase == "pending" then
            return
        end
        local held = api.held_item()
        if held == "home_requested_cushion" then
            api.handoff("home_caretaker")
            phase = "pending"
            pending_frames = 0
        elseif held ~= nil then
            api.message("THE CARETAKER", {
                "That is not the one I need.",
                "The checked cushion is on the bedroom chair.",
            })
        else
            api.message("THE CARETAKER", {
                "Could you bring me the checked cushion?",
                "I left it on the chair in the bedroom.",
            })
        end
    end,

    update = function(api)
        if phase == "waiting" or phase == "complete" then
            return
        end
        local actor = api.actor("home_caretaker")
        if phase == "pending" then
            if actor.carried_item == "home_requested_cushion" then
                phase = "departing"
                navigate(api)
                api.message("THE CARETAKER", {
                    "Yes. Thank you for finding it.",
                    "I will put this away now.",
                })
            else
                pending_frames += 1
                if pending_frames >= 4 then
                    phase = "waiting"
                    api.message("THE CARETAKER", {
                        "Bring it a little closer, please.",
                        "Keep hold of the checked cushion and speak to me.",
                    })
                end
            end
            return
        end
        if phase == "departing" then
            local goal = route[waypoint]
            local dx, dz = actor.x - goal.x, actor.z - goal.z
            if dx * dx + dz * dz <= .04 then
                if waypoint < #route then
                    waypoint += 1
                    navigate(api)
                else
                    phase = "hidden_wait"
                    api.retire_hidden("home_caretaker")
                end
            end
        elseif phase == "hidden_wait" and actor.retired then
            phase = "complete"
            api.notice("Caretaker and carried cushion retired only after whole-model occlusion")
        end
    end,

    reset = function(api)
        api.reset()
    end,

    hint = function(api)
        if phase == "waiting" then
            api.message("HOME", {
                "Speak to the caretaker in the living room.",
                "The checked cushion is on the bedroom chair.",
                "The entry-hall switch resets this visit.",
            })
        else
            api.message("HOME", {
                "The caretaker will wait while you keep them in view.",
                "Return to the Hub through the entry hall.",
                "The entry-hall switch resets this visit.",
            })
        end
    end,
}
