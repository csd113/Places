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

`tools/bench/performance/views.json` and adjacent native action files retain
repeatable camera/setup inputs. The lab-flow input explicitly places the player
between scenarios; movement, triggers, spawn, lifetime and physics use production
paths. These diagnostic placements are separate from authored content.

For example, after Save+Build:

```sh
python3 tools/bench/frame_time.py --binary target/release/places \
  --package projects/performance-lab/levels/performance_lab.placesmap \
  --out local/lab-timing-new --level performance_lab \
  --spawn=14.5,1.6,11,315 --camera=315,0 \
  --size 640x360 --expected-drawable 1280x720 --warmup 120 --frames 2400
```

V-Sync defaults off in this harness. Verify actual Immediate presentation,
physical drawable, zero missed ready surfaces and uncapped pacing in the receipt.
Use `--seconds 12 --warmup 0 --real-delta` for equal wall-clock interaction phases;
a faster build otherwise ends a fixed-frame run earlier. Video capture is a
separate paced diagnostic and must never supply authoritative timing numbers.
`PLACES_BENCH_SCRIPT_TRACE=1` on a direct diagnostic launch records at most 64
invocation timings, including native capability setup; it changes no script API.
