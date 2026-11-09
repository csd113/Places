# Archived Places projects

These five projects left the active rotation during 0.13.1 development. Every
file was moved unchanged from source commit
`68136b44f14f3abca83a0699e18f3e9032f9672f`: five project manifests, seven level
JSON files, seven Luau modules, three thumbnails and three original READMEs.
The original READMEs describe their delivery stage and may name their former
`projects/` paths or local evidence that is absent from a fresh checkout.
From the repository root, `shasum -a 256 -c archive/projects/SHA256SUMS` verifies
all 25 preserved files against their pre-archive bytes.

| Project | Preserved folder | Former folder |
| --- | --- | --- |
| Closing Time | [closing-time/](closing-time/) | `projects/closing-time` |
| One Guest Too Many | [one-guest-too-many/](one-guest-too-many/) | `projects/one-guest-too-many` |
| The Shape of an Exit | [shape-of-an-exit/](shape-of-an-exit/) | `projects/shape-of-an-exit` |
| After Hours - Rooms Beyond | [untitled-1792640069720/](untitled-1792640069720/) | `projects/untitled-1792640069720` |
| Relay Annex | [relay-annex/](relay-annex/) | `projects/relay-annex` |

Beam stays active with all four chapters. Backrooms, the nine-map Quiet Collection,
the bundled menu and development controls remain in `projects/`.

## Dependencies and discovery

Keep the repository's `assets/` library with these sources: catalog, gameplay
profiles, models, textures and audio are shared with active projects. No shared
asset, unique project file, compiled map or existing user import was deleted.
All local Luau modules and Relay Annex's three thumbnails remain beside their
manifests. Relay Annex's internal links remain intact. Active project transitions
do not target any archived member.

The archive is tracked source, outside `levels/lineup.txt` and the active
[`tools/release-projects.json`](../../tools/release-projects.json) allowlist.
With the later Long Way Home addition, the release inventory has four packages,
17 maps and 34 Static/Full variants.
The levels ZIP requires exactly that inventory. Desktop packagers also reject
the archived package names, project IDs and level IDs, including renamed bundles
and standalone exports of archived members. The ordinary player still discovers
installed `.placesmap` packages and supports existing imports; it does not scan
these JSON folders. CI's engine-only checkout excludes `archive/`. Source
downloads retain these tracked folders for recovery. Cargo metadata is unchanged
because it participates in the compiler preparation fingerprint; this archive
operation does not make prepared active maps stale. No automatic job builds
these maps.

Project-specific authoring/evidence tools remain in `tools/authoring/`.
Closing Time's two source-reading tools now use the archive path. Its recipe
helpers also serve the active Beam and Shadow Gallery tools, so retain them.
Historical reports and ignored local evidence remain at their original paths.

## Recover an editable working copy

From the repository root, copy a project to a new destination. `copytree` refuses
an existing destination, avoiding a merge into unrelated work:

```sh
python3 -c 'import shutil,sys; shutil.copytree(sys.argv[1], sys.argv[2])' \
  archive/projects/closing-time /tmp/places-closing-time-restored
target/release/places-compile check-project /tmp/places-closing-time-restored \
  --asset-root "$PWD/assets"
PLACES_ASSET_ROOT="$PWD/assets" target/release/places-editor \
  /tmp/places-closing-time-restored
```

Use the same commands with any folder in the table. Keep level IDs, relative
script paths and thumbnail paths unchanged. An explicit `export-project` can
produce a playable/recoverable package for manual use; no export is needed to
open or edit the source. Baking is an opt-in native operation, never part of
archive verification. Existing source-bearing packages can also be recovered
with `places-compile import-project PACKAGE --out NEW_FOLDER`.

To return a project to the release rotation, move its complete folder back to
its former location, remove its `archived_projects` record/exclusion from
`tools/release-projects.json`, and add an active inventory row. Check the project,
then explicitly prepare and verify its package with the matching compiler before
release staging. Changing only a filename does not reactivate archived content.

## Development/review spaces retained

None of the ten current spaces is demonstrably obsolete. Each has distinct
editable input or an active diagnostic use, so none was discarded. They stay
outside the release inventory and ordinary full-map CI baking.

| Folder under `projects/` | Current purpose and reference |
| --- | --- |
| `celestial-placement` | West/east sun and moon direction/shadow controls; [map guide](../../docs/MAP_AUTHORING_GUIDE.md), 0.13.0 native acceptance. |
| `elevator-lab` | Fixed vertical landings, cargo/checkpoint travel and rollback; `tools/bench/elevators.py`, `elevator_motion.py`, `elevator_cargo_fixture.py`. |
| `elevator-links` | Place/link, same-height and cross-level travel, door safety and NPC/held-item custody; `elevator_links.py`, `elevator_doors.py`, `predator_search.py`, current physics audit. |
| `janitor-supplies` | Static 15-model preview and portable-physics closet; `tools/authoring/environment_home_pool.py` consumes `janitor_physics.json`. |
| `mannequin-appearances` | Five appearance presets, spawn templates, animation and ragdoll comparison; current [map guide](../../docs/MAP_AUTHORING_GUIDE.md). |
| `mannequin-r2-review` | Ordered 19-clip playback plus door/carry/chair sequences, sockets and locomotion speeds; [asset specification](../../docs/ASSET_SPECIFICATION.md). |
| `movement-gym` | Stairs, slopes, step thresholds, acceleration and blocked unduck; `places-movement-check` and [map guide](../../docs/MAP_AUTHORING_GUIDE.md). |
| `performance-lab` | Bounded contact groups, actor/spawn/lifetime load and repeatable timing views; `tools/bench/performance/`, 0.13.1 acceptance. |
| `ragdoll-lab` | Wall/stair/prop contacts, eight-ragdoll stress, socket release and lifecycle/reset; `tools/bench/ragdoll.py`, [scripting guide](../../docs/SCRIPTING.md). |
| `shadow-only-review` | Hidden/visible placed and spawned shadows, >24 m range, occlusion, mirror and interaction controls; `tools/authoring/shadow_only_native.py`. |

CI and source tests primarily use the smaller `tests/fixtures/` controls; those
fixtures, all ten projects, useful tools and their unique evidence are preserved.
