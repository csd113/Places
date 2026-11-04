# Temporary physical-shaft elevator lab

This small regression lab is retained for playtesting and excluded from
release-map preparation. The simpler place-and-link demonstration is in
[../elevator-links](../elevator-links).

Call the lift, walk fully inside, then aim at a destination and click or press E.
G and 1 move the cabin physically between two stacked halls in one map.
A loads the separate Annex, whose G button loads the original lobby again.
Doors reopen on arrival. Settled landings support checkpoints; saving during
travel retains the previous valid save and reports that the cabin is moving.

Open places.project.json in the native editor to inspect the mechanism.
Build > Elevator provides click placement and linking for normal authoring;
Advanced retains physical-shaft elevations, speed and detailed destinations.
See [authoring, direct launch and validation](../../docs/ELEVATORS.md).
