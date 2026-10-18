#!/bin/sh
# Shared Rust gate; run from the repo root. Optional stage names expose CI timings.
set -eu
run_stage() {
  stage_name=$1
  shift
  stage_started=$(date +%s)
  printf '\n%s\n' "Rust gate: $stage_name"
  "$@"
  stage_elapsed=$(($(date +%s) - stage_started))
  printf '%s\n' "Rust gate: $stage_name passed in ${stage_elapsed}s"
}
run_selected() {
  case "$1" in
    format) run_stage format cargo fmt --all --check ;;
    check) run_stage check cargo check --locked --workspace --all-targets --all-features ;;
    clippy) run_stage clippy cargo clippy --locked --workspace --all-targets --all-features -- -D warnings ;;
    clippy-release) run_stage clippy-release cargo clippy --locked --release --workspace --all-targets --all-features -- -D warnings ;;
    test) run_stage test cargo test --locked --workspace ;;
    build)
      run_stage build cargo build --locked --release --workspace --all-features
      run_stage luau-release python3 tools/verify-luau-release.py
      ;;
    *) printf '%s\n' "Unknown Rust gate stage: $1" >&2; exit 2 ;;
  esac
}
case "${1:-all}" in
  all)
    for stage in format check clippy test clippy-release build; do run_selected "$stage"; done
    ;;
  *) run_selected "$1" ;;
esac
