#!/bin/sh
# Captures the Stage 10 expanded parity view set: targeted cameras for the
# render features the canonical 25-view set covers only incidentally
# (geometry junctions, materials, lightmap regions, reflections, props,
# decals, fog and height). Meant to be captured with BOTH renderers and
# compared view by view, exactly like capture_baseline_views.sh:
#
#     PLACES_ASSET_ROOT=$PWD/target/agent-work/stage10/baseline-assets/asset-root \
#     PLACES_CAPTURE_DIR=$PWD/target/agent-work/stage10/expanded-opengl \
#     PLACES_BASELINE_STATE=$PWD/target/agent-work/stage10/state/expanded-opengl \
#         sh tools/bench/capture_expanded_views.sh
#
# The view list is a comment-annotated superset of the canonical features; it
# never replaces docs/renderer-baseline/. Views are pinned the same way:
# `spawn` is x,z[,yaw] (or x,y,z,yaw) and `camera` is the absolute
# PLACES_CAMERA yaw,pitch override, which needs PLACES_BENCH=1. Yaw points
# where the camera looks: 0 = -Z, 90 = +X, 180 = +Z, 270 = -X.
#
# Environment:
#   PLACES_BIN            executable to capture (default target/release/places)
#   PLACES_CAPTURE_DIR    output root; the script writes <root>/high and <root>/low
#   PLACES_QUALITY        capture only `full` or `low` instead of both
#   PLACES_BASELINE_STATE scratch state root
#   PLACES_RENDERER       opengl (default) or wgpu, read by the binary
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="${PLACES_BIN:-$REPO/target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-$REPO/target/agent-work/stage10-expanded}"
STATE="${PLACES_BASELINE_STATE:-$REPO/target/agent-work/stage10-expanded-state}"

if [ ! -x "$BIN" ]; then
    echo "capture_expanded_views: $BIN is not executable; run 'cargo build --release' first" >&2
    exit 1
fi

mkdir -p "$STATE"
cat > "$STATE/settings.json" <<JSON
{
  "bindings": {
    "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
    "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
  },
  "look_speed_h": 90.0, "look_speed_v": 60.0, "walk_speed": 3.0, "fov_degrees": 60.0,
  "invert_look": false, "vsync": false, "texture_filtering": "linear",
  "quality": "full", "bloom": true, "reflections": true, "lightmaps": true,
  "window_mode": "windowed", "window_width": 640, "window_height": 360
}
JSON

# Geometry: junctions, corridor, thresholds, arch, stairs, balcony, near/far.
# Materials: linoleum, brushed metal, plastic normal map, cutout, three glass
# kinds. Lightmaps: bright pool, dark hallway, doorway, undersides, contact.
# Reflections: probe floor/board, planar head-on and oblique. Props: office,
# pool, home, contact shadow, dynamic drum. Decals: floor, wall, oblique,
# distance, steps. Fog: long corridor, height-dependent.
VIEWS="
junction_reception:7.90,3.60,90:90,0:
corridor_long:34.00,13.00,90:90,-1:
doorway_threshold:31.60,13.20,90:90,-8:
archway_west:50.90,12.00,90:90,0:
stairs_up:54.20,13.00,90:90,-8:
stairs_from_balcony:58.60,13.10,270:270,-30:
balcony_west:64.60,13.20,270:270,-6:
home_hall_west:67.50,1.50,270:270,-1:
very_near_wall:0.60,3.50,270:270,-2:
home_distant:54.20,14.20,47:47,-2:
pool_entry_look:21.50,6.60,0:0,-4:
linoleum_floor:13.90,1.60,90:90,-20:
metal_board:24.20,8.20,90:90,-2:
plastic_board:6.20,9.40,336:336,-6:
grille_vent:7.50,3.60,90:90,12:
glass_tinted:17.40,4.30,90:90,4:
glass_clear_pool:12.30,8.10,0:0,5:
glass_dirty_reception:3.10,5.40,180:180,6:
pool_bright:7.50,17.20,41:41,4:
home_hall_dark:65.50,-1.80,134:134,-3:
doorway_baseline:23.80,13.20,90:90,-3:
stair_underside:53.90,13.40,90:90,-22:
prop_shadow_floor:13.90,2.60,180:180,-35:
balcony_underside:61.00,13.40,180:180,30:
probe_board_edge:22.80,8.90,77:77,-3:
planar_head_on:21.50,11.60,0:0,-28:
planar_east:23.80,8.60,270:270,-30:
office_desk_props:11.40,3.10,180:180,-10:
pool_table_props:4.00,13.00,180:180,-8:
home_living_props:59.20,5.60,93:93,-7:
prop_occlusion_close:14.60,4.00,221:221,-25:
dynamic_drum_close:27.60,13.60,20:20,-20:
decal_floor_basin:10.50,11.00,0:0,-40:
decal_floor_angle:8.80,11.20,40:40,-30:
decal_wall_south:14.50,8.80,0:0,3:
decal_stripes:21.20,8.60,111:111,-14:
decal_distance:12.50,16.50,62:62,-3:
decal_steps:24.00,12.60,13:13,-20:
fog_long_corridor:50.00,13.00,270:270,-1:
fog_home_height:59.60,12.50,9:9,-3:
"

capture_profile() {
    profile=$1
    dir=$2
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
    echo "capture_expanded_views: ${dir#"$REPO"/} ($profile): $captured captured, $failures failed"
    if [ "$failures" -ne 0 ]; then
        return 1
    fi
}

status=0
case "${PLACES_QUALITY:-both}" in
    full)
        capture_profile full "$OUT/high" || status=1
        ;;
    low)
        capture_profile low "$OUT/low" || status=1
        ;;
    both)
        capture_profile full "$OUT/high" || status=1
        capture_profile low "$OUT/low" || status=1
        ;;
    *)
        echo "capture_expanded_views: PLACES_QUALITY must be 'full', 'low' or unset" >&2
        exit 2
        ;;
esac
exit "$status"
