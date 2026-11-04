# Temporary place-and-link elevator demo

This prototype is kept for playtesting and is excluded from release-map builds.
Ground and logical Floor 1 are separate halls **on the same world-height plane**,
10 metres apart. Their cabins are linked both ways; the label does not encode
world height. Annex is a separate level. All three cabins have two destination
buttons, excluding the current stop. G / 1 / A identify the destinations.

Call the ground cabin, walk inside, then aim at a button and click or press E.
1 travels inside this map; G returns. A closes the doors and loads the Annex
over the frozen game view. The Annex offers both G and 1 as return destinations.
The NPC and pickup box inside Ground travel with you on either route and return
without duplicate source instances. Try moving the box or holding it before
selecting a destination. Keep each passenger completely inside the cabin.
The separate green arrow button below G/1/A toggles the doors at the current
stop without travelling. It also reopens/cancels a trip before departure;
moving/loading cabins stay sealed and doorway obstructions safely reopen them.

Open places.project.json in the native editor. Build > Elevator starts click
placement. Place two cabins and Link them; no coordinates or JSON are required.
R rotates before placing. Names are optional. Load another level offers the
project map and its saved arrival cabin. Three stops / two destinations per cabin
is the current limit; Advanced preserves physical-shaft configuration.

See [authoring and direct launch](../../docs/ELEVATORS.md). The original
[elevator lab](../elevator-lab) remains the explicit physical-shaft regression.
