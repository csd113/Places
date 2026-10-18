--!strict
local stamped = false
local function hint(api)
    api.message('SNOW INTAKE', {
        if stamped then 'Supply logged. The north airlock is open.' else 'Ives needs the cardboard box on the ice.',
        if stamped then 'Open the north door and step into the airlock.' else 'Carry it to the pale square by the crate.',
        'E carries and sets down. Let it settle.',
        'F: beam. The noticeboard restarts this visit.',
        'Airlocks clear your hands and local systems.',
    })
end
return {
    start = function(api)
        assert(api.state("commission","stamp")==false)
        api.notice("Fresh local commissioning state verified")
        api.message('RELAY ANNEX', {
            'One last inspection before you can leave.',
            'Speak to Ives beside the intake crate.',
            'WASD: move. E: use/carry. F: beam. F1: hint.',
            'Clear intake, four floors, then discharge.',
        })
    end,
    talk = function(api)
        api.sound('relay_click',true)
        if stamped then
            api.message('IVES / INTAKE', {'Good. That supply is staying here.', 'Continue through the north airlock.', 'Your hands are cleared at the threshold.'})
        else
            api.message('IVES / INTAKE', {'Our last supply slid onto the ice.', 'Leave the box on the pale receiving square.', 'Wait for the bell. Take the north airlock.', 'Mind the ice. The marked well returns you.'})
        end
    end,
    returned = function(api)
        if stamped then return end
        stamped=true
        api.set_state('commission','stamp',true)
        api.door('airlock','unlock')
        api.sound('ack',true)
        api.notice('Intake accepted; tower airlock unlocked')
    end,
    depart = function(api)
        if stamped then api.change_level('relay_tower') end
    end,
    well = function(api)
        api.message('MAINTENANCE RECOVERY WELL', {'Step into the marked well for a safe reset.', 'This visit and carried supplies will reset.', 'This drill is optional.'})
    end,
    recover = function(api) api.reset() end,
    hint = hint,
}
