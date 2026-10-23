# One Guest Too Many

A complete single-player birthday room and arcade, authored with the native
Places editor using installed Arcade/Home assets and the shared mannequin.

Open `projects/one-guest-too-many` in `places-editor`, then Save+Build or
Build+Play. Export the complete project as a `.placesmap` for ordinary game
import or editable source recovery. No external images, models or scripts are
required. The project is separate from the eight-favor environment collection.

## Runtime contract (spoilers)

- The initial party entry admits the first guest. Visit the far arcade return
  zone, then enter the party again for each next arrival. Six guests is the cap.
  Crossing a threshold repeatedly without completing the return route adds none.
- The third arrival is the follower on every deterministic replay. Its appearance
  is identical to the harmless Original mannequins; the clue is its movement
  around the chairs and following through doorways. The other five stay put.
- Luau samples real collision visibility within 12 m at 10 Hz, with a 360-degree
  awareness field and no hearing. Occlusion starts a four-second search of the
  last observed position, followed by a visible stop; a new sighting resumes
  pursuit. Shared baked navigation and swept collision handle every move.
- The waiting-room door opens with its handle. The visible outside switch closes
  and locks it only after the actual leaf settles unobstructed and the player
  is safely outside. At the spare chair the follower surveys the room for 2.5 s
  after the player leaves, providing a visible, bounded opportunity to use the
  latch. It resumes pursuit if the room remains open.
- Containment requires the full conservative body envelope inside the room, the
  real closed/locked leaf, and the player outside. A request, obstructed leaf,
  empty room, or harmless guest cannot substitute for that proof. Reopen with
  the outside latch; the inside handle also releases a locked door for a player.
- Once all six guests have arrived and the follower remains contained, use the
  birthday cake. Premature use only flickers the party light. There are no hint,
  tutorial or dialogue popups. The completion screen supports Enter replay and
  Escape to menu. The entrance-hall wall switch resets at any time.
- Runs are volatile: restart, menu reentry and application reload recreate the
  guests, follower identity, navigation memory, scripts, doors, locks, switches
  and progression. No campaign completion or in-progress mode save is written.

The physical navigation body is radius 0.4 m and height 1.9 m. Containment adds
0.4 m to that radius, beyond the current standing/walking posed limbs. The
waiting-room interior is X 0.2..5.4, Z 11.0..15.8, under the 3 m ceiling. Player
clearance uses the shared 0.8 m square movement hull. Physics, animation, Luau
budgets, door collision and navigation are shared engine capabilities.

Implementation and native acceptance evidence belong only in
`docs/reports/release-0.13.0.md`; raw Windows receipts remain outside `target`.
