#!/bin/sh
# Shared Rust 1.99 gate for CI and desktop verification; run from the repo root.
set -eu
cargo fmt --all --check
cargo check --locked --workspace --all-targets --all-features
# Cargo.toml owns the complete lint policy; do not duplicate its flags here.
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# Also lint cfg(not(debug_assertions)) paths used by the shipped executable.
cargo clippy --locked --release --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
