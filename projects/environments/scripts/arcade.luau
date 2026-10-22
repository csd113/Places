--!strict
local active, done, settled = false, false, 0
local pins = { "lane_1_pin_0_0", "lane_1_pin_1_0", "lane_1_pin_1_1",
    "lane_1_pin_2_0", "lane_1_pin_2_1", "lane_1_pin_2_2",
    "lane_1_pin_3_0", "lane_1_pin_3_1", "lane_1_pin_3_2", "lane_1_pin_3_3" }
local fallen, ball_held, launched = 0, false, false
local function talk(api)
    if done then
        api.message("THE LANE ATTENDANT", {"Ten down. Your name belongs among the stars.", "The lanes are yours for as long as you like."})
        return
    end
    if not active then
        for _, id in pins do
            local pin = api.actor(id)
            if not pin.physical or pin.up_y < .95 or pin.held
                or pin.x < 10.1 or pin.x > 11.2 or pin.z < 2.7 or pin.z > 3.9 then
                api.message("THE LANE ATTENDANT", {"Let's start with ten upright pins in lane one.", "Press the lane reset switch, then speak to me."})
                return
            end
        end
        ball_held, launched = false, false
    end
    active = true
    api.message("THE LANE ATTENDANT", {
        "Cosmic Lanes has one last constellation to knock down.",
        "Use the BLUE ball on lane one. All ten pins must fall.",
        "Stay behind the foul line at the near end of the lane.",
        "Several throws are fine. Lifting pins stops this attempt.",
        "The lane reset switch gives you a fresh rack.",
        "Pins down: " .. tostring(fallen) .. " of 10.",
    })
end
return {
    start = function(api) done = api.completed("arcade") end,
    talk = talk,
    hint = function(api) api.message("COSMIC LANES", { "Speak to the lane attendant near the seating end.", "Use the blue ball and stay behind lane one's foul line." }) end,
    restart = function(api)
        if done then return end
        active, settled, fallen = false, 0, 0
        api.message("THE LANE ATTENDANT", {"A fresh rack. Speak to me to start scoring again."})
    end,
    update = function(api, event)
        if not active or done then return end
        local player = api.player()
        if player.z < 20 then
            active, settled = false, 0
            api.message("THE LANE ATTENDANT", {"Over the foul line! Reset lane one for another try.", "Roll from the seating end; leave the pins to the ball."})
            return
        end
        local ball = api.actor("rack_0_ball_2")
        if api.held_item() == "rack_0_ball_2" then ball_held = true end
        if ball_held and ball.physical and not ball.held
            and ball.x > 10 and ball.x < 11.3 and ball.z < 20 then
            launched = true
        end
        fallen = 0
        for _, id in pins do
            local pin = api.actor(id)
            if pin.held then
                active, settled = false, 0
                api.message("THE LANE ATTENDANT", {"Hands off the pins, star bowler!", "Reset lane one, then speak to me to try again."})
                return
            end
            if pin.physical and pin.up_y < .50 then fallen += 1 end
        end
        settled = if launched and fallen == 10 then settled + event.dt else 0
        if settled > 1 then
            done = true
            api.complete()
            api.animate("arcade_attendant", "pose_arms_up", true)
            api.sound("arcade_quest_cheer", true)
            api.message("THE LANE ATTENDANT", {"A constellation cleared! Ten honest fallen pins.", "Arcade favor complete. The Hub will remember."})
        end
    end,
}
