# Performance Lab

A 16 × 12 m editable project using installed assets and ordinary editor features.
It supplements the shipped Home, Pool and Arcade repros. It does not define new
runtime capabilities, raise content limits, or run as a Cargo/CI map bake.

Open `projects/performance-lab` in `places-editor`. Save+Build, close/reopen and
Project > Export all levels + sources use the normal workflow. Both Static and
Full remain available. The baseline export was 7.7 MB and took about two minutes
on the M2 Pro; rebuilding the shipped collection is unnecessary.

- Northwest: fifteen bookshelves and forty-five book stacks stress static models,
  materials, bounds and submission through passages.
- Southwest: twenty-four cups plus one pickup cushion start frozen at authored
  release heights. Enter the gate at X5.5–7, Z6.5–8 to activate `lab_contacts`;
  leave to request its ordinary atomic reset. Clear original positions first.
- Southeast: twelve independent wandering mannequins exercise animation, posed
  bounds, navigation/replanning, perception and doorway visibility.
- Northeast: authored mist, room lighting and a visitor spawn gate at X9–10.5,
  Z1–2.5. The visitor has a three-second lifetime and one-active group; reenter
  after expiry to repeat. Use the ordinary flashlight for moving shadow views.

`scripts/lab.luau` performs twelve current actor queries per tick and links
activation, reset and spawn handlers to enter/exit events. Reloading or reentering the whole level creates
fresh script state; the contact-group reset only restores its physics bodies. F1 logs bounded tick/visibility counts. Use Scripts > Check
and the trigger's Script handler sheet to inspect/edit those links.

Open the lab in the editor and prepare the package with Save+Build. Use the
native player's built-in benchmark mode for timing, with V-sync and all frame
limiters off. Record actual presentation mode and physical drawable size. Treat
a refresh-rate plateau as inconclusive, and keep video capture out of FPS runs.
`PLACES_BENCH_SCRIPT_TRACE=1` on a direct diagnostic launch records at most 64
invocation timings, including native capability setup; it changes no script API.

## Replaying measured workloads

The [frame-time runner](acceptance/frame_time.py) wraps the native benchmark
for explicit prepared packages. The [recorded views](acceptance/views.json)
include Home, Pool, Arcade and Performance Lab. Use the matching authored
package and recheck the physical drawable and presentation mode.

For the Windows Home bed reproduction, use the side spawn below. The
[books actions](acceptance/home-books.json) put two books on the bed,
crouch-jump onto it, stand and retreat. The old rear-wall spawn cannot fit
the full player body. Run scripted input with real delta so the later
wall-clock actions occur before measurement ends.

```sh
python projects/performance-lab/acceptance/frame_time.py --binary PATH_TO_PLACES --package PATH_TO_ENVIRONMENTS --out NEW_RUN_DIRECTORY --level home --spawn=3.6,1.6,10.7,270 --camera=270,-25 --actions projects/performance-lab/acceptance/home-books.json --seconds 7 --warmup 0 --real-delta --size 1280x720 --expected-drawable 1280x720
```

The runner refuses an unrecoverable spawn before it writes a timing receipt.
It does not establish that every route succeeded. Inspect native snapshots
separately and treat a refresh-rate plateau as inconclusive.
