#!/usr/bin/env python3
"""Analyze existing hero lighting dumps without running the compiler or player.

Run from the repository root. Output goes to stdout; --write saves only the
companion final-transport-analysis.json in this script's directory.
"""

import argparse
import array
import bisect
import collections
import json
import math
from pathlib import Path
import statistics
import sys


STAGES = ("direct", "indirect", "filtered", "filled", "global-direct")
ROOTS = ("stage3-before", "stage3-sealed-solve")
QUALITIES = ("medium", "full")
BEVEL_POINT = (4.4520015717, 0.3699985147, 0.8420000076)
REGION_BOUNDS = {
    "warm_practical": (0.0, 2.2, 0.0, 2.2),
    "panel_pool": (4.4, 6.2, 2.2, 4.2),
    "window_floor": (2.2, 5.4, 0.0, 0.8),
    "far_left_recess": (0.3, 1.7, 4.0, 5.7),
}


def read_array(directory, filename, kind="f"):
    values = array.array(kind)
    values.frombytes((directory / filename).read_bytes())
    if sys.byteorder != "little":
        values.byteswap()
    return values


def analyze_dump(directory):
    receiver = read_array(directory, "receivers.f32le")
    surfaces = read_array(directory, "receiver-surfaces.u32le", "I")
    geometric = read_array(directory, "geometric-normals.rgb-f32le")
    stages = {name: read_array(directory, f"{name}.rgb-f32le") for name in STAGES}
    charts = json.loads((directory / "charts.json").read_text())
    offsets = [chart["offset"] for chart in charts]
    owners = json.loads((directory / "caster-ranges.json").read_text())
    provenance = json.loads((directory / "provenance.json").read_text())
    count = len(surfaces)
    if len(receiver) != count * 16 or len(geometric) != count * 3:
        raise ValueError(f"inconsistent receiver storage: {directory}")
    if any(len(values) != count * 3 for values in stages.values()):
        raise ValueError(f"inconsistent lighting storage: {directory}")

    def owned_indices(names):
        return {
            index
            for owner in owners
            if owner["owner"] in names
            for index in range(owner["first"], owner["end"])
        }

    room_surfaces = owned_indices({
        "architecture:Floor:home:carpet_cream_01",
        "architecture:Floor:home:hardwood_oak_warm_01",
    })
    garden_surfaces = owned_indices({"architecture:Floor:outdoor:grass_ground_01"})
    table_surfaces = owned_indices({"model:environment/pool/props/models/pool_table.glb"})
    sofa_surfaces = owned_indices({"model:environment/home/props/models/sofa.glb"})
    room = [i for i, surface in enumerate(surfaces)
            if surface in room_surfaces and receiver[16 * i + 7] > 0.99]
    regions = {"room_floor": room}
    for name, bounds in REGION_BOUNDS.items():
        x0, x1, z0, z1 = bounds
        regions[name] = [i for i in room
                         if x0 < receiver[16 * i] < x1
                         and z0 < receiver[16 * i + 2] < z1]
    regions["garden"] = [i for i, surface in enumerate(surfaces)
                         if surface in garden_surfaces
                         and receiver[16 * i + 7] > 0.99
                         and receiver[16 * i + 2] < 0.0
                         and abs(receiver[16 * i + 1]) < 0.01]
    regions["table_top"] = [i for i, surface in enumerate(surfaces)
                            if surface in table_surfaces
                            and receiver[16 * i + 7] > 0.99
                            and receiver[16 * i + 1] > 0.72]
    center = [i for i in regions["table_top"]
              if abs(receiver[16 * i] - 9.45) <= 0.0001
              and abs(receiver[16 * i + 2] - 1.2) <= 0.0001]
    center_indices = set(center)
    regions["table_top_center"] = center
    regions["table_top_without_center"] = [i for i in regions["table_top"]
                                            if i not in center_indices]

    def region_statistics(indices):
        if not indices:
            return {"samples": 0, "stages": {}}
        bounds = [[min(receiver[16 * i + axis] for i in indices),
                   max(receiver[16 * i + axis] for i in indices)]
                  for axis in range(3)]
        output = {}
        for name, values in stages.items():
            means = [sum(values[3 * i + channel] for i in indices) / len(indices)
                     for channel in range(3)]
            output[name] = {
                "mean_rgb": means,
                "mean_rgb_scalar": sum(means) / 3.0,
                "median_rgb_scalar": statistics.median(
                    sum(values[3 * i:3 * i + 3]) / 3.0 for i in indices),
                "zero_samples_at_1e_9": sum(
                    max(values[3 * i:3 * i + 3]) <= 1e-9 for i in indices),
                "blue_over_red_of_mean": means[2] / means[0] if means[0] > 0 else None,
            }
        return {"samples": len(indices), "world_bounds": bounds, "stages": output}

    groups = collections.defaultdict(list)
    for index, surface in enumerate(surfaces):
        if surface in sofa_surfaces:
            key = tuple(receiver[16 * index + axis] for axis in (0, 1, 2, 6, 7, 8))
            key += tuple(geometric[3 * index:3 * index + 3])
            groups[key].append(index)
    records = []
    for key, indices in groups.items():
        chart_ids = [bisect.bisect_right(offsets, index) - 1 for index in indices]
        if len(set(chart_ids)) < 2:
            continue
        spread = {
            name: [max(values[3 * i + channel] for i in indices)
                   - min(values[3 * i + channel] for i in indices)
                   for channel in range(3)]
            for name, values in stages.items()
        }
        records.append({
            "position": key[:3], "normal": key[3:6], "encoded_geometric_normal": key[6:],
            "receivers": indices, "charts": chart_ids,
            "surfaces": [surfaces[i] for i in indices],
            "ray_origins": [list(receiver[16 * i + 3:16 * i + 6]) for i in indices],
            "spread_rgb": spread,
        })

    def duplicate_statistics(selected):
        output = {}
        for name in STAGES:
            spreads = [record["spread_rgb"][name] for record in selected]
            output[name] = {
                "max_spread_rgb": [max((spread[c] for spread in spreads), default=0.0)
                                   for c in range(3)],
                "mean_spread_rgb": [sum(spread[c] for spread in spreads) / len(spreads)
                                    if spreads else 0.0 for c in range(3)],
                "groups_above_1e_6": sum(max(spread) > 1e-6 for spread in spreads),
            }
        return {"groups": len(selected),
                "samples": sum(len(record["receivers"]) for record in selected),
                "stages": output}

    def detailed_record(record):
        return dict(record, stage_rgb={
            name: [list(values[3 * i:3 * i + 3]) for i in record["receivers"]]
            for name, values in stages.items()
        })

    nearest = min(records, key=lambda record: sum(
        (record["position"][axis] - BEVEL_POINT[axis]) ** 2 for axis in range(3)))
    bevel = detailed_record(nearest)
    bevel["distance_from_requested_point_m"] = math.sqrt(sum(
        (nearest["position"][axis] - BEVEL_POINT[axis]) ** 2 for axis in range(3)))
    upward = [record for record in records if record["normal"] == (0.0, 1.0, 0.0)]
    worst = {name: detailed_record(max(records, key=lambda record: max(record["spread_rgb"][name])))
             for name in ("direct", "indirect", "filtered")}
    return {
        "path": str(directory),
        "provenance": {name: provenance[name] for name in
                       ("source", "source_sha256", "solver_revision", "geometry_revision", "quality", "settings")},
        "total_receivers": count,
        "regions": {name: region_statistics(indices) for name, indices in regions.items()},
        "sofa_duplicates": {"all_normals": duplicate_statistics(records),
                            "upward": duplicate_statistics(upward)},
        "nearest_bevel_shared_point": bevel,
        "worst_shared_sofa_points": worst,
    }


def gather_audits(path):
    quality = None
    output = []
    for line in path.read_text().splitlines():
        if "[compiler-progress] preparing " in line:
            quality = line.rsplit(" ", 1)[-1]
        if "[transport-gather] " not in line:
            continue
        record = json.loads(line.split("[transport-gather] ", 1)[1])
        front = record["opaque_or_mask_hits"] - record["backface_hits"]
        record.update({
            "quality": quality,
            "front_hits": front,
            "escape_per_traced_percent": 100.0 * record["escaping_paths"] / record["traced_paths"],
            "cache_empty_per_front_percent": 100.0 * record["cache_empty"] / front,
            "nearest_fallback_per_front_percent": 100.0 * record["cache_nearest_fallback"] / front,
            "zero_radiance_per_front_percent": 100.0 * record["all_active_layers_zero_radiance_hits"] / front,
            "zero_radiance_with_nonempty_stencil": record["all_active_layers_zero_radiance_hits"] - record["cache_empty"],
        })
        output.append(record)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    arguments = parser.parse_args()
    base = Path("debug-maps/art-style-hero/evidence")
    log = Path("docs/art-style/stage3/hero-diagnostic-sealed-build.log")
    report = {
        "format_version": 1,
        "origin": "read-only analysis of existing compiler diagnostic dumps",
        "definitions": {
            "region_bounds_x0_x1_z0_z1_open": REGION_BOUNDS,
            "regional_means": "Unweighted raw receiver RGB averages in the same physical bounds; not luminance or native pixels. Receiver densities differ, so these are distribution summaries, not matched-sample causal effects.",
            "sofa_groups": "Exact decoded-f32 position, shading normal and encoded geometric normal; Python identifies signed zeros. Only groups containing at least two distinct chart indices qualify.",
            "spread": "Channelwise maximum minus minimum within a qualifying group. Mean spread gives each qualifying group equal weight.",
            "indirect": "All previous-order diffuse gathers, including escaping sky in base order one. This is not an isolated surface-bounce-only export.",
            "gather": "Geometric counts, one active energy layer. Cache-empty is a subset of zero-radiance counts, not an energy-loss amount; fallbacks are separate successful lookup paths. Order-two escape paths add no second sky term.",
            "table_center": "Repeated samples at x9.45,z1.2 inside the plant positioned at y.73; covered geometry remains valid darkness.",
            "aggregate_limits": "Baseline compiler architecture images were unresolved; final compiler resolves numeric alpha and linear bounce albedo. This before/after includes that correction, receiver density/ownership, direct cosines, and filtering, so it cannot isolate any one change.",
        },
        "dumps": {root: {quality: analyze_dump(base / root / "lighting" / quality)
                         for quality in QUALITIES} for root in ROOTS},
        "gather_log": str(log),
        "gather_audits": gather_audits(log),
    }
    encoded = json.dumps(report, indent=2, allow_nan=False) + "\n"
    if arguments.write:
        output = Path(__file__).with_name("final-transport-analysis.json")
        output.write_text(encoded)
        print(output)
    else:
        print(encoded, end="")


if __name__ == "__main__":
    main()
