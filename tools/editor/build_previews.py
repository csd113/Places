#!/usr/bin/env python3
"""Offline, bounded catalog previews from shipped geometry and texture files.

No preview imagery is synthesized by the application. Reuses the existing
GLB inspection renderer; material swatches preserve texture aspect and tint.
"""
from pathlib import Path
import argparse
import hashlib
import io
import json
import sys

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/props"))
sys.path.insert(0, str(ROOT / "tools"))
import preview
from execution import atomic_write, ordered_map


def build(job):
    entry, source, output = job
    path = ROOT / "assets" / source
    tint = entry.get("tint", [1, 1, 1])
    if path.suffix == ".glb":
        model = preview.load_model(str(path))
        model.colors = [tuple(c * tint[i] if i < 3 else c for i, c in enumerate(color))
                        for color in model.colors]
        image = Image.frombytes("RGBA", (160, 120), preview.render(model, 160, 120))
    else:
        image = Image.open(path).convert("RGBA")
        # Multiply linear light like the runtime, then encode display RGB.
        channels = image.split()
        image = Image.merge("RGBA", tuple(
            channel.point([round(255 * (((v / 255 + .055) / 1.055) ** 2.4 * tint[i]) ** (1 / 2.4) * 1.055 - 255 * .055)
                           if v > 10 else round(v * tint[i]) for v in range(256)]) if i < 3 else channel
            for i, channel in enumerate(channels)))
        image.thumbnail((160, 120), Image.Resampling.LANCZOS)
        canvas = Image.new("RGBA", (160, 120), (26, 27, 30, 255))
        canvas.alpha_composite(image, ((160 - image.width) // 2, (120 - image.height) // 2))
        image = canvas
    data = io.BytesIO()
    image.save(data, format="PNG", optimize=True)
    atomic_write(output, data.getvalue())
    return entry["id"], {"path": Path(output).relative_to(ROOT / "assets").as_posix(),
                         "source": source, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--missing-materials", action="store_true", help="add only absent material swatches; preserve all existing/native previews")
    args = parser.parse_args()
    existing = json.loads((ROOT / "assets/editor/previews.json").read_text(encoding="utf-8")) if args.missing_materials else {}
    entries = json.loads((ROOT / "assets/catalog.json").read_text())["assets"]
    by_id = {e["id"]: e for e in entries}
    directory = ROOT / "assets/editor/previews"
    directory.mkdir(parents=True, exist_ok=True)
    native = json.loads((ROOT / "assets/editor/native-previews.json").read_text())
    jobs = []
    for entry in entries:
        if args.missing_materials and (entry["asset_type"] != "material" or entry["id"] in existing):
            continue
        if entry["id"] in native:
            continue
        if entry["asset_type"] not in ("prop", "entity", "material", "light", "decal"):
            continue
        source = entry.get("model")
        if entry["asset_type"] == "material":
            source = by_id.get(entry.get("texture"), {}).get("model")
        if source:
            name = hashlib.sha256(entry["id"].encode()).hexdigest()[:24] + ".png"
            jobs.append((entry, source, str(directory / name)))
    result = existing.copy()
    result.update(ordered_map(build, jobs, workers=6, progress="editor previews"))
    # Native fixture/door derivatives are committed assets, captured explicitly
    # when their runtime geometry changes. Never substitute a flat diffuser for
    # a 3D fixture or regenerate previews while the editor is running.
    for asset_id, metadata in native.items():
        if args.missing_materials:
            continue
        image_path = ROOT / "assets" / metadata["path"]
        with Image.open(image_path) as image:
            if image.size != (160, 120):
                raise ValueError(f"Invalid native preview: {image_path}")
        source = ROOT / metadata["source"] if metadata["source"].startswith("src/") else ROOT / "assets" / metadata["source"]
        result[asset_id] = dict(metadata, sha256=hashlib.sha256(source.read_bytes()).hexdigest())
    atomic_write(ROOT / "assets/editor/previews.json", (json.dumps(result, sort_keys=True, indent=2) + "\n").encode())
    print(f"Wrote {len(result)} catalog previews")


if __name__ == "__main__":
    main()
