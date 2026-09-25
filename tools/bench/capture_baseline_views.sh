#!/bin/sh
# Captures the canonical frozen reference set: the fixed view set below, in the
# High and Low quality profiles, from Places Demo.
#
# The frozen reference lives in docs/renderer-baseline/ (high/ and low/ beside
# BASELINE.md). A current build is captured with the same script and compared
# view by view, or used to refresh a comparison capture directory:
#
#     sh tools/bench/capture_baseline_views.sh                       # both profiles
#     PLACES_BIN=target/release/places \
#         PLACES_CAPTURE_DIR=target/agent-work/baseline-current \
#         sh tools/bench/capture_baseline_views.sh                   # a comparison build
#     PLACES_QUALITY=low sh tools/bench/capture_baseline_views.sh   # one profile
#
# Pinned session: the script owns a scratch state root
# (target/renderer-baseline-state/ by default) whose settings.json fixes the
# window at 640x360 logical, which is a 1280x720 drawable on a 2x display. Every
# view pins its spawn and camera through PLACES_SPAWN / PLACES_CAMERA, so the
# same command produces the same image on any machine with the same display
# backing scale. Delete the state root before a run to force a cold lightmap
# bake instead of reusing the cache below it.
#
# Environment:
#   PLACES_BIN            executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR    output root; the script writes <root>/high and <root>/low
#   PLACES_QUALITY        capture only `high` or `low` instead of both
#                         (`full` is the historical alias of `high`)
#   PLACES_BASELINE_STATE scratch state root (default target/renderer-baseline-state)
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/docs/renderer-baseline}"
STATE="${PLACES_BASELINE_STATE:-$REPO/target/renderer-baseline-state}"

if [ ! -x "$BIN" ]; then
    echo "capture_baseline_views: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi

# The fixed canvas: 640x360 logical is a 1280x720 drawable at 2x, with a 60
# degree field of view. The overall quality level and the three Advanced
# settings are independent, so each profile pins all four explicitly to its
# preset defaults (Low -> low/off/off, High -> high/full/full); a bare
# `PLACES_QUALITY` override would leave the advanced settings at the saved
# values.
mkdir -p "$STATE"
write_state() {
    profile=$1
    case "$profile" in
        low) filtering=low; lightmaps=off; reflections=off ;;
        medium) filtering=medium; lightmaps=medium; reflections=medium ;;
        *) filtering=high; lightmaps=full; reflections=full ;;
    esac
    cat > "$STATE/settings.json" <<JSON
{
  "bindings": {
    "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
    "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
  },
  "look_speed_h": 90.0, "look_speed_v": 60.0, "walk_speed": 3.0, "fov_degrees": 60.0,
  "invert_look": false, "vsync": false, "texture_filtering": "$filtering",
  "quality": "$profile", "bloom": true, "reflections": "$reflections", "lightmaps": "$lightmaps",
  "window_mode": "windowed", "window_width": 640, "window_height": 360
}
JSON
}

# One line per view: <name>:<spawn>:<camera>
# `spawn` is x,z[,yaw] (three numbers drop the player onto the local floor, so
# the Home balcony views land on the upper storey) and `camera` is the absolute
# PLACES_CAMERA yaw,pitch override, which needs PLACES_BENCH=1.
VIEWS="
reception:2.0,5.6,74::
office:9.5,3.5,90::
office_win_close:3.1,4.8,0:180,-10:
pool_win_from_pool:3.1,9.4,0:0,10:
pool_wide:14.0,13.0,45::
pool_north:12.0,14.0,0::
pool_entry:21.0,8.6,270:270,-10:
wet_deck_shallow:20.6,-0.1,10.8,0:0,-25:
pool_basin_from_deck:10.5,10.6,0:0,-26:
pool_steps:22.0,11.5,0:90,-10:
plastic_panel:6.0,10.5,0::
panels_east:23.6,8.2,90::
stair_mid:21.5,3.2,180:180,-14:
corridor_mid:30.5,13.0,90:90,-4:
drum:28.4,13.6,0:0,-22:
home_approach:50.8,12.6,90::
home_arch_entry:54.4,12.6,90:90,-2:
home_stairs_low:54.0,13.6,90:90,-10:
home_main_south:62.4,4.8,180:180,-2:
home_main_north:61.0,9.4,0:0,-2:
home_main_west:63.4,7.0,250:250,-2:
home_kitchen:60.6,6.6,300:300,-3:
home_balcony_east:59.6,12.5,90:90,-6:
home_ceiling:61.0,7.0,180:180,38:
home_under_balcony:62.6,13.6,0:0,-2:
"

capture_profile() {
    profile=$1
    dir=$2
    write_state "$profile"
    mkdir -p "$dir"
    rm -f "$dir"/*.png
    : > "$dir/manifest.txt"
    failures=0
    for view in $VIEWS; do
        name="${view%%:*}"
        rest="${view#*:}"
        spawn="${rest%%:*}"
        rest="${rest#*:}"
        camera="${rest%%:*}"
        set -- env PLACES_STATE_ROOT="$STATE" PLACES_BENCH=1 PLACES_BENCH_NOSWAP=1 \
            PLACES_LEVEL=places_demo PLACES_QUALITY="$profile"
        if [ -n "$spawn" ]; then
            set -- "$@" PLACES_SPAWN="$spawn"
        fi
        if [ -n "$camera" ]; then
            set -- "$@" PLACES_CAMERA="$camera"
        fi
        set -- "$@" PLACES_CAPTURE="$dir/${name}.png" "$BIN"
        if "$@" >/dev/null 2>&1; then
            printf '%s\tlevel=places_demo\tspawn=%s\tcamera=%s\tquality=%s\n' \
                "${name}.png" "${spawn:-<level-spawn>}" "${camera:-<spawn-yaw>}" "$profile" \
                >> "$dir/manifest.txt"
        else
            echo "FAILED ${profile}/${name}" >&2
            failures=$((failures + 1))
        fi
    done
    captured=$(wc -l < "$dir/manifest.txt" | tr -d ' ')
    echo "capture_baseline_views: ${dir#"$REPO"/} ($profile): $captured captured, $failures failed"
    if [ "$failures" -ne 0 ]; then
        return 1
    fi
}

status=0
case "${PLACES_QUALITY:-both}" in
    high | full)
        capture_profile high "$OUT/high" || status=1
        ;;
    low)
        capture_profile low "$OUT/low" || status=1
        ;;
    both)
        capture_profile high "$OUT/high" || status=1
        capture_profile low "$OUT/low" || status=1
        ;;
    *)
        echo "capture_baseline_views: PLACES_QUALITY must be 'high', 'low' or unset ('full' is the historical alias of 'high')" >&2
        exit 2
        ;;
esac
exit "$status"
