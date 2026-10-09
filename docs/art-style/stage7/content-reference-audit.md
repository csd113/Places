# Stage 7 content and reference audit

Entry: `26486b8538424f013c243ae6edea8720ac07d7f2`, existing Art-style checkout.
Content/asset/tool writes are frozen. The primary owns all Cargo jobs, map
compilation, bakes, native visual acceptance, Git and publication.

The [complete inventory](content-inventory.json) records **50 JSON sources**:
five shipped, five local and forty fixtures, including nested invalid and repair
controls. Each row has classification, source hash, generation ownership,
catalog/dependency references, scalar-affected models and exact compilation,
package-validation, currency and geometry commands. These map commands are
planned gates; their presence is not a claim that they have run.

## Exact content preservation

All 48 existing sources remain byte-identical to entry, including the original
hero and every staged arrangement, demo outdoor route, Winter weather/sky,
navigation/gameplay definitions and all **218** current Zoo placements. The
unchanged Zoo preserves all 217 pre-Stage6 placements and its one additive
refined-chair display. Catalog bytes are unchanged; no registration is added or
removed. The original dining-chair GLB and exact builder function remain intact.
No PNG, immutable concept, source texture, atlas, UV contract or collision field
changes.

Historical local Home remains a valid core-furniture control (16 props). The
separate Home fixture remains the completed Home kit (18 props). Fixed core
showcase/stress fixtures retain 39/152 placements, including legal sunk and
overlapping instances. Hero variants retain their original comparison roles;
invalid/geometry-invalid and planted defect/repair sources retain their negative
test purposes. These controls are not obsolete content to replace with new art.

The independent recursive asset-ID walk resolves every authored reference to the
**353-entry** catalog. `capacity_beyond_former_limits` deliberately has **81,582**
synthetic `cap:*` IDs under the pre-existing explicit capacity exception, pinned
by `src/assets/tests.rs` and `src/zoo_audit.rs`. No new skip or exception is added.
`capacity_sparse.json` is retired and absent; documentation now names the actual
maintained capacity sources.

## Restrained scalar adoption

The approved eleven model exports use the existing `Mesh.material` API:

| Family | Models | Roughness / metallic |
| --- | --- | --- |
| Varnished timber | Home coffee table, dining table, additive refined chair | .72 / 0 |
| Satin resin | Pool table and chair | .78 / 0 |
| Chrome | Pool ladder rails, deck flanges and tread bodies | .48 / .65 |
| Ice | Winter short, medium, long, mixed-cluster and sparse-cluster icicles | .55 / 0 |

Ladder boots/grips retain matte rubber, 1 / 0; snow and imported completed
Outdoors bases retain their existing response. Fabrics and unselected natural
stocks remain deliberately matte. Original dining chair is the immutable
comparison asset. Unsupported model normal maps/tangents remain a documented
limitation; no texture or broad geometric regeneration is introduced.

[Scalar receipt](content-scalar-adoption.json) proves exact decoded positions,
UVs, colours, triangle indices, bounds, embedded PNG bytes and non-scalar
triangle-material response for every changed GLB. Catalog sizes/solid flags and
all placement/size/scale/yaw/collision fields remain exact. Container metadata and
named material groups change where the established exporter requires them.
The same 2,040 triangles remain across these models; archive bytes increase
760,348 → 768,188 (+7,840). Ladder has two material families over the same 608
triangles, with 496 chrome and 112 rubber triangles. Native draw/material cost is
left to the primary's actual acceptance receipts rather than inferred from bytes.

## Review-map source recovery

The two ignored package-only maps are retained, with [exact recovery receipts](content-recovered-local-sources.json).
`levels/blizzard_review.json` and `levels/snowfall_contrast.json` are identity
extractions of their original packages' hash-verified `semantics.json` bytes.
They are **recovered serialized LevelDef sources**, not claims of matching the
original pre-prepare authoring bytes. Original package/semantics hashes are pinned;
the primary preserved original archives. Historical raw JSON candidates under
debug-maps cannot establish original-source identity because these old packages
have no `build-inputs.json` record.

Current compiler validation already passes package semantics directly through
`LevelDef::from_json` and `loader::validate_level`, the same path as source loading.
Rebuild must independently compare canonical prepared semantics to these original
bytes and compare collision/navigation gameplay data. Fixture/decal alignment and
automatic-baseboard preparation must be idempotent; any difference must be
reported rather than hidden. The primary owns those serialized checks.

## Authoring validator and documentation

`tools/assets/validate.py` now checks the existing Stage5 environment presentation
and fog bounds/defaults, strict nested field names, sky ambient RGB/brightness/
ambient bounds and logical texture reference, and water attenuation 0..16.
Environment/sky omission or optional null remains accepted; numeric booleans,
non-finite values, invalid channel shapes and invalid nested null are rejected.
Sky keeps Rust's permissive unknown-field behavior. Defaults are unchanged.
Independent `tests/test_environment_validation.py` is owned by the validation
specialist; its six tests pass, including full validator wiring and missing sky
catalog references. No test or canonical renderer guide was written by content.

The two level README files now identify all five shipped packages, real capacity
sources, explicit compiled-map launch, generator versus historical source
ownership, completed Winter ice/snow content and actual export behavior.
`tools/package.sh` copies existing compiled drop-in maps as well as bundled maps;
raw JSON is never playable discovery or packaged source. The fixture builder's
docstring now describes its fixed core witness set rather than one shipped map
or every catalog prop. No generator output changes.

## Capacity-generator adoption correction

The primary's later generator gate found `capacity_dense` stale while
`capacity_beyond_former_limits` remained current. [Read-only investigation](content-capacity-generator-investigation.json)
shows why blindly regenerating would be a regression: catalog expansion from
48 to 205 models changes footprint rejection, cyclic model assignment and global
random-number consumption. Its unrestricted output would remove 1,624 existing
IDs and change 3,670 retained placements, shrinking 5,312→3,689 props and
2,760→774 solid prop boxes. That fails the existing dense witness's >5,000
instance and >2,000 collision-box pressure contracts. All non-prop arrays
already match; reducing those budgets or deleting content is unnecessary.

The bounded root fix pins the original 48 authoring `{id,size,solid}` witness
specs in `tools/levels/build_capacity_fixtures.py`, validating that every ID is
still a current catalog prop/entity with a model registration. They are explicit
level placement inputs, not replacement catalog entries. Missing registrations
raise a named error rather than fall back to placeholders. Existing actual GLBs
continue to supply model geometry. Model Zoo owns current all-catalog coverage.
The historical Home ball-light non-solid placement footprint stays 0.2×0.8×0.2;
catalog dimensions stay 0.38×0.8×0.38. Skeleton's historical 0.427 dimension already
matches the current catalog's 0.4266 after the generator's existing rounding.

No fixture is regenerated or edited. [Fix receipt](content-capacity-generator-fix.json)
proves exact dense source bytes, all 5,312 placements, 2,760 solid props, 48 models,
120 fixtures and 18 routes; both maintained fixture checks exit 0. The active
package campaign therefore consumes exactly its previously pinned sources.
Validation owns independent exact-output, catalog-expansion and missing-ID
refusal tests. The primary owns the serialized prose correction for stale
“every registered model” claims in the canonical guide's capacity table and
the level README; the content owner does not edit canonical guidance.

## Demo wall-joint audit

The [full-asset CPU planner receipt](content-demo-repair-plan-current-assets.json)
returns **zero findings, zero edits, zero confirmed errors** (12 heuristic warnings)
on the exact entry demo SHA `65833e98579a696167d616ce0cc94ebab85f744fdd72837ed247c483db1f9dec`.
The earlier regression's wall31/37, 0.15 m comment and accept-either-state assertion
were historical. The current regression explicitly requires zero joints and zero
confirmed errors. Current raw wall31 is the bedroom
wall at x70.55; wall37 is the Home post at x58.25. There is no current joint to move,
so demo source remains unchanged. The primary's final compiler/checker will verify
this independently using its current binary.

An initial planner run accidentally selected the hero snapshot's incomplete
asset root and emitted missing-model placeholders. That run was rejected and
stopped (exit143); it produced no accepted plan or source edit. The corrected
CPU-only run explicitly selected the full repository asset root and exited0.
Both task-owned processes are joined; no bake or native render was performed.

## Ledger resolutions and remaining gates

| Exact ledger item | Content disposition |
| --- | --- |
| `home-shared-source-families` | Resolved decision: historical local Home and fixed core controls remain valid; completed fixture/demo Home already uses completed kit. |
| `model-scalar-adoption` | Implemented approved families and exact preservation proof; native/package adoption gate remains primary-owned. |
| `normal-detail-model-support` | Deliberate unsupported limitation retained; no required material path is missing. |
| `stage5-environment-and-surface-adoption` | Existing authored values/defaults preserved; Python authoring parity fixed. Supported-map package/native validation remains primary-owned. |
| `stage6-refined-chair-catalogue-adoption` | Existing catalog identity and unchanged additive Zoo placement retained; final Zoo compile/currency/native proof remains primary-owned. |
| `package-home_showcase`, `package-geometry_intentional`, `package-level0_pit` | Exact source/reference validity established. Their stale dependency archives must be rebuilt, not removed. |
| `all-theme-final-rebake`, `runtime-spawn-package-dependencies` adoption | Inventory provides every supported source and dependency path. Final current compiler closure/format/package gates remain primary-owned. |
| `hero-open-garden-warning` | Existing inaccessible raised-window garden/control warning is preserved; no playable-space workaround or suppression. |
| `documentation-renderer-limits`, `outdoors-low-wording`, `geometry-check-split-face-centroid` | Owned by renderer/primary/validation specialists; content adds no workaround or conflicting guide edit. |

Executed content checks: selective canonical prop export exits0; full shipped
GLB check exits0 (the tool's check mode inspects all205 placeables even with
`--only`); global asset validator exits0 before/after parity additions; the
[reproducible content audit](content-audit.py) exits0 on all preservation/reference
assertions. Required Clippy, workspace tests, all-map builds, package integrity/
currency, geometry findings, recovered semantic/gameplay equality and native
appearance are still the primary/validation gates. No content owner result
claims those pending checks passed.

Exact entry bytes/hashes were saved before every source/asset/doc/tool mutation
under `debug-maps/art-style-hero/evidence/stage7-entry/content/`, indexed by
`content-entry-preservation.json`; the primary also preserved source/package
archives under its sibling maps directory. All content ownership is released to
the primary after this handoff; there are no outstanding content worker jobs.
