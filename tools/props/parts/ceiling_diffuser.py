"""600 mm commercial four-way ceiling diffuser; geometry, not painted shadows.

The committed ImageGen paint PNG is loaded verbatim. All vents use ordinary
non-emissive static-prop materials and receiver lighting. Base origin Y=0,
mounting back Y=0.065; placement is ceiling height minus that depth.
"""
from pathlib import Path

from tex import Texture, decode_png

ROOT = Path(__file__).resolve().parents[3]
PNG = ROOT / "assets/shared/props/ceiling_diffuser_01.png"

# Built-in ImageGen source prompt. Retained 1254-square output is fitted to the
# committed 1024-square RGB master, then 256-square native sheet with Lanczos.
# Rebuilding the model only loads that native PNG; it never repaints the sheet.
TEXTURE_PROMPT = "Use case: photorealistic-natural. Asset type: seamless base-colour texture for an economical low-poly commercial office ceiling air diffuser, 600 mm square, with geometry-authored frame and four-way inset louvers. Primary request: a completely flat, front-on, edge-to-edge square swatch of neutral off-white powder-coated steel, subtle fine orange-peel paint grain and very restrained age variation. Uniform unlit albedo, no directional lighting, no shadows, no highlights, no bevels or object silhouette. The swatch is only the painted-metal surface, not a picture of a vent. No louvers, slots, screws, text, symbols, borders, dark vignette, perspective, transparency or seams. Keep all edges seamless, identical average colour across the swatch; fine understated texture readable when downsampled from 1024 square to 256 square. Neutral institutional painted steel, RGB around 220-235 rather than luminous white."


def build(builder):
    width, height, pixels = decode_png(PNG.read_bytes())
    if (width, height) != (256, 256) or any(pixels[i] != 255 for i in range(3, len(pixels), 4)):
        raise ValueError("ceiling diffuser requires its opaque 256-square paint sheet")
    builder.tex = Texture(256)
    builder.tex.pixels = bytearray(pixels)
    mesh = builder.mesh
    mesh.begin_material(mesh.material("painted_steel", roughness=0.85, metallic=0.05))
    uv = (0.0, 0.0, 1.0, 1.0)
    paint = (255, 255, 255)

    def corners(half, y):
        return [(-half, y, -half), (half, y, -half), (half, y, half), (-half, y, half)]

    def ring(outer, inner, outer_y, inner_y, color=paint, reverse=False):
        a, b = corners(outer, outer_y), corners(inner, inner_y)
        for i in range(4):
            j = (i + 1) % 4
            points = [a[i], a[j], b[j], b[i]]
            if reverse:
                points.reverse()
            mesh.quad(*points, uv, color)

    def edge(half, low, high, inward=False):
        a, b = corners(half, low), corners(half, high)
        for i in range(4):
            j = (i + 1) % 4
            points = [a[j], a[i], b[i], b[j]]
            if inward:
                points.reverse()
            mesh.quad(*points, uv, paint)

    # The deep dark plenum is an ordinary opaque metal surface, not a luminous
    # texture or an alpha hole into the world. Its back closes at the ceiling.
    mesh.box((0, 0.0625, 0), (0.6, 0.005, 0.6), uv, (58, 62, 65), shade=False)
    ring(0.3, 0.263, 0.0, 0.013)
    edge(0.3, 0.0, 0.065)
    edge(0.263, 0.013, 0.06, inward=True)
    # Five concentric sloping square blades distribute air in four directions.
    # Each has a real underside, thin top and closed edges; gaps expose depth.
    for outer in (0.247, 0.200, 0.153, 0.106, 0.059):
        inner = outer - 0.032
        ring(outer, inner, 0.044, 0.022)
        ring(outer, inner, 0.047, 0.025, reverse=True)
        edge(outer, 0.044, 0.047)
        edge(inner, 0.022, 0.025, inward=True)
    mesh.box((0, 0.025, 0), (0.036, 0.006, 0.036), uv, paint, shade=False)


PROPS = {"core:ceiling_diffuser_01": build}
