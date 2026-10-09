# Stop sign remake

Historical acceptance: September 2026 sign remake. Counts, timings and validation below describe
that tested version; the canonical guides govern current contracts.

Rebuilt only `core:stop_sign`: 88 triangles, 0.45 × 1.8 × 0.06 m, +Y up / +Z front, floor-contact origin.

- Thin 6 mm octagonal metal plate replaces the bulky plate.
- Eight-sided 40 mm diameter pole is behind the plate with 14 mm clearance; it reaches both rear stand-offs without crossing the face.
- Two closed stand-offs connect the pole to the plate rear.
- Clean 256×256 opaque PNG atlas with bold white STOP lettering, a white border, red face and neutral grey metal regions. Neutral face vertex colors keep the lettering white.
- Updated the source builder so rebuilding the sign preserves the fix.

Files:
- `assets/core/props/models/stop_sign.glb`
- `assets/core/props/models/stop_sign.png`
- `tools/props/parts/signage.py`
- `docs/reports/stop-sign-remake/stop_sign.png` — gameplay-style preview
- `docs/reports/stop-sign-remake/mounting.png` — close front, rear and side views

Validation: targeted prop build passed. Export reloaded successfully; exact catalog bounds, sensible pivot, 0..1 UVs, source/embedded texture pixel match, zero degenerate triangles, zero boundary/non-manifold edges, and consistent outward winding. Front, rear and side previews inspected. No Rust tests, Cargo builds or Clippy runs performed.

Texture created with the built-in imagegen tool, then mechanically resized to the project's native 256×256 with macOS sips. No runtime texture generation.

Generation prompt:

Create a production-ready square 1024x1024 flat texture atlas for a low-poly PS1/PS2 stylized STOP sign. This is texture artwork, NOT a 3D render, no lighting, no perspective, no mockup. Exact atlas layout: upper-left quadrant x=0..511 y=0..511 is the sign face: a flat-top regular octagon centered at (256,256), bounding box (8,8)-(504,504), dark brick-red face, narrow ivory-white octagonal border following the perimeter, tiny red outer margin. Large bold condensed highway-style white sans-serif text exactly 'STOP', centered, crisp conventional letter shapes, very legible. No bolts or pole on this face. Fill outside the octagon with the same brick red, no transparency. Upper-right quadrant x=512..1023,y=0..511 is uniform neutral medium-light grey galvanized pole metal, extremely subtle broad tonal variation, no objects. Lower-left quadrant x=0..511,y=512..1023 is uniform dark neutral grey bracket metal, no objects. Lower-right quadrant x=512..1023,y=512..1023 is uniform medium grey painted metal sign backing, no symbols/text. Every quadrant is filled edge-to-edge, hard exact divisions at 50%, no gutters or separators. Restrained flat stylized colors, clean intentional surfaces, no grain noise, no scratches, no photorealism, no rust, no shadows, no bevel highlights. Clear readable typography is the priority; keep border comfortably away from lettering. This atlas will be downsampled to 256x256 for a game.
