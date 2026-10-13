# Compiler terminal progress

`places-compile build` and `build-collection` now show the source, current
variant/stage and elapsed time on stderr. Receiver passes, irradiance probes and
reflection-probe encoding show completed work against their actual denominator.
Preparation, integrity checks, GPU setup/readback and publication remain named
indeterminate stages. Percentages describe one pass; there is no estimated
whole-build percentage or ETA.

```text
[0:00] full: switched direct visibility 4919/20831 receivers (23%) [##--------]
[0:00] full: shared bounce 1/2 11387/20831 (54%)
[compile 0:01] packing_shadow_control.json: complete
```

The default `--progress auto` redraws one line on a capable stderr terminal and
uses plain stage lines when redirected, on CI or with `TERM=dumb`. `plain` forces
plain lines; `off` suppresses progress. The display uses uncoloured ASCII, respects
`NO_COLOR`, measures stderr width during redraw and keeps narrow lines within the
terminal. Warnings and verbose telemetry share its output lock; compiler telemetry
also moves to stderr so `--json` stdout stays parseable. Player logging remains as
before. Worker updates are relaxed atomics in batches of at most 64 completed
receivers, with no output or mutex in the ray loops. Redraw is capped at 10 Hz;
plain work updates are spaced by ten seconds, plus stage boundaries and final counts.

Verified package hits and prepared-record reuse are named explicitly. Completion
and failure clear the live line. Ctrl-C uses the existing transport cancellation
flag, exits 130 and checks other stage boundaries, archive entries and the final
publication boundary. It can wait for the current preparation/GPU/compression
call to return. The display never hides the cursor or changes terminal modes.

Bake arithmetic, ordering, quality, codecs, cache rules and maps are unchanged.
As before, a different compiler executable changes its stored tool identity.
All existing compiler/transport timing diagnostics and report phases are retained
for later performance work; reflection capture already uses the GPU, while light
transport runs on CPU workers.

Validation passed: formatting/diff checks, strict debug and release Clippy,
workspace tests (2,253 library, two compiler CLI and seven integration),
all-feature tests (2,259 library, two CLI and seven integration), with 18 existing
ignores retained, and 65 focused Python package/benchmark source checks. Small
deterministic tests protect width/count policy, concurrent counters, warning
redraw and cancellation immediately before publication; no full-map test was added.

Real pseudo-terminal/plain captures use the existing 4×4 packing control and
8×6 material-routing fixture: progress enabled/disabled produces identical
control archives; JSON with verbose logs, 120→60-column resize, warnings,
current/prepared reuse, collection failures and actual terminal Ctrl-C pass.
Native Metal capture verifies Full/Medium reflection stages and probe counts.
Evidence is retained in `local/compiler-progress/` outside the build directory.
Production maps were not baked; active source/package and unrelated audit hashes
are preserved. Linux/Windows native acceptance and broad performance measurement
remain unrun.
