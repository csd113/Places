#!/bin/sh
# Authoritative desktop gate; see docs/VERIFICATION.md. Run from the repo root.
set -eu
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo
cargo test --workspace --all-features
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 -m unittest tests.test_package
cargo build --release
python3 -m unittest tests.test_compiled_build
python3 -m unittest tests.test_wgpu_bootstrap
git diff --check
