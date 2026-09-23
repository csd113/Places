#!/bin/sh
# Captures the Batch 4 validation views into target/agent-work/captures_b4/.
#
# Every view is a fixed spawn and camera, so the same command produces the same
# image on any machine and the before/after, Full/Low and direct-path captures
# are directly comparable. Run from the repository root:
#
#     sh tools/bench/capture_batch4.sh                       # Full profile
#     LIMINAL_QUALITY=low sh tools/bench/capture_batch4.sh
#     LIMINAL_NO_OFFSCREEN=1 sh tools/bench/capture_batch4.sh
#     LIMINAL_NO_BLOOM=1 sh tools/bench/capture_batch4.sh
#     LIMINAL_NO_REFLECTIONS=1 sh tools/bench/capture_batch4.sh
#
# `LIMINAL_BIN` overrides the binary, which is how the same view set is captured
# from the previous batch's checkout for a before/after pair:
#
#     LIMINAL_BIN=target/agent-work/baseline-b3/target/release/liminal-rust \
#         sh tools/bench/capture_batch4.sh
#
# The suffix is built from the environment so a comparison run never overwrites
# the reference capture. `LIMINAL_BENCH_NOSWAP=1` keeps a capture from blocking
# on a display that has gone to sleep; it does not change the pixels.
set -eu

BIN="${LIMINAL_BIN:-target/release/liminal-rust}"
OUT="${LIMINAL_CAPTURE_DIR:-target/agent-work/captures_b4}"
mkdir -p "$OUT"

SUFFIX=""
if [ "${LIMINAL_QUALITY:-full}" = "low" ]; then
    SUFFIX="${SUFFIX}_low"
fi
if [ "${LIMINAL_NO_OFFSCREEN:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_direct"
fi
if [ "${LIMINAL_NO_BLOOM:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_nobloom"
fi
if [ "${LIMINAL_NO_REFLECTIONS:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_norefl"
fi

# One line per view: <name>:<spawn>:<camera yaw,pitch>:<pause>
# `spawn` is x,z[,yaw] (the eye height comes from the local floor) and `camera`
# is the `LIMINAL_CAMERA` override, which needs LIMINAL_BENCH=1 to take effect.
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
sign_corridor:34.4,13.6,0::
linoleum:13.6,1.6,90::
drum:28.4,13.6,0:0,-22:
pause_office:9.5,3.5,90::1
pause_pool:18.0,12.0,58::1
"

for view in $VIEWS; do
    name="${view%%:*}"
    rest="${view#*:}"
    spawn="${rest%%:*}"
    rest="${rest#*:}"
    camera="${rest%%:*}"
    pause="${rest#*:}"
    set -- env LIMINAL_BENCH=1 LIMINAL_BENCH_NOSWAP=1 LIMINAL_LEVEL=places_demo
    if [ -n "$spawn" ]; then
        set -- "$@" LIMINAL_SPAWN="$spawn"
    fi
    if [ -n "$camera" ]; then
        set -- "$@" LIMINAL_CAMERA="$camera"
    fi
    if [ "$pause" = "1" ]; then
        set -- "$@" LIMINAL_PAUSE=1
    fi
    set -- "$@" LIMINAL_CAPTURE="$PWD/$OUT/b4_${name}${SUFFIX}.png" "$BIN"
    "$@" >/dev/null 2>&1 || echo "FAILED $name" >&2
done

echo "captures written to $OUT (suffix '${SUFFIX}')"
