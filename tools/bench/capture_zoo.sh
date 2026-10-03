#!/bin/sh
# Captures the Model Zoo walkthrough views for the run's visual evidence.
# Usage: sh tools/bench/capture_zoo.sh [output-dir]
set -eu
DIR=${1:-target/agent-work/zoo-captures}
BIN=${PLACES_BIN:-target/release/places}
QUALITY=${PLACES_QUALITY:-high}
mkdir -p "$DIR"
run() {
    name=$1
    spawn=$2
    camera=${3:-}
    PLACES_BENCH=1 \
    PLACES_QUALITY=$QUALITY \
    PLACES_LEVEL=model_zoo \
    PLACES_SPAWN="$spawn" \
    PLACES_CAMERA="$camera" \
    PLACES_CAPTURE="$DIR/$name.png" \
    PLACES_CAPTURE_FRAME=4 \
    "$BIN" >"$DIR/$name.log" 2>&1
    echo "wrote $DIR/$name.png"
}
# Yaw convention: 0 = north (-Z), 90 = east (+X), 180 = south (+Z), 270 = west.
# spawn, overview of the floor rows from the aisle looking north across them
run zoo-00-overview "24.6,129.0,0"
# the mannequin pose row (stand / arms up / arms forward)
run zoo-01-mannequin-row "18.6,125.6,0"
# the skeleton row, including the real chair
run zoo-02-skeleton-row "36.6,125.6,0"
# the tabletop setting and the CRT television
run zoo-03-table-setting "42.6,17.5,0" "0,-8"
# the wall-mounted displays (switches, wall cabinet) and the exit sign
run zoo-04-wall-mounts "25.9,3.5,0"
# the pool basin, water, duck, ladder and guardrail
run zoo-05-pool-basin "7.6,129.0,0" "0,-25"
# the animated lane (rat walking and running, Spooner-Man)
run zoo-06-animated-lane "24.0,129.0,0"
# the curved walls and circular pillars
run zoo-07-architecture "13.1,16.5,0" "0,8"
# the exit sign and hanging ball light
run zoo-08-ceiling-props "24.6,116.0,180" "180,28"
# the ceiling vent decals on the room's own panel grid
run zoo-09-ceiling-vents "15.6,70.0,0" "0,55"
