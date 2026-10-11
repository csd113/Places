#!/bin/sh
# Capacity/zoo measurement batch, run after the test suite is quiet.
set -eu
cd "$(dirname "$0")/../.."
REPO=$(pwd)
OUT=${PLACES_CAPACITY_OUT:-target/step07-08}
BIN=${PLACES_BIN:-./target/release/places}
PACKAGE=${PLACES_CAPACITY_PACKAGE:-target/verification/maps/tests__fixtures__levels__capacity_dense.placesmap}
if [ ! -f "$PACKAGE" ]; then
  echo "Compile tests/fixtures/levels/capacity_dense.json and set PLACES_CAPACITY_PACKAGE to its package" >&2
  exit 1
fi
for level in model_zoo places_demo; do
  if [ ! -f "assets/levels/$level.placesmap" ]; then
    echo "Missing curated package assets/levels/$level.placesmap; compile the lineup first" >&2
    exit 1
  fi
done
STATE="$OUT/state"
mkdir -p "$STATE/levels"
cp "$PACKAGE" "$STATE/levels/capacity_dense.placesmap"
measure() {
  label=$1; level=$2; quality=$3; capture=$4
  PLACES_STATE_ROOT="$STATE" PLACES_ASSET_ROOT="$REPO" \
  PLACES_VERBOSE=1 PLACES_QUALITY=$quality PLACES_LEVEL=$level \
  PLACES_CAPTURE="$OUT/$capture" PLACES_CAPTURE_FRAME=4 \
  "$BIN" > "$OUT/$label.log" 2>&1
  if ! grep -F "[loading] committed $level" "$OUT/$label.log" >/dev/null || [ ! -s "$OUT/$capture" ]; then
    echo "Requested level $level did not produce its native capture: $OUT/$label.log" >&2
    exit 1
  fi
  echo "== $label =="
  grep -E "\[level\]|\[lightmaps\] atlas|\[wgpu\] characters|\[spatial\]|startup. total|\[lighting\] baked" "$OUT/$label.log" | tail -8
}
measure dense-cold capacity_dense low dense.png
measure zoo-warm model_zoo high zoo-warm.png
measure demo-cold places_demo high demo.png
measure demo-warm places_demo high demo-warm.png
