--!strict
-- It Only Exists in the Beam. Map-local progression and pacing; the engine
-- supplies real renderer light observations, navigation, physics and outcomes.
local returned = { red = false, green = false, stars = false }
local elapsed = 0
local nav_wait = 0
local close_seconds = 0
local lost_seconds = 99
local last_seen = { x = 3, y = 0, z = 5.6 }
local patrol = { {3,0,5.6}, {10,0,6}, {8,0,11.8}, {12.8,3,4.3}, {9,3,1.5}, {5.3,0,11.5} }
local post = 1
local finished = false
local step_wait = 0
local previous_x, previous_z = 3, 5.6
local function count()
    return (if returned.red then 1 else 0) + (if returned.green then 1 else 0) + (if returned.stars then 1 else 0)
end
local function hint(api)
    local n = count()
    local lines = { tostring(n) .. ' of 3 gifts on the exit counter.' }
    if n == 3 then
        table.insert(lines, 'All three are safe. Use the exit door to leave.')
    elseif not returned.red then
        table.insert(lines, 'Red gift: the desk in the left ground-floor aisle.')
    elseif not returned.green then
        table.insert(lines, 'Green gift: UPSTAIRS office, via the east ramp.')
    else
        table.insert(lines, 'Star gift: the low table in the ground-floor lounge.')
    end
    if n < 3 then
        table.insert(lines, 'E picks up. Carry it to the counter at the exit.')
        table.insert(lines, 'Stand close; hold it above the counter, then E.')
        table.insert(lines, 'Lost a gift? Use the EXIT sign above the door to reset.')
    end
    table.insert(lines, 'F reveals and holds it. The lit entrance is a refuge.')
    api.message('THE NEXT STEP', lines)
end
local function mark(api, event, present)
    if finished then return end
    local key = ({ return_red = 'red', return_green = 'green', return_stars = 'stars' })[event.subject]
    if key == nil or returned[key] == present then return end
    returned[key] = present
    local n = count()
    api.set_state('exit', key .. '_returned', present)
    api.set_state('exit', 'count', n)
    api.set_state('exit', 'ready', n == 3)
    api.door('exit', if n == 3 then 'unlock' else 'lock')
    if present then api.sound('returned_cue', true) end
    api.notice(tostring(n) .. '/3 gifts returned')
end
return {
    start = function(api)
        api.message('IT ONLY EXISTS IN THE BEAM', {
            'Find three gifts: two below, one upstairs.',
            'Bring them to the counter at the lit exit.',
            'E: pick up, set down, open doors. F: flashlight.',
            'Light reveals and holds it. Darkness lets it move.',
            'Return all three, then use the exit door.',
            'F1: ask for a hint. The entrance is a refuge.',
        })
    end,
    hint = function(api) if not finished then hint(api) end end,
    returned = function(api, event) mark(api, event, true) end,
    removed = function(api, event) mark(api, event, false) end,
    exit = function(api)
        if finished then return end
        if count() ~= 3 then hint(api); return end
        finished = true
        api.set_state('exit', 'won', true)
        api.sound('store_hum', false)
        api.sound('exit_cue', true)
        api.finish(true, 'YOU BROUGHT THEM HOME', 'All three gifts. Time to leave.')
    end,
    restart = function(api)
        api.physics('beam_gifts', false)
        api.reset()
    end,
    update = function(api, event)
        if finished then return end
        local dt = event.dt
        elapsed += dt
        if elapsed < 8 then return end
        local p = api.player()
        local a = api.actor('visitor')
        local dx, dz = a.x - p.x, a.z - p.z
        local distance = math.sqrt(dx*dx + dz*dz)
        step_wait = math.max(0, step_wait - dt)
        local moved_x, moved_z = a.x - previous_x, a.z - previous_z
        local moving = moved_x*moved_x + moved_z*moved_z > (0.1*dt)^2
        previous_x, previous_z = a.x, a.z
        if moving and not a.illuminated and a.player_visible and distance < 3.5 and step_wait == 0 then
            api.sound('visitor', true)
            step_wait = 0.9
        end
        if a.player_visible then last_seen = p; lost_seconds = 0 else lost_seconds += dt end
        if not a.illuminated and a.player_visible and distance < 1.05
            and math.abs(a.y-p.y) < 0.6 and p.z < 12.7 then
            close_seconds += dt
            if close_seconds >= 1.0 then
                finished = true
                api.set_state('exit', 'dead', true)
                api.sound('store_hum', false)
                api.finish(false, 'YOU WERE FOUND', 'Keep it in the light. Try again.')
                return
            end
        else close_seconds = 0 end
        nav_wait -= dt
        if nav_wait > 0 or a.illuminated then return end
        nav_wait = 0.8
        if p.z < 12.7 and lost_seconds < 6 then
            api.navigate('visitor', last_seen.x, last_seen.y, last_seen.z)
        else
            local goal = patrol[post]
            local gx, gy, gz = goal[1], goal[2], goal[3]
            if math.abs(a.x-gx) + math.abs(a.y-gy) + math.abs(a.z-gz) < 0.8 then
                post = post % #patrol + 1
                goal = patrol[post]
            end
            api.navigate('visitor', goal[1], goal[2], goal[3])
        end
    end,
}
