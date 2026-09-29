#!/bin/sh
# Captures Places Demo's night outdoor route from the real renderer.
#
# The demo is a bundled level: the game loads assets/levels/places_demo.placesmap
# (the maintained package), so rebuild it before capturing:
#
#     cargo run --release --bin places-compile -- build assets/levels/places_demo.json
#     sh tools/bench/capture_night_route.sh
#     PLACES_QUALITY=low sh tools/bench/capture_night_route.sh
#
# Vsync is on in the scratch settings, so a frame number is a close proxy for
# elapsed simulation time (~1/60 s per frame) and the entity views are genuine
# frames of the running simulation (the pumpkin hop, the ghost fade cycle, the
# skeleton's wander) rather than posed stills. Repeated runs agree on the static
# scene; a moving silhouette and its light pool can differ by a frame of motion.
#
# Environment:
#   PLACES_BIN              executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR      output root (default target/agent-work/night-route-captures)
#   PLACES_QUALITY          `high` (default), `medium` or `low`
#   PLACES_NIGHT_STATE      scratch state root (default target/night-route-state)
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/target/agent-work/night-route-captures}"
STATE="${PLACES_NIGHT_STATE:-$REPO/target/night-route-state}"
PROFILE="${PLACES_QUALITY:-high}"

if [ ! -x "$BIN" ]; then
    echo "capture_night_route: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi
if [ ! -f "$REPO/assets/levels/places_demo.placesmap" ]; then
    echo "capture_night_route: assets/levels/places_demo.placesmap is missing; compile the demo first" >&2
    exit 1
fi

case "$PROFILE" in
    low) filtering=low; lightmaps=off; reflections=off ;;
    medium) filtering=medium; lightmaps=medium; reflections=medium ;;
    high | full) filtering=high; lightmaps=full; reflections=full ;;
    *)
        echo "capture_night_route: PLACES_QUALITY must be 'low', 'medium' or 'high'" >&2
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

# One line per view: <name>:<spawn x,z,yaw>:<camera yaw,pitch>:<frame list>
# The route runs north (-Z): yaw 0 looks up the gravel route and yaw 180 looks
# back at the source house. The pumpkin's loop is ~34 s and the ghost cycles are
# 7-9.2 s, so the frame lists sample several seconds of each.
VIEWS="
source_inside_open:4.5,1.8,0:0,-4:210
source_door_outside:4.5,-2.6,0:0,-2:210
source_look_back_near:4.5,-7.0,0:180,1:210
source_look_back_mid:4.5,-30.0,0:180,1:210
source_look_back_far:4.5,-86.0,0:180,1:210
route_forward_start:4.5,-2.5,0:0,-3:210
route_forward_mid:4.5,-40.0,0:0,-3:210
route_forward_north:4.5,-84.0,0:0,-3:210
grass_blend_west:4.5,-22.0,0:255,-22:210
grass_blend_east:4.5,-22.0,0:105,-22:210
walkway_from_route:4.5,-30.0,0:90,-10:210
connector_approach:4.5,-84.0,0:14,-8:210
connector_junction:9.0,-84.6,0:0,-12:210
walkway_forward:13.5,-79.0,0:0,-3:210
destination_approach:13.5,-87.0,0:0,-1:210
destination_door:13.5,-90.4,0:0,2:210
destination_inside:13.5,-93.2,0:0,-3:210
destination_inside_out:13.5,-95.6,0:180,0:210
night_overview:4.5,-40.0,0:0,10:210
sky_and_route:4.5,-62.0,0:6,32:210
pumpkin_jump:13.5,-80.0,0:0,-8:200,400,600,800,1000,1200
ghost_a_fade:5.0,-20.5,270:270,-6:1,105,210,315,420
ghost_b_fade:5.0,-48.5,270:270,-6:1,105,210,315,420
ghost_c_fade:5.0,-73.0,270:270,-6:1,105,210,315,420
skeleton_walk:6.9,-52.0,90:90,-4:1,120,240,360,480
skeleton_close:7.2,-52.0,90:90,-4:120,300
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
        set -- env PLACES_STATE_ROOT="$STATE" PLACES_LEVEL=places_demo \
            PLACES_QUALITY="$PROFILE" PLACES_BENCH=1
        set -- "$@" PLACES_SPAWN="$spawn"
        set -- "$@" PLACES_CAMERA="$camera"
        set -- "$@" PLACES_CAPTURE_FRAME="$frame"
        set -- "$@" "PLACES_CAPTURE=$OUT/$PROFILE/${name}_f${frame}.png" "$BIN"
        if "$@" >/dev/null 2>&1; then
            printf '%s\tlevel=places_demo\tspawn=%s\tcamera=%s\tframe=%s\tquality=%s\n' \
                "${name}_f${frame}.png" "$spawn" "$camera" "$frame" "$PROFILE" \
                >> "$OUT/$PROFILE/manifest.txt"
        else
            echo "FAILED ${PROFILE}/${name}_f${frame}" >&2
            failures=$((failures + 1))
        fi
    done
done

captured=$(wc -l < "$OUT/$PROFILE/manifest.txt" | tr -d ' ')
echo "capture_night_route: $OUT/$PROFILE: $captured captured, $failures failed"
[ "$failures" -eq 0 ]
