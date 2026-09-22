# Places

Places is a standalone home for the Liminal PocketCHIP walking game, its assets,
level editor, development tools, tests, and Vitrallis package metadata.

The installable package remains at `apps/liminal-rust` so Vitrallis App Center
can continue to use its existing catalog-v1 package path and stable application
ID. Run the game workspace from this repository root:

```sh
cargo check
cargo test
cargo build
```

See [`apps/liminal-rust/README.md`](apps/liminal-rust/README.md) for game,
runtime, and PocketCHIP details.
