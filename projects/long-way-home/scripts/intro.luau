--!strict
-- Event-only handlers avoid per-frame polling and preserve success semantics.
return {
 start=function(api,event)
  api.message("The Long Way Home", {"This place is not your world.","Beyond the hum, someone is waiting."})
 end,
 quest_done=function(api,event)
  api.set_state("editor_prop_1","feltling_route","to_elevator")
  api.set_state("editor_prop_1","feltling_mode","guide")
  api.complete()
 end,
}
