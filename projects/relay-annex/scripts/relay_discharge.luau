--!strict
local returned=false
local function hint(api)
    api.message('SOL / DISCHARGE', {
        if returned then 'Duck ashore. Take the south exit.' else 'Leave the floating duck on the west square.',
        'Use the east slope to enter and leave.',
        'E carries and sets down. Let it settle.',
        'The east noticeboard restarts this visit.',
        'West noticeboard: return to snow intake.',
    })
end
return {
    start=function(api)
        assert(api.state("commission","stamp")==false)
        api.notice("Fresh local commissioning state verified")
        api.physics('float',true)
        api.message('DISCHARGE', {'Sol is waiting on the south walkway.', 'One last recovery, then your shift is over.', 'Use the east slope. F1 asks for help.'})
    end,
    talk=function(api)
        if returned then
            api.message('SOL / DISCHARGE', {'Perfect. Passenger dry. Basin clear.', 'Relief is here. Leave by the south door.', 'See you next shift.'})
        else
            api.message('SOL / DISCHARGE', {'Mira called. One passenger is still afloat.', 'Bring the duck to the pale west square.', 'Use the east slope. The basin is deep.', 'Set it down and listen for the receipt bell.'})
        end
    end,
    returned=function(api)
        if returned then return end
        returned=true;api.set_state('commission','stamp',true)
        api.door('airlock','unlock');api.spawn('signoff_point');api.sound('ack',true)
        api.notice('Discharge complete; south exit unlocked')
    end,
    depart=function(api)
        if returned then api.finish(true,'THE SHIFT IS OVER','Intake clear. Four floors signed. All ashore.') end
    end,
    recover=function(api) api.reset() end,
    restart=function(api) api.change_level('relay_intake') end,
    hint=hint,
}
