--!strict
-- Handlers receive a scoped Places API and event context.
local count = 0
return {
    entered = function(api, event)
        count += 1
        api.notice("Trigger calls: " .. tostring(count))
    end,
}
