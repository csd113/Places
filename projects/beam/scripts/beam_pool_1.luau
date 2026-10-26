--!strict
-- It Only Exists in the Beam: authored section rules.
local C = {["next_name"]="Office",["title"]="Pool",["heading"]="POOL (2 OF 4)",["next_level"]="beam_office",["previous_level"]="beam",["final"]=false,["grace"]=10,["speed"]=0.8064,["memory"]=5,["refuge"]={2.7,6.5,23.3,28},["post"]={20,0,16},["patrol"]={{20,0,16},{20,0,7},{14,0,6.8},{5,0,8},{5,0,20},{12,0,22},{20,0,22},{20,0,4}},["receivers"]={["return_duck"]=1,["return_towel"]=2,["return_cleaner"]=3},["items"]={{["state"]="item_1_returned",["hint"]="The rubber duck is floating in the pool.",["destination"]="Put it on the plastic table in the service corridor."},{["state"]="item_2_returned",["hint"]="The blue towel is on a bench in the changing area.",["destination"]="Put it on the bench in the service corridor."},{["state"]="item_3_returned",["hint"]="The detergent is on a low shelf by the utility sink. Hold C to crouch.",["destination"]="Put it on the top shelf of the empty rack in the service corridor."}},["exit_hint"]="Use the exit door to go to the Office.",["intro"]={"Put the rubber duck on the plastic table in the service corridor.","Put the blue towel on the bench beside that table.","Put the detergent on the top shelf of the empty rack.","E: carry or put down. Hold C to crouch.","F: flashlight on/off. Keep the shadow in the beam to stop it.","F1: item hints. EXIT sign: restart. Exit door: go to the Office."}}
-- The renderer owns actual beam occlusion/reveal/hold. Navigation owns movement.
-- This map-local layer owns objectives, refuge boundaries, search pacing and ending.
-- Shared AI owns patrol/search; this layer retains narrative rules and timing.
local returned = { false, false, false }
local elapsed, nav_wait, close_seconds = 0, 0, 0
local finished, step_wait, beam_hold, sealed = false, 0, 0, false
local previous_x, previous_z = C.post[1], C.post[3]
local step_distance, step_sounding = 0, false
local function count()
    return (if returned[1] then 1 else 0) + (if returned[2] then 1 else 0) + (if returned[3] then 1 else 0)
end
local function refuge(p)
    local r = C.refuge
    return p.x >= r[1] and p.x <= r[2] and p.z >= r[3] and p.z <= r[4] and p.y < 0.6
end
local door_ready = false
local function ready(api)
    local n = count()
    local can_leave = n == 3 and (not C.final or sealed)
    api.set_state('exit', 'count', n)
    api.set_state('exit', 'ready', can_leave)
    -- The authored door starts locked. Send commands only when its state changes.
    if can_leave ~= door_ready then
        api.door('exit', if can_leave then 'unlock' else 'lock')
        door_ready = can_leave
    end
    if can_leave then
        api.prompt('exit', if C.final then 'Leave the arcade' else 'Continue to ' .. C.next_name)
    elseif C.final and n == 3 then
        api.prompt('exit', 'All items returned. Leave the foyer for the flashlight check.')
    else
        api.prompt('exit', tostring(n) .. ' of 3 returned. F1: item hints.')
    end
end
local function hint(api)
    local n = count()
    local lines = { C.title .. ': ' .. tostring(n) .. ' of 3 items returned.' }
    if n < 3 then
        for i = 1, 3 do
            if not returned[i] then
                table.insert(lines, C.items[i].hint)
                table.insert(lines, C.items[i].destination)
                break
            end
        end
        table.insert(lines, 'Put the item down to return it. Picking it up undoes the return.')
    elseif C.final and not sealed then
        table.insert(lines, 'Leave the foyer and point your flashlight at the moving shadow.')
        table.insert(lines, 'Keep the beam on it for two uninterrupted seconds.')
        table.insert(lines, 'Move closer if the exit stays locked, then go back to the exit.')
    else
        table.insert(lines, C.exit_hint)
        table.insert(lines, 'Leave the items where you put them.')
    end
    table.insert(lines, 'Press F for the flashlight. Keep the shadow in the beam to stop it.')
    table.insert(lines, 'Use the EXIT sign to restart. Start from the menu to return to Home.')
    api.message('WHAT TO DO NEXT', lines)
end
local function mark(api, event, present)
    if finished then return end
    local index = C.receivers[event.subject]
    if index == nil or returned[index] == present then return end
    returned[index] = present
    api.set_state('exit', C.items[index].state, present)
    ready(api)
    if present then api.sound('returned_cue', true) end
    api.notice(C.title .. ': ' .. tostring(count()) .. ' of 3 items returned.')
end
return {
    start = function(api)
        ready(api)
        api.message(C.heading, C.intro)
    end,
    hint = function(api) if not finished then hint(api) end end,
    returned = function(api, event) mark(api, event, true) end,
    removed = function(api, event) mark(api, event, false) end,
    exit = function(api)
        if finished then return end
        if count() ~= 3 or (C.final and not sealed) then hint(api); return end
        finished = true
        api.set_state('exit', 'won', true)
        api.sound('store_hum', false)
        api.sound('exit_cue', true)
        if C.final then
            api.finish(true, 'YOU ESCAPED', 'You escaped. Press Enter to play Arcade again.')
        else
            api.change_level(C.next_level)
        end
    end,
    back = function(api)
        if not finished and C.previous_level ~= '' then api.change_level(C.previous_level) end
    end,
    restart = function(api)
        api.physics('beam_items', false)
        api.reset()
    end,
    update = function(api, event)
        if finished then return end
        local dt = event.dt
        elapsed += dt
        if elapsed < C.grace then return end
        local p, a = api.player(), api.actor('visitor')
        local dx, dz = a.x - p.x, a.z - p.z
        local distance = math.sqrt(dx*dx + dz*dz)
        local safe = refuge(p)
        step_wait = math.max(0, step_wait - dt)
        local mx, mz = a.x - previous_x, a.z - previous_z
        local moving = mx*mx + mz*mz > (0.1*dt)^2
        previous_x, previous_z = a.x, a.z
        if moving and not a.illuminated then
            step_distance += math.sqrt(mx*mx + mz*mz)
            if step_distance >= 0.55 and step_wait == 0 then
                step_distance = step_distance % 0.55
                if distance < 12 then
                    api.sound('visitor', true)
                    step_sounding = true
                end
                step_wait = 0.35
            end
        else
            step_distance = 0
            if step_sounding then
                api.sound('visitor', false)
                step_sounding = false
            end
        end
        if C.final and not sealed and count() == 3 then
            if not safe and a.player_visible and a.intensity >= 0.035 and distance < 8 then
                beam_hold += dt
            else beam_hold = 0 end
            if beam_hold >= 2 then
                sealed = true
                api.set_state('exit', 'beam_sealed', true)
                ready(api)
                api.sound('returned_cue', true)
                api.message('EXIT UNLOCKED', { 'You can leave now.', 'Go back to the foyer and use the exit door.' })
            end
        end
        if not a.illuminated and a.player_visible and distance < 1.05 and math.abs(a.y-p.y) < 0.6 and not safe then
            close_seconds += dt
            if close_seconds >= 1 then
                finished = true
                api.set_state('exit', 'dead', true)
                api.sound('store_hum', false)
                api.finish(false, 'YOU WERE CAUGHT', 'The shadow caught you. Press Enter to restart this chapter.')
                return
            end
        else close_seconds = 0 end
        nav_wait -= dt
        if nav_wait > 0 or a.illuminated then return end
        nav_wait = 0.8
        api.ai_task('visitor', 'beam_patrol', {
            key='beam_patrol', priority=80,
            definition={task='patrol',points=C.patrol,speed=C.speed+count()*0.0672,
                memory_seconds=C.memory,track_player=not safe,decision_seconds=0.8},
        })
    end,
}
