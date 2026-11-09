--!strict
-- The receipt remembers the witness event across ordinary level reentry.
return {
 start=function(api,event)
  api.set_state("editor_prop_75","stage_ready",true)
  if api.completed("long_way_home_danger") then
   api.set_state("editor_prop_75","visited",true)
  end
 end,
 mark_seen=function(api,event)
  api.complete()
  api.notice("First actual exterior elevator exit: witness encounter began.")
 end,
}
