--!strict
local destinations = { "home", "pool", "office", "arcade", "lantern_hollow", "winter", "beach", "aero" }
local unlocked = false
local function count(api)
    local total = 0
    for _, id in destinations do if api.completed(id) then total += 1 end end
    return total
end
local function ledger(api)
    local total = count(api)
    if total == 8 then
        api.message("THE ROOM OF SMALL KINDNESSES", {"Eight places. Eight small kindnesses.", "The eastern door is open. Come in and rest a while."})
    else
        -- Two short rows retain the full roster within the guidance contract.
        api.message("THE COLLECTION LEDGER", {
            tostring(total) .. " of 8 favors remembered.",
            "Find the people in each place. F1 offers a local hint.",
            "Home, Pool, Office, Cosmic Lanes;",
            "Lantern Hollow, Winter, Beach, Frutiger Aero.",
            "Every favor earns its own keepsake in the eastern room.",
        })
    end
end
return {
    start = function(api)
        unlocked = count(api) == 8
        if unlocked then
            api.door("kindness_door", "unlock")
            api.door("kindness_door", "open")
            api.animate("kindness_host", "pose_arms_up", true)
            api.sound("kindness_music", true)
            api.message("WELCOME BACK", {"All eight favors are remembered.", "The eastern door opens into the Room of Small Kindnesses.", "There is a place for you here."})
        end
    end,
    ledger = ledger,
    hint = ledger,
    door = function(api)
        if unlocked then api.door("kindness_door", "toggle") else ledger(api) end
    end,
    host = function(api)
        if not unlocked then return end
        api.message("THE KEEPER OF SMALL THINGS", {
            "A cushion. A duck. A page that found its place.",
            "Fallen stars. A circle of friends. A figure on the ice.",
            "A little shade. A garden breathing again.",
            "None of them changed the whole world.",
            "Every one of them changed somebody's evening.",
            "This room is yours. Stay as long as you like.",
        })
    end,
}
