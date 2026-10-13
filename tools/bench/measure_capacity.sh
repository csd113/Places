#!/bin/sh
# Capacity measurement batch, run after the test suite is quiet.
# Compile tests/fixtures/levels/capacity_dense.json first, then set
# PLACES_CAPACITY_PACKAGE to the resulting package if using a different path.
# Optional historical measurements require explicit packages:
#   PLACES_CAPACITY_ZOO_PACKAGE=/path/to/model_zoo.placesmap
#   PLACES_CAPACITY_DEMO_PACKAGE=/path/to/places_demo.placesmap
# These packages are developer inputs, outside the maintained map lineup.
set -eu
cd "$(dirname "$0")/../.."
REPO=$(pwd)
OUT=${PLACES_CAPACITY_OUT:-target/step07-08}
BIN=${PLACES_BIN:-./target/release/places}
PACKAGE=${PLACES_CAPACITY_PACKAGE:-target/verification/maps/tests__fixtures__levels__capacity_dense.placesmap}
ZOO_PACKAGE=${PLACES_CAPACITY_ZOO_PACKAGE:-}
DEMO_PACKAGE=${PLACES_CAPACITY_DEMO_PACKAGE:-}
if [ ! -f "$PACKAGE" ]; then
  echo "Compile tests/fixtures/levels/capacity_dense.json and set PLACES_CAPACITY_PACKAGE to its package" >&2
  exit 1
fi
for historical_package in "$ZOO_PACKAGE" "$DEMO_PACKAGE"; do
  if [ -n "$historical_package" ] && [ ! -f "$historical_package" ]; then
    echo "Missing explicit historical package: $historical_package" >&2
    exit 1
  fi
done
STATE="$OUT/state"
mkdir -p "$STATE/levels"
cp "$PACKAGE" "$STATE/levels/capacity_dense.placesmap"
if [ -n "$ZOO_PACKAGE" ]; then
  cp "$ZOO_PACKAGE" "$STATE/levels/model_zoo.placesmap"
fi
if [ -n "$DEMO_PACKAGE" ]; then
  cp "$DEMO_PACKAGE" "$STATE/levels/places_demo.placesmap"
fi
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
if [ -n "$ZOO_PACKAGE" ]; then
  measure zoo-warm model_zoo high zoo-warm.png
fi
if [ -n "$DEMO_PACKAGE" ]; then
  measure demo-cold places_demo high demo.png
  measure demo-warm places_demo high demo-warm.png
fi
