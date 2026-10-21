--!strict
-- Ten-minute miniature: 120 authored beats at five-second intervals.
-- No spawns, subscriptions or growing tables. A fresh cycle uses the normal whole-world reset.
local clock = 0
local nextBeat = 1
local pendingActor = nil
local pendingKind = nil
local pendingAt = 0
local function walk(api, id, x, z) api.navigate(id, x, 0, z) end
local function pose(api, id, clip) api.animate(id, clip, false) end
local function hold(api, id) api.handoff(id) end
local function duck(api, i) api.physics("duck_" .. tostring(i), false); api.physics("duck_" .. tostring(i), true) end
local function put(api, actor)
    api.animate(actor,"set_down_floor",false)
    pendingActor, pendingKind, pendingAt = actor, "release", clock + 0.9
end
local function take(api, actor)
    api.animate(actor,"pick_up_floor",false)
    pendingActor, pendingKind, pendingAt = actor, "take", clock + 0.9
end
local beats = {
    -- 001 / 000s: The late parcel: porter bends for the delivery
    function(api) pose(api,"porter","pick_up_floor") end,
    -- 002 / 005s: Parcel takes the posed right hand
    function(api) hold(api,"porter");walk(api,"porter",-.45,.65) end,
    -- 003 / 010s: Reader looks up from the office
    function(api) pose(api,"reader","pose_arms_forward") end,
    -- 004 / 015s: Porter presents the parcel to the reader
    function(api) walk(api,"porter",.15,.65);pose(api,"reader","pose_arms_up") end,
    -- 005 / 020s: Reader steps aside to let the delivery through
    function(api) walk(api,"reader",1.1,1.7) end,
    -- 006 / 025s: Porter crosses the Office foreground
    function(api) walk(api,"porter",1.25,.65) end,
    -- 007 / 030s: The first duck joins the Pool current
    function(api) duck(api,0) end,
    -- 008 / 035s: Attendant follows its progress from the dry deck
    function(api) walk(api,"attendant",.45,-3.5) end,
    -- 009 / 040s: Porter sets the parcel on the Office floor
    function(api) put(api,"porter") end,
    -- 010 / 045s: Delivery porter returns along the opposite edge
    function(api) walk(api,"porter",-1.35,.65) end,
    -- 011 / 050s: The borrowed book: reader reaches down
    function(api) pose(api,"reader","pick_up_floor") end,
    -- 012 / 055s: Reader collects the book stack
    function(api) hold(api,"reader") end,
    -- 013 / 060s: Caretaker notices someone across the water
    function(api) pose(api,"caretaker","pose_arms_up") end,
    -- 014 / 065s: Reader approaches the Pool threshold with the books
    function(api) walk(api,"reader",.65,1.7) end,
    -- 015 / 070s: Caretaker comes to the Home threshold
    function(api) walk(api,"caretaker",-.4,-4.55) end,
    -- 016 / 075s: Reader holds up the books for inspection
    function(api) pose(api,"reader","pose_arms_forward") end,
    -- 017 / 080s: Guest joins the caretaker to look
    function(api) walk(api,"guest",.55,-4.7) end,
    -- 018 / 085s: Reader puts the book stack down while two ducks pass
    function(api) put(api,"reader");duck(api,1);duck(api,2) end,
    -- 019 / 090s: Guest acknowledges the reader collecting the books
    function(api) pose(api,"guest","pose_arms_forward");take(api,"reader") end,
    -- 020 / 095s: Reader carries the books back to the Office
    function(api) walk(api,"reader",1.55,1.7) end,
    -- 021 / 100s: Pool shift begins: attendant bends for the towel
    function(api) pose(api,"attendant","pick_up_floor") end,
    -- 022 / 105s: Attendant gathers the folded towel
    function(api) hold(api,"attendant") end,
    -- 023 / 110s: Towel crosses the rear deck while porter returns for the parcel
    function(api) walk(api,"attendant",-1.4,-3.5);walk(api,"porter",1.25,.65) end,
    -- 024 / 115s: Guest yields as the porter stoops for the delivery
    function(api) walk(api,"guest",.9,-6.5);take(api,"porter") end,
    -- 025 / 120s: Caretaker waves the attendant over
    function(api) pose(api,"caretaker","pose_arms_forward") end,
    -- 026 / 125s: Attendant brings the towel to the Home side
    function(api) walk(api,"attendant",-.4,-3.5) end,
    -- 027 / 130s: Guest waves across the warm waiting room
    function(api) pose(api,"guest","pose_arms_up") end,
    -- 028 / 135s: A single duck drifts beneath the towel
    function(api) duck(api,2) end,
    -- 029 / 140s: Attendant places the blue towel on the dry deck
    function(api) put(api,"attendant") end,
    -- 030 / 145s: Caretaker makes room while the towel is collected again
    function(api) walk(api,"caretaker",-.35,-5.2);take(api,"attendant") end,
    -- 031 / 150s: A quiet favour: caretaker reaches for a cushion
    function(api) pose(api,"caretaker","pick_up_floor") end,
    -- 032 / 155s: Cushion follows the animated hand
    function(api) hold(api,"caretaker") end,
    -- 033 / 160s: Guest approaches to inspect the checked fabric
    function(api) walk(api,"guest",.6,-6.4) end,
    -- 034 / 165s: Caretaker puts the checked cushion beside the Home chair
    function(api) walk(api,"caretaker",.65,-5.6) end,
    -- 035 / 170s: The cushion is set down where the guest can inspect it
    function(api) put(api,"caretaker");pose(api,"guest","pose_arms_forward") end,
    -- 036 / 175s: Closet folds open on its four real leaf colliders
    function(api) api.toggle_animation("menu_closet","fold_open") end,
    -- 037 / 180s: Guest examines the open closet while the cushion waits
    function(api) walk(api,"guest",-.3,-6.3) end,
    -- 038 / 185s: Caretaker picks the cushion back up beside the chair
    function(api) take(api,"caretaker") end,
    -- 039 / 190s: Closet folds closed while the caretaker waits clear
    function(api) api.toggle_animation("menu_closet","fold_open") end,
    -- 040 / 195s: Caretaker brings the cushion back to the warm threshold
    function(api) walk(api,"caretaker",-.45,-4.65) end,
    -- 041 / 200s: Arcade prize: player notices a gift on the floor
    function(api) pose(api,"player","pick_up_floor") end,
    -- 042 / 205s: The little gift is picked up in the Arcade
    function(api) hold(api,"player") end,
    -- 043 / 210s: Player walks left between the two cabinets
    function(api) walk(api,"player",-.8,-8.45) end,
    -- 044 / 215s: Gift is placed between the Arcade cabinets as guest arrives
    function(api) put(api,"player");walk(api,"guest",.5,-6.35) end,
    -- 045 / 220s: Arcade player retrieves the prize from the cabinet aisle
    function(api) take(api,"player") end,
    -- 046 / 225s: Guest congratulates the gift-bearing player at center
    function(api) pose(api,"guest","pose_arms_up");walk(api,"player",0,-8.15) end,
    -- 047 / 230s: Office reader joins the distant celebration
    function(api) walk(api,"reader",.5,1.7) end,
    -- 048 / 235s: Caretaker carries the cushion clear of the congratulating guest
    function(api) walk(api,"caretaker",-1.1,-5.1) end,
    -- 049 / 240s: Player takes the gift to the right cabinet
    function(api) walk(api,"player",.9,-8.6) end,
    -- 050 / 245s: Guest returns from the Arcade threshold
    function(api) walk(api,"guest",.65,-5.05) end,
    -- 051 / 250s: Duck parade: attendant prepares at the left deck end
    function(api) walk(api,"attendant",-1.45,-3.5) end,
    -- 052 / 255s: First duck enters ahead of the others
    function(api) duck(api,0) end,
    -- 053 / 260s: Porter stops to watch with the parcel
    function(api) walk(api,"porter",-.5,.65) end,
    -- 054 / 265s: Reader puts books down to watch the second water lane
    function(api) put(api,"reader");duck(api,1) end,
    -- 055 / 270s: Reader collects the stack beside the waiting parcel porter
    function(api) take(api,"reader") end,
    -- 056 / 275s: Third duck completes the little flotilla
    function(api) duck(api,2) end,
    -- 057 / 280s: Guest comes to the rear of the Pool opening
    function(api) walk(api,"guest",.4,-4.5) end,
    -- 058 / 285s: Attendant walks alongside the current
    function(api) walk(api,"attendant",1.45,-3.5) end,
    -- 059 / 290s: Porter takes the parcel past the watching reader
    function(api) walk(api,"porter",1.35,.65) end,
    -- 060 / 295s: Reader leaves the water view for the book desk
    function(api) walk(api,"reader",1.5,1.7) end,
    -- 061 / 300s: A missed greeting: guest raises a hand at the porter
    function(api) pose(api,"guest","pose_arms_up") end,
    -- 062 / 305s: Porter has already crossed out toward the Office side
    function(api) walk(api,"porter",-1.45,.65) end,
    -- 063 / 310s: Reader steps forward to acknowledge the guest
    function(api) walk(api,"reader",.65,1.7) end,
    -- 064 / 315s: Guest comes closer with a second greeting
    function(api) walk(api,"guest",.45,-4.55) end,
    -- 065 / 320s: The attendant sets the blue towel down during the greeting
    function(api) put(api,"attendant") end,
    -- 066 / 325s: Reader waits to one side of the opening
    function(api) walk(api,"reader",1.15,1.7) end,
    -- 067 / 330s: Guest points after the departing attendant
    function(api) pose(api,"guest","pose_arms_forward") end,
    -- 068 / 335s: Attendant doubles back to complete the greeting
    function(api) walk(api,"attendant",.6,-3.5) end,
    -- 069 / 340s: A duck interrupts the reunion while the towel is collected
    function(api) duck(api,1);take(api,"attendant") end,
    -- 070 / 345s: Guest relaxes and returns to the Home chair
    function(api) pose(api,"guest","pose_stand");walk(api,"guest",.85,-6.35) end,
    -- 071 / 350s: Office crossing: porter approaches with the parcel
    function(api) walk(api,"porter",-.35,.65) end,
    -- 072 / 355s: Reader yields toward the near edge with the books
    function(api) walk(api,"reader",.65,1.7) end,
    -- 073 / 360s: Porter passes through the center of the Office
    function(api) walk(api,"porter",.9,.65) end,
    -- 074 / 365s: Porter puts the parcel down as the reader passes with books
    function(api) put(api,"porter");walk(api,"reader",-.9,1.7) end,
    -- 075 / 370s: Porter pauses at the delivery desk
    function(api) walk(api,"porter",1.35,.65) end,
    -- 076 / 375s: Reader turns back along the Pool threshold
    function(api) walk(api,"reader",-.65,1.7) end,
    -- 077 / 380s: Guest watches the porter stoop to collect the delivery
    function(api) walk(api,"guest",.45,-4.55);take(api,"porter") end,
    -- 078 / 385s: Porter leaves the desk by the far lane
    function(api) walk(api,"porter",-.9,.65) end,
    -- 079 / 390s: Reader completes a dry-floor loop with the books
    function(api) walk(api,"reader",.9,1.7) end,
    -- 080 / 395s: Guest answers with a small open-handed sign
    function(api) pose(api,"guest","pose_arms_forward") end,
    -- 081 / 400s: Closet inspection: caretaker waits outside the sweep
    function(api) walk(api,"caretaker",-.35,-5.65) end,
    -- 082 / 405s: Guest opens the four-leaf closet from clear ground
    function(api) api.toggle_animation("menu_closet","fold_open") end,
    -- 083 / 410s: Guest steps back to look inside
    function(api) walk(api,"guest",.95,-6.4) end,
    -- 084 / 415s: Caretaker sets the cushion down for the closet inspection
    function(api) put(api,"caretaker") end,
    -- 085 / 420s: Attendant arrives with the contrasting blue towel
    function(api) walk(api,"attendant",-.45,-3.5) end,
    -- 086 / 425s: Guest points out the towel while caretaker retrieves the cushion
    function(api) pose(api,"guest","pose_arms_forward");take(api,"caretaker") end,
    -- 087 / 430s: Caretaker carries the cushion across Home
    function(api) walk(api,"caretaker",.65,-5.6) end,
    -- 088 / 435s: Closet closes after the inspection
    function(api) api.toggle_animation("menu_closet","fold_open") end,
    -- 089 / 440s: Guest gives the caretaker space beside the chair
    function(api) walk(api,"guest",-.5,-5.25) end,
    -- 090 / 445s: The blue-towel attendant returns to the Pool end
    function(api) walk(api,"attendant",1.5,-3.5) end,
    -- 091 / 450s: Evening Arcade: player carries the prize to center
    function(api) walk(api,"player",0,-8.2) end,
    -- 092 / 455s: Guest comes to watch the cabinet player
    function(api) walk(api,"guest",.55,-6.4) end,
    -- 093 / 460s: Left Arcade lamp pauses while the cabinets remain visible
    function(api) api.light("arcade_left_lamp",false) end,
    -- 094 / 465s: Arcade player places the gift at the left cabinet
    function(api) walk(api,"player",-.8,-8.6) end,
    -- 095 / 470s: Guest points to the other cabinet as the gift is put down
    function(api) pose(api,"guest","pose_arms_forward");put(api,"player") end,
    -- 096 / 475s: Player collects the prize before crossing the Arcade
    function(api) take(api,"player") end,
    -- 097 / 480s: Violet practical returns as the gift crosses to the right
    function(api) api.light("arcade_left_lamp",true);walk(api,"player",.8,-8.6) end,
    -- 098 / 485s: Caretaker joins the guest carrying the cushion
    function(api) walk(api,"caretaker",-.5,-6.35) end,
    -- 099 / 490s: Player brings the prize back to the two visitors
    function(api) walk(api,"player",0,-8.15) end,
    -- 100 / 495s: Guest celebrates a second, different arrival
    function(api) pose(api,"guest","pose_arms_up") end,
    -- 101 / 500s: Closing round: porter takes the parcel past the doorway
    function(api) walk(api,"porter",1.25,.65) end,
    -- 102 / 505s: Reader moves the books to the opposite Office lane
    function(api) walk(api,"reader",-1.2,1.7) end,
    -- 103 / 510s: Attendant places the towel down before the closing duck round
    function(api) put(api,"attendant") end,
    -- 104 / 515s: Last single duck enters while attendant collects the towel
    function(api) duck(api,0);take(api,"attendant") end,
    -- 105 / 520s: Guest returns from the Arcade to the warm room
    function(api) walk(api,"guest",.7,-5.05) end,
    -- 106 / 525s: Caretaker follows with the checked cushion
    function(api) walk(api,"caretaker",-.7,-5.05) end,
    -- 107 / 530s: Porter passes behind the reader on the return walk
    function(api) walk(api,"porter",-1.2,.65) end,
    -- 108 / 535s: Reader carries the books toward the right desk
    function(api) walk(api,"reader",1.1,1.7) end,
    -- 109 / 540s: Attendant walks with the duck to the far end
    function(api) walk(api,"attendant",1.45,-3.5) end,
    -- 110 / 545s: Arcade player leaves the central opening clear
    function(api) walk(api,"player",.85,-8.6) end,
    -- 111 / 550s: A last conversation: Home guest greets the caretaker
    function(api) pose(api,"guest","pose_arms_forward") end,
    -- 112 / 555s: Caretaker shows the guest the cushion on the floor
    function(api) put(api,"caretaker") end,
    -- 113 / 560s: Office reader pauses with the books at the threshold
    function(api) walk(api,"reader",.6,1.7) end,
    -- 114 / 565s: The parcel is put down in the opposite Office foreground
    function(api) put(api,"porter") end,
    -- 115 / 570s: Two final ducks pass while porter collects the parcel
    function(api) duck(api,1);duck(api,2);take(api,"porter") end,
    -- 116 / 575s: Attendant carries the blue towel back along the deck
    function(api) walk(api,"attendant",.2,-3.5) end,
    -- 117 / 580s: Arcade player leaves the gift beside the distant cabinet
    function(api) put(api,"player") end,
    -- 118 / 585s: Guest raises a quiet farewell across all four rooms
    function(api) pose(api,"guest","pose_arms_up") end,
    -- 119 / 590s: Caretaker collects the cushion while porter clears the sightline
    function(api) take(api,"caretaker");walk(api,"porter",-1.2,.65) end,
    -- 120 / 595s: Reader and guest settle before the next ten-minute visit
    function(api) walk(api,"porter",-2.3,.65);walk(api,"reader",1.1,.75);walk(api,"attendant",1.4,-3.5);walk(api,"caretaker",-.65,-5.4);walk(api,"guest",.85,-6.7);walk(api,"player",0,-8.45) end,
}
return {
    update = function(api, event)
        clock += event.dt
        if pendingActor ~= nil and clock >= pendingAt then
            if pendingKind == "release" then api.release_item(pendingActor) else api.handoff(pendingActor) end
            pendingActor, pendingKind = nil, nil
        end
        if clock >= 600 then api.reset(); return end
        if nextBeat <= #beats and clock >= (nextBeat - 1) * 5 then
            beats[nextBeat](api)
            api.set_state("guest", "beat", nextBeat)
            nextBeat += 1
        end
    end,
}
