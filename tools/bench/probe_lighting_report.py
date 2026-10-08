#!/usr/bin/env python3
"""Read-only PLPF v2/v3 / atlas audit, using neutral (unit-albedo) illumination.

Usage: probe_lighting_report.py PACKAGE --locations locations.json [--variant full]
Locations: [{"name": "pool", "position": [x,y,z], "room": 3}, ...].
Reports candidate outliers; room boundaries and occlusion must explain them before
calling them defects. Measurements are raw working-light units, before tone mapping.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import zipfile

LIMIT = 256 * 1024 * 1024
LUMA = (0.2126, 0.7152, 0.0722)


def luminance(rgb):
    return sum(value * weight for value, weight in zip(rgb, LUMA))


def read_field(data):
    if len(data) < 38 or data[:4] != b"PLPF":
        raise ValueError("expected PLPF v2/v3")
    version = struct.unpack_from("<H", data, 4)[0]
    if version not in (2, 3):
        raise ValueError("expected PLPF v2/v3")
    x, y, z, cell, nx, ny, nz, count = struct.unpack_from("<4f4I", data, 6)
    if not all(math.isfinite(value) for value in (x, y, z, cell)) or cell <= 0:
        raise ValueError("invalid probe lattice")
    if any(value < 1 or value > 64 for value in (nx, ny, nz)) or count != nx * ny * nz:
        raise ValueError("invalid probe count")
    base_end = 38 + 36 * count
    if len(data) < base_end or (version == 2 and len(data) != base_end):
        raise ValueError("invalid PLPF length")
    probes = []
    for index, values in enumerate(struct.iter_unpack("<8fi", data[38:base_end])):
        if not all(math.isfinite(value) for value in values[:8]) or min(values[:3]) < 0 or max(values[:3]) > 65504:
            raise ValueError("invalid probe energy")
        if not -1 <= values[8] <= 32767:
            raise ValueError("invalid room label")
        if any(value < 0 or value > 1 for value in values[6:8]):
            raise ValueError("invalid reserved axes")
        if math.sqrt(sum(value * value for value in values[3:6])) > sum(values[:3]) * (1 + 32 * 2**-23):
            raise ValueError("moment exceeds probe energy")
        position = (x + (index % nx + 0.5) * cell,
                    y + ((index // nx) % ny + 0.5) * cell,
                    z + (index // nx // ny + 0.5) * cell)
        probes.append({"index": index, "position": position, "irradiance": values[:3],
                       "moment": values[3:6], "room": values[8]})
    selected = ()
    if version == 3:
        if len(data) < base_end + 4:
            raise ValueError("missing PLPF selected direct extension")
        light_count, = struct.unpack_from("<I", data, base_end)
        direct_start = base_end + 4 + light_count * 4
        expected_end = direct_start + (count * 24 if light_count else 0)
        if light_count > 8 or len(data) != expected_end:
            raise ValueError("invalid PLPF selected direct length")
        selected = struct.unpack_from(f"<{light_count}I", data, base_end + 4)
        if any(a >= b for a, b in zip(selected, selected[1:])):
            raise ValueError("invalid PLPF selected source order")
        if light_count:
            for probe, direct in zip(probes, struct.iter_unpack("<6f", data[direct_start:])):
                means, moment = direct[:3], direct[3:]
                if not all(math.isfinite(value) for value in direct) or min(means) < 0 or max(means) > 65504:
                    raise ValueError("invalid selected direct energy")
                tolerance = sum(probe["irradiance"]) * 32 * 2**-23
                if math.sqrt(sum(value * value for value in moment)) > sum(means) * (1 + 32 * 2**-23):
                    raise ValueError("selected direct moment exceeds energy")
                residual = tuple(total - local for total, local in zip(probe["irradiance"], means))
                remaining_moment = tuple(total - local for total, local in zip(probe["moment"], moment))
                if min(residual) < -tolerance or math.sqrt(sum(value * value for value in remaining_moment)) > sum(max(0, value) for value in residual) + tolerance:
                    raise ValueError("invalid selected direct residual")
                probe["selected_direct"] = means
                probe["selected_direct_moment"] = moment
                probe["nonlocal_irradiance"] = tuple(max(0, value) for value in residual)
                probe["nonlocal_moment"] = remaining_moment
    return {"origin": (x, y, z), "cell_m": cell, "dims": (nx, ny, nz), "probes": probes,
            "record_version": version, "selected_light_indices": selected}


def reconstruct(irradiance, moment, normal):
    energy = sum(irradiance)
    if energy <= 1e-6:
        return irradiance
    magnitude = math.sqrt(sum(value * value for value in moment))
    adjustment = 2 * max(0, sum(a * b for a, b in zip(moment, normal))) - magnitude
    return tuple(max(0, value + value / energy * adjustment) for value in irradiance)


def atlas_samples(meta, image, locations=()):
    if meta["record_version"] != 3 or image[:12] != b"\xabKTX 20\xbb\r\n\x1a\n":
        raise ValueError("expected moment atlas v3 / KTX2")
    fmt, size, edge, height, depth, layers, faces, levels, compression = struct.unpack_from("<9I", image, 12)
    if (fmt, size, height, depth, faces, levels, compression) != (97, 2, edge, 0, 1, 1, 0):
        raise ValueError("expected uncompressed RGBA16F atlas")
    if not 1 <= edge <= 4096 or edge != meta["page_edge"] or layers < 2 * meta["page_count"]:
        raise ValueError("invalid atlas dimensions")
    offset, length, raw_length = struct.unpack_from("<3Q", image, 80)
    if length != raw_length or length != edge * edge * 8 * layers or offset + length > len(image):
        raise ValueError("invalid atlas length")
    samples = []
    nearest = [None] * len(locations)
    # Bound diagnostics to 65536 distributed samples plus one per location.
    charts = meta["charts"]
    stride = max(1, math.ceil(len(charts) / 4096))
    def sample_at(patch, chart, normal, tx, ty):
        px, py = chart["x"] + tx, chart["y"] + ty
        if px >= edge or py >= edge:
            raise ValueError("chart exceeds page")
        u = tx / (chart["width"] - 1) if chart["width"] > 1 else 0.5
        v = ty / (chart["height"] - 1) if chart["height"] > 1 else 0.5
        position = tuple(o + u*a + v*b for o, a, b in zip(patch["origin"], patch["u_axis"], patch["v_axis"]))
        pixel = py * edge + px
        base = (chart["page"] * 2 * edge * edge + pixel) * 8 + offset
        irr = struct.unpack_from("<4e", image, base)[:3]
        moment = struct.unpack_from("<4e", image, base + edge*edge*8)[:3]
        return {"position":position,"normal":normal,"room":patch["room"],
                "kind":patch["kind"],"light":reconstruct(irr,moment,normal)}

    for chart_index, record in enumerate(charts):
        patch, chart = record["patch"], record["chart"]
        if chart["page"] >= meta["page_count"]:
            raise ValueError("invalid chart page")
        uaxis, vaxis = patch["u_axis"], patch["v_axis"]
        normal = (uaxis[1]*vaxis[2]-uaxis[2]*vaxis[1],
                  uaxis[2]*vaxis[0]-uaxis[0]*vaxis[2],
                  uaxis[0]*vaxis[1]-uaxis[1]*vaxis[0])
        norm = math.sqrt(sum(value * value for value in normal))
        if norm <= 0:
            raise ValueError("degenerate chart")
        normal = tuple(value / norm for value in normal)
        if normal[1] > 0.5:
            # Project each requested point onto the patch and round to a real
            # atlas texel. This avoids choosing a distant coarse diagnostic tap.
            dot = lambda a,b: sum(x*y for x,y in zip(a,b))
            uu, uv, vv = dot(uaxis,uaxis), dot(uaxis,vaxis), dot(vaxis,vaxis)
            determinant = uu*vv - uv*uv
            if determinant <= 0:
                raise ValueError("degenerate patch axes")
            for index, location in enumerate(locations):
                if location["room"] != patch["room"]: continue
                delta = tuple(p-o for p,o in zip(location["position"],patch["origin"]))
                du, dv = dot(delta,uaxis), dot(delta,vaxis)
                u = max(0,min(1,(du*vv-dv*uv)/determinant))
                v = max(0,min(1,(dv*uu-du*uv)/determinant))
                tx, ty = round(u*(chart["width"]-1)), round(v*(chart["height"]-1))
                sample = sample_at(patch,chart,normal,tx,ty)
                distance = sum((p-q)**2 for p,q in zip(sample["position"],location["position"]))
                if nearest[index] is None or distance < nearest[index][0]:
                    nearest[index] = (distance,sample)
        if chart_index % stride: continue
        tapsx, tapsy = min(4, chart["width"]), min(4, chart["height"])
        for j in range(tapsy):
            for i in range(tapsx):
                tx = i * (chart["width"] - 1) // max(1, tapsx - 1)
                ty = j * (chart["height"] - 1) // max(1, tapsy - 1)
                samples.append(sample_at(patch,chart,normal,tx,ty))
    samples.extend(value[1] for value in nearest if value is not None)
    return samples


def declared_entry(archive, entry, limit=LIMIT):
    info = archive.getinfo(entry["name"])
    if info.file_size > limit or info.file_size != entry["bytes"]:
        raise ValueError("entry exceeds declared size or diagnostic budget")
    with archive.open(info) as stream:
        data = stream.read(limit + 1)
    if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
        raise ValueError("entry size/hash mismatch")
    return data


def report(package, locations, variant="full"):
    if len(locations) > 64:
        raise ValueError("at most 64 representative locations")
    with zipfile.ZipFile(package) as archive:
        with archive.open("manifest.json") as stream:
            manifest = json.loads(stream.read(2 * 1024 * 1024 + 1))
        entries = {entry["name"]: entry for entry in manifest["entries"]}
        selected = next(item for item in manifest["variants"] if item["lightmap_quality"] == variant)["entries"]
        raw = declared_entry(archive, entries[selected["irradiance"]])
        field = read_field(raw)
        valid = [probe for probe in field["probes"] if probe["room"] >= 0]
        probe_locations = []
        for location in locations:
            candidates = [probe for probe in valid if probe["room"] == location["room"]]
            if candidates:
                probe = min(candidates,key=lambda p:sum((a-b)**2 for a,b in zip(p["position"],location["position"])))
                probe_locations.append({"position":probe["position"],"room":probe["room"]})
        meta = json.loads(declared_entry(archive, entries[selected["lightmaps_meta"]]))
        surfaces = atlas_samples(meta, declared_entry(archive, entries[selected["lightmaps"]]), locations+probe_locations)
    outliers = []
    nx, ny, _ = field["dims"]
    for probe in valid:
        index = probe["index"]
        neighbors = []
        if index % nx + 1 < nx: neighbors.append(index + 1)
        if (index // nx) % ny + 1 < ny: neighbors.append(index + nx)
        if index + nx*ny < len(field["probes"]): neighbors.append(index + nx*ny)
        for index2 in neighbors:
            other = field["probes"][index2]
            if other["room"] != probe["room"]: continue
            a, b = luminance(probe["irradiance"]), luminance(other["irradiance"])
            if max(a,b) > 100 * max(min(a,b), 1e-6):
                outliers.append((index, index2, a, b))
    measured = []
    for location in locations:
        position, room = location["position"], location["room"]
        distance = lambda point: sum((a-b)**2 for a,b in zip(point["position"],position))
        candidates = [probe for probe in valid if probe["room"] == room]
        floors = [surface for surface in surfaces if surface["room"] == room and surface["normal"][1] > 0.5]
        probe = min(candidates,key=distance) if candidates else None
        surface = min(floors,key=distance) if floors else None
        beneath_probe = min(floors,key=lambda p:sum((a-b)**2 for a,b in zip(p["position"],probe["position"]))) if floors and probe else None
        upward = reconstruct(probe["irradiance"], probe["moment"], (0,1,0)) if probe else None
        probe_luma = luminance(upward) if upward else None
        static_luma = luminance(surface["light"]) if surface else None
        flags = []
        if probe_luma is not None and static_luma is not None:
            if static_luma > 0.05 and probe_luma < static_luma / 100:
                flags.append("near-zero probe beside lit static sample")
            if probe_luma > 0.1 and static_luma < probe_luma / 100:
                flags.append("bright probe beside dark static sample")
        measured.append({"name":location["name"],"requested_position":position,"room":room,
                         "probe":probe,"probe_distance_m":math.sqrt(distance(probe)) if probe else None,
                         "probe_upward_rgb":upward,"probe_upward_luminance":probe_luma,
                         "nearby_static":surface,"static_distance_m":math.sqrt(distance(surface)) if surface else None,
                         "static_luminance":static_luma,"static_beneath_probe":beneath_probe,
                         "static_beneath_probe_luminance":luminance(beneath_probe["light"]) if beneath_probe else None,
                         "candidate_flags":flags})
    return {"variant":variant,"origin":field["origin"],"dims":field["dims"],"cell_m":field["cell_m"],
            "record_version":field["record_version"],"selected_light_indices":field["selected_light_indices"],
            "serialized_probe_bytes":len(raw),
            "probe_count":len(field["probes"]),"valid_count":len(valid),
            "irradiance_sha256":hashlib.sha256(raw).hexdigest(),
            "valid_luminance_range":(min(map(lambda p:luminance(p["irradiance"]),valid), default=0),
                                     max(map(lambda p:luminance(p["irradiance"]),valid), default=0)),
            "adjacent_100x_candidates":outliers,"locations":measured}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package",type=Path)
    parser.add_argument("--locations",type=Path,required=True)
    parser.add_argument("--variant",choices=("medium","full"),default="full")
    args = parser.parse_args()
    print(json.dumps(report(args.package,json.loads(args.locations.read_text()),args.variant),indent=2,allow_nan=False))


if __name__ == "__main__":
    main()
