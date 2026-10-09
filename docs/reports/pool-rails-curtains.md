# Pool guard rails and curtain remakes

Historical acceptance: September 2026 pool asset refinement. Counts, timings and validation below describe
that tested version; the canonical guides govern current contracts.

Completed all six modules. Existing catalog dimensions, origins, orientation,
solid flags and texture pixels are preserved.

- Guardrail straight, end and corner: added matching Ø42 mm middle bars at
  0.525 m, below the existing 0.98 m top bars. Both corner legs have both bars.
  Rebuilt the closed posts/flanges and removed coincident cap seams.
- Curtain straight, end and corner: remade the fabric as thin closed shells
  with gentler pleats, gathered headers, continuous texture mapping, hanging
  tabs and slightly scalloped hems. The corner is one continuous L-shaped
  fabric shell. Retained the modular tracks and post locations.

## Files

Replaced GLBs and added matching PNG sources under
`assets/environment/pool/props/models/`, for each basename:

- `pool_guardrail_straight`
- `pool_guardrail_end`
- `pool_guardrail_corner`
- `pool_curtain_straight`
- `pool_curtain_end`
- `pool_curtain_corner`

PNG sources were extracted from the existing embedded artwork; no textures
were painted or altered. Runtime still reads embedded PNGs from the GLBs.

Other changes:

- `tools/props/parts/pool_remade.py`: reproducible geometry builders.
- `tools/props/parts/pool.py`: routes these six entries to the new builders.
- `assets/catalog.json`: descriptions updated for two rails and revised pleats.
- `docs/ASSET_SPECIFICATION.md`: records source atlases and new geometry contracts.

## Validation

Passed:

```sh
python3 tools/props/build.py --only core:pool_guardrail_straight core:pool_guardrail_end core:pool_guardrail_corner core:pool_curtain_straight core:pool_curtain_end core:pool_curtain_corner
python3 -m py_compile tools/props/parts/pool_remade.py
```

Export checks cover scale/origin, triangle budgets and UV bounds. An additional
`geometry.inspect` audit of the six shipped GLBs found zero open edges,
non-manifold edges, degenerate triangles or winding conflicts. Decoded texture
pixels were compared against the pre-edit GLBs and match exactly. All models
are below 800 triangles. The finished contact sheet was visually inspected.

No animation changes. No Rust tests or in-engine playtest were performed.
No requested model remains incomplete.

Preview order: curtain corner/end/straight, then guardrail corner/end/straight.
