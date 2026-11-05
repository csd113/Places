--!strict
-- A run is deliberately deterministic. The third arrival is always the same
-- physical guest until reset; appearances never identify the follower.
local arrivals = 0
local armed = true
local clock = 0
local lockPending = false
local caught = false
local phase = "quiet"
local pulse = 0
local latchLabel = ""
local follower = "guest_3#3"
local enteredTogether = false
local roomPause = 0

local function state(api, value)
    if phase ~= value then
        phase = value
        api.set_state("party_cake", "pursuit", value)
    end
end
local function outside(p)
    -- Standing hull half width .4, plus .2 clearance from the doorway plane.
    return p.z < 10.2 or p.x > 6.2
end
local function inside(a)
    -- .4 extra beyond the .4 m navigation body covers the
    -- posed arms in both authored stand/walk clips. Never test just a torso.
    local r = a.radius + .4
    return a.x-r > .2 and a.x+r < 5.4 and a.z-r > 11
        and a.z+r < 15.8 and a.y >= -.02 and a.y+a.height < 3
end
local function proven(api)
    if arrivals < 3 or not api.exists(follower) then return false end
    local d = api.door_status("store_door")
    return d.phase == "closed" and d.locked and not d.obstructed
        and outside(api.player()) and inside(api.actor(follower))
end
local function latch_prompt(api, d)
    local label = d.locked and "Unlock waiting room"
        or (lockPending and "Cancel locking" or "Close and lock waiting room")
    if label ~= latchLabel then
        latchLabel = label
        api.prompt("store_latch", label)
    end
end
return {
    start = function(api)
        api.prompt("party_cake", "Birthday cake - waiting for guests")
        api.prompt("store_latch", "Close and lock waiting room")
    end,
    arcade = function(api)
        if arrivals < 6 then armed = true end
    end,
    returned = function(api)
        if not armed or arrivals >= 6 then return end
        armed = false
        arrivals += 1
        api.spawn("guest_" .. tostring(arrivals))
        api.set_state("party_cake", "arrivals", arrivals)
        if arrivals == 6 then api.prompt("party_cake", "Blow out birthday candles") end
    end,
    latch = function(api)
        local d = api.door_status("store_door")
        -- The external latch always reverses a lock or pending close. Door
        -- handles also unlock from inside: the player can never be imprisoned.
        if d.locked or lockPending then
            lockPending = false
            api.door("store_door", "unlock")
            api.door("store_door", "open")
        elseif outside(api.player()) then
            lockPending = true
            api.door("store_door", "close")
        end
    end,
    handle = function(api)
        local d = api.door_status("store_door")
        lockPending = false
        -- Only an inside occupant can defeat the external lock at the handle.
        if d.locked and not outside(api.player()) then
            api.door("store_door", "unlock")
            api.door("store_door", "open")
        elseif not d.locked then api.door("store_door", "toggle") end
    end,
    finish = function(api)
        if arrivals == 6 and proven(api) then
            api.finish(true, "The party is over", "The follower is locked in the waiting room. The birthday party is finished.")
        else
            -- In-world light response, no tutorial, hint panel or quip.
            pulse = .7
            api.light("party_light", false)
        end
    end,
    restart = function(api) api.reset() end,
    update = function(api, event)
        if pulse > 0 then
            pulse -= event.dt
            if pulse <= 0 then api.light("party_light", true) end
        end
        clock += event.dt
        if clock < .1 then return end
        local dt = clock
        clock = 0
        local p = api.player()
        local d = api.door_status("store_door")
        if lockPending then
            if not outside(p) then
                lockPending = false
                api.door("store_door", "unlock")
                api.door("store_door", "open")
                api.prompt("store_latch", "Close and lock waiting room")
            elseif d.phase == "closed" and not d.obstructed then
                lockPending = false
                api.door("store_door", "lock")
                api.prompt("store_latch", "Unlock waiting room")
            end
        end
        latch_prompt(api, d)
        if arrivals < 3 or not api.exists(follower) then return end
        local a = api.actor(follower)
        -- Doorway caution is physical and visible. A guest already entirely in
        -- the room waits while the leaf sweeps, rather than walking into it.
        if (d.phase == "closing" or d.phase == "opening") and inside(a) then
            api.ai_pause(follower,"guest_follow",true)
            api.navigate(follower, a.x, a.y, a.z)
            state(api, "doorway")
            return
        end
        local contained = d.locked and d.phase == "closed" and not d.obstructed
            and outside(p) and inside(a)
        if contained then
            if not caught then
                caught = true
                api.ai_pause(follower,"guest_follow",true)
                api.navigate(follower, a.x, a.y, a.z)
                api.prompt("party_cake", arrivals == 6 and "Finish the birthday party" or "Birthday cake - waiting for guests")
                api.set_state("party_cake", "contained", true)
                state(api, "contained")
            end
            return
        end
        if caught then
            caught = false
            api.set_state("party_cake", "contained", false)
            api.prompt("party_cake", "Blow out birthday candles")
        end
        -- At the spare chair the guest briefly surveys the room. This visible,
        -- bounded pause gives an ordinary player time to reach the external
        -- latch; after 2.5 seconds an open room releases pursuit normally.
        if inside(a) and p.z > 11.5 and p.x > .6 and p.x < 5 then
            enteredTogether = true
        elseif enteredTogether and outside(p) then
            enteredTogether = false
            roomPause = 2.5
        end
        if roomPause > 0 and inside(a) then
            roomPause -= dt
            api.ai_pause(follower,"guest_follow",true)
            api.navigate(follower, a.x, a.y, a.z)
            state(api, "surveying")
            return
        end
        api.ai_task(follower,"guest_follow",{
            key="guest_follow",priority=80,
            definition={task="follow",target="player",distance=1.5,memory_seconds=4},
        })
        api.ai_pause(follower,"guest_follow",false)
        state(api, a.ai_phase == "target_lost" and "waiting"
            or (a.ai_phase == "searching" and "searching" or "following"))
    end,
}
