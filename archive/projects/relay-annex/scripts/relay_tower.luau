--!strict
local ball=false
local circuit=false
local seen=false
local beam_time=0
local floor=0
local visits={false,false,false,false}
local function hint(api)
    local next = if not ball then 'Floor 2: toss the ball onto the pale target.' elseif not circuit then 'Floor 3: restore power at the wall switch.' elseif not seen then 'Floor 4: sweep your beam on the north wall.' else 'All signed. Take the north door on floor 4.'
    api.message('MIRA / SERVICE TOWER', {
        next,
        'East stairs reach floor 2 at the south end.',
        'Middle ramp: north. Far ramp: south.',
        'Left click throws a held item. E sets it down.',
        'F1: hint. Ground noticeboard: restart visit.',
    })
end
local function check(api)
    if ball and circuit and seen then
        api.set_state('commission','stamp',true)
        api.door('airlock','unlock')
    end
end
return {
    start=function(api)
        assert(api.state("commission","stamp")==false)
        api.notice("Fresh local commissioning state verified")
        api.message('THE FOUR FLOORS', {
            'Find Mira on the ground floor.',
            'Climb all four floors to reach discharge.',
            'Airlock check: hands and systems are fresh.',
        })
    end,
    talk=function(api)
        api.message('MIRA / GROUND FLOOR', {'Prove the tower so Sol can sign us out.', 'Floor 2: toss the ball onto the pale target.', 'Floor 3: restore power at the wall switch.', 'Floor 4: find our invisible witness by beam.', 'Take the east stairs. Follow the landings.'})
    end,
    upper=function(api)
        if seen and ball and circuit then
            api.message('MIRA / OBSERVATION', {'I saw it too. Even unseen, it casts a shadow.', 'All signed. The north airlock leads to Sol.', 'Thank you for taking all four floors on foot.'})
        else hint(api) end
    end,
    ball=function(api)
        if ball then return end
        ball=true;api.sound('ack',true);api.notice('Cobalt return accepted');check(api)
    end,
    circuit=function(api)
        circuit=true;api.light('observation_circuit',true);api.sound('relay_click',true);api.notice('Observation circuit restored');check(api)
    end,
    update=function(api,event)
        if seen or not circuit then return end
        local player=api.player()
        if player.y<9 then return end
        local witness=api.actor('witness')
        if witness.illuminated and witness.intensity>.015 then beam_time+=event.dt else beam_time=0 end
        if beam_time>.35 then
            seen=true;api.sound('ack',true);api.notice('Witness located by its flashlight shadow');check(api)
        end
    end,
    gate=function(api)
        if ball and circuit and seen then api.door('airlock','unlock');api.door('airlock','toggle') else hint(api) end
    end,
    depart=function(api) if ball and circuit and seen then api.change_level('relay_discharge') end end,
    floor0=function(api) visits[1]=true;floor=0;api.notice('Ground floor') end,
    floor1=function(api) visits[2]=true;floor=1;api.notice('Floor 2 / impact return') end,
    floor2=function(api) visits[3]=true;floor=2;api.notice('Floor 3 / power') end,
    floor3=function(api) visits[4]=true;floor=3;api.notice('Floor 4 / observation') end,
    recover=function(api) api.reset() end,
    hint=hint,
}
