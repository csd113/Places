--!strict
local destinations = { "home", "pool", "office", "arcade", "lantern_hollow", "winter", "beach", "aero" }
local unlocked, clearing = false, false
local clear_armed = 0
local function count(api)
    local total = 0
    for _, id in destinations do if api.completed(id) then total += 1 end end
    return total
end
local function ledger(api)
    clear_armed = 0
    local total = count(api)
    local status = api.campaign_status()
    local persistence = if status == "saved" then "Saved here. Quit and return to resume your collection."
        elseif status == "pending" then "Saving your collection progress..."
        else "Save unavailable. Old data kept; new favors last this session."
    if total == 8 then
        api.message("THE ROOM OF SMALL KINDNESSES", {
            "Eight places. Eight small kindnesses.",
            "The eastern door is open. Come in and rest a while.", persistence,
        })
    else
        api.message("THE COLLECTION LEDGER", {
            tostring(total) .. " of 8 favors remembered. Reward room locked.",
            "Find each host. F1 gives your local next step.",
            "Home, Pool, Office, Cosmic Lanes;",
            "Lantern Hollow, Winter, Beach, Frutiger Aero.", persistence,
            "The separate CLEAR switch erases this collection's save.",
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
        else ledger(api) end
    end,
    ledger = ledger,
    hint = ledger,
    door = function(api)
        clear_armed = 0
        if unlocked then api.door("kindness_door", "toggle") else ledger(api) end
    end,
    clear_progress = function(api)
        if clearing then return end
        if clear_armed <= 0 then
            clear_armed = 12
            api.message("CLEAR THIS COLLECTION?", {
                "This erases all eight saved favors and locks the reward room.",
                "Other projects and their saves are kept.",
                "Press this same CLEAR switch again within 12 seconds to confirm.",
                "Wait 12 seconds or read the ledger to cancel.",
                "Ordinary local retry switches always keep collection progress.",
            })
        else
            clear_armed, clearing = 0, true
            api.reset_campaign()
        end
    end,
    update = function(api, event)
        clear_armed = math.max(0, clear_armed-event.dt)
        if not clearing then return end
        local status = api.campaign_status()
        if status == "pending" then return end
        clearing = false
        if status == "saved" and count(api) == 0 then
            unlocked = false
            -- Reset reloads this module; start then displays the saved-zero ledger.
            api.reset()
        else
            api.message("COLLECTION KEPT", {
                "The saved favors could not be cleared. Your progress is kept.",
                "Check the save error and try the CLEAR switch again later.",
            })
        end
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
