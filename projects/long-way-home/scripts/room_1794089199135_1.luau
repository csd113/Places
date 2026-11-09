--!strict
-- The Long Way Home: introduction dialogue, authored in the native editor.
local greeted = false
local departure_spoken = false
return {
    start = function(api, event)
        api.message("The Long Way Home", {"This place is not your world.", "Somewhere beyond the hum, someone is waiting."})
    end,
    update = function(api, event)
        local p = api.player()
        if not greeted and (p.x-10)^2 + p.y^2 + (p.z-8)^2 <= 6.25 then
            greeted = true
            api.message("Feltling", {"You are lost too? We need your help.", "Come with me. My friends are through the small opening."})
        end
        if greeted and not departure_spoken and p.x >= 45 and p.z >= 34 then
            departure_spoken = true
            api.message("Feltling", {"One of us is already looking below.", "Please bring our duck back. We will wait for you."})
        end
    end,
    duck_magic = function(api, event)
        api.message("Feltlings", {"Our little duck holds a protective magic.", "It kept this place gentle. Then we lost it below."})
    end,
    darkness = function(api, event)
        api.message("Feltlings", {"Something waits in the dark down there.", "It cannot see. Be careful what it hears."})
    end,
    request = function(api, event)
        api.message("Feltlings", {"Will you bring our duck home?", "Its magic can send you back to your own world.", "Follow our friend to the elevator when you are ready."})
    end,
}
