--!strict
-- Seven complete shuffled events. Internal steps are not separate events.
-- Persistent cast waits in opaque wings; insects/ghosts/ragdolls have bounded
-- runtime ownership and are removed only outside view or in complete darkness.
local clock, startAt, stageAt, step = 0, 2, 0, 0
local order, current, cursor, lastBeat = {}, 1, 1, -1
local doorStates = {}
local cast={"porter","guest","attendant","caretaker","arcade_player"}
local homes={porter={-5.5,1,"office_left_door"},guest={5.5,1,"office_right_door"},attendant={-5.5,-3.1,"pool_left_door"},caretaker={5.5,-5.5,"home_right_door"},arcade_player={-5.5,-7.85,"arcade_left_door"}}
local roomLights={"arcade_lamp","home_lamp","pool_lamp","office_lamp"}
local doors={"office_left_door","office_right_door","pool_left_door","pool_right_door","home_right_door","arcade_left_door","arcade_right_door"}
local ragNames={"office_left","office_right","pool_left","pool_right","home_right","arcade_left","arcade_right"}
local ragGoals={{-1.35,1.8},{1.75,1.8},{-1.65,-3.4},{1.65,-3.4},{.1,-5.45},{-.65,-8.3},{.65,-8.3}}
local escapeGoals={{-2.4,3.3},{-1.4,3.8},{-.5,3.1},{.4,3.6},{2.45,3.5},{2.1,3.9},{1.2,3.6},{-2.2,3.7}}
local function walk(api,id,x,z,speed) api.navigate(id,x,0,z,speed) end
local function near(api,id,x,z) local p=api.actor(id);return (p.x-x)^2+(p.z-z)^2<.12 end
local function hidden(api,id,limit) return math.abs(api.actor(id).x)>(limit or 4.6) end
local function door(api,id,on) if doorStates[id]~=on then api.door(id,on and "open" or "close");doorStates[id]=on end end
local function roomLight(api,index,on)
 api.light(roomLights[index],on)
 if index==1 then api.light("arcade_left",on);api.light("arcade_right",on) end
end
local function allLights(api,on) for i=1,4 do roomLight(api,i,on) end end
local function leave(api,id,speed) local h=homes[id];walk(api,id,h[1],h[2],speed) end
local function advance() step+=1;stageAt=clock;cursor=1 end
local function take(api,id) api.animate(id,"pick_up_floor",false);api.handoff(id) end
local function put(api,id) api.animate(id,"set_down_floor",false);api.release_item(id) end
local function noRuntime(api)
 if api.exists("closet_ghost") then return false end
 for i=0,4 do if api.exists("menu_balloon_"..tostring(i)) then return false end end
 for i=0,7 do if api.exists("roach_"..tostring(i)) then return false end end
 for _,name in ragNames do if api.exists("vanish_"..name) then return false end end
 return true
end
local function quiet(api)
 for _,id in cast do if not hidden(api,id) then return false end end
 return noRuntime(api)
end
local function parcel(api)
 local x,z=-.9,1.2;local age=clock-stageAt
 if step==0 then if api.actor("porter").carried_item==nil then take(api,"porter") end;advance()
 elseif step==1 and age>=3.5 then
  if api.actor("porter").carried_item==nil then return false end
  door(api,"office_left_door",true);walk(api,"porter",x,z-.5);advance()
 elseif step==2 and near(api,"porter",x,z-.5) then walk(api,"porter",x,z);advance()
 elseif step==3 and age>=1 and near(api,"porter",x,z) then put(api,"porter");advance()
 elseif step==4 and age>=3.3 then
  if api.actor("porter").carried_item~=nil then return false end
  leave(api,"porter");advance()
 elseif step==5 and hidden(api,"porter") then door(api,"office_left_door",false);advance()
 elseif step==6 and age>=1.5 then door(api,"office_right_door",true);walk(api,"guest",x,z-.5);advance()
 elseif step==7 and near(api,"guest",x,z-.5) then walk(api,"guest",x,z);advance()
 elseif step==8 and age>=1 and near(api,"guest",x,z) then take(api,"guest");advance()
 elseif step==9 and age>=3.5 then
  if api.actor("guest").carried_item==nil then return false end
  walk(api,"guest",x,1.95);advance()
 elseif step==10 and near(api,"guest",x,1.95) then walk(api,"guest",1.8,1.95);advance()
 elseif step==11 and near(api,"guest",1.8,1.95) then walk(api,"guest",2.4,1);advance()
 elseif step==12 and near(api,"guest",2.4,1) then leave(api,"guest");advance()
 elseif step==13 and hidden(api,"guest") then door(api,"office_right_door",false);api.release_item("guest");advance()
 elseif step==14 and api.actor("guest").carried_item==nil then api.physics("parcel",false);advance()
 end
 return step==15 and clock-stageAt>=1.5
end
local function duck(api)
 local age=clock-stageAt
 if step==0 then if api.actor("attendant").carried_item==nil then take(api,"attendant") end;advance()
 elseif step==1 and age>=3.5 then
  if api.actor("attendant").carried_item==nil then return false end
  door(api,"pool_left_door",true);walk(api,"attendant",-2.25,-3.7);advance()
 elseif step==2 and near(api,"attendant",-2.25,-3.7) then walk(api,"attendant",-2.25,-3.35);advance()
 elseif step==3 and age>=1 and near(api,"attendant",-2.25,-3.35) then
  api.set_state("attendant","face_pool",1);api.animate("attendant","pose_arms_forward",true);advance()
 elseif step==4 and age>=.8 then api.release_item("attendant");advance()
 elseif step==5 and age>=.5 then
  if api.actor("attendant").carried_item~=nil then return false end
  walk(api,"attendant",2.35,-3.7);advance()
 elseif step==6 and near(api,"attendant",2.35,-3.7) then walk(api,"attendant",2.35,-3.35);advance()
 elseif step==7 and age>=1 and near(api,"attendant",2.35,-3.35) then
  api.set_state("attendant","face_pool",2);advance()
 elseif step==8 and age>=1 then
  local d=api.actor("duck_0")
  if d.x<2.1 or d.z> -2.7 then return false end
  take(api,"attendant");advance()
 elseif step==9 and age>=3.5 then
  if api.actor("attendant").carried_item==nil then return false end
  door(api,"pool_right_door",true);walk(api,"attendant",5.5,-3.1);advance()
 elseif step==10 and hidden(api,"attendant") then door(api,"pool_left_door",false);door(api,"pool_right_door",false);advance()
 end
 return step==11 and clock-stageAt>=1.5
end
local chaseAt=0
local function ghost(api)
 local age=clock-stageAt
 if step==0 then
  api.spawn("closet_ghost_point","closet_ghost");door(api,"home_right_door",true);walk(api,"caretaker",-.75,-5.5);advance()
 elseif step==1 and near(api,"caretaker",-.75,-5.5) then api.toggle_animation("menu_closet","fold_open");advance()
 elseif step==2 and age>=.6 then
  api.animate("caretaker","pose_arms_up",true);advance()
 elseif step==3 and age>=1 then walk(api,"caretaker",-.35,-5.5,.35);advance()
 elseif step==4 and age>=1 and near(api,"caretaker",-.35,-5.5) then leave(api,"caretaker",2.8);walk(api,"closet_ghost",-1.05,-5.5,1.1);chaseAt=clock+.65;advance()
 elseif step==5 then
  if clock>=chaseAt then local p=api.actor("caretaker");walk(api,"closet_ghost",p.x,p.z,2.35);chaseAt=clock+.35 end
  if hidden(api,"caretaker") and hidden(api,"closet_ghost") then api.despawn("closet_ghost");advance() end
 elseif step==6 and not api.exists("closet_ghost") then
  api.toggle_animation("menu_closet","fold_open");door(api,"home_right_door",false);advance()
 end
 return step==7 and clock-stageAt>=1.5
end
local function arcade(api)
 local age=clock-stageAt
 if step==0 then door(api,"arcade_left_door",true);walk(api,"arcade_player",-.95,-8);advance()
 elseif step==1 and near(api,"arcade_player",-.95,-8) then walk(api,"arcade_player",-.95,-8.65);advance()
 elseif step==2 and age>=.8 and near(api,"arcade_player",-.95,-8.65) then api.animate("arcade_player","pose_arms_forward",true);api.light("arcade_left",false);advance()
 elseif step==3 and age>=.4 then api.light("arcade_left",true);advance()
 elseif step==4 and age>=3 then api.light("arcade_left",false);advance()
 elseif step==5 and age>=.25 then api.light("arcade_left",true);api.animate("arcade_player","pose_arms_up",true);advance()
 elseif step==6 and age>=1.6 then leave(api,"arcade_player");advance()
 elseif step==7 and hidden(api,"arcade_player") then door(api,"arcade_left_door",false);advance()
 end
 return step==8 and clock-stageAt>=1.5
end
local flicker, darkAt, restoreAt, revealAt = 0,0,0,0
local function roaches(api,t)
 local times={0,.24,.58,.92,1.27,1.58,1.95,2.32,2.65}
 if step==0 then
  if flicker<#times and t>=times[flicker+1] then
   flicker+=1;allLights(api,flicker%2==0)
   if flicker==#times then darkAt=clock;advance() end
  end
 elseif step==1 then
  for _=1,2 do
   if cursor<=8 then api.spawn("roach_"..tostring(cursor-1).."_point","roach_"..tostring(cursor-1));cursor+=1 end
  end
  if cursor>8 then advance() end
 elseif step==2 and clock-darkAt>=1 then
  for i=0,7 do if not api.exists("roach_"..tostring(i)) then return false end end
  restoreAt=clock;advance()
 elseif step==3 then
  if cursor<=4 and clock-restoreAt>=(cursor-1)*.45 then
   roomLight(api,cursor,true);cursor+=1
   if cursor==5 then revealAt=clock;advance() end
  end
 elseif step==4 and clock-revealAt>=.35 then
  for _=1,2 do
   if cursor<=8 then local q=escapeGoals[cursor];walk(api,"roach_"..tostring(cursor-1),q[1],q[2],3.5);cursor+=1 end
  end
  if cursor>8 then advance() end
 elseif step==5 then
  local removed=0;local alive=false
  for i=0,7 do
   local id="roach_"..tostring(i)
   if api.exists(id) then
    alive=true
    -- At z>2.9 the whole small mesh is below the fixed 55-degree camera,
    -- with floor support retained; no doorway or visible lifetime timeout.
    if api.actor(id).z>2.9 and removed<2 then api.despawn(id);removed+=1 end
   end
  end
  if not alive then advance() end
 end
 return step==6 and clock-stageAt>=1.2
end
local function balloons(api)
 if step==0 then
  for _=1,2 do if cursor<=5 then local id="menu_balloon_"..tostring(cursor-1);api.spawn(id.."_point",id);cursor+=1 end end
  if cursor>5 then door(api,"arcade_right_door",true);door(api,"arcade_left_door",true);advance() end
 elseif step==1 then
  for i=0,4 do if not api.exists("menu_balloon_"..tostring(i)) then return false end end
  if cursor<=5 and clock-stageAt>=.8+(cursor-1)*1.1 then
   walk(api,"menu_balloon_"..tostring(cursor-1),-5.9+.5*(cursor-1),-8.65,.32);cursor+=1
  end
  if cursor>5 then advance() end
 elseif step==2 then
  local alive=false;local removed=0
  for i=0,4 do
   local id="menu_balloon_"..tostring(i)
   if api.exists(id) then
    alive=true
    if api.actor(id).x< -3.75 and removed<2 then api.despawn(id);removed+=1 end
   end
  end
  if not alive then door(api,"arcade_left_door",false);door(api,"arcade_right_door",false);advance() end
 end
 return step==3 and clock-stageAt>=1.5
end
local function ragdolls(api)
 local age=clock-stageAt
 if step==0 then
  for _=1,2 do if cursor<=#doors then door(api,doors[cursor],true);cursor+=1 end end
  if cursor>#doors then advance() end
 elseif step==1 and age>=.8 then
  for _=1,2 do if cursor<=#ragNames then local id="vanish_"..ragNames[cursor];api.spawn(id.."_point",id);cursor+=1 end end
  if cursor>#ragNames then advance() end
 elseif step==2 then
  for _,name in ragNames do if not api.exists("vanish_"..name) then return false end end
  for _=1,2 do if cursor<=#ragNames then local q=ragGoals[cursor];walk(api,"vanish_"..ragNames[cursor],q[1],q[2]);cursor+=1 end end
  if cursor>#ragNames then advance() end
 elseif step==3 then
  for i,name in ragNames do local q=ragGoals[i];if not near(api,"vanish_"..name,q[1],q[2]) then return false end end
  advance()
 elseif step==4 then
  for _=1,2 do
   if cursor<=#ragNames then local q=ragGoals[cursor];api.ragdoll("vanish_"..ragNames[cursor],true,q[1]<0 and -.9 or .9,0,.2);cursor+=1 end
  end
  if cursor>#ragNames then advance() end
 elseif step==5 and age>=1.2 then flicker=0;advance()
 elseif step==6 then
  local times={0,.28,.65,1.05,1.35}
  if flicker<#times and age>=times[flicker+1] then
   flicker+=1;allLights(api,flicker%2==0)
   if flicker==#times then darkAt=clock;advance() end
  end
 elseif step==7 and clock-darkAt>=.2 then
  for _=1,2 do if cursor<=#ragNames then api.despawn("vanish_"..ragNames[cursor]);cursor+=1 end end
  if cursor>#ragNames then advance() end
 elseif step==8 and clock-darkAt>=1 then
  for _,name in ragNames do if api.exists("vanish_"..name) then return false end end
  flicker=0;advance()
 elseif step==9 then
  local times={0,.22,.58,1.0,1.4}
  if flicker<#times and age>=times[flicker+1] then
   flicker+=1;allLights(api,flicker%2==1)
   if flicker==#times then advance() end
  end
 elseif step==10 then
  for _=1,2 do if cursor<=#doors then door(api,doors[cursor],false);cursor+=1 end end
  if cursor>#doors then advance() end
 end
 return step==11 and clock-stageAt>=1.2
end
local vignettes={parcel,duck,ghost,arcade,roaches,balloons,ragdolls}
return {
 start=function(api)
  math.randomseed(api.state("guest","menu_seed"))
  for i=1,#vignettes do order[i]=i end
  for i=#order,2,-1 do local j=math.random(1,i);order[i],order[j]=order[j],order[i] end
  local forced=api.state("guest","menu_first")
  if forced>0 then for i,id in order do if id==forced then order[1],order[i]=order[i],order[1];break end end
  elseif order[1]==api.state("guest","previous_vignette") then order[1],order[2]=order[2],order[1] end
 end,
 update=function(api,event)
  clock+=event.dt
  if clock<startAt then return end
  if current>#order then if quiet(api) then allLights(api,true);api.reset() end;return end
  local id=order[current];local beat=id*100+step
  if beat~=lastBeat then api.set_state("guest","vignette",id);api.set_state("guest","beat",beat);lastBeat=beat;return end
  if vignettes[id](api,clock-startAt) and quiet(api) then
   allLights(api,true);current+=1;startAt=clock+1;stageAt=startAt;step=0;cursor=1;flicker=0
  end
 end,
}
