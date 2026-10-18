# Authoring projects (0.12.0)

`engine::project` owns the shared source contract for the future editor and Map
Compiler. The game continues to load compiled `.placesmap` files through
`engine::package`; it does not open authoring projects. Stage 6 executes only the Luau source snapshots carried by the selected compiled map.
Single-map JSON input remains supported.

## Folder and manifest

```text
my-project/
  places.project.json
  levels/entry.json
  levels/second.json
  scripts/                 optional project-local .luau modules
  thumbnails/              optional .png metadata references, no UI yet
  assets/textures/          reserved for later custom importing
  assets/models/            reserved for later custom importing
  assets/sounds/            reserved for later custom importing
```

Only the manifest and declared maps are required. Reserved asset folders are
conventions, not a working importer or alternate resource search path. Do not
place required runtime assets here: this stage resolves installed built-ins.

```json
{
  "schema": "places-project",
  "project_format": 1,
  "id": "quiet_rooms",
  "name": "Quiet Rooms",
  "author": "An author",
  "description": "Two connected ideas, packaged separately for now.",
  "entry_level": "entry",
  "levels": [
    { "id": "entry", "source": "levels/entry.json" },
    { "id": "second", "source": "levels/second.json" }
  ]
}
```

Project and level records optionally carry `description`, `thumbnail` (for
example `thumbnails/entry.png`) and `scripts` (for example
`["scripts/entry.luau"]`). Thumbnail references are authoring metadata only;
pixel layout, generated previews and player thumbnail UI are later work. Stage 6 resolves script references into selected-map runtime snapshots. Declared optional files must exist; see [Luau source and lifecycle](SCRIPTING.md).

The manifest rejects unknown fields and unsupported schema/version pairs. It
admits 1–256 levels, at most 256 script references in total, 256 KiB of metadata,
96-byte lowercase project/level IDs (`a-z`, `0-9`, `_`, `-`), 256-byte names and
authors, and 4096-byte descriptions. Text excludes control characters. Level IDs
must be unique and match the selected source's `LevelDef.id`. `entry_level`
must name a declared level. Level IDs and source paths must also be unique under
ASCII case folding to prevent common macOS/Windows output collisions.

`ProjectPath` validates both deserialization and direct construction. Paths use
`/`, stay within 240 bytes, and reject absolute/drive/device paths, traversal,
empty or dot components, Windows reserved basenames and ambiguous trailing dots
or spaces. Sources belong below `levels/` with `.json`, thumbnails below
`thumbnails/` with `.png`, and scripts below `scripts/` with `.luau`. Opening a
project canonicalizes its root and checks regular-file confinement after
resolving symlinks. Map reads are capped at the shared level-source limit;
scripts at 64 KiB and thumbnails at 4 MiB each. Bounded reads also reject growth
beyond the limit after opening. These are local authoring APIs, not a sandbox
for untrusted concurrent filesystem mutation.

## Assets and material definitions

Maps keep their existing stable logical IDs: `core:chair`,
`core:carpet_beige_01`, `spooner-man`, and audio cue IDs. Do not serialize runtime
table indices or physical installed asset paths into authored references.
`BuiltinAssetRef { kind, id }` validates material, texture, model, fixture, decal
and sound identities against `AssetCatalog` / `AudioCatalog`. A material ID
cannot resolve as a model; `pack:` remains reserved and unsupported here.

`validate_level_assets` uses the engine's `referenced_material_ids` set and
the same catalog type checks as `MaterialTable`. The compiler runs this after
its existing `prepare_level` pass, so generated trim uses those same rules.
The editor must call these APIs instead of copying default-material, face,
fixture-alignment or baseboard rules. Existing semantic validation owns audio
action references. Existing package dependency hashing owns installed resource
identity; this contract adds no custom texture/model/sound importer.

## Compiler API and commands

```sh
places-compile build ./my-project --asset-root ./assets
places-compile build ./my-project/places.project.json --level second --out /tmp/second.placesmap --asset-root ./assets
places-compile verify ./my-project --level second --package /tmp/second.placesmap --asset-root ./assets --require-current
places-compile build-project ./my-project --out /tmp/existing-output-directory --asset-root ./assets
places-compile build levels/office_demo.json --out /tmp/office.placesmap --asset-root ./assets
```

`build` defaults to the entry level and its source's sibling `.placesmap`.
`--level` is exact and is accepted only for project build/verify. A missing ID
fails; it never falls back to another map. The existing GUI remains a standalone
single-JSON interface. `build-collection` keeps its existing directory behavior.

`BuildRequest.project_level` carries selection. `project::output_path` resolves
the default output. `project::build_plan` validates all declared sources before
returning one request per level, in manifest order, targeting `<id>.placesmap`
inside an explicit existing output directory. `build-project` executes those
ordinary builds sequentially and stops on failure. Each package is published
atomically; the group is not a transaction. Already successful outputs survive
a later failure. Project metadata and selected source bytes participate in the
compiler fingerprint. Metadata changes cannot silently reuse a stale package.

This is the foundation for multi-level export, not a player-loadable project
bundle. Compiled packages embed selected-map script snapshots, but do not yet embed complete project membership or thumbnail metadata. Group packaging, transitions and clean installed-player
round trips remain the later export stage.

## Deliberate pre-release reset

The source schema now requires `schema: "places-level"` and `format_version: 1`.
Compiled manifests require `schema: "places-map"` and `package_format: 1`.
The schema markers distinguish these contracts from historical pre-release
version-1 files. Binary record signatures also identify the current layout;
old files must be recompiled, never guessed or migrated at runtime.

| Revision family | Previous | Current | Role |
|---|---:|---:|---|
| Authored/semantic level | 3 | 1 | `LevelDef` JSON schema |
| Package manifest | 1 | 1 + schema marker | Archive index and capability contract |
| Mesh / props / collision | 3 / 9 / 2 | 1 each | Current prepared binary layouts |
| Compiled lighting / irradiance field | 4 / 4 | 1 each | Runtime light-source and probe-field records |
| Lightmap metadata / reflection positions | 4 / 3 | 1 each | Atlas chart and HDR cube metadata |
| Navigation / shared-prop recipe / compact atlas | 1 each | 1, fresh signatures | Bounded prepared binary layouts |
| Lightmap cache / solver / geometry | 15 / 21 / 16 | 1 / 1 / 3 | Offline rebuild identity, not extra source schemas |
| Compiler build inputs / checker / repair plan | 1 each | 1 | Provenance and developer diagnostic protocols |
| Compiler asset-input / lighting-stage key | 2 / 3 | 1 each | Compiler cache protocols, with structural changes owned by solver/geometry revisions |

KTX2 container identifiers, GPU vertex layouts, GLB versions, asset catalog
format 2, audio/gameplay format 1, settings, and game release/history versions
are unrelated contracts and retain their current values. The Model Zoo's model
inspection cache version 2 and alignment repair plan version 1 also retain
their separate tooling roles. Entity/door change revisions are live cache
generations, not serialized format versions. There is no migration
revision counter or functional upgrader. A future editor upgrade boundary should
read the schema/version before loading an editable document, refuse unsupported
inputs without writing, and introduce a named, explicit conversion only when
there is an actual supported upgrade. No placeholder claims to upgrade files.

Tracked maintained maps, archived source examples and deterministic generators
were migrated explicitly by changing their schema/version headers. Geometry,
assets and authored behavior were preserved. Existing untracked unique sources
are not rewritten. Old compiled outputs and offline caches are stale; rebake
the real maps explicitly when needed. Ordinary Rust builds/tests/CI do not bake
the lineup.

## Handoff

Stage 2 (installation and path discovery): project roots are explicit canonical
authoring folders, separate from installed read-only resources and writable
player data. Reuse `engine::assets` resource APIs and the package loader for the
installation policy; do not infer state storage from `ProjectFolder.root()`.
`ProjectFolder::open`, `root`, `manifest`, `resolve` and `read_level` are the
authoring boundaries. No new global environment override was introduced.
The shared `engine::paths` API owns installation/user storage. Both compiler
`--asset-root` and `PLACES_ASSET_ROOT` accept the directory containing
`catalog.json` (canonical) or the historical parent containing `assets/`.
Installed resources and platform user folders are separate; launch cwd is
preserved. See [installation paths and overrides](INSTALLATION.md). Direct
compiler `build` calls pin `BuildRequest.asset_root` before resource use and
reject a different already selected process tree.

Stage 3 (editor save/compile/play): retain one `ProjectManifest` plus engine
`LevelDef` documents, validate them before atomic saves, retain unknown/unsupported
source data safely until the editor can handle it, and call the existing
compiler process/progress APIs. Use `BuildRequest.project_level`,
`project::output_path` and `project::build_plan` for selection/output planning.
Play the resulting ordinary package through the native game's existing import
and loading path. Source JSON, project manifests and compiled outputs are
different documents; do not let Save overwrite a compiled package.
Use `paths::initialize_user()` and `UserPaths::projects()` for the default new
project location, then retain the user's explicit chosen project root. The
installed compiler discovers its own resources. `places --import <package>` and
`places --list-levels` provide the same import/discovery path without a window.

The implemented basic editor retains raw scene values alongside validated
`LevelDef` views, with per-level history, conflict-checked atomic saves and
explicit refusal of unsupported/lossy input. Compiler `build --events` exposes
structured progress/results and stdin cancellation. See [EDITOR.md](EDITOR.md)
for the save/build/play loop and remaining scope. No schema revision changed.

## Task-1 validation (2026-10-17)

The workspace passed 71 Rust tests, locked checks, strict debug/release
all-target/all-feature Clippy, the production all-feature build, 368 Python
tests (four native/optional skips) and the 593-asset catalog check. The four new
Rust regressions use small source/metadata records and a one-probe binary record;
they do not bake a shipped map.

A generated 4 m room with walls, built-in materials, a chair and a light was
compiled under two project IDs into Static and Full packages, without warnings
or lightmap fallback. Both validated and verified current. Ten direct compiler
checks covered project selection, standalone JSON, metadata invalidation,
failed-plan output preservation and explicit rejection of a superseded schema.
The release player loaded and captured the Full room on macOS/Metal when run
directly from `/tmp` with explicit repository assets and isolated state. This
is a native format/load smoke, not clean-install, platform or performance
acceptance. Existing real-map packages were preserved and reported that they
require recompilation; the full lineup was not baked.


Stage 4 retains the exact manifest and raw level contracts. Ramps and regions use
existing arrays; openings are explicit room strips rather than a new portal
schema. Upper floors are ordinary elevated rooms. Geometry revision 2 corrects
carrier floors and skirts around descending ramps and invalidates prepared
geometry, without changing source or package record layouts. The editor's safety
candidates are memory-only raw snapshots until explicitly applied and saved.
See [geometry authoring, safety and limitations](EDITOR.md).

## Stage 5 gameplay and editor metadata

Gameplay retains the shared typed level collections. Prop `pitch_degrees` and
`roll_degrees` supplement `rotation_degrees`; geometry revision 3 requires
recompiling prepared output for the full orientation. `sound_emitters[]`,
`ground_fog[]`, door `bidirectional` and exact `entity` trigger filters are
additive source fields. Built-in logical asset IDs remain authoritative; no
custom importer is introduced; stage 6 adds the separate bounded Luau contract.

The editor preserves optional `editor_instance_id` fields on individual raw
records and a level-level `editor_placement_intent` object with `version: 1`
and at most 256 `ignores`. Each decision records stable instance IDs, a specific
check/issue signature, contextual source hash, bounds and review label. These
are authoring review decisions, never exemptions from engine validation. The
editor rejects duplicate identities, malformed metadata and unsupported versions.
Material changes to the placement or affected geometry make a decision stale;
the saved record remains reviewable and undo can restore its original context.
Compiler/player behavior never interprets an ignore as permission to move scenery.

An empty trigger may retain `editor_trigger_policy` with its pending enter/exit
event, one-shot and cooldown settings. The editor validates this draft and
materializes a normal binding when its first action is added. Removing the final
action restores a draft; an empty runtime action binding remains invalid. Drafts
produce no runtime action. Both metadata forms survive raw save/reopen and
history while the compiled semantic level omits them.

## Stage 6 selected-map scripts

A level JSON may add `scripts: ["scripts/entry.luau"]`. Project-wide and selected
manifest-level references remain supported. Compiler input resolves their union
through `ProjectFolder`, validates bounded UTF-8 and exported handler links, and
embeds compiler-owned `script_sources: [{path, source}]` in that map’s semantics.
Authored inline snapshots are refused. Exact source bytes join the build identity.
The player never reads these paths from disk. `run_script` is an ordinary shared
action with `script` and `handler`; event policies and action order remain intact.
No format version changed. Old un-scripted maps remain valid; older binaries do
not implement these new actions. Complete multi-level distribution remains stage 7.
