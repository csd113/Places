--!strict
-- A local, resettable favour. Ownership confirmation precedes dialogue/motion.
local phase = "waiting"
local pending_frames = 0
local waypoint = 1
local route = {
    { x = 1.5, z = 4.3 },
    { x = -1.5, z = 4.3 },
    { x = -1.5, z = 1.5 },
    { x = -7.5, z = 1.5 },
    { x = -7.5, z = -5.5 },
    { x = -13.5, z = -5.5 },
    { x = -13.5, z = 3.5 },
    { x = -19.5, z = 3.5 },
}

local function navigate(api)
    local goal = route[waypoint]
    api.navigate("home_caretaker", goal.x, 0, goal.z)
end

return {
    start = function(api)
        if api.completed("home") then phase = "complete" end
    end,

    talk = function(api)
        if phase == "complete" then
            api.message("THE CARETAKER", { "The cushion is safe. I remember your help.", "The Hub remembers it, too." })
            return
        end
        if phase == "departing" or phase == "hidden_wait" then
            api.message("THE CARETAKER", { "Thank you. That is the one.", "I am taking it down the west halls. Keep the path clear." })
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
                    "I will put this away down the winding west halls.",
                    "Feel free to explore; I will let you know when it is done.",
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
            api.complete()
            api.message("THE CARETAKER", {"All put away. Thank you for your help.", "Home favor complete. The Hub will remember."})
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
                "Lost the cushion? The entry-hall switch resets this visit.",
            })
        elseif phase == "departing" or phase == "pending" then
            api.message("HOME", {
                "The caretaker is carrying the cushion down the west halls.",
                "Leave room to pass if you follow them.",
                "Once they reach the end, step back around a corner.",
                "The entry-hall switch can restart a blocked visit.",
            })
        elseif phase == "hidden_wait" then
            api.message("HOME", {
                "The cushion has reached the end of the west halls.",
                "Step back around a solid corner to give them privacy.",
                "Home completes once both caretaker and cushion are hidden.",
                "Use the Hub elevator at the end of the entry hall.",
            })
        else
            api.message("HOME", {
                "Home favour complete. The Hub has recorded your help.",
                "Use the Hub elevator at the end of the entry hall.",
                "The entry-hall switch resets this visit only.",
            })
        end
    end,
}
