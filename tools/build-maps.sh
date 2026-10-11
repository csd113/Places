#!/bin/sh
# Compile the curated map lineup explicitly. Pass options through to compile.py.
set -eu
REPO=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$REPO"
cargo build --locked --release --bin places-compile
exec python3 tools/levels/compile.py "$@"
