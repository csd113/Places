# Independent prior-availability audit of stopped v3

The read-only audit compared only the eight confirmed passed per-case records
from `target/verification/map-regression-20261008T235019Z-94829`. It found two
lost resources, both in Hallows Full. Every other supported source remains pending
in this partial comparison; unchanged/stale archives are not treated as current
completed cases.

| Completed installed map | Prior → current Medium atlas | Prior → current Full atlas | Atlas/field loss |
| --- | --- | --- | --- |
| Winter | 3 → 4 pages; 1,091,383 → 1,611,674 texels | 5 → 6 pages; 1,844,385 → 2,457,655 texels | None; both fields retained |
| Hallows | 6 → 8 pages; 2,227,632 → 3,504,615 texels | 8 pages/3,680,557 texels → no atlas (`page overflow`) | Full atlas and Full irradiance lost |
| Zoo | 6 → 6 pages; 3,344,554 → 3,635,445 texels | 8 → 8 pages; 5,868,924 → 6,224,982 texels | None; both fields retained |
| Movement | 3 → 3 pages; 1,546,686 → 1,653,677 texels | 5 → 5 pages; 2,737,282 → 2,883,339 texels | None; both fields retained |

All these pages have edge 1024. Hallows charts grew 245,447 → 245,495;
its exact original installed package SHA is
`e187a09a28a5300626bfa77c3684d8a08823d243b09e8461aaf81724467d1447`.
The table records actual atlas growth without assigning a cause. Native texture
residency, visibility and frame performance were not measured.

Home/Office/Pool/Outdoors fixture cases also completed, with current Medium/Full
atlas and irradiance present. No exact accepted entry archive was matched for
these four paths, so preservation equality is not claimed. Six other installed
sources and 34 supported fixtures have no completed current v3 case and remain
explicitly pending. The invalid loader and synthetic CPU boundary contracts are
classified separately, without deleting or waiving findings.

The new ignored `compare_prior_availability.py` pins the ten original installed
archives using the existing entry-preservation receipt. It also matches all eleven
hero fixtures to original accepted snapshots by exact source SHA, rather than
level ID. Across those 21 matched source paths it requires every formerly
successful atlas/irradiance variant to remain available. Missing former Full data
cannot be accepted merely because the newly compiled manifest declares fallback.

The full detailed receipt, with per-case command-receipt SHA and actual package/
manifest identities, is
`debug-maps/art-style-hero/evidence/stage7-validation/availability-v3-completed-only.json`.
The metadata/hash Python audit exited 1 for the two genuine losses, with forty
supported current cases pending. It launched no compiler, Cargo, native or bake
process and wrote no target/source/asset data. Primary owns the ledger and the
compiler/layout root fix. Repeat under a new path with `--require-complete` after
the final frozen campaign; it is the first gate of native orchestration.
