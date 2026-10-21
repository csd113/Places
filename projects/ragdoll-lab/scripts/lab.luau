--!strict
-- Every request uses the production entity action/native solver handoff.
local actors = {"floor_actor", "wall_actor", "stair_actor", "prop_actor", "extra_1", "extra_2", "extra_3", "extra_4"}
return {
    reset = function(api, event)
        for _, id in actors do api.ragdoll(id, false) end
    end,
    all = function(api, event)
        for _, id in actors do api.ragdoll(id, true, 0.8, 0, -0.7) end
    end,
    floor = function(api, event) api.ragdoll("floor_actor", true, 1.5, 0, -0.4) end,
    carry = function(api, event)
        api.handoff("floor_actor")
    end,
}
