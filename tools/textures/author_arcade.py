#!/usr/bin/env python3
"""Explicit Arcade source import, native derivatives and visual QA.

The built-in imagegen tool creates deliberate source art. This script imports
those outputs into the documented asset shapes, joins only surface boundaries,
and retains authoritative 1024px masters. Native albedo downsamples in linear
light; response masks downsample as numeric coverage. --check reads committed
masters and natives without repainting any image or invoking generation.

Normal build.py exports use arcade_art.py's saved-sheet readers. Runtime loads
PNG assets and never invokes this offline authoring tool.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from arcade_art import CARPETS, SHEETS

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
ARCADE = ASSETS / "environments" / "arcade"

SOURCES = {
    **{f"carpet_{name}_01": ("textures/floors", (1024, 1024), (256, 256), True)
       for name in CARPETS},
    "wall_navy_01": ("textures/walls", (1024, 1024), (256, 256), True),
    "ceiling_acoustic_01": ("textures/ceilings", (1024, 1024), (256, 256), True),
    "lane_wood_01": ("textures/floors", (1024, 1024), (256, 256), True),
    "gutter_dark_01": ("textures/floors", (1024, 1024), (256, 256), True),
    "fluorescent_panel_01": ("textures/lights", (1024, 512), (256, 128), False),
    "blacklight_tube_01": ("textures/lights", (1024, 128), (256, 32), False),
    "bowling_mural_01": ("decals", (1024, 256), (1024, 256), False),
}


def dependencies():
    import numpy as np
    from PIL import Image, ImageDraw
    return np, Image, ImageDraw


def paths(name):
    directory = SOURCES[name][0]
    return ARCADE / directory / f"{name}_master.png", ARCADE / directory / f"{name}.png"


def save(image, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, format="PNG", compress_level=9)


def periodic(image, band=32):
    """Use the image's own rolled pixels to join a narrow boundary band.

    All interior pixels outside 32px remain exact. Four opposing edge samples
    agree exactly, including adjacent samples used by the seam/mip metric.
    This is deliberate offline boundary correction, never runtime artwork.
    """
    np, Image, _ = dependencies()
    values = np.asarray(image.convert("RGB"), dtype=np.float32).copy()
    height, width = values.shape[:2]
    for axis, length in ((1, width), (0, height)):
        rolled = np.roll(values, length // 2, axis=axis)
        for distance in range(band):
            weight = 0.5 * (1 + np.cos(np.pi * distance / band))
            for position in (distance, length - 1 - distance):
                selection = [slice(None), slice(None), slice(None)]
                selection[axis] = position
                key = tuple(selection)
                values[key] = values[key] * (1 - weight) + rolled[key] * weight
    values = np.clip(np.rint(values), 0, 255).astype(np.uint8)
    for distance in range(4):
        left = values[:, distance].astype(np.uint16)
        right = values[:, width - 1 - distance].astype(np.uint16)
        column = ((left + right + 1) // 2).astype(np.uint8)
        values[:, distance] = column
        values[:, width - 1 - distance] = column
        top = values[distance].astype(np.uint16)
        bottom = values[height - 1 - distance].astype(np.uint16)
        row = ((top + bottom + 1) // 2).astype(np.uint8)
        values[distance] = row
        values[height - 1 - distance] = row
    return Image.fromarray(values)


def exact_edges(image):
    """Preserve one matching three-pixel ring on periodic native derivatives."""
    np, Image, _ = dependencies()
    values = np.asarray(image).copy()
    height, width = values.shape[:2]
    for distance in range(3):
        pair = (values[:, distance].astype(np.uint16)
                + values[:, width - distance - 1].astype(np.uint16) + 1) // 2
        values[:, distance] = pair
        values[:, width - distance - 1] = pair
        pair = (values[distance].astype(np.uint16)
                + values[height - distance - 1].astype(np.uint16) + 1) // 2
        values[distance] = pair
        values[height - distance - 1] = pair
    return Image.fromarray(values.astype(np.uint8))


def downsample(image, size, numeric=False):
    """Integer-area filter matching the colour/numeric role distinction."""
    np, Image, _ = dependencies()
    if image.size == size:
        return image.copy()
    width, height = image.size
    out_width, out_height = size
    if width % out_width or height % out_height:
        raise ValueError("native derivative must use exact integer area blocks")
    factor_x, factor_y = width // out_width, height // out_height
    channels = np.asarray(image, dtype=np.float64) / 255.0
    if not numeric:
        channels = np.where(channels <= 0.04045, channels / 12.92,
                            ((channels + 0.055) / 1.055) ** 2.4)
    filtered = channels.reshape(out_height, factor_y, out_width, factor_x, 3).mean(axis=(1, 3))
    if not numeric:
        filtered = np.where(filtered <= 0.0031308, filtered * 12.92,
                            1.055 * filtered ** (1 / 2.4) - 0.055)
    return Image.fromarray(np.clip(np.rint(filtered * 255), 0, 255).astype(np.uint8))


def response_mask(image):
    """Coverage of fluorescent ink, with navy backing explicitly zero.

    Master navy texels whose peak channel is <=80 are non-reactive. Brighter
    graphic/pile edges ramp smoothly to full coverage at peak >=145. The
    mask is numeric RGB, never an emissive or colour sheet.
    """
    np, Image, _ = dependencies()
    peak = np.asarray(image.convert("RGB"), dtype=np.float64).max(axis=2)
    coverage = np.clip((peak - 80) / 65, 0, 1)
    coverage = coverage * coverage * (3 - 2 * coverage)
    values = np.clip(np.rint(coverage * 255), 0, 255).astype(np.uint8)
    return Image.fromarray(np.repeat(values[:, :, None], 3, axis=2))


def fit_fitted(image, name, size):
    """Preserve complete generated art while fitting a documented face ratio."""
    np, Image, _ = dependencies()
    if name == "blacklight_tube_01":
        image = image.convert("RGB")
        dark = np.asarray(image).mean(axis=2) < 220
        rows = np.flatnonzero(dark.mean(axis=1) > 0.65)
        if not len(rows):
            raise ValueError("blacklight source has no dark fitted lens")
        image = image.crop((0, int(rows[0]), image.width, int(rows[-1]) + 1))
        canvas = Image.new("RGB", size, (16, 17, 24))
        image.thumbnail(size, Image.Resampling.LANCZOS)
        canvas.paste(image, ((size[0] - image.width) // 2, (size[1] - image.height) // 2))
        return canvas
    if name == "bowling_mural_01":
        image = image.convert("RGBA")
        bounds = image.getchannel("A").point(lambda value: 255 if value >= 128 else 0).getbbox()
        if bounds is None:
            raise ValueError("mural source has no opaque artwork")
        image = image.crop(bounds)
        canvas = Image.new("RGBA", size, (0, 0, 0, 0))
        image.thumbnail((size[0] - 8, size[1] - 8), Image.Resampling.LANCZOS)
        canvas.paste(image, ((size[0] - image.width) // 2, (size[1] - image.height) // 2))
        return canvas
    if image.width * size[1] != image.height * size[0]:
        raise ValueError(f"{name}: generated fitted source has the wrong ratio")
    return image.convert("RGB").resize(size, Image.Resampling.LANCZOS)


def import_receipt(path):
    _, Image, _ = dependencies()
    receipt = json.loads(path.read_text())
    for entry in receipt["images"]:
        name = entry["asset"]
        if name not in SOURCES:
            raise ValueError(f"unknown Arcade source: {name}")
        image = Image.open(ROOT / entry["source"])
        _, size, _, tiled = SOURCES[name]
        if tiled:
            if image.width != image.height:
                raise ValueError(f"{name}: tiling source must be square")
            master = periodic(image.convert("RGB").resize(size, Image.Resampling.LANCZOS))
        else:
            master = fit_fitted(image, name, size)
        save(master, paths(name)[0])
        print(f"imported {name}: {master.width}x{master.height}")


def derivatives(write=False):
    _, Image, _ = dependencies()
    expected = {}
    for name, (_, master_size, native_size, tiled) in SOURCES.items():
        master_path, native_path = paths(name)
        image = Image.open(master_path)
        if image.size != master_size:
            raise ValueError(f"{name}: master must be {master_size}")
        native = image.copy() if name == "bowling_mural_01" else downsample(image.convert("RGB"), native_size)
        if tiled:
            native = exact_edges(native)
        expected[native_path] = native
        if name.startswith("carpet_"):
            mask_name = name[:-3] + "_uv_mask_01"
            mask_master = response_mask(image)
            mask_path = master_path.with_name(f"{mask_name}_master.png")
            mask_native = exact_edges(downsample(mask_master, native_size, numeric=True))
            expected[mask_path] = mask_master
            expected[native_path.with_name(f"{mask_name}.png")] = mask_native
    for path, image in expected.items():
        if write:
            save(image, path)
        elif not path.exists():
            raise ValueError(f"missing derivative: {path.relative_to(ROOT)}")
        else:
            actual = Image.open(path).convert(image.mode)
            if actual.size != image.size or actual.tobytes() != image.tobytes():
                raise ValueError(f"stale derivative: {path.relative_to(ROOT)}")
    return expected


def markings(write=False):
    _, Image, ImageDraw = dependencies()
    arrow = Image.new("RGBA", (128, 128), (0, 0, 0, 0))
    ImageDraw.Draw(arrow).polygon(((64, 20), (98, 106), (30, 106)), fill=(15, 13, 11, 255))
    stripe = Image.new("RGBA", (256, 128), (0, 0, 0, 0))
    ImageDraw.Draw(stripe).rectangle((4, 54, 251, 73), fill=(21, 18, 15, 255))
    for name, image in (("lane_arrow_01", arrow), ("foul_line_01", stripe)):
        path = ARCADE / "decals" / f"{name}.png"
        if write:
            save(image, path)
        elif not path.exists() or Image.open(path).convert("RGBA").tobytes() != image.tobytes():
            raise ValueError(f"stale marking: {path.relative_to(ROOT)}")


def checks():
    np, Image, _ = dependencies()
    files = []
    for asset_id, model in SHEETS.items():
        path = ASSETS / model
        image = Image.open(path)
        if "decal_" not in asset_id and image.convert("RGBA").getchannel("A").getextrema() != (255, 255):
            raise ValueError(f"{asset_id}: opaque sheet has transparency")
        if "uv_mask" in asset_id:
            data = np.asarray(image.convert("RGB"))
            if not (np.array_equal(data[:, :, 0], data[:, :, 1])
                    and np.array_equal(data[:, :, 0], data[:, :, 2])):
                raise ValueError(f"{asset_id}: response mask must be numeric grey RGB")
        if image.width > 1024 or image.height > 1024:
            raise ValueError(f"{asset_id}: sheet exceeds ordinary image budget")
        files.append({"id": asset_id, "path": model, "size": list(image.size), "mode": image.mode})
    derivatives()
    markings()
    return files


def qa(directory):
    np, Image, ImageDraw = dependencies()
    directory.mkdir(parents=True, exist_ok=True)
    contact = Image.new("RGB", (1152, 6 * 304), (14, 18, 29))
    draw = ImageDraw.Draw(contact)
    families = []
    for row, family in enumerate(CARPETS):
        name = f"carpet_{family}_01"
        master_path, native_path = paths(name)
        master = Image.open(master_path).convert("RGB")
        native = Image.open(native_path).convert("RGB")
        low = downsample(native, (128, 128))
        mask = Image.open(native_path.with_name(f"carpet_{family}_uv_mask_01.png")).convert("RGB")
        y = row * 304 + 24
        contact.paste(master.resize((256, 256), Image.Resampling.LANCZOS), (0, y))
        contact.paste(native, (272, y))
        contact.paste(low.resize((256, 256), Image.Resampling.NEAREST), (544, y))
        contact.paste(mask, (816, y))
        draw.text((0, row * 304 + 5), family.replace("_", " ").title(), fill="white")
        for x, label in ((0, "1024 master overview"), (272, "256 native"), (544, "128 stress / 2x"), (816, "numeric UV response mask")):
            draw.text((x, y + 263), label, fill=(180, 190, 205))
        wrap = Image.new("RGB", (768, 768))
        for z in range(3):
            for x in range(3):
                wrap.paste(native, (x * 256, z * 256))
        wrap.save(directory / f"carpet_{family}_wrap_3x3.png")
        low.save(directory / f"carpet_{family}_stress_128.png")
        pixels = np.asarray(native)
        mask_values = np.asarray(mask)[:, :, 0]
        families.append({"family": family, "dark_native_fraction": round(float((pixels.max(axis=2) < 85).mean()), 4),
                         "mask_zero_fraction": round(float((mask_values == 0).mean()), 4),
                         "mask_full_fraction": round(float((mask_values >= 245).mean()), 4)})
    contact.save(directory / "carpet-native-masks-contact.png")
    files = checks()
    report = {"sheet_count": len(files), "contract_checks": "pass", "derivative_checks": "pass", "families": families, "files": files,
              "note": "128px carpet sheets are QA mip stress images only; the unchanged Low Surface budget retains native256 albedo."}
    (directory / "texture-qa.json").write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--import-receipt", type=Path, help="explicitly import built-in imagegen output records")
    parser.add_argument("--derive", action="store_true", help="explicitly write native PNGs and masks from retained masters")
    parser.add_argument("--author-markings", action="store_true", help="explicitly create simple triangle/stripe decal PNGs")
    parser.add_argument("--check", action="store_true", help="verify saved contracts and source/native parity without writes")
    parser.add_argument("--qa-dir", type=Path, help="write separate contact sheets, wrapped views and metadata")
    args = parser.parse_args()
    if args.import_receipt:
        import_receipt(args.import_receipt)
    if args.derive:
        derivatives(write=True)
    if args.author_markings:
        markings(write=True)
    if args.check:
        files = checks()
        print(f"PASS {len(files)} Arcade texture/decal/fixture sheets; source/native parity and class contracts")
    if args.qa_dir:
        qa(args.qa_dir)
    if not any((args.import_receipt, args.derive, args.author_markings, args.check, args.qa_dir)):
        parser.error("choose an explicit authoring, checking or QA operation")


if __name__ == "__main__":
    main()
