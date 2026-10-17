# Places documentation

Start with [developer orientation](DEVELOPMENT.md), then follow the guide for
what you are changing:

| Task | Canonical guide |
| --- | --- |
| Understand engine ownership and data flow | [Architecture](ARCHITECTURE.md) |
| Understand rendering and diagnostics | [Renderer](RENDERER.md) |
| Author maps, placement, lights and materials | [Map authoring](MAP_AUTHORING_GUIDE.md) |
| Create or replace PNGs, models and source masters | [Asset specification](ASSET_SPECIFICATION.md) |
| Select a generator or offline check | [Python tooling](OFFLINE_TOOLING.md) |
| Run source, package or native acceptance | [Verification](VERIFICATION.md) |
| Inspect archive formats and safety limits | [Package format](PACKAGE_FORMAT.md) |
| Work with authoring project folders and compiler selection | [Project format](PROJECT_FORMAT.md) |
| Install resources, locate user data and import maps | [Installation and paths](INSTALLATION.md) |
| Diagnose movement | [Movement diagnostics](MOVEMENT_DIAGNOSTICS.md) |

[Environment profiles](assets/ENVIRONMENTS.md) and [entity profiles](assets/ENTITIES.md)
explain kit-specific contracts. The [map index](../levels/README.md) lists eight
active sources and their owners; [regression scenes](../tests/fixtures/levels/README.md)
are separate developer controls. Places Demo and Model Zoo remain frozen JSON in
`archive/levels/`, outside discovery and default compilation.

## Project evolution and visual history

Historical reports describe the version they measured. They do not establish
current package currency, test counts or platform performance.

- [0.11.0 prerelease summary](reports/release-0.11.0.md): completed release queue,
  exact-head verification, issue completion, source delivery and remaining limits.
- [0.11.0 developer console](reports/developer-console-0.11.0.md): reusable typed
  actions, curated pickups, simulation controls and model representation.
- [0.11.0 fixture audit](reports/fixture-leaks-0.11.0.md): Office closure evidence,
  procedural housing corrections and restricted Home lamp emission.
- [0.10.0 prerelease summary](reports/release-0.10.0.md): integrated gameplay,
  delivered models, compiler GUI, final checks and compatibility limits.
- [0.9.0 release summary](reports/release-0.9.0.md): final supported-package,
  empty-install and native acceptance, with remaining limitations.
- [Feature expansion](reports/feature-expansion-summary.md): movement, entities,
  geometry, capacity and measured offline tooling.
- [Winter development](reports/winter-final-audit.md): settlement, snow kit, ice,
  warm lights, aurora and sheltered weather, with curated comparisons.
- [Environment reconstruction](style-upgrade-20261007/README.md) and
  [seven Art-style milestones](style-upgrade-20261007/art-style-history.md):
  the original-to-final visual progression and bounded technical results.
- [Completed renderer integration](art-style/stage7/README.md) and
  [model-lighting correction](reports/model-lighting-root-cause-and-fix.md):
  what the campaigns proved and the defects later close-up controls exposed.
- [Map independence](reports/map-independence.md): empty installation and external
  package acceptance. [Storage and lighting](reports/map-storage-lighting.md)
  records exact compact formats and independent quality controls.
- [Office implementation](reports/office-demo.md) and
  [Office defect repairs](reports/office-defect-repair.md): the connected environment,
  collision/recovery and lighting fixes.

Other finalized reports in `reports/` retain distinct investigations and asset
provenance. Raw receipts, timing logs and repeated campaign captures belong in
ignored local verification output. `images/` holds the curated original native
PNG selection; immutable concept art lives under `assets/references/`.
Reusable camera/sequence inputs and the former renderer's 50 comparison PNGs
live in `tests/fixtures/native/`; those are functional inputs, not audit clutter.
Recoverable tracked deletions remain in Git history. A fresh checkout does not
need machine-specific Trash payloads or local backup collections.
