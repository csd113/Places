#!/bin/sh
# Authoritative desktop gate; see docs/VERIFICATION.md. Run from the repo root.
set -eu
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo
cargo test --workspace --all-features
python3 tools/assets/validate.py
# The package suite executes the texture CLI --check once.
python3 tools/props/build.py --check
# The bundled packages must be current for their sources and must decode.
cargo run --quiet --release --bin places-compile -- build assets/levels/places_demo.json
cargo run --quiet --release --bin places-compile -- build assets/levels/model_zoo.json --workers 8
cargo run --quiet --release --bin places-compile -- validate assets/levels/places_demo.placesmap
cargo run --quiet --release --bin places-compile -- validate assets/levels/model_zoo.placesmap
python3 -m unittest tests.test_package
python3 -m unittest tests.test_packaging tests.test_glb_accessors
python3 -m unittest tests.test_tool_execution tests.test_zoo_generator tests.test_bench_metrics tests.test_lightmap_harness
cargo build --release
python3 -m unittest tests.test_compiled_build
python3 -m unittest tests.test_wgpu_bootstrap
git diff --check
