--!strict
local requested = false
return {
 cup = function(api, event) api.become("home:mug") end,
 chair = function(api, event) api.become("core:chair", 2.4) end,
 cabinet = function(api, event) api.become("core:cabinet", 2.0) end,
 power = function(api, event)
  if api.player().form == "core:chair" then
   api.set_state("power_switch", "powered", true)
   api.door("records_door", "unlock")
   api.door("records_door", "open")
   api.light("exit_lamp", true)
  end
 end,
 pressure = function(api, event)
  local p = api.player()
  if api.state("power_switch", "powered") and p.form == "core:cabinet"
     and p.x > 15.5 and p.x < 17.15 and p.z > 2.2 and p.z < 4.8 then
   api.door("exit_door", "open")
   requested = true
  elseif requested then
   api.door("exit_door", "close")
   requested = false
  end
 end,
 finish = function(api, event)
  if api.state("power_switch", "powered") then
   api.finish(true, "The Shape of an Exit", "You reached the exit.")
  end
 end,
 return_door = function(api, event)
  if api.player().x > 18.3 and api.state("power_switch", "powered") then
   api.door("exit_door", "open")
  end
 end,
 reset = function(api, event) api.physics("office", false) api.reset() end,
}
