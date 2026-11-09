--!strict
return {
 greet=function(api,event)
  -- Cross-level bodies have a travel identity. The duck-filtered item volume
  -- owns confirmation; this greeting must not make a false source-ID claim.
  api.message("Feltling",{"Welcome back. Come through the opening.","Bring our duck in when you have it."})
 end,
 celebrate=function(api,event)
  for i=2,7 do api.set_state("editor_prop_"..tostring(i),"feltling_mode","cheer") end
  api.message("Feltlings",{"You brought our magic home!","You came back for us. Thank you."})
 end,
 eat=function(api,event)
  api.complete()
  api.finish(true,"The Long Way Home","For a moment, the hum becomes a familiar sound. You are home.")
 end,
}
