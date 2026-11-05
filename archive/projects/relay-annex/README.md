# Relay Annex

A commissioning campaign with three connected internal sections. Explore shows
one Relay Annex entry; Intake, Tower and Discharge remain available through
their links. Talk to the
night staff, clear intake, climb four connected service floors, restore the
observation circuit and recover the last passenger from discharge.

WASD moves, E talks/picks up/sets down/opens doors, left click throws, F toggles
the flashlight, F1 gives the current hint. Enter dismisses conversations and
restarts after the ending. A noticeboard restarts the current visit. The west
Discharge noticeboard returns to Intake. Door transitions deliberately create
fresh local state and clear hands; persistent campaign inventory is unsupported.

Intake returns an unheld required box lost in the well to its starting place.
Use the well sign to recover it from another inaccessible place or retry after
clearing an occupied return point. Recovery preserves intake progress and other
props; after acceptance the sign does not move the delivered box. Falling into
the well still restarts the visit.

From the repository root, the preserved delivery launches directly:

```sh
./local/relay-annex-0.12.0/play.sh
./local/relay-annex-0.12.0/editor.sh
```

To compile a fresh package using your built tools (requires native Metal on Mac
for the compiler's offline probes):

```sh
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places-compile export-project \
  projects/relay-annex --out /tmp/Relay-Annex.placesmap --workers 6
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places --import /tmp/Relay-Annex.placesmap
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=relay_intake ./target/release/places
```

Full lighting is required throughout the campaign for its tower circuit and
consistent airlock policy. The game uses only installed catalog assets; the complete package contains the prepared maps and
exact editable sources, scripts and native screenshot thumbnails. No routine
Cargo test or CI job bakes this campaign. The report and final receipt are
`docs/reports/release-0.12.0.md#editor-0120-relay-annex` and
`local/relay-annex-0.12.0/handoff.json`.
