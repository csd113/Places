--!strict
return {
    start = function(api, _event)
        assert(api.actor("shadow_near").shadow_only)
        assert(not api.actor("reset_marker").shadow_only)
        api.spawn("specter_spawn")
        api.notice("Shadow-only policy verified; spawned animated specter")
    end,
    touched = function(api, _event)
        assert(api.state("shadow_near", "touched"))
        api.notice("Hidden entity interaction remains active")
    end,
}
