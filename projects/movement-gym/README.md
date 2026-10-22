# Movement Gym

Open this project in Places Editor, then use ordinary Build + Play. It is an
opt-in diagnostic map outside the shipped lineup and Cargo tests.

The floor extends to X=-8 so both mirrored strafes remain supported.
The west lane is clear for acceleration, stopping, reversal, diagonal movement,
air strafes and jump timing. East lanes contain the 16-riser stair flight
(X14-16/Z4-10), a 0.533 slope (X18-20/Z4-10), a steep 1.5 slope (X22-24/Z4-7),
0.44/0.46 m step thresholds at Z18, and a high-speed L corner near X28/Z32.
The west overhead slab at X4-8/Z30-34 has a 1.0 m underside for blocked unduck.
All records use existing editor geometry and existing PNG-backed materials.

Run `places-movement-check projects/movement-gym/levels/movement_gym.json
<new-output-directory>` to save 17 deterministic command trajectories. Each
JSONL row distinguishes feet, eye, hull top, velocity and grounded state.
They are engine trajectories, not measured retail Portal trajectories.
The consolidated 0.13.0 report owns research, acceptance and limitations.
