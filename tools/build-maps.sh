#!/bin/sh
# Export the curated distributions explicitly, including every project sub-map.
set -eu
REPO=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$REPO"
cargo build --locked --release --bin places-compile
exec python3 tools/levels/compile.py "$@"
