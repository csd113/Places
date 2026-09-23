#!/bin/sh
# Captures the Batch 3 validation views into target/agent-work/captures/.
#
# Every view is a fixed spawn and camera, so the same command produces the same
# image on any machine and the Full / Low / no-offscreen captures are directly
# comparable. Run from the repository root:
#
#     sh tools/bench/capture_batch3.sh              # Full profile
#     LIMINAL_QUALITY=low sh tools/bench/capture_batch3.sh
#     LIMINAL_NO_OFFSCREEN=1 sh tools/bench/capture_batch3.sh
#
# The suffix is built from the environment so a comparison run never overwrites
# the reference capture.
set -eu

OUT="target/agent-work/captures"
mkdir -p "$OUT"

SUFFIX=""
if [ "${LIMINAL_QUALITY:-full}" = "low" ]; then
    SUFFIX="_low"
fi
if [ "${LIMINAL_NO_OFFSCREEN:-0}" = "1" ]; then
    SUFFIX="${SUFFIX}_direct"
fi

# One line per view: <name>:<x>,<z>,<yaw>
VIEWS="
glass_office:9.5,3.5,90
glass_pool_side:12.0,9.5,0
panels_east:23.6,8.2,90
plastic_panel:6.0,10.5,0
wet_deck:21.5,11.4,0
sign_corridor:34.4,13.6,0
grille_vent:5.0,3.7,90
linoleum:13.6,1.6,90
"

for view in $VIEWS; do
    name="${view%%:*}"
    spawn="${view#*:}"
    LIMINAL_LEVEL=places_demo \
        LIMINAL_SPAWN="$spawn" \
        LIMINAL_CAPTURE="$OUT/b3_${name}${SUFFIX}.png" \
        cargo run --release --quiet
done

echo "captures written to $OUT (suffix '${SUFFIX}')"
