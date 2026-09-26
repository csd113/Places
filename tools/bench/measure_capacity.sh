#!/bin/sh
# Capacity/zoo measurement batch, run after the test suite is quiet.
set -eu
cd "$(dirname "$0")/../.."
OUT=target/step07-08
BIN=./target/release/places
for level in capacity_sparse capacity_dense; do
  cp "tests/fixtures/levels/$level.json" "levels/_${level}_dev.json"
done
measure() {
  label=$1; level=$2; quality=$3; capture=$4
  PLACES_VERBOSE=1 PLACES_QUALITY=$quality PLACES_LEVEL=$level \
  PLACES_CAPTURE="$OUT/$capture" PLACES_CAPTURE_FRAME=4 \
  $BIN > "$OUT/$label.log" 2>&1 || true
  echo "== $label =="
  grep -E "\[level\]|\[lightmaps\] atlas|\[wgpu\] characters|\[spatial\]|startup. total|\[lighting\] baked" "$OUT/$label.log" | tail -8
}
measure sparse-cold capacity_sparse low sparse.png
measure sparse-warm capacity_sparse low sparse-warm.png
measure dense-cold capacity_dense low dense.png
measure zoo-warm model_zoo high zoo-warm.png
measure demo-cold places_demo high demo.png
measure demo-warm places_demo high demo-warm.png
for level in capacity_sparse capacity_dense; do rm -f "levels/_${level}_dev.json"; done
