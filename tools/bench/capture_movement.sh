#!/bin/sh
# Captures real motion sequences in Places Demo through the scripted-input
# override: the player jumping onto and off the kitchen counter/stove/sink,
# repeated pool entry and exit over the deck rim, and the moving entities
# (Spooner-Man, the rat, the night pumpkin-head skeleton) travelling.
#
# Every capture is a genuine frame of the running simulation: the level is the
# compiled package, the input is applied by `PLACES_MOVE_SCRIPT` (real held
# controls with real press edges, see src/input.rs), and `PLACES_CAPTURE_TIME`
# names the ready-world simulation second to write. Both clocks advance on the
# real (clamped) simulation delta, so a capture shows the same in-world moment
# whatever the run's frame rate; the per-capture CSV beside each PNG records the
# exact player trajectory that produced it.
#
#     cargo run --release --bin places-compile -- build assets/levels/places_demo.json
#     sh tools/bench/capture_movement.sh                      # High
#     PLACES_QUALITY=low sh tools/bench/capture_movement.sh   # Low
#
# Environment:
#   PLACES_BIN             executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR     output root (default target/agent-work/movement-captures)
#   PLACES_QUALITY         `high` (default), `medium` or `low`
#   PLACES_MOVEMENT_PACKAGE  compiled demo package
#                            (default assets/levels/places_demo.placesmap)
#   PLACES_MOVEMENT_STATE    scratch state root (default target/movement-state)
#   PLACES_MOVEMENT_CASES    extended-grep filter on the case name, to capture a
#                            subset (for example `kitchen_stove|pool_walk_in`)
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/target/agent-work/movement-captures}"
STATE="${PLACES_MOVEMENT_STATE:-$REPO/target/movement-state}"
PACKAGE="${PLACES_MOVEMENT_PACKAGE:-$REPO/assets/levels/places_demo.placesmap}"
PROFILE="${PLACES_QUALITY:-high}"

if [ ! -x "$BIN" ]; then
    echo "capture_movement: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi
if [ ! -f "$PACKAGE" ]; then
    echo "capture_movement: $PACKAGE is missing; compile the demo first" >&2
    exit 1
fi

case "$PROFILE" in
    low) filtering=low; lightmaps=off; reflections=off ;;
    medium) filtering=medium; lightmaps=medium; reflections=medium ;;
    high | full) filtering=high; lightmaps=full; reflections=full ;;
    *)
        echo "capture_movement: PLACES_QUALITY must be 'low', 'medium' or 'high'" >&2
        exit 2
        ;;
esac

mkdir -p "$STATE/levels" "$OUT/$PROFILE"
cp "$PACKAGE" "$STATE/levels/places_demo.placesmap"
cat > "$STATE/settings.json" <<JSON
{
  "bindings": {
    "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
    "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
  },
  "look_speed_h": 90.0, "look_speed_v": 60.0, "walk_speed": 3.0, "fov_degrees": 60.0,
  "invert_look": false, "vsync": true, "texture_filtering": "$filtering",
  "quality": "$PROFILE", "bloom": true, "reflections": "$reflections",
  "lightmaps": "$lightmaps", "window_mode": "windowed", "window_width": 640, "window_height": 360
}
JSON

# One case per line, fields separated by `|`:
#   name | spawn x,z[,yaw] | camera yaw,pitch or "-" | capture seconds |
#   move script or "-" | PLACES_INTERACT ids or "-"
#
# Times are ready-world simulation seconds, so a case shows the same in-world
# moments at any frame rate. The kitchen floor is -0.9 and the counter tops are
# world 0.0 (0.9 above it); the stove and the sink jump from 1.45 m in front of
# their faces (spawn z 5.2, facing north: camera yaw 0 faces -Z). After landing
# the player strafes left along the run, then walks back off the front edge.
COUNTER_SCRIPT="forward@0-1.2,jump@0-0.06,strafe_left@1.35-1.75,backward@2.0-2.9"
# The pool deck is -1.5 and the water surface -1.65; the basin is x 8..20,
# z 10..16 and the north deck beyond z=10 is the exit at x=19 (clear of the
# guardrail run). The player walks north off the deck at z=16, surfaces, swims
# the basin, climbs out at the north rim, walks back into the water backwards
# (no turn needed), and exits again.
POOL_SCRIPT="forward@0-6.4,jump@1.25-20,backward@6.8-8.2,forward@8.3-20"
# The kitchen cases are split by camera: the approach looks north at the run,
# the landing looks back south over the drop (the player stands against the
# north wall, so only a downward/back view sees the counter), the run view
# looks east along the counter, and the leave view follows the walk-off.
CASES="
kitchen_counter_approach|55.4,5.2,0|0,-35|0.05,0.25,0.45|$COUNTER_SCRIPT|-
kitchen_counter_land|55.4,5.2,0|180,-60|0.6,0.75,0.9|$COUNTER_SCRIPT|-
kitchen_counter_run|55.4,5.2,0|90,-40|1.4,1.6,1.75|$COUNTER_SCRIPT|-
kitchen_counter_leave|55.4,5.2,0|180,-45|2.2,2.5,2.8|$COUNTER_SCRIPT|-
kitchen_stove_approach|56.0,5.2,0|0,-35|0.05,0.25,0.45|$COUNTER_SCRIPT|-
kitchen_stove_land|56.0,5.2,0|180,-60|0.6,0.75,0.9|$COUNTER_SCRIPT|-
kitchen_stove_run|56.0,5.2,0|90,-40|1.4,1.6,1.75|$COUNTER_SCRIPT|-
kitchen_stove_leave|56.0,5.2,0|180,-45|2.2,2.5,2.8|$COUNTER_SCRIPT|-
kitchen_sink_approach|54.8,5.2,0|0,-35|0.05,0.25,0.45|$COUNTER_SCRIPT|-
kitchen_sink_land|54.8,5.2,0|180,-60|0.6,0.75,0.9|$COUNTER_SCRIPT|-
kitchen_sink_run|54.8,5.2,0|90,-40|1.4,1.6,1.75|$COUNTER_SCRIPT|-
kitchen_sink_leave|54.8,5.2,0|180,-45|2.2,2.5,2.8|$COUNTER_SCRIPT|-
pool_walk_in|19,18,0|0,-18|0.1,0.4,0.7,1.0,1.3,1.7,2.2,2.8,3.4,4.0,4.4,4.8,5.2,5.6,6.0,6.4,6.8,7.2,7.6,8.0,8.4,9.0,9.6,10.2|$POOL_SCRIPT|-
pool_jump_in|19,18.5,0|0,-18|0.05,0.2,0.4,0.6,0.8,1.0,1.3,1.7,2.2,2.8,3.4,4.0,4.6,5.2,5.8,6.4,7.0|forward@0-6,jump@0-0.06,jump@1.3-20|-
pool_walk_in_back|19,18,0|180,-18|4.0,4.4,4.8,7.2,7.6,8.0,8.4,9.6,10.2|$POOL_SCRIPT|-
pool_jump_in_back|19,18.5,0|180,-18|1.3,2.2,3.4,4.0,4.6,5.2,5.8|forward@0-6,jump@0-0.06,jump@1.3-20|-
home_encounter|56,13.5,0|0,-8|0.5,1.0,1.5,1.8,2.2,2.6,3.0,3.6,4.4,5.4,6.4,7.4,8.4|forward@0-0.6|rat_release_switch
home_encounter_front|51.0,12.6,90|90,-6|0.5,1.5,2.5,3.0,3.5,4.5,5.5|forward@0-0.6|rat_release_switch
night_skeleton|11.6,-52,270|270,-4|0.1,1,2,3,4,5,6,7,8,9,10|-|-
"

# `PLACES_MOVEMENT_CASES` filters the case table by name (an extended grep
# pattern), so a quicker quality-profile run or a verifier can select a subset.
SELECT="${PLACES_MOVEMENT_CASES:-.}"
failures=0
: > "$OUT/$PROFILE/manifest.txt"
while IFS= read -r case; do
    [ -n "$case" ] || continue
    name="${case%%|*}"
    if ! printf '%s' "$name" | grep -Eq "$SELECT"; then
        continue
    fi
    rest="${case#*|}"
    spawn="${rest%%|*}"
    rest="${rest#*|}"
    camera="${rest%%|*}"
    rest="${rest#*|}"
    times="${rest%%|*}"
    rest="${rest#*|}"
    script="${rest%%|*}"
    interact="${rest#*|}"
    for seconds in $(printf '%s' "$times" | tr ',' ' '); do
        # shellcheck disable=SC2086
        set -- env PLACES_STATE_ROOT="$STATE" PLACES_LEVEL=places_demo \
            PLACES_QUALITY="$PROFILE" PLACES_BENCH=1 PLACES_SPAWN="$spawn"
        if [ "$camera" != "-" ]; then
            set -- "$@" PLACES_CAMERA="$camera"
        fi
        if [ "$script" != "-" ]; then
            set -- "$@" PLACES_MOVE_SCRIPT="$script"
        fi
        if [ "$interact" != "-" ]; then
            set -- "$@" PLACES_INTERACT="$interact"
        fi
        # The recorded trajectory is evidence beside the frame: frame,x,y,z,yaw,pitch.
        set -- "$@" PLACES_STATE_LOG="$OUT/$PROFILE/${name}_t${seconds}.csv"
        set -- "$@" PLACES_CAPTURE_TIME="$seconds" \
            "PLACES_CAPTURE=$OUT/$PROFILE/${name}_t${seconds}.png" "$BIN"
        if "$@" >/dev/null 2>&1; then
            printf '%s\tlevel=places_demo\tspawn=%s\tcamera=%s\tseconds=%s\tscript=%s\tinteract=%s\tquality=%s\n' \
                "${name}_t${seconds}.png" "$spawn" "$camera" "$seconds" "$script" "$interact" "$PROFILE" \
                >> "$OUT/$PROFILE/manifest.txt"
        else
            echo "FAILED ${PROFILE}/${name}_t${seconds}" >&2
            failures=$((failures + 1))
        fi
    done
done <<EOF
$CASES
EOF

captured=$(wc -l < "$OUT/$PROFILE/manifest.txt" | tr -d ' ')
echo "capture_movement: $OUT/$PROFILE: $captured captured, $failures failed"
[ "$failures" -eq 0 ]
