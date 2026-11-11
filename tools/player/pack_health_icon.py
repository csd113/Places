"""Pack the GIMP-authored icon without reinterpreting any other atlas pixels.

This is byte-preserving offline atlas assembly, not artwork generation.
Run after paint_tools_gimp.py. Runtime loads only the committed atlas PNGs.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
icon = Image.open(ROOT/'assets/shared/ui/health-heart.png').convert('RGBA')
assert icon.size == (64,64)
for name, origin, size in [('font_01.png',(64,64),(256,128)),
                          ('editor_font.png',(0,384),(512,448))]:
    path = ROOT/'assets/shared/ui'/name
    atlas = Image.open(path).convert('RGBA')
    assert atlas.size == size
    original = atlas.copy()
    atlas.paste(icon,origin)
    # A full RGBA byte comparison, including invisible RGB, protects all glyphs.
    outside = atlas.copy()
    box = (*origin, origin[0]+64, origin[1]+64)
    outside.paste(original.crop(box),origin)
    assert outside.tobytes() == original.tobytes()
    atlas.save(path)
    print('Packed',name)
