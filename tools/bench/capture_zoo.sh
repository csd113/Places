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
run zoo-00-overview "16.8,44.0,0"
# the mannequin pose row (stand / arms up / arms forward)
run zoo-01-mannequin-row "25.2,34.5,0"
# the skeleton row, including the real chair
run zoo-02-skeleton-row "17.0,39.5,0"
# the tabletop setting and the CRT television
run zoo-03-table-setting "34.2,13.6,0"
# the wall-mounted displays (switches, wall cabinet) and the exit sign
run zoo-04-wall-mounts "20.4,9.5,0"
# the pool basin, water, duck, ladder and guardrail
run zoo-05-pool-basin "7.6,43.5,180"
# the animated lane (rat walking and running, Spooner-Man)
run zoo-06-animated-lane "24.0,47.5,0"
# the curved walls and circular pillars
run zoo-07-architecture "9.0,14.5,0"
# the exit sign and hanging ball light
run zoo-08-ceiling-props "20.4,30.0,180"
# the ceiling vent decals on the room's own panel grid
run zoo-09-ceiling-vents "16.8,28.0,0" "0,55"
