--!strict
-- All calls use the editor's published host API. Reentry/reset starts fresh.
local ticks = 0
local observed = 0
return {
    update = function(api, event)
        ticks += 1
        -- Twelve ordinary entity interactions per tick, separately timed by VM receipts.
        for i = 0, 11 do
            local actor = api.actor("actor_" .. tostring(i))
            if actor ~= nil and actor.player_visible then observed += 1 end
        end
    end,
    activate = function(api, event) api.physics("lab_contacts", true) end,
    restore = function(api, event) api.physics("lab_contacts", false) end,
    visitor = function(api, event) api.spawn("visitor_point") end,
    hint = function(api, event)
        api.notice("ticks=" .. tostring(ticks) .. " visible=" .. tostring(observed))
    end,
}
