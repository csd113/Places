#!/bin/sh
# Captures the fixed validation views into target/agent-work/captures/.
#
# Every view is a fixed spawn and camera, so the same command produces the same
# image on any machine and the before/after, Full/Low and no-post runs are
# directly comparable. Run from the repository root:
#
#     sh tools/bench/capture_views.sh                       # Full profile
#     PLACES_QUALITY=low sh tools/bench/capture_views.sh
#     PLACES_NO_BLOOM=1 sh tools/bench/capture_views.sh
#     PLACES_NO_REFLECTIONS=1 sh tools/bench/capture_views.sh
#
# `PLACES_BIN` overrides the binary, which is how the same view set is captured
# from a baseline checkout for a before/after pair:
#
#     PLACES_BIN=target/agent-work/baseline/target/release/places \
#         sh tools/bench/capture_views.sh
#
# `PLACES_CAPTURE_DIR` overrides the output directory (default
# target/agent-work/captures). Files are named view_<name><suffix>.png; the
# suffix is built from the environment so a comparison run never overwrites the
# reference capture. `PLACES_BENCH_NOSWAP=1` keeps a capture from blocking on a
# display that has gone to sleep; it does not change the pixels.
set -eu

BIN="${PLACES_BIN:-target/release/places}"
OUT="${PLACES_CAPTURE_DIR:-target/agent-work/captures}"
mkdir -p "$OUT"
case "$OUT" in
    /*) OUT_ABS="$OUT" ;;
    *) OUT_ABS="$PWD/$OUT" ;;
esac

SUFFIX=""
if [ "${PLACES_QUALITY:-full}" = "low" ]; then
    SUFFIX="${SUFFIX}_low"
fi
if [ "${PLACES_NO_BLOOM:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_nobloom"
fi
if [ "${PLACES_NO_REFLECTIONS:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_norefl"
fi

# One line per view: <name>:<spawn>:<camera yaw,pitch>:<pause>
# `spawn` is x,z[,yaw] (or x,y,z,yaw for an elevated spot; the eye height comes
# from the local floor) and `camera` is the `PLACES_CAMERA` override, which
# needs PLACES_BENCH=1 to take effect. `pause` = 1 opens the pause menu.
VIEWS="
spawn:::
office:9.5,3.5,90::
office_win_close:3.1,6.5,0:180,-16:
office_win_shallow:1.0,6.4,0:150,-3:
pool_win_from_pool:3.1,9.4,0:0,10:
pool_win_close:3.1,8.2,0:0,22:
jamb_left:0.9,9.2,0:20,26:
jamb_right:5.3,9.2,0:-18,26:
vent_office:5.0,4.4,0:90,12:
grille_pool:8.9,6.2,0:0,10:
pool_wide:14.0,13.0,45::
pool_north:12.0,14.0,0::
pool_east:22.0,12.0,90::
curtains:18.0,12.0,58::
wet_deck:20.5,10.0,58::
wet_deck_shallow:20.6,-0.1,10.8,0:0,-25:
panels_east:23.6,8.2,90::
plastic_panel:6.0,10.5,0::
home_approach:50.8,12.6,90::
linoleum:13.6,1.6,90::
drum:28.4,13.6,0:0,-22:
pause_office:9.5,3.5,90::1
pause_pool:18.0,12.0,58::1
reception:2.0,5.6,74::
workroom:14.0,3.5,0:0,-4:
stair_top:21.0,1.6,180:180,-18:
stair_mid:21.5,3.2,180:180,-14:
pool_entry:21.0,8.6,270:270,-10:
pool_basin_from_deck:14.0,9.5,180:180,-22:
pool_steps:22.0,11.5,0:90,-10:
pool_overview:3.0,10.0,180:135,-18:
corridor_entry:27.5,13.0,90:90,-4:
corridor_mid:30.5,13.0,90:90,-4:
corridor_end:35.0,13.0,90:90,-4:
home_arch_entry:54.4,12.6,90:90,-2:
home_main_north:61.0,9.4,0:0,-2:
home_balcony_east:59.6,12.5,90:90,-6:
ceiling_office:13.5,3.5,45:45,55:
"

failures=0
for view in $VIEWS; do
    name="${view%%:*}"
    rest="${view#*:}"
    spawn="${rest%%:*}"
    rest="${rest#*:}"
    camera="${rest%%:*}"
    pause="${rest#*:}"
    set -- env PLACES_BENCH=1 PLACES_BENCH_NOSWAP=1 PLACES_LEVEL=places_demo
    if [ -n "$spawn" ]; then
        set -- "$@" PLACES_SPAWN="$spawn"
    fi
    if [ -n "$camera" ]; then
        set -- "$@" PLACES_CAMERA="$camera"
    fi
    if [ "$pause" = "1" ]; then
        set -- "$@" PLACES_PAUSE=1
    fi
    set -- "$@" PLACES_CAPTURE="$OUT_ABS/view_${name}${SUFFIX}.png" "$BIN"
    if ! "$@" >/dev/null 2>&1; then
        echo "FAILED $name" >&2
        failures=$((failures + 1))
    fi
done

echo "captures written to $OUT (suffix '${SUFFIX}')"

[ "$failures" -eq 0 ]
