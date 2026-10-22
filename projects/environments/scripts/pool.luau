--!strict
local active, done, afloat = false, false, 0
local function talk(api)
    if done then
        api.message("THE POOL ATTENDANT", {"A champion in a very small boat.", "Your kindness is remembered at the Hub."})
    else
        active = true
        api.message("THE POOL ATTENDANT", {
            "The quiet pool deserves its own regatta.",
            "Find the yellow duck on the table beyond the pool door.",
            "Launch it into that room's water, away from the edge.",
            "Let go and give it four quiet seconds to float upright.",
            "The table's restore control can recover a lost duck.",
        })
    end
end
return {
    start = function(api) done = api.completed("pool") end,
    talk = talk,
    hint = function(api) api.message("POOL", { "Speak to the attendant beside the first pool.", "The yellow duck waits on a table through the pool door." }) end,
    update = function(api, event)
        if not active or done then return end
        local duck = api.actor("pool_physics_duck")
        local safe = duck.physical and not duck.held and duck.up_y > .75
            and duck.x > 4.5 and duck.x < 11.5
            and duck.z > 14.5 and duck.z < 18
            and duck.y > -.30 and duck.y < .03
        afloat = if safe then afloat + event.dt else 0
        if afloat >= 4 then
            done = true
            api.complete()
            api.animate("pool_attendant", "pose_arms_up", true)
            api.message("THE POOL ATTENDANT", {
                "Four seconds! A flawless maiden voyage.",
                "Our little captain has the pool to itself.",
                "Pool favor complete. The Hub will remember.",
            })
        end
    end,
}
