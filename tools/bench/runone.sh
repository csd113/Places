#!/bin/sh
# Run one benchmark scene on the PocketCHIP and print the game's own output.
#
# Everything happens inside the temporary benchmark directory; the installed
# Vitrallis shell, Xorg, /boot and the DTB are never touched. The game writes
# its settings.json relative to the working directory, so running from
# /tmp/liminal-benchmark also keeps the user's real preferences untouched.
#
# Usage: runone.sh <level-id> <label> [extra env assignments...]
set -e

ROOT=/tmp/liminal-benchmark
cd "$ROOT"

LEVEL="$1"
LABEL="$2"
shift 2

export DISPLAY=:0
export XAUTHORITY="$HOME/.Xauthority"
export XDG_DATA_HOME="$ROOT/.xdg/data"
export XDG_CONFIG_HOME="$ROOT/.xdg/config"
export XDG_CACHE_HOME="$ROOT/.xdg/cache"
export LIMINAL_LEVEL="$LEVEL"
export LIMINAL_BENCH=1
export LIMINAL_BENCH_OUT="$ROOT/out/${LABEL}.csv"
export LIMINAL_BENCH_WARMUP="${LIMINAL_BENCH_WARMUP:-60}"
export LIMINAL_BENCH_FRAMES="${LIMINAL_BENCH_FRAMES:-600}"

mkdir -p "$ROOT/out" "$ROOT/.xdg/data" "$ROOT/.xdg/config" "$ROOT/.xdg/cache"

exec env "$@" ./liminal-rust
