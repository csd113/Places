# Hanging exit sign remake

Rebuilt `core:exit_sign` as a closed, low-poly fixture with a chamfered painted-metal case, 6 mm recessed green lens, two suspension rods, four mounting sockets, and a wider ceiling canopy spanning both rods. The front retains EXIT with a right-pointing arrow. Only the panel emits; level lighting is unchanged.

300 triangles, eight closed components, two materials. Original 0.45 × 0.57 × 0.08 m bounds, bottom origin and +Z front preserved. Source builder and atlas specification updated to match.

New 256×256 opaque PNG atlas: top half is the 2:1 sign face; bottom left is off-white housing paint; bottom right is grey suspension metal. Neutral emissive factors preserve white lettering instead of tinting it green.

Files:
- `assets/core/props/models/exit_sign.glb`
- `assets/core/props/models/exit_sign.png`
- `tools/props/parts/signage.py`
- `docs/ASSET_SPECIFICATION.md`
- `docs/reports/exit-sign-remake/exit_sign.png` — overall preview
- `docs/reports/exit-sign-remake/inspection.png` — front/rear/side views
- `docs/reports/exit-sign-remake/validation.json`

Validation: targeted asset build passed; exported GLB reloaded; exact dimensions and pivot checked; UVs in 0..1; embedded texture matches source pixels; zero degenerate triangles, boundary edges, non-manifold edges or winding errors. Front/rear/side previews inspected. No Rust tests, Cargo builds or Clippy runs.

Texture generated with the built-in imagegen tool and resized to 256×256 using macOS sips. Final generation prompt:

Create a square flat game texture atlas, 1024x1024. Production texture artwork only, no 3D render, no perspective, no mockup, no shadows. Exact layout: the ENTIRE TOP HALF x=0..1023 y=0..511 is a single 2:1 horizontal green EXIT sign panel. Uniform deep institutional green background, large crisp bold conventional condensed sans-serif white text exactly 'EXIT', placed in left 72 percent, centered vertically, and one clear large white right-pointing arrow in right 22 percent. Even balanced comfortable margins all around, lettering height approximately half the panel height, no border, no symbols other than text and arrow. Text must read E X I T, solid professionally shaped letters, no pixel font and no decorative stencils. Bottom-left quarter x=0..511 y=512..1023 is plain warm light grey/off-white painted metal housing material, edge to edge, no objects or details. Bottom-right quarter x=512..1023 y=512..1023 is plain medium neutral grey satin metal material for suspension rods and fittings, edge to edge, no objects or details. Exact hard boundaries at image midlines, no gutters, no labels. PS1/PS2 stylized game palette, clean flat readable finishes, almost no surface variation, no grain, no scratches, no rust, no photographed realism, no bevel or glow effects painted into texture. Entire image opaque. Will downsample to a 256x256 PNG, keep large high-contrast typography and a simple solid arrow.

