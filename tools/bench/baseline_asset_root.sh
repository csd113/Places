#!/bin/sh
# Builds a scratch asset root whose files match the committed baseline revision.
#
# The working tree's assets can be mid-edit while renderer work continues: a
# capture taken now may legitimately differ from `docs/renderer-baseline/`
# because a texture or model changed, not because the renderer did. This script
# copies `assets/` and restores every file that differs from HEAD, so a capture
# or a comparison can be run against the exact asset tree the baseline was
# captured from:
#
#     sh tools/bench/baseline_asset_root.sh
#     PLACES_ASSET_ROOT="$PWD/target/agent-work/baseline-assets/asset-root" \
#         sh tools/bench/capture_baseline_views.sh
#
# Each capture run should use its own scratch asset root, so the artefacts of
# different runs never alias each other:
#
#     PLACES_BASELINE_ASSET_ROOT=target/agent-work/reference-assets/asset-root \
#         sh tools/bench/baseline_asset_root.sh
#
# Environment:
#   PLACES_BASELINE_ASSET_ROOT  output directory (default
#                               target/agent-work/baseline-assets)
#
# The script never touches the repository's own `assets/` tree.
set -eu

REPO=$(cd "$(dirname "$0")/../.." && pwd)
cd "$REPO"
ROOT="${PLACES_BASELINE_ASSET_ROOT:-target/agent-work/baseline-assets}"

rm -rf "$ROOT/asset-root"
mkdir -p "$ROOT/asset-root"
cp -R assets "$ROOT/asset-root/assets"
git status --porcelain assets/ | while read -r code file; do
    mkdir -p "$ROOT/asset-root/$(dirname "$file")"
    git show "HEAD:$file" > "$ROOT/asset-root/$file"
done

echo "baseline asset root: $ROOT/asset-root"
