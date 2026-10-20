--!strict
local stamped = false
local function hint(api)
    api.message('SNOW INTAKE', {
        if stamped then 'Supply logged. The north airlock is open.' else 'Ives needs the cardboard box on the ice.',
        if stamped then 'Open the north door and step into the airlock.' else 'Carry it to the pale square by the crate.',
        'E carries and sets down. Let it settle.',
        'Lost box? Use the sign beside the well.',
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
            api.message('IVES / INTAKE', {'Our last supply slid onto the ice.', 'Leave the box on the pale receiving square.', 'Wait for the bell. Take the north airlock.', 'Mind the ice. The well sign recovers lost supplies.'})
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
        if stamped then
            api.message('SUPPLY RECOVERY', {'Intake is already accepted.', 'Continue through the north airlock.'})
            return
        end
        api.physics('essential_supply',false)
        api.message('SUPPLY RECOVERY', {
            'The box returns to its starting place on the ice.',
            'If that place is occupied, clear it and use this sign again.',
            'Your progress and other supplies stay as they are.',
            'Falling into the well still restarts this visit.',
        })
    end,
    recover = function(api) api.reset() end,
    hint = hint,
}
