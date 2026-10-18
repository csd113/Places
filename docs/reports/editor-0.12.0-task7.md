# Stage 7: complete project export, recovery and distribution round trips

Implemented on `0.12.0-staging` after stage 6 `bfe36887538bd1a2da02625488b2b929efe0da6d`.
No maintained map, model, texture, sound, catalog or lighting revision changed.
No main merge, release, branch deletion or `.app` launch is part of this stage.

## Result

The editor exports a project containing multiple authored JSON levels, project
metadata, Luau and custom thumbnails into one `.placesmap`. It can recover exact
editable files into a new folder and reopen them. Build+Play uses the complete
project; Save+Build retains the selected-map compiler path. The player imports
one file and loads the selected level without eagerly reading other levels’
semantics, scripts, geometry or lightmaps.

`engine::package::project` owns the shared bounded project index and recovery
contract. Each level retains the ordinary manifest and prepared records;
content-identical records are stored once and selected through a virtual entry
table in the existing reader. Models, textures and sounds remain installed
built-ins, with the existing dependency hashes. No custom runtime asset importer
or second compiler GUI was introduced. All source/package/lighting versions stay
at the established reset version 1; `project-transitions-v1` explicitly gates the
new transition action.

The recovery payload contains exact bytes for the manifest, declared levels,
referenced scripts and thumbnails. Unknown fields, stable IDs, warning ignores,
trigger drafts, authored settings and module order survive because recovery
never serializes a typed level back over its source. Prepared semantics remain
separate and may omit editor-only fields. An ordinary compiled single-map package
cannot recover an authoring project and receives an explicit refusal.

## Workflow and protections

`Project >` groups metadata, adding/selecting levels, explicit entry selection,
thumbnail attachment, permitted destinations, complete export, recovery and
check/migration commands. Scene transforms remain contextual. Level selection
clears the previous object selection/scroll and frames the new scene. Project
recovery is also available directly from an empty editor. Normal 1440×816
and minimum 960×544 layouts were inspected. Thumbnail attachment validates an
existing 16:9 RGB8/RGBA8 PNG (maximum 1280×720 / 4 MiB), copies its original bytes
by hash and links it; no crop or resize occurs. The player’s thumbnail card UI
is still the separate #6 work.

Export freezes validated source bytes in owned staging, then calls the existing
compiler sequentially for every level, reusing its
structured progress, cancellation, resource preparation and atomic writer.
Progress includes the selected source/level label without repurposing the
variant field. Export rechecks source snapshots and destination identity before
publication. A failed or cancelled operation retains the previous distribution.
Recovery validates in owned staging and publishes with an atomic no-replace
rename; existing files, symlinks and even empty directories are refused.

Validation covers bounded counts/metadata, duplicate and case-conflicting IDs
and paths, unsupported schemas, confined roles, source symlink components,
archive symlink/directory/traversal entries, compressed/decompressed totals,
per-record limits and hashes, UTF-8/Luau/PNG contents, source/runtime script and
transition agreement and all project references. Player import checks every
incoming level ID, not just the entry. Updating the same project can change its
entry level. Shared engine validation now owns the editor’s preserved placement
intent and trigger-draft contracts; automatic correction behavior is unchanged.

A level declares allowed destinations in `transitions`. Ordinary bindings and
sequence steps use `change_level`; Luau uses `api.change_level`. The player
resolves only within the current physical package. Navigation loads the authored
destination spawn with fresh world/VM state; first request wins and reset cancels
pending navigation. `api.reset()` uses the existing reset dispatcher. There is
no persistent campaign-variable system.

`check-project` and `migrate-project` validate supported version-1 data. They
report that no conversion is needed and leave files untouched. Unsupported data
is refused with the expected contract; no historical conversion exists or is
claimed. See [project contract](../PROJECT_FORMAT.md) and [editor controls](../EDITOR.md).

## Verification

The Rust suite passes **122 tests**, including four added small regressions for
exact source recovery/shared identities, lazy selected loading versus full import
validation, cancellation/unsafe-index refusal, and declared atomic script
transitions/reset. They use tiny metadata/ZIP fixtures and no map bake or GPU.
Strict all-target/all-feature Clippy, formatting and the locked all-feature
production build pass. The production Luau error-boundary tool passes all three
bounded refusals; `panic = "unwind"` and the compile-time abort guard remain.
The source Python suite passes **368 tests** (three optional/native skips).
The 593-asset catalog check has zero warnings; prop checks pass. Texture checks
retain their 96 existing warnings.

The release compiler/player pass **97 explicit roundtrip/refusal assertions**:
exact source recovery and re-exported compiled-record identity, membership,
metadata, script/thumbnail bytes, repeated imports, project/secondary-ID conflicts,
unsupported/malformed/oversized indices, unsafe paths, symlink and duplicate ZIP
entries, decompression limits, corrupt hashes, invalid images/UTF-8/script syntax,
existing-folder refusal, failed exports, external source edits during a frozen build,
unsupported source migration and structured export/recovery cancellation.
Existing installed packages and editable sources retain their hashes.

Native checks pass **58 state/path assertions**, with framebuffer captures.
Direct-binary editor checks exercise create/edit/save/reopen, adding a
second level, metadata, entry selection and thumbnail attachment, Check/Migration,
export/recovery, script syntax diagnostics/correction, cancelled and failed exports,
and ordinary handler/trigger controls for forward/return transitions. The tiny
rich fixture derives from the retained stage 6 room; fixture setup is documented
in `prepare_native.py`. Transition triggers and links are authored through the
actual editor, not through a manual level-file workaround. Explicit developer
scripts drive ordinary native event handlers and retain framebuffer captures and
state checkpoints. Recovery from an empty editor also opens the recovered sources.

A copied player plus actual built-in assets under
`/tmp/places-stage7-clean-install/` runs from `/tmp`, with no asset-root override
and fresh isolated user storage outside the checkout. It starts with no installed
maps, imports one project twice, discovers both levels and plays it. Actual W-key
movement crosses the script trigger: the door opens, the light switches off and
`entered 1` is recorded. Reset restores authored state and the next crossing
again reports `entered 1`. The script-linked transition loads the second level;
its ordinary return trigger loads the entry with a closed door and fresh VM.
A further restart uses the same installed state without reimporting and repeats
the full sequence successfully. The read trace verifies the entry → second → entry sequence and physical resource
paths. Screenshots are inspected evidence, not a claim of exhaustive visual QA.
No throughput/FPS comparison was performed.

Evidence and reproducible developer tooling are retained outside `target` under
`local/verification/0.12.0-task7/`: `checks/`, `native/`, `project/`, `recovered-ui/`,
`campaign.placesmap`, `payload/`, and the launchers. Final commit, CI, payload
hashes and cleanup results are recorded in `stage7-handoff.json`.

## Rust test-count audit

The final 0.11.0 commit `d5942839` already had **67** essential Rust tests; the
cleanup retired the old expensive suite and a focused suite was restored before
0.12.0. This baseline is corroborated by the release report and the source
`#[test]` declarations at that exact commit. These are totals, not new-test counts.

| Checkpoint | Total Rust tests | Net added in editor stage |
| --- | ---: | ---: |
| 0.11.0 baseline `d5942839` | 67 | — |
| Stage 1 `af94e3f1` | 71 | 4 |
| Stage 2 `169748c4` | 74 | 3 |
| Stage 3 `816ef4e5` | 80 | 6 |
| Stage 4 `31ec68a8` (`69363d03` handoff unchanged) | 91 | 11 |
| Stage 5 `34903df8` | 109 | 18 |
| Stage 6 `bfe36887` | 118 | 9 |
| Stage 7 | 122 | 4 |

Thus stages 1–7 add 55 net tests, and this stage adds four. Source counts were
checked against each checkpoint and the stage reports; this is not a new
individual value review of all inherited tests. Stage 7’s exact additions are:

- `engine/tests/project_package.rs::project_recovers_exact_sources_and_preserves_shared_record_identity`:
  explicit entry selection, exact raw recovery and shared prepared-record identity.
- `...::selected_level_does_not_read_other_level_payloads_but_import_checks_them`:
  an intentionally corrupt inactive record cannot break selected loading, but full
  validation/recovery must reject it before publication.
- `...::project_index_and_cancellation_refuse_unsafe_or_partial_publication`:
  cancellation, unsupported index versions and unsafe virtual paths publish no
  project; invalid image bytes are refused.
- `engine/tests/scripting.rs::script_transitions_are_declared_atomic_and_cleared_by_reset`:
  a declared request reaches the existing dispatcher, an invalid later request
  discards staged navigation atomically, and reset clears pending navigation.

The archive tests use a few KiB of ZIP/JSON and a shared placeholder prepared
record, with short-lived file operations. They never bake/decode a full world or
resolve game assets. The script case adds three bounded handler calls to the
existing tiny metadata world. All four live under the owning `src/engine/tests/`
folder and are wired through `engine/tests/mod.rs`; the archived fixture test
module is explicitly registered there. The entire 122-test body remains under
one second after compilation on this host; exact duration is in the retained log.

`tools/verify-luau-release.py` performs three **production executable** refusal
checks; the native and roundtrip Python files perform explicit developer
acceptance. They are scripts, not additional Rust tests or automatic full-map
CI requirements. The retained 368-test Python source suite is another separate
count. No test-suite rewrite or deletion was performed in this stage.

## Limits and next task

Native acceptance covers macOS/Metal. Host SDL remains required; Windows/Linux
runtime acceptance, installers/signing and broad distribution stage 9 remain
outstanding. Thumbnail cards, custom runtime assets, menu scenes and persistent
campaign state are not implemented. Export rebuilds temporary ordinary packages;
it does not add a whole-campaign incremental build cache. VM deadlines remain
cooperative and in-process, with the stage 6 allocation/native-call limits.
Concurrent malicious filesystem mutation is not a promised local authoring sandbox.

Issues #18/#21 remain open for their broader acceptance criteria. The next
separately authorized task is the navigation pass, followed by editor-authored
Closing Time and three passes on It Only Exists in the Beam using currently
integrated assets. Integrate the accepted Home/shared, rubber duck and janitor-closet
assets after both maps are made, then run the dedicated Luau/map-import safety
review of both finished modes.
Stop before roadmap stages 8/9. See the [current handoff](editor-0.12.0-next-handoff.md).
