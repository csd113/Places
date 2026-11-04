--!strict
local active, done, afloat = false, false, 0
-- A 25 cm inset still excludes the rim while allowing a gentle deck-side drop.
local function safe_float(duck)
    return duck.physical and not duck.held and duck.up_y > .75
        and duck.x > 4.25 and duck.x < 11.75
        and duck.z > 14.25 and duck.z < 18.25
        and duck.y > -.30 and duck.y < .03
end
local function talk(api)
    if done then
        api.message("THE POOL ATTENDANT", {"A champion in a very small boat.", "Pool favor complete. Return using the Hub elevator when ready."})
    else
        active = true
        api.message("THE POOL ATTENDANT", {
            "Find the duck on the table in the SECOND pool room.",
            "Carry it to that room's main basin.",
            "E gently puts it down; let go and step back.",
            "Keep it upright in the water for FOUR quiet seconds.",
            "Holding or tipping restarts the four-second count.",
            "Lost duck? The switch beside its table restores it.",
        })
    end
end
local function restored(api)
    afloat = 0
    api.message("REGATTA DUCK RESTORED", {
        "Restore requested: the duck returns if its table is clear.",
        if done then "Your completed favor stays recorded at the Hub." else "The float timer is back to 0/4. You can try as often as you like.",
        if active or done then "Carry the duck to that room's main basin and gently put it down." else "Speak to the pool attendant in the first room to begin the regatta.",
    })
end
return {
    start = function(api)
        done = api.completed("pool")
        if done then api.prompt("pool_attendant", "Regatta complete / speak") end
    end,
    talk = talk,
    restore = function(api)
        api.physics("pool_water_demo", false)
        restored(api)
    end,
    restored = restored,
    hint = function(api)
        if done then talk(api)
        elseif not active then
            api.message("POOL", {"Speak to the attendant beside the FIRST pool to start.", "The yellow duck waits on a table through the pool door, in the SECOND room."})
        else
            local duck = api.actor("pool_physics_duck")
            local detail = if duck.held then "You are holding the duck. E puts it down."
                elseif safe_float(duck) then "It is upright in the right basin. Give it time."
                else "Use the SECOND room's main basin, away from edges."
            api.message("REGATTA PROGRESS", {
                tostring(math.min(4, math.floor(afloat))) .. "/4 consecutive seconds upright and afloat.",
                detail,
                "The duck must be released, upright and in the water.",
                "Lost or stuck? The switch by its table restores it.",
            })
        end
    end,
    update = function(api, event)
        if not active or done then return end
        local duck = api.actor("pool_physics_duck")
        afloat = if safe_float(duck) then afloat + event.dt else 0
        if afloat >= 4 then
            done = true
            api.complete()
            api.animate("pool_attendant", "pose_arms_up", true)
            api.prompt("pool_attendant", "Regatta complete / speak")
            api.message("THE POOL ATTENDANT", {
                "Four seconds! A flawless maiden voyage.",
                "Pool favor complete. The Hub will remember.",
                "Return using the Hub elevator when ready.",
            })
        end
    end,
}
