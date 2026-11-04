--!strict
local active, done, point = false, false, 1
local marks = { {x=8,z=-7}, {x=13,z=-10}, {x=9,z=-13} }
local next_steps = {
    "Start at lantern 1, nearest the coach on the left.",
    "Glide across to lantern 2 on the right of the pond.",
    "Glide to lantern 3 at the far left of the pond.",
}
local function guidance(api)
    if done then
        api.message("THE ICE COACH", {
            "Winter favor complete. Your Hub progress is saved.",
            "The Hub elevator is back beside the main snowy path.",
        })
    elseif point == 4 then
        api.message("WINTER / THREE OF THREE", {
            "All three lanterns passed. You may leave the ice now.",
            "Return to the coach on the near bank and press E.",
            "The favor is awarded when you speak to the coach.",
        })
    elseif active then
        api.message("WINTER / ICE FIGURE", {
            tostring(point-1) .. " of 3 lanterns passed.",
            next_steps[point],
            "Walk close past it; no E press or time limit is needed.",
            "Stay on the ice until all three have rung. F1: next step.",
        })
    else
        api.message("WINTER / ICE FIGURE", {
            "Find the ice coach at the near left bank of the pond.",
            "Speak with E to begin the three-lantern figure.",
            "Pass lanterns 1, 2, 3 in order, then speak to him again.",
            "F1 shows your next step at any time.",
        })
    end
end
local function on_pond(player)
    if player.x <= 6 or player.x >= 16 or player.z <= -16
        or player.z >= -4 or player.y <= -.4 or player.y >= 2 then
        return false
    end
    -- The six snowy corner strips are real floor regions, not part of the ice.
    local edge = math.min(player.z+16, -4-player.z)
    local inset = if edge < .6 then 2.4 elseif edge < 1.2 then 1.65
        elseif edge < 1.8 then .9 else 0
    return player.x > 6+inset and player.x < 16-inset
end
return {
    start = function(api)
        done = api.completed("winter")
        api.prompt("winter_coach", if done then "Winter favor complete"
            else "Begin the three-lantern ice figure")
        guidance(api)
    end,
    talk = function(api)
        if done then guidance(api)
        elseif active and point == 4 then
            done, active = true, false
            api.complete()
            api.animate("winter_coach", "pose_arms_up", true)
            api.prompt("winter_coach", "Winter favor complete / Hub remembers")
            api.message("THE ICE COACH", {
                "You drew the whole figure, and came back safely.",
                "Winter favor complete. The Hub will remember.",
                "Return using the Hub elevator beside the main snowy path.",
            })
        elseif active then guidance(api)
        else
            point, active = 1, true
            api.message("THE ICE COACH", {
                "Pass the three numbered lanterns ON THE ICE in order.",
                "1: near left. 2: across right. 3: far left.",
                "Walk within a metre of each; it will ring when counted.",
                "Stay on the pond between lanterns. There is no time limit.",
                "Ashore early? Start again at 1; no need to talk again.",
                "After the third bell, return here and press E to finish.",
            })
        end
    end,
    hint = guidance,
    mark = guidance,
    update = function(api)
        if not active or done or point == 4 then return end
        local player = api.player()
        if point > 1 and not on_pond(player) then
            point = 1
            api.message("WINTER / TRY AGAIN", {
                "You reached the bank before finishing the figure.",
                "Progress is back to 0 of 3. Walk to lantern 1 to retry.",
                "No need to return to the coach until all three bells ring.",
            })
            return
        end
        local mark = marks[point]
        local dx, dz = player.x-mark.x, player.z-mark.z
        local feet_on_ice = player.y > -.22 and player.y < -.08
        if on_pond(player) and feet_on_ice and dx*dx+dz*dz < 1.1*1.1 then
            point += 1
            api.sound("winter_quest_bell", true)
            if point == 4 then
                api.prompt("winter_coach", "Finish the ice figure / speak with E")
                guidance(api)
            else
                api.message("WINTER / LANTERN PASSED", {
                    tostring(point-1) .. " of 3 lanterns passed.",
                    next_steps[point],
                })
            end
        end
    end,
}
