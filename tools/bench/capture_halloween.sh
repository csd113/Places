#!/bin/sh
# Captures the Halloween entities fixture in motion from the real renderer.
#
# The fixture level is compiled with places-compile; this script stages the
# package into its own scratch state root and captures a fixed view set at
# several frame numbers, so every capture is a genuine frame of the running
# simulation (jump arc, ghost fade cycle, skeleton gait) rather than a posed
# still. Vsync is enabled in the scratch settings: a frame number is then a
# close proxy for elapsed simulation time (~1/60 s per frame), which is what
# makes two runs of the same view comparable.
#
# The simulation advances by the real (clamped) frame delta, so a frame number
# is *not* byte-reproducible: repeated runs agree on the static scene and on
# the entity poses to within about one display level, but a moving silhouette
# and its light pool can differ by a frame of motion. The manifest records the
# exact spawn/camera/frame of every capture.
#
#     cargo run --release --bin places-compile -- build \
#         tests/fixtures/levels/halloween_entities.json \
#         --out target/halloween/halloween_entities.placesmap
#     sh tools/bench/capture_halloween.sh
#     PLACES_QUALITY=low sh tools/bench/capture_halloween.sh
#
# Environment:
#   PLACES_BIN            executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR    output root (default target/agent-work/halloween-captures)
#   PLACES_QUALITY        `high` (default), `medium` or `low`
#   PLACES_HALLOWEEN_PACKAGE  compiled fixture package
#                             (default target/halloween/halloween_entities.placesmap)
#   PLACES_HALLOWEEN_STATE    scratch state root (default target/halloween-state)
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/target/agent-work/halloween-captures}"
STATE="${PLACES_HALLOWEEN_STATE:-$REPO/target/halloween-state}"
PACKAGE="${PLACES_HALLOWEEN_PACKAGE:-$REPO/target/halloween/halloween_entities.placesmap}"
PROFILE="${PLACES_QUALITY:-high}"

if [ ! -x "$BIN" ]; then
    echo "capture_halloween: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi
if [ ! -f "$PACKAGE" ]; then
    echo "capture_halloween: $PACKAGE is missing; compile the fixture first" >&2
    exit 1
fi

case "$PROFILE" in
    low) filtering=low; lightmaps=off; reflections=off ;;
    medium) filtering=medium; lightmaps=medium; reflections=medium ;;
    high | full) filtering=high; lightmaps=full; reflections=full ;;
    *)
        echo "capture_halloween: PLACES_QUALITY must be 'low', 'medium' or 'high'" >&2
        exit 2
        ;;
esac

mkdir -p "$STATE/levels" "$OUT/$PROFILE"
cp "$PACKAGE" "$STATE/levels/halloween_entities.placesmap"
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

# One line per view: <name>:<spawn x,z,yaw>:<camera yaw,pitch>:<frame list>
# The pumpkin hop cycle is 1.2 s and the ghost fade cycle is 7 s, so the
# frame lists sample a full cycle of each (~1/60 s per vsynced frame). The
# player camera's yaw 0 faces -Z (entity yaw 0 faces +Z), so the walkway views
# look north with yaw 180 from the south side.
VIEWS="
pumpkin_side:-2.4,-2.5,90:90,-8:1,18,36,54,72
pumpkin_face:-1.3,2.6,27:27,-12:120,300,480
pumpkin_laugh:0.0,6.2,0:0,-14:1,120,180,260,340
ghost_a_cycle:9.0,-9.6,180:180,-8:1,105,210,315,420
ghost_bc_overlap:4.0,0.5,90:90,-6:1,105,210,315,420
skeleton_walk:-4.5,-2.0,270:270,-4:1,120,240,360,480
skeleton_close:-7.6,-1.2,255:255,-6:120,300,480,660
garden_overview:0.0,9.4,0:0,-10:210
wall_shadow_side:-5.2,4.0,90:90,-2:210
"

failures=0
: > "$OUT/$PROFILE/manifest.txt"
for view in $VIEWS; do
    name="${view%%:*}"
    rest="${view#*:}"
    spawn="${rest%%:*}"
    rest="${rest#*:}"
    camera="${rest%%:*}"
    frames="${rest#*:}"
    for frame in $(printf '%s' "$frames" | tr ',' ' '); do
        set -- env PLACES_STATE_ROOT="$STATE" PLACES_LEVEL=halloween_entities \
            PLACES_QUALITY="$PROFILE" PLACES_BENCH=1
        set -- "$@" PLACES_SPAWN="$spawn"
        set -- "$@" PLACES_CAMERA="$camera"
        set -- "$@" PLACES_CAPTURE_FRAME="$frame"
        set -- "$@" "PLACES_CAPTURE=$OUT/$PROFILE/${name}_f${frame}.png" "$BIN"
        if "$@" >/dev/null 2>&1; then
            printf '%s\tlevel=halloween_entities\tspawn=%s\tcamera=%s\tframe=%s\tquality=%s\n' \
                "${name}_f${frame}.png" "$spawn" "$camera" "$frame" "$PROFILE" \
                >> "$OUT/$PROFILE/manifest.txt"
        else
            echo "FAILED ${PROFILE}/${name}_f${frame}" >&2
            failures=$((failures + 1))
        fi
    done
done

captured=$(wc -l < "$OUT/$PROFILE/manifest.txt" | tr -d ' ')
echo "capture_halloween: $OUT/$PROFILE: $captured captured, $failures failed"
[ "$failures" -eq 0 ]
