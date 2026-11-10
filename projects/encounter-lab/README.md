# Backrooms encounter lab

Open this folder in Places Editor. Build the entry map and play its compiled
package for full AI/ragdoll testing. The room has a triple-height 10.5 m ceiling.

Three buttons sit on the wall beside the player spawn: Start zombies, Clear
zombies, Toggle companion. Aim and press E. Start produces twenty zombies from
exactly two corner zones over roughly ten seconds. Clear removes living zombies
and all zombie bodies; toggle controls the robot separately. Death offers E to
respawn and reset the entire encounter. No waves or spectating are implemented.

Health is explicitly enabled only in this map. The default crowbar takes two
hits to kill each zombie. Hit protection is initially 0.35 seconds and can be
adjusted in map settings. Zone counts and independent cooldowns are editable in
the Spawn point Advanced > Object settings inspector. See the map authoring guide for the bounded
initial encounter source contract and editor Quick test limitations.
