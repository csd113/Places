#!/usr/bin/env python3
"""Explicit offline rasterization; runtime loads only the committed PNG.
Roboto Mono (OFL) from google/fonts/ofl/robotomono, weight 500. Pillow required.
16 columns x 8 rows, 32x56 cells, ASCII 32..126; cell zero is solid white.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
ROOT = Path(__file__).resolve().parents[2]
font = ImageFont.truetype(str(Path(__file__).with_name('RobotoMono.ttf')), 48)
font.set_variation_by_axes([500])
atlas = Image.new('RGBA', (512, 448), (255, 255, 255, 0))
draw = ImageDraw.Draw(atlas)
draw.rectangle((0, 0, 31, 55), fill='white')
for code in range(33, 127):
    col, row = (code - 32) % 16, (code - 32) // 16
    draw.text((col * 32 + 1.5, row * 56 + 44), chr(code), font=font,
              anchor='ls', fill='white', stroke_width=0)
atlas.save(ROOT / 'assets/shared/ui/editor_font.png')
