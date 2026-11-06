--!strict
-- Focused fixture hooks, not an elevator quest or cinematic.
return {
    checkpoint_state = function() return {} end,
    restore_state = function(saved) end,
    interact = function(api, event)
        api.message("Feltling", {"Come along. I will wait if you need me."})
        if api.state("guide", "feltling_status") == "arrived" then
            local route = if api.state("guide", "feltling_route") == "outbound" then "return" else "outbound"
            api.set_state("guide", "feltling_route", route)
        end
        api.set_state("guide", "feltling_mode", "guide")
    end,
    status = function(api, event)
        if api.state("guide", "feltling_status") == "arrived" then
            api.message("Feltling", {"Here we are. We can go back together."})
        end
    end,
}
