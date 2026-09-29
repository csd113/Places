#!/bin/sh
# Captures every maintained door leaf in Places Demo from the real renderer.
#
# The demo is a bundled level: the game loads assets/levels/places_demo.placesmap
# (the maintained package), so rebuild it after any source edit:
#
#     cargo run --release --bin places-compile -- build assets/levels/places_demo.json
#     sh tools/bench/capture_doors.sh
#     PLACES_QUALITY=low sh tools/bench/capture_doors.sh
#
# One line per view: <name>|<spawn x,z,yaw>|<camera yaw,pitch or ->|<capture
# seconds>|<move script or ->|<PLACES_INTERACT ids or ->. A view is a genuine
# frame of the running simulation: `PLACES_CAPTURE_TIME` names the ready-world
# simulation second, `PLACES_MOVE_SCRIPT` holds real controls and
# `PLACES_INTERACT` dispatches the authored interaction once, so an `_arc` case
# shows the leaf at successive points of its real swing.
#
# The door families in the demo, and the side each view looks from:
#   hall_door            Home corridor (z<3) <-> living room (z>3)
#   study_door           Home living room (x<64.7) <-> study (x>65)
#   sauna_door           pool deck (-1.5, x<26.08) <-> sauna (-0.9, x>26.08)
#   sauna_shower_door    shower bay (z<10.85) <-> sauna (z>11.15)
#   night_source_door    yard (z<0) <-> source house interior (z>0)
#   night_house_door     yard (z>-91.7) <-> destination house (z<-92)
#
# Environment:
#   PLACES_BIN            executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR    output root (default target/agent-work/door-captures)
#   PLACES_QUALITY        `high` (default), `medium` or `low`
#   PLACES_DOOR_STATE     scratch state root (default target/door-state)
#   PLACES_DOOR_CASES     extended-grep filter on the view name, to capture a
#                         subset (for example `hall_door|night_house`)
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/target/agent-work/door-captures}"
STATE="${PLACES_DOOR_STATE:-$REPO/target/door-state}"
PROFILE="${PLACES_QUALITY:-high}"

if [ ! -x "$BIN" ]; then
    echo "capture_doors: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi
if [ ! -f "$REPO/assets/levels/places_demo.placesmap" ]; then
    echo "capture_doors: assets/levels/places_demo.placesmap is missing; compile the demo first" >&2
    exit 1
fi

case "$PROFILE" in
    low) filtering=low; lightmaps=off; reflections=off ;;
    medium) filtering=medium; lightmaps=medium; reflections=medium ;;
    high | full) filtering=high; lightmaps=full; reflections=full ;;
    *)
        echo "capture_doors: PLACES_QUALITY must be 'low', 'medium' or 'high'" >&2
        exit 2
        ;;
esac

mkdir -p "$STATE/levels" "$OUT/$PROFILE"
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

# `forward@0-1.1` walks the player through an opened leaf; the other scripts
# only stand and look, so the frame is a stable view of the leaf.
CASES="
hall_door_corridor|61.0,1.3,180|180,-2|0.05|-|-
hall_door_corridor_close|61.0,2.1,180|180,0|0.05|-|-
hall_door_living|61.0,5.2,0|0,-2|0.05|-|-
hall_door_living_close|61.0,4.3,0|0,0|0.05|-|-
hall_door_arc|61.0,1.3,180|180,-4|0.05,0.35,0.7,1.2,1.8|-|hall_door
hall_door_pass|61.0,1.3,180|180,-8|0.05,0.8,1.8,2.6|forward@0.6-2.4|hall_door
study_door_living|62.9,5.1,90|90,-2|0.05|-|-
study_door_study|66.8,5.1,270|270,-2|0.05|-|-
sauna_door_pool|24.5,13.3,90|90,3|0.05|-|-
sauna_door_sauna|27.9,13.3,270|270,-2|0.05|-|-
sauna_door_arc|27.9,13.3,270|270,-2|0.05,0.5,1.0,1.6|-|sauna_door
sauna_shower_shower|31.9,9.4,180|180,3|0.05|-|-
sauna_shower_sauna|31.9,12.9,0|0,-2|0.05|-|-
sauna_shower_arc|31.9,12.9,0|0,-3|0.05,0.45,0.95,1.5|-|sauna_shower_door
night_source_outside|4.5,-2.2,180|180,-2|0.05|-|-
night_source_inside|4.5,2.4,0|0,-2|0.05|-|-
night_source_close|4.5,-1.35,180|180,-1|0.05|-|-
night_source_arc|4.5,-2.2,180|180,-3|0.05,0.5,1.0,1.6|-|night_source_door
night_house_yard|13.5,-89.6,0|0,-2|0.05|-|-
night_house_inside|13.5,-93.8,180|180,-2|0.05|-|-
night_house_close|13.5,-90.6,0|0,-1|0.05|-|-
night_house_arc|13.5,-89.6,0|0,-3|0.05,0.5,1.0,1.6|-|night_house_door
"

SELECT="${PLACES_DOOR_CASES:-.}"
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
        # The trajectory beside each frame is evidence: frame,x,y,z,yaw,pitch.
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
echo "capture_doors: $OUT/$PROFILE: $captured captured, $failures failed"
[ "$failures" -eq 0 ]
