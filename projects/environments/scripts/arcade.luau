--!strict
local active, done, settled = false, false, 0
local pins = { "lane_1_pin_0_0", "lane_1_pin_1_0", "lane_1_pin_1_1",
    "lane_1_pin_2_0", "lane_1_pin_2_1", "lane_1_pin_2_2",
    "lane_1_pin_3_0", "lane_1_pin_3_1", "lane_1_pin_3_2", "lane_1_pin_3_3" }
local fallen, ball_held, launched = 0, false, false
local down = {}
local resetting, shot_active, shot_elapsed = false, false, 0
local return_cooldown = 0
local function ready_rack(api)
    for _, id in pins do
        local pin = api.actor(id)
        if not pin.physical or pin.up_y < .95 or pin.held
            or pin.x < 10.1 or pin.x > 11.2 or pin.z < 2.7 or pin.z > 3.9 then
            return false
        end
    end
    return true
end
local function clear_score()
    settled, fallen, ball_held, launched = 0, 0, false, false
    down = {}
    shot_active, shot_elapsed = false, 0
end
local function return_ball(api)
    api.physics("arcade_blue_ball", false)
    ball_held, shot_active, shot_elapsed = false, false, 0
    return_cooldown = 2
end
local function talk(api)
    if api.state("lane_1_secret_switch", "miniatures") then
        api.message("THE LANE ATTENDANT", {"You found our smallest regulars. Enjoy the secret rack!", "Use the normal reset switch for the blue-ball favor."})
        return
    end
    if done then
        api.message("THE LANE ATTENDANT", {"Ten down. Your name belongs among the stars.", "Your Arcade favor is saved. The lanes are yours.", "Return BLUE ball keeps the pins; Fresh rack resets everything."})
        return
    end
    if not active then
        if not ready_rack(api) then
            api.message("THE LANE ATTENDANT", {"Use FRESH RACK beside lane one for ten upright pins.", "It starts scoring automatically. You have unlimited retries."})
            return
        end
        clear_score()
    end
    active, resetting = true, false
    api.message("THE LANE ATTENDANT", {
        "Use the BLUE ball on lane one. Knock down all ten pins.",
        "Stand behind the black foul line and aim along the arrows.",
        "Press E to pick up the ball; left click to bowl it.",
        "RETURN BLUE BALL beside the lane keeps your fallen pins.",
        "FRESH RACK resets the pins and starts another unlimited try.",
        "Pins down: " .. tostring(fallen) .. " of 10.",
    })
end
return {
    start = function(api) done = api.completed("arcade") end,
    talk = talk,
    hint = function(api)
        if done then
            api.message("COSMIC LANES", {"Arcade favor complete and saved. Return through the Hub door.", "You can keep bowling, return the ball or reset the rack."})
        else
            api.message("COSMIC LANES", {
                "Pins down: " .. tostring(fallen) .. " of 10.",
                "Use the BLUE ball on lane one.",
                "Knock down all ten; stay behind black line.",
                "E picks up; left click bowls.",
                "RETURN BLUE BALL keeps fallen pins.",
                "FRESH RACK: another unlimited try.",
            })
        end
    end,
    secret = function(_api)
        active, resetting = false, false
        clear_score()
    end,
    return_ball = function(api)
        return_ball(api)
    end,
    restart = function(api)
        active, resetting = false, not done
        clear_score()
        api.prompt("arcade_attendant", if done then "Arcade favor complete - free bowling" else "Fresh rack preparing - unlimited retries")
        api.message("THE LANE ATTENDANT", {"Fresh rack requested. Scoring starts when all ten pins are upright.", "The blue ball returns to the approach. Step clear if a reset is blocked.", "RETURN BLUE BALL keeps fallen pins between throws."})
    end,
    update = function(api, event)
        if api.state("lane_1_secret_switch", "miniatures") then return end
        return_cooldown = math.max(0, return_cooldown - event.dt)
        local player = api.player()
        local ball = api.actor("rack_0_ball_2")
        if api.held_item() == "rack_0_ball_2" then
            if player.z >= 20 then ball_held = true end
            shot_active = false
        elseif ball_held and ball.physical and not ball.held
            and ball.x > 10 and ball.x < 11.3 and ball.z < 20 then
            launched, ball_held, shot_active, shot_elapsed = true, false, true, 0
        end
        if shot_active and not ball.held then
            shot_elapsed += event.dt
            if return_cooldown == 0 and ((ball.sleeping and shot_elapsed > 2)
                or shot_elapsed > 14 or ball.y < -2
                or ball.x < -7 or ball.x > 31 or ball.z < -1 or ball.z > 31) then
                return_ball(api)
            end
        end
        if resetting and ready_rack(api) then
            resetting, active = false, true
            api.prompt("arcade_attendant", "Lane one ready - 0 of 10 pins down")
        end
        if not active or done then return end
        if player.x > 9.7 and player.x < 11.6 and player.z < 20 then
            active, settled = false, 0
            api.prompt("arcade_attendant", "Foul - use Fresh rack for another unlimited try")
            api.message("THE LANE ATTENDANT", {"You crossed the foul line. Come back to the seating end.", "Press FRESH RACK for another try; scoring restarts automatically.", "Your saved collection progress is unchanged."})
            return
        end
        local previous = fallen
        fallen = 0
        for _, id in pins do
            local pin = api.actor(id)
            if pin.held then
                active, settled = false, 0
                api.message("THE LANE ATTENDANT", {"Hands off the pins, star bowler!", "Use FRESH RACK to start another unlimited try."})
                return
            end
            if launched and pin.physical and pin.up_y < .50 then down[id] = true end
            if down[id] then fallen += 1 end
        end
        if fallen ~= previous then
            api.prompt("arcade_attendant", "Lane one: " .. tostring(fallen) .. " of 10 pins down")
        end
        settled = if launched and fallen == 10 then settled + event.dt else 0
        if settled > 1 then
            done = true
            api.complete()
            api.animate("arcade_attendant", "pose_arms_up", true)
            api.sound("arcade_quest_cheer", true)
            api.prompt("arcade_attendant", "Arcade favor complete - return to Hub or keep bowling")
            api.message("THE LANE ATTENDANT", {"A constellation cleared! Ten honest fallen pins.", "Arcade favor complete and saved. Return through the Hub door.", "You can keep bowling. Ball return and fresh racks stay available."})
        end
    end,
}
