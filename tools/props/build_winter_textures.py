#!/usr/bin/env python3
"""Reproduce Winter PNG derivatives from committed artwork, never paint pixels."""
import argparse
from io import BytesIO
from pathlib import Path
import sys

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/textures'))
import seam_repair


def png(image):
    output = BytesIO()
    image.save(output, format='PNG')
    return output.getvalue()


def derivatives():
    winter = ROOT / 'assets/environment/winter'
    sheets = ('floors/snow_01', 'floors/ice_01', 'walls/stone_masonry_01', 'walls/timber_01')
    results = {}
    for name in sheets:
        target = winter / 'textures' / (name + '.png')
        source = seam_repair.read_png(target.with_name(target.stem + '_master.png'))
        offset = 370 if name == 'walls/timber_01' else 512
        pixels = seam_repair.repair(source.pixels, source.width, source.height,
                                    source.channels, 32, 64, 12, offset, 512)
        repaired = seam_repair.PngImage(source.width, source.height, source.colour_type,
                                       pixels, source.ancillary)
        results[target] = seam_repair.encode_png(repaired)
    models = winter / 'props/models'
    for source, target in (('floors/snow_01', 'snow_surface'), ('floors/ice_01', 'ice_surface')):
        image = Image.open(BytesIO(results[winter / 'textures' / (source + '.png')]))
        results[models / (target + '.png')] = png(image.resize((256, 256), Image.Resampling.LANCZOS))
    lamp = Image.open(ROOT / 'assets/environment/outdoor/props/models/lamp_stand_master.png').convert('RGB')
    stone = Image.open(winter / 'textures/walls/stone_masonry_01_master.png').convert('RGB')
    wood = Image.open(winter / 'textures/walls/timber_01_master.png').convert('RGB')
    crops = (stone.crop((320,280,520,370)), wood.crop((330,0,535,1024)),
             lamp.crop((0,0,512,512)), lamp.crop((512,0,1024,512)))
    atlas = Image.new('RGB', (1024,1024))
    for index, crop in enumerate(crops):
        atlas.paste(crop.resize((512,512), Image.Resampling.LANCZOS),
                    ((index % 2)*512, (index // 2)*512))
    results[models / 'village_materials_master.png'] = png(atlas)
    results[models / 'village_materials.png'] = png(atlas.resize((256,256), Image.Resampling.LANCZOS))
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    failures = []
    for path, data in derivatives().items():
        if args.check:
            if not path.exists() or path.read_bytes() != data:
                failures.append(str(path.relative_to(ROOT)))
        else:
            path.write_bytes(data)
    if failures:
        print('STALE Winter derivatives: ' + ', '.join(failures))
        return 1
    print('Winter artwork derivatives: current' if args.check else 'Winter artwork derivatives: rebuilt')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
