places-lighting-gpu-probe is an explicit developer experiment. The normal compiler
and player never call it. Build with the root workspace's locked dependencies.

1. Set PLACES_SOLVER_GPU_PROBE to a new evidence directory for a deliberately
   bounded compiler build. The optional export captures at most 16,384 real rays
   and 131,072 opaque triangles. Pool collection can cancel after the export;
   no full Pool bake is required. Remove the variable for ordinary compilation.
2. Run places-lighting-gpu-probe <snapshot.json> <new-report.json> --tile 16384
   --repeats 5. On Windows, retain the usual dxcompiler.dll beside the executable.
   @adversarial replaces the snapshot argument with an asset-free numerical
   fixture. Reports refuse existing destinations.
3. Every repeat alternates CPU, gated, and verify-all controls. Report cold setup
   and first dispatch separately from warm repeats. Include process wall/CPU and
   host memory from an external observer; query timing alone is not a bake gain.

The device returns canonical source IDs, never a final boolean. A candidate hit
must pass the production f64 intersection and endpoint test, its original
canonical leaf slab, and every ray-BVH ancestor with the same CPU predicates.
All misses, ambiguity flags, invalid IDs, verification failures and device errors
run the full unchanged CPU query. Verify-all does so even for certified hits.
The shader's boundary flags are advisory rather than a proven interval error
bound. Exactness never depends on f32 flags or on the absence of a flag.

Geometry and pipeline stay resident for the explicit process. Ray, result and
readback owners are fixed to one tile; the last partial tile uses its own active
count. The host keeps one bounded capture, CPU scene, identity/ancestry arrays,
and result batch. Device errors retire the entire backend and discard partial
results before CPU fallback. There is no package/output installation by this tool.
GPU waits have a ten-second bound. --cancel-after <tiles> cancels at a tile
boundary and publishes only a cancellation report; rerun starts fresh owners.
--fault setup|dispatch|readback|identity|destroy exercises refusal/fallback,
including an explicit device destroy. This is not a real driver-reset test.

Snapshots reconstruct and compare source geometry, packed IDs and hierarchy
bit for bit before querying. Legacy captures lack canonical identity and cannot
be certified; replay their original raw prototype separately. A 64 MiB file cap
precedes decoding, and root CPU scene bounds still apply.

Only filtered opaque any-hit is supported. Excluded MASK/BLEND/transmissive/water
faces are counted, not treated as opaque transport. Nearest-hit/tie ordering,
alpha coverage, colored transmission, connectivity, footprint support, bounces,
probes and production scheduling remain the exact CPU implementation. This
experiment cannot authorize any of those classes or change a lighting sample.
Do not integrate it unless exact class-specific proof and material total bake
benefit including setup, transfer, certification/fallback and failure costs exist.

The single CPU regression is in src/engine/tests/gpu_probe.rs. It protects false
hit/miss, untrusted identity and boundary admission with a tiny deterministic soup,
without assets, device initialization, files or a full-map test. Keep performance
and native campaigns explicit and outside ordinary CI.
