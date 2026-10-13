#!/usr/bin/env python3
"""Author tiny, explicit developer controls for linear colour diagnostics.

Usage: colour_response_control.py --author local/office-repair/control
This authors JSON and a local copied catalogue referencing committed PNGs. It
does not compile, capture, change production assets or run in default tests/CI.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[2]
COLOURS = {"white": [1.0, 1.0, 1.0], "pink": [1.0, 0.1, 0.45],
           "blue": [0.0, 0.0, 1.0]}
MATERIALS = {"neutral": [0.6, 0.6, 0.6], "red": [0.6, 0.0, 0.0],
             "blue": [0.0, 0.0, 0.6]}


def material(name: str) -> str:
    return "diagnostic:colour_control_" + name


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def source(name: str, colour: list[float]) -> dict:
    neutral, red = material("neutral"), material("red")
    walls = []
    for x in (0.0, 5.0):
        walls.extend([
            dict(x=x - 0.12, z=-0.12, width=3.24, depth=0.12),
            dict(x=x - 0.12, z=3.0, width=3.24, depth=0.12),
            dict(x=x - 0.12, z=0.0, width=0.12, depth=3.0),
            dict(x=x + 3.0, z=0.0, width=0.12, depth=3.0,
                 material=red if x == 5.0 else neutral),
        ])
    walls.append(dict(x=5.5, z=1.6, width=2.0, depth=0.12,
                      height=1.8, material=red))
    for wall in walls:
        wall.setdefault("height", 2.8)
    return dict(
        format_version=3, id="colour_response_" + name,
        name="Colour response control: " + name,
        spawn=dict(x=1.5, z=2.75, yaw_degrees=0),
        defaults=dict(wall=neutral, floor=neutral, ceiling=neutral),
        rooms=[dict(x=x, z=0, width=3, depth=3, height=2.8) for x in (0, 5)],
        walls=walls,
        floor_patches=[dict(x=index, z=0, width=1, depth=3,
                            material=material(surface))
                       for index, surface in enumerate(MATERIALS)],
        ceiling_lights=[
            dict(id="source", fixture="core:fluorescent_panel_01", x=1.5,
                 z=1.5, y=2.79, align="none", color=colour,
                 brightness=0.52, emission=0.4, range=6),
            dict(id="bounce_source", fixture="core:fluorescent_panel_01",
                 x=6.5, z=0.7, y=2.79, align="none", color=COLOURS["white"],
                 brightness=0.52, emission=0.4, range=6),
        ],
    )


def author(out: Path) -> dict:
    assets = out / "payload/assets"
    levels = assets / "levels"
    sources = out / "sources"
    levels.mkdir(parents=True, exist_ok=True)
    sources.mkdir(parents=True, exist_ok=True)
    if (assets / "catalog.json").is_symlink():
        raise ValueError("The diagnostic catalogue must be a local regular file")
    for item in (ROOT / "assets").iterdir():
        if item.name not in ("catalog.json", "levels"):
            target = assets / item.name
            if not target.exists() and not target.is_symlink():
                target.symlink_to(item, target_is_directory=item.is_dir())
    catalogue = json.loads((ROOT / "assets/catalog.json").read_text())
    definitions = [dict(id=material(name), display_name="Colour control " + name,
                        asset_class="diagnostic", asset_type="material",
                        source="definition", texture="core:tex_white_fallback_01",
                        tile_metres=1, tint=tint)
                   for name, tint in MATERIALS.items()]
    ids = {entry["id"] for entry in definitions}
    catalogue["assets"] = [entry for entry in catalogue["assets"]
                           if entry["id"] not in ids] + definitions
    write_json(assets / "catalog.json", catalogue)
    for name, colour in COLOURS.items():
        write_json(sources / ("colour_response_" + name + ".json"), source(name, colour))
    manifest = dict(
        format_version=1, purpose="Developer-only incident RGB, albedo and bounce controls",
        asset_root=str(assets), runtime_asset_root=str(assets.parent),
        source_root=str(sources), package_root=str(levels),
        settings=dict(quality="high", texture_filtering="high", lightmaps="full",
                      reflections="off", bloom=False, vsync=True,
                      fov_degrees=65, window_mode="windowed",
                      window_width=640, window_height=360),
        capture=dict(ready_world_seconds=0.7, expected_drawable=[1280, 720]),
        views=[
            dict(name="albedo", spawn=[1.5, 1.6, 2.75, 0], camera=[0, -38]),
            dict(name="source", spawn=[1.5, 1.6, 2.75, 0], camera=[0, 40]),
            dict(name="bounce", spawn=[6.5, 1.6, 2.75, 0], camera=[0, -38]),
        ],
        samples=[
            dict(name="neutral_direct", position=[0.5, 0, 1.5], room=0,
                 kind="floor", albedo=MATERIALS["neutral"]),
            dict(name="red_direct", position=[1.5, 0, 1.5], room=0,
                 kind="floor", albedo=MATERIALS["red"]),
            dict(name="blue_direct", position=[2.5, 0, 1.5], room=0,
                 kind="floor", albedo=MATERIALS["blue"]),
            dict(name="red_bounce_shadow", position=[6.5, 0, 2.5], room=1,
                 kind="floor", albedo=MATERIALS["neutral"]),
        ],
        expected=dict(
            source_rgb=COLOURS,
            direct="All first-room direct incident RGB ratios equal the selected source; receiving albedo is excluded",
            outgoing="Linear diffuse radiance is incident RGB multiplied once by the sample's listed albedo",
            bounce="The baffle blocks direct light at the shadow sample; reflected red surfaces contribute red indirect light",
            recovery="Neutral reflectors preserve source hue; coloured reflectors may legitimately tint combined indirect energy",
            presentation="Compare linear stage dumps for exact ratios; native PNGs include display encoding and geometric face shade",
        ),
    )
    write_json(out / "capture-manifest.json", manifest)
    return dict(author_root=str(out), sources=list(COLOURS),
                manifest=str(out / "capture-manifest.json"))


def inspect(dump: Path, manifest: Path) -> dict:
    """Read the control's stage samples; never infer linear ratios from PNGs."""
    control = json.loads(manifest.read_text())
    records = json.loads((dump / "charts.json").read_text())
    stages = {}
    for stage in ("direct", "indirect", "filtered", "filled"):
        path = dump / (stage + ".rgb-f32le")
        if path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError("The tiny control reader accepts stages up to 64 MiB")
        stages[stage] = path.read_bytes()
    emitters = json.loads((dump / "emitters.json").read_text())
    samples = []
    for sample in control["samples"]:
        nearest = None
        for record in records:
            if (record["room"], record["kind"]) != (sample["room"], sample["kind"]):
                continue
            origin, uaxis, vaxis = (record[key] for key in ("origin", "u_axis", "v_axis"))
            dot = lambda a, b: sum(x * y for x, y in zip(a, b))
            delta = [a - b for a, b in zip(sample["position"], origin)]
            uu, uv, vv = dot(uaxis, uaxis), dot(uaxis, vaxis), dot(vaxis, vaxis)
            determinant = uu * vv - uv * uv
            if determinant <= 0:
                raise ValueError("Degenerate diagnostic chart")
            du, dv = dot(delta, uaxis), dot(delta, vaxis)
            u = max(0.0, min(1.0, (du * vv - dv * uv) / determinant))
            v = max(0.0, min(1.0, (dv * uu - du * uv) / determinant))
            width, height = record["rectangle"][2:]
            column, row = round(u * (width - 1)), round(v * (height - 1))
            u, v = column / max(1, width - 1), row / max(1, height - 1)
            position = [o + u * a + v * b for o, a, b in zip(origin, uaxis, vaxis)]
            distance = sum((a - b) ** 2 for a, b in zip(position, sample["position"]))
            if nearest is None or distance < nearest[0]:
                nearest = distance, record, position, record["offset"] + row * width + column
        if nearest is None or nearest[0] > 0.01:
            raise ValueError("No nearby control chart for " + sample["name"])
        _, record, position, index = nearest
        incident = {name: list(struct.unpack_from("<3f", data, index * 12))
                    for name, data in stages.items()}
        if not all(math.isfinite(value) and value >= 0
                   for rgb in incident.values() for value in rgb):
            raise ValueError("Nonfinite or negative stage sample")
        samples.append(dict(name=sample["name"], position=position, chart=record["index"],
                            albedo=sample["albedo"], incident=incident,
                            outgoing={name: [value * albedo for value, albedo in zip(rgb, sample["albedo"])]
                                      for name, rgb in incident.items()}))
    return dict(dump=str(dump), source_rgb=emitters[0][2], samples=samples,
                semantics="Raw linear incident stage RGB; outgoing applies listed linear albedo exactly once, before face shade and presentation")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--author", type=Path,
                        help="Explicit disposable output root; performs no bake or capture")
    mode.add_argument("--inspect", type=Path, help="Read a tiny control's Full stage dump")
    parser.add_argument("--manifest", type=Path, help="Control capture manifest required for --inspect")
    parser.add_argument("--out", type=Path, help="Optional JSON report destination")
    args = parser.parse_args()
    if args.author:
        out = args.author.resolve()
        if out == ROOT or out == ROOT / "assets" or ROOT / "assets" in out.parents:
            parser.error("The control root must be separate from production assets")
        result = author(out)
    else:
        if args.manifest is None:
            parser.error("--inspect requires --manifest")
        result = inspect(args.inspect.resolve(), args.manifest.resolve())
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        write_json(args.out, result)
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
