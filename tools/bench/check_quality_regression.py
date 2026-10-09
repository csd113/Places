#!/usr/bin/env python3
"""Reject stale or missing capture-time native quality/resource receipts.

Reads the genuine visual-diagnostics `final` capture records already emitted by
the native player. It supplies no synthetic scene or replacement image.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import zipfile


def package_resources(package: Path) -> dict[str, dict]:
    """Expected bindings come from the real selected archive, including named fallback."""
    resources = {}
    with zipfile.ZipFile(package) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        for variant in manifest["variants"]:
            entries = variant["entries"]
            metadata = json.loads(archive.read(entries["lightmaps_meta"])) if entries["lightmaps_meta"] else None
            atlas = dict(pages=0, page_edge=0, charts=0, chart_texels=0, bytes=0)
            if metadata:
                atlas.update(pages=metadata["page_count"], page_edge=metadata["page_edge"],
                             charts=metadata["stats"]["charts"], chart_texels=metadata["stats"]["texels"],
                             bytes=2 * (1 + len(metadata["switchable_lights"])) * metadata["page_count"]
                             * metadata["page_edge"] ** 2 * 8)
            resources[variant["lightmap_quality"]] = dict(
                atlas=atlas, lightmaps_resident=bool(metadata), irradiance_present=bool(entries["irradiance"]),
                lightmap_failure=variant["lightmap_failure"])
    return resources


def inspect(log: str, expected: list[dict], level: str, minimum_dynamic: int = 0,
            resources: dict[str, dict] | None = None, chair: dict | None = None) -> list[dict]:
    captures = []
    entity_blocks = []
    pending_entities = []
    for line in log.splitlines():
        if line.startswith("[entity-spatial] "):
            pending_entities.append(json.loads(line.removeprefix("[entity-spatial] ")))
        if line.startswith("[visual-diagnostic] "):
            receipt = json.loads(line.removeprefix("[visual-diagnostic] "))
            if receipt.get("event") == "capture":
                captures.append(receipt)
                entity_blocks.append(pending_entities)
                pending_entities = []
    if len(captures) != len(expected):
        raise ValueError(f"Expected {len(expected)} actual captures, received {len(captures)}")
    preserved = None
    for index, (receipt, spec) in enumerate(zip(captures, expected)):
        if receipt.get("mode") != "final" or receipt.get("level") != level:
            raise ValueError(f"Capture {index}: wrong diagnostic/world")
        for state in ("requested", "applied"):
            if any(receipt[state].get(key) != value for key, value in spec.items()):
                raise ValueError(f"Capture {index}: {state} graphics differ from expected")
        resident = receipt["resident"]
        for key in ("quality", "filtering", "lightmaps"):
            if resident.get(key) != spec[key]:
                raise ValueError(f"Capture {index}: resident {key} differs from expected")
        if resources is not None:
            selected = resources[spec["lightmaps"]]
            if resident.get("lightmaps_resident") != selected["lightmaps_resident"]:
                raise ValueError(f"Capture {index}: actual atlas availability differs from package")
            if any((resident.get("atlas") or {}).get(key) != value for key, value in selected["atlas"].items()):
                raise ValueError(f"Capture {index}: actual atlas inventory differs from package")
            if (resident.get("irradiance_field") is not None) != selected["irradiance_present"]:
                raise ValueError(f"Capture {index}: actual entity field availability differs from package")
            if resident.get("reflection_enabled") != (spec["reflections"] != "off"):
                raise ValueError(f"Capture {index}: actual reflection resource gate differs from request")
        submission = receipt.get("capture_submission") or {}
        if submission.get("draw_calls", 0) <= 0 or submission.get("submitted_indices", 0) <= 0:
            raise ValueError(f"Capture {index}: no actual scene submission")
        if (resident.get("dynamic") or {}).get("objects", 0) < minimum_dynamic:
            raise ValueError(f"Capture {index}: runtime object did not survive")
        if resident.get("movable_visibility_error"):
            raise ValueError(f"Capture {index}: invalid actual visibility resource")
        if receipt["frame"].get("drawable") != [1280, 720]:
            raise ValueError(f"Capture {index}: drawable changed")
        if receipt["frame"].get("diagnostic_identity_post"):
            raise ValueError(f"Capture {index}: final composition was bypassed")
        if chair is not None:
            if preserved is None:
                candidates = [entity for entity in entity_blocks[index] if entity["path"] == chair["path"]
                              and all(abs(entity["position"][axis] - chair["centre"][axis]) <= chair["tolerance_m"]
                                      for axis in range(3))]
                if len(candidates) != 1:
                    raise ValueError("First capture must uniquely identify the scripted chair near its planned bounds centre")
                preserved = {key: candidates[0][key] for key in ("name", "path", "position")}
            matches = [entity for entity in entity_blocks[index] if entity["name"] == preserved["name"]
                       and entity["path"] == preserved["path"]]
            if len(matches) != 1 or matches[0]["position"] != preserved["position"] or matches[0].get("spatial") is None:
                raise ValueError(f"Capture {index}: identified chair identity/position/uploaded spatial payload was lost")
            receipt["checked_scripted_chair"] = dict(matches[0], scope="Actual uploaded object preservation; pixel visibility requires native image inspection.")
    return captures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    parser.add_argument("--level", required=True)
    parser.add_argument("--minimum-dynamic", type=int, default=0)
    parser.add_argument("--package", type=Path, help="Require native atlas/probe availability to match the actual archive")
    parser.add_argument("--chair-sequence", type=Path, help="Require scripted key9007 chair's actual capture-only entity trace to persist")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    resources = package_resources(args.package) if args.package else None
    chair = None
    if args.chair_sequence:
        script = json.loads(args.chair_sequence.read_text())
        spawned = [action for step in script for action in step.get("actions", [])
                   if action.get("action") == "spawn" and action.get("key") == 9007]
        if len(spawned) != 1 or spawned[0]["model"] != "home:dining_chair" or spawned[0]["scale"] != 1:
            parser.error("Chair sequence must contain exactly the planned unit-scale key9007 dining chair")
        position = spawned[0]["position"]
        chair = dict(path="dynamic:environment/home/props/models/dining_chair.glb",
                     centre=[position[0], position[1] + 0.451, position[2]], tolerance_m=0.15,
                     centre_scope="Declared .902m chair bounds height; capture1 pins the actual centre and identity thereafter.")
    captures = inspect(args.log.read_text(), json.loads(args.expected.read_text()),
                       args.level, args.minimum_dynamic, resources, chair)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("x") as stream:
        json.dump(dict(log=str(args.log), level=args.level, captures=captures,
                       minimum_dynamic=args.minimum_dynamic, status="passed",
                       package=str(args.package) if args.package else None, package_resources=resources,
                       chair_sequence=str(args.chair_sequence) if args.chair_sequence else None, chair=chair,
                       scope="Settled native capture endpoints; no loading-transient or presented-FPS claim."), stream, indent=2)
        stream.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
