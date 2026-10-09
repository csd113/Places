# Lighting dump audit page parity

`inspect_lighting_dump.py::audit_records` previously allocated eight occupancy
pages and rejected page indices 8 and 9. The actual Rust dump envelope uses
`lightmap::LIGHTMAP_ATLAS_MAX_PAGES = 10`, and
`transport::diagnostics::chart_within_dump_bounds` accepts indices 0 through 9
at the same 1024-pixel edge. That mismatch could falsely reject valid current
Full dumps after an ordinary package build succeeded.

The diagnostic helper now uses one fixed ten-page constant for both allocation
and the page-index guard. Its occupancy array remains bounded at 10,485,760
boolean bytes. World gutters remain two texels, prop gutters one; padded overlap,
edge, finite geometry, nondegenerate area and unit-normal checks are unchanged.
Lower Rust quality profiles retain their separate eight-page budgets. This
diagnostic envelope does not alter quality policy, Rust source, compiler inputs,
cache identities or prepared packages and requires no rebake.

Four focused audit tests add valid indices 8/9, invalid 10/−1, last-page overlap
between disjoint receiver rectangles whose gutters intersect, adjacent disjoint
padded reservations and exact edge/one-texel gutter overflows for world and prop
charts. The existing saved-indirect-component test remains intact.

The first command was:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.test_lighting_dump
```

It failed at module import because the actual default Python 3.13 interpreter
has no NumPy. **The five logic tests have not passed or run.** The
[failure receipt](lighting-dump-page-parity-checks.json) retains the actual raw
ImportError, exit 1, command, UTC times and file hashes. Its unconditional
`syntax_check` description is not successful-import evidence; the raw failure
is authoritative. Read-only checks also found no NumPy in the installed
framework Python 3.12 or Homebrew Python 3.14. The existing projection test
already needs NumPy through the same helper; no test skip, mock dependency or
weakened assertion was introduced.

Independent stdlib parsing of both allocated files
[passed](lighting-dump-page-parity-syntax.json), without imports or emitted
bytecode. The primary must supply the analysis dependency for the actual
normal-gate interpreter and then rerun the focused command before whole Python
acceptance. The helper/test hashes are available in both receipts for the
primary's extra identity seal, outside the frozen compiler/package inputs.

No Cargo, compiler, bake, native, target or Git/index job was launched. No
dependency installation or persistent environment change was attempted. The
allocated helper and tests are ready for freeze; no validation-owned jobs remain.

## Focused rerun after dependency provision

The primary provisioned NumPy 2.5.3 for the actual default Python 3.13
interpreter; its independent receipt is
`docs/art-style/stage7/execution/analysis-dependency-numpy.json`. The exact same
focused command then **passed all five tests**, exit 0, in 0.131 seconds. The
[fresh result](lighting-dump-page-parity-passed.json) preserves command/cwd,
start/end UTC, elapsed time, raw output, interpreter/dependency identities and
helper/test SHA-256. Both source hashes match the preserved failed attempt and
remained unchanged during execution. The earlier failure and syntax receipts
remain unchanged.

This result verifies pages 8/9, rejection of 10/negative indices, gutter overlap
and exact world/prop edge boundaries, alongside the existing saved-indirect
component contract. It is bounded helper verification; it is not a native or
compiler campaign. Whole Python and ordinary package acceptance remain with the
primary's normal gate. The helper/test identities are frozen and released for
the primary's extra identity receipt. No validation-owned jobs remain.
