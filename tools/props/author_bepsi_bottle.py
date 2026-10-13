#!/usr/bin/env python3
"""Fit existing approved Bepsi branding into a dedicated bottle UV sheet.

Explicit offline authoring only. The vending master remains unchanged. Three
upright logo repeats make the small cylindrical label readable from either side;
ordinary model builds load the resulting opaque PNG without drawing artwork.
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2] / "assets/environments/arcade/props"


def main():
    with Image.open(ROOT / "vending_machine_master.png") as vending:
        globe = vending.crop((122, 0, 389, 224)).resize((208, 176), Image.Resampling.LANCZOS)
        word = vending.crop((45, 223, 480, 310)).resize((312, 76), Image.Resampling.LANCZOS)
    with Image.open(ROOT / "bowling_palette_master.png") as palette:
        sheet = Image.new("RGB", (1024, 1024), (18, 59, 126))
        # Whole-circumference label: 256 x 128 native pixels. The logo's
        # globe and word retain their source orientation and colour branding.
        for x in (16, 356, 696):
            sheet.paste(globe, (x + 52, 74))
            sheet.paste(word, (x, 267))
        for src, dst in (
            ((0, 768, 256, 1024), (0, 512)),
            ((512, 0, 768, 256), (512, 512)),
        ):
            sheet.paste(palette.crop(src).resize((512, 512), Image.Resampling.LANCZOS), dst)
    sheet.save(ROOT / "soda_bottle_master.png")
    sheet.resize((256, 256), Image.Resampling.LANCZOS).save(ROOT / "soda_bottle.png")
    print("Fitted Bepsi bottle master/native from unchanged production branding")


if __name__ == "__main__":
    main()
