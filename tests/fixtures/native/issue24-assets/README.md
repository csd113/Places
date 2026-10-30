# Issue 24 explicit native controls

These four small level sources reproduce the modular rail/fence interfaces,
fixed ghost opacity/overlap controls, absent-instance zero comparison and routed
proximity fade. They are explicit developer diagnostics, outside automatic CI,
Rust tests and the release lineup. Do not bake a shipped collection for them.

Compile one source with `places-compile build <source> --out <new-package>
--asset-root assets --workers 1 --variants static,full`, then install that package
under an isolated state root's `levels/` directory. The internal level IDs remain
`issue24_ghost_controls`, `issue24_ghost_absent`, `issue24_ghost_motion` and
`issue24_rail_controls`.

The [Windows review](../../../../docs/reports/issue24-assets-windows.md) records
the exact binary, asset hash, native settings, cameras, defect and queued renderer
implementation. For the ghost rear reproduction use High/Full, bloom on, 960x640,
FOV 55, player spawn `0,1.6,6,0`, render eye `7.6,1.05,-2.7`, camera `204,-2`,
fixed delta `1/60` and ready capture frame 340. Opacity_4 is fixed at 1.0. Its raised
face must be occluded by the rear cloth; current native rendering fails this.

Keep both BLEND primitives, the face/cloth geometry and artwork unchanged while
fixing rendering. Partial-opacity, overlap, zero-opacity/glow, fade reversal and
idle/float behavior are required preservation controls.
