#!/usr/bin/env python3
"""Read-only reproduction of the source-derived Home animation qualification.

Run from the repository root; emits JSON to stdout only. Requires NumPy/Pillow.
It never changes captures, production inputs, binaries, or original receipts.
The input pins are independent of observed pixel bounds. Pose/time is untraced.
"""
import hashlib
import json
import math
from pathlib import Path
import re
import struct
import sys

import numpy as np
from PIL import Image


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def canonical_sha(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def rotation(quaternion):
    x, y, z, w = quaternion / np.linalg.norm(quaternion)
    return np.array([[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)],
                     [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)],
                     [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)]])


def globals_for(nodes, parents, translations, rotations, scales):
    result = [None] * len(nodes)

    def visit(index):
        if result[index] is None:
            local = np.eye(4)
            local[:3, :3] = rotation(rotations[index]) @ np.diag(scales[index])
            local[:3, 3] = translations[index]
            parent = parents[index]
            result[index] = local if parent is None else visit(parent) @ local
        return result[index]

    return np.stack([visit(i) for i in range(len(nodes))])


def idle_envelope(model_path, clip_name):
    blob = Path(model_path).read_bytes()
    assert blob[:4] == b"glTF" and struct.unpack_from("<I", blob, 4)[0] == 2
    json_size, json_type = struct.unpack_from("<II", blob, 12)
    assert json_type == 0x4e4f534a
    model = json.loads(blob[20:20+json_size])
    binary_size, binary_type = struct.unpack_from("<II", blob, 20+json_size)
    assert binary_type == 0x004e4942
    binary = blob[28+json_size:28+json_size+binary_size]

    def accessor(index):
        item = model["accessors"][index]
        assert "sparse" not in item
        view = model["bufferViews"][item["bufferView"]]
        dtype = {5121:"<u1", 5123:"<u2", 5125:"<u4", 5126:"<f4"}[item["componentType"]]
        width = {"SCALAR":1, "VEC2":2, "VEC3":3, "VEC4":4, "MAT4":16}[item["type"]]
        offset = view.get("byteOffset", 0) + item.get("byteOffset", 0)
        stride = view.get("byteStride", np.dtype(dtype).itemsize * width)
        data = np.ndarray((item["count"], width), dtype=dtype, buffer=binary,
                          offset=offset, strides=(stride, np.dtype(dtype).itemsize)).copy()
        if item.get("normalized", False):
            data = data.astype(np.float64) / np.iinfo(dtype).max
        assert np.isfinite(data).all()
        return data

    nodes = model["nodes"]
    assert all("matrix" not in node for node in nodes)
    parents = [None] * len(nodes)
    for parent, node in enumerate(nodes):
        for child in node.get("children", []):
            assert parents[child] is None
            parents[child] = parent
    translations = np.array([n.get("translation", [0,0,0]) for n in nodes], dtype=float)
    rotations = np.array([n.get("rotation", [0,0,0,1]) for n in nodes], dtype=float)
    scales = np.array([n.get("scale", [1,1,1]) for n in nodes], dtype=float)
    assert (scales > 0).all()
    rest = globals_for(nodes, parents, translations, rotations, scales)
    mesh_nodes = [(i, n) for i, n in enumerate(nodes) if "mesh" in n]
    assert len(mesh_nodes) == 1
    mesh_index, mesh_node = mesh_nodes[0]
    assert np.array_equal(rest[mesh_index], np.eye(4)), "formula requires actual identity mesh node"
    skin = model["skins"][mesh_node["skin"]]
    joint_nodes = skin["joints"]
    primitives = model["meshes"][mesh_node["mesh"]]["primitives"]
    assert all("targets" not in p for p in primitives)
    positions = np.concatenate([accessor(p["attributes"]["POSITION"]) for p in primitives]).astype(float)
    joint_slots = np.concatenate([accessor(p["attributes"]["JOINTS_0"]) for p in primitives]).astype(int)
    weights = np.concatenate([accessor(p["attributes"]["WEIGHTS_0"]) for p in primitives]).astype(float)
    assert (weights >= 0).all() and (weights.sum(axis=1) > 0).all()
    assert ((joint_slots >= 0) & (joint_slots < len(joint_nodes))).all()
    weights /= weights.sum(axis=1, keepdims=True)
    # gltf::append_skinned_vertices first bakes raw attributes through the
    # joint-rest/inverseBind matrices. They are close to identity for this
    # asset, but not bit-exact identity: audit the actual parser contract.
    raw_positions = positions.copy()
    inverse_bind = accessor(skin["inverseBindMatrices"]).reshape((-1,4,4)).transpose(0,2,1).astype(float)
    assert len(inverse_bind) == len(joint_nodes)
    bind_matrices = np.stack([rest[node] @ inverse_bind[slot] for slot,node in enumerate(joint_nodes)])
    raw_homogeneous = np.column_stack((raw_positions, np.ones(len(raw_positions))))
    bind_by_joint = np.stack([(raw_homogeneous @ matrix.T)[:, :3] for matrix in bind_matrices], axis=1)
    rows = np.arange(len(positions))[:, None]
    positions = np.sum(bind_by_joint[rows, joint_slots] * weights[:, :, None], axis=1)
    maximum_bind_displacement = float(np.linalg.norm(positions-raw_positions, axis=1).max())
    maximum_bind_matrix_residual = float(np.abs(bind_matrices-np.eye(4)).max())
    clip = next(a for a in model["animations"] if a["name"] == clip_name)
    channels = []
    times = None
    for channel in clip["channels"]:
        sampler = clip["samplers"][channel["sampler"]]
        assert sampler.get("interpolation", "LINEAR") == "LINEAR"
        channel_times = accessor(sampler["input"]).ravel()
        if times is None:
            times = channel_times
        assert np.array_equal(times, channel_times)
        assert channel["target"]["path"] in ("rotation", "translation"), "no animated scale in audited idle"
        channels.append((channel["target"]["node"], channel["target"]["path"], accessor(sampler["output"]).astype(float)))
    assert np.all(np.diff(times) > 0)
    poses = []
    for key in range(len(times)):
        current_t, current_q = translations.copy(), rotations.copy()
        for node, path, values in channels:
            (current_t if path == "translation" else current_q)[node] = values[key]
        current_q /= np.linalg.norm(current_q, axis=1, keepdims=True)
        poses.append((current_t, current_q, globals_for(nodes, parents, current_t, current_q, scales)))
    homogeneous = np.column_stack((positions, np.ones(len(positions))))
    inverse_rest = [np.linalg.inv(rest[node]) for node in joint_nodes]
    rest_local = [homogeneous @ inverse.T for inverse in inverse_rest]
    minimum, maximum = np.full(3, np.inf), np.full(3, -np.inf)
    max_rotation, max_displacement = 0.0, 0.0
    for first, second in zip(poses, poses[1:]):
        first_t, first_q, first_globals = first
        second_t, second_q, _ = second
        dot = np.abs(np.sum(first_q * second_q, axis=1)).clip(0, 1)
        angles = 2 * np.arccos(dot)
        max_rotation = max(max_rotation, float(angles.max()))
        errors_by_joint, reference_by_joint = [], []
        for slot, node in enumerate(joint_nodes):
            radius = np.linalg.norm(rest_local[slot][:, :3], axis=1)
            error = np.zeros(len(positions))
            current = node
            while current is not None:
                scale = float(np.max(scales[current]))
                error = scale * (error + 2*math.sin(float(angles[current])/2)*radius)
                error += np.linalg.norm(second_t[current] - first_t[current])
                radius = scale*radius + max(np.linalg.norm(first_t[current]), np.linalg.norm(second_t[current]))
                current = parents[current]
            errors_by_joint.append(error)
            reference_by_joint.append((rest_local[slot] @ first_globals[node].T)[:, :3])
        errors_by_joint = np.stack(errors_by_joint, axis=1)
        reference_by_joint = np.stack(reference_by_joint, axis=1)
        rows = np.arange(len(positions))[:, None]
        error = np.sum(errors_by_joint[rows, joint_slots] * weights, axis=1)
        reference = np.sum(reference_by_joint[rows, joint_slots] * weights[:, :, None], axis=1)
        minimum = np.minimum(minimum, np.min(reference-error[:, None], axis=0))
        maximum = np.maximum(maximum, np.max(reference+error[:, None], axis=0))
        max_displacement = max(max_displacement, float(error.max()))
    return {"vertices":len(positions), "joint_count":len(joint_nodes), "idle_keys":len(times),
            "idle_duration_seconds":float(times[-1]-times[0]), "intervals":len(times)-1,
            "mesh_rest_identity":True, "unweighted_vertices":0,
            "raw_position_bounds":[raw_positions.min(axis=0).tolist(), raw_positions.max(axis=0).tolist()],
            "bind_bounds":[positions.min(axis=0).tolist(), positions.max(axis=0).tolist()],
            "parser_bind_bake_included":True,
            "maximum_parser_bind_displacement_m":maximum_bind_displacement,
            "maximum_joint_rest_inverse_bind_identity_residual":maximum_bind_matrix_residual,
            "idle_all_time_conservative_bounds":[minimum.tolist(), maximum.tolist()],
            "maximum_interval_rotation_degrees":math.degrees(max_rotation),
            "maximum_conservative_interval_vertex_displacement_m":max_displacement}


def project(bounds, transform, camera, size):
    corners = np.array([[x,y,z,1] for x in [bounds[0][0],bounds[1][0]]
                       for y in [bounds[0][1],bounds[1][1]] for z in [bounds[0][2],bounds[1][2]]])
    world = (corners @ np.array(transform).T)[:, :3]
    yaw, pitch = np.radians(camera["camera"])
    forward = np.array([math.sin(yaw)*math.cos(pitch), math.sin(pitch), -math.cos(yaw)*math.cos(pitch)])
    right = np.cross(forward, [0,1,0]); right /= np.linalg.norm(right)
    up = np.cross(right, forward)
    relative = world-np.array(camera["spawn"][:3])
    depth = relative @ forward
    assert (depth > 0).all()
    width, height = size
    focal = height / (2*math.tan(math.radians(camera["fov_degrees"])/2))
    screen = np.column_stack((width/2 + focal*(relative@right)/depth,
                              height/2 - focal*(relative@up)/depth))
    continuous = [*screen.min(axis=0), *screen.max(axis=0)]
    return {"continuous":list(map(float,continuous)),
            "integer_xyxy_exclusive":[math.floor(continuous[0]), math.floor(continuous[1]),
                                      math.ceil(continuous[2]), math.ceil(continuous[3])],
            "minimum_camera_depth_m":float(depth.min())}


def compare_pixels(first, second, footprint):
    assert first.shape == second.shape
    changed = np.any(first != second, axis=2)
    outside = changed.copy()
    x0,y0,x1,y1 = footprint
    outside[y0:y1,x0:x1] = False
    if outside.any():
        raise AssertionError("RGBA mismatch outside source-derived animation footprint")
    ys,xs = np.nonzero(changed)
    bbox = [int(xs.min()),int(ys.min()),int(xs.max()+1),int(ys.max()+1)] if len(xs) else None
    return {"changed_rgba_pixels":int(changed.sum()), "outside_changed_rgba_pixels":0,
            "alpha_changed_pixels":int(np.count_nonzero(first[:,:,3] != second[:,:,3])),
            "bbox_xyxy_exclusive":bbox}


def compare_payloads(reference, candidate):
    if reference != candidate:
        raise AssertionError("Restored captured entity payload differs")


def traces_for(log):
    centres, spatial, captures = {}, {}, {}
    visual = None
    for line in Path(log).read_text().splitlines():
        if line.startswith("[entity-light] name="):
            name = line.split("name=",1)[1].split(" path=",1)[0]
            centres[name] = line
        elif line.startswith("[entity-spatial] "):
            value = json.loads(line[len("[entity-spatial] "):]); spatial[value["name"]] = value
        elif line.startswith("[visual-diagnostic] "):
            value = json.loads(line[len("[visual-diagnostic] "):])
            if value.get("event") == "capture": visual = value
        elif line.startswith("PLACES_CAPTURE: wrote "):
            path = line[len("PLACES_CAPTURE: wrote "):]
            captures[path] = {"centres":centres.copy(), "spatial":spatial.copy(), "visual":visual}
            centres, spatial, visual = {}, {}, None
    return captures


def main():
    inputs_path = Path(__file__).with_suffix(".inputs.json")
    inputs = json.loads(inputs_path.read_text())
    for pin in inputs["pins"]:
        assert sha(pin["path"]) == pin["sha256"], f"changed input: {pin['path']}"
    envelope = idle_envelope(inputs["model"], inputs["clip"])
    manifest = json.loads(Path(inputs["projection_manifest"]).read_text())[0]
    camera = {**manifest["view"], "fov_degrees":manifest["settings"]["fov_degrees"]}
    assert inputs["drawable"][0]/inputs["drawable"][1] >= 480/272
    projection = project(envelope["idle_all_time_conservative_bounds"], inputs["transform"], camera, inputs["drawable"])
    footprint = projection["integer_xyxy_exclusive"]
    images = json.loads(Path(inputs["image_report"]).read_text())["comparisons"]
    pixels = []
    for comparison in images:
        initial, restored = comparison["initial"], comparison["restored"]
        assert sha(initial["path"]) == initial["sha256"] and sha(restored["path"]) == restored["sha256"]
        first = np.array(Image.open(initial["path"]).convert("RGBA"))
        second = np.array(Image.open(restored["path"]).convert("RGBA"))
        assert [first.shape[1], first.shape[0]] == inputs["drawable"]
        result = compare_pixels(first, second, footprint)
        assert result["changed_rgba_pixels"] == comparison["changed_pixels"]
        pixels.append({"index":comparison["index"], "kind":comparison["kind"],
                       "initial":initial, "restored":restored, **result})
    captures = {}
    for log in inputs["logs"]:
        captures.update(traces_for(log))
    payloads, visual_snapshots = [], []
    endpoints = []
    for path in inputs["high_endpoint_paths"]:
        capture = captures[path]
        payload = {"cat_centre":capture["centres"][inputs["cat_name"]],
                   "cat_spatial":capture["spatial"][inputs["cat_name"]],
                   "drum_centre":capture["centres"][inputs["drum_name"]],
                   "drum_spatial":capture["spatial"][inputs["drum_name"]]}
        payloads.append(payload)
        assert "source: Prepared" in payload["cat_centre"] and "source: Prepared" in payload["drum_centre"]
        for name in ("cat_spatial", "drum_spatial"):
            assert payload[name]["spatial"]["enabled"]
            assert all(anchor["residual_irradiance_validity"][3] == 1 for anchor in payload[name]["spatial"]["anchors"])
        matrix_columns = re.findall(r"[xyzw]_axis: Vec4\(([^)]+)\)", payload["cat_centre"])
        matrix = np.array([[float(v) for v in column.split(",")] for column in matrix_columns]).T
        assert np.array_equal(matrix, np.array(inputs["transform"]))
        snapshot = capture["visual"]
        if snapshot is not None:
            selected = {key:snapshot["resident"][key] for key in ("world", "props", "lighting_uniform")}
            visual_snapshots.append(selected)
        endpoints.append({"path":path, "payload_sha256":canonical_sha(payload),
                          "cat_centre_sha256":hashlib.sha256(payload["cat_centre"].encode()).hexdigest(),
                          "cat_spatial_sha256":canonical_sha(payload["cat_spatial"]),
                          "drum_centre_sha256":hashlib.sha256(payload["drum_centre"].encode()).hexdigest(),
                          "drum_spatial_sha256":canonical_sha(payload["drum_spatial"]),
                          "has_visual_snapshot":snapshot is not None})
    for payload in payloads[1:]: compare_payloads(payloads[0], payload)
    assert len(payloads) == 12 and len(visual_snapshots) == 11
    assert all(snapshot == visual_snapshots[0] for snapshot in visual_snapshots)
    # Negative controls change only in-memory copies; original PNGs/logs remain untouched.
    corrupt_image = first.copy(); corrupt_image[0,0,0] ^= 1
    try:
        compare_pixels(first, corrupt_image, footprint)
    except AssertionError:
        outside_rejected = True
    else:
        raise AssertionError("outside-footprint negative control accepted")
    corrupt_payload = json.loads(json.dumps(payloads[0]))
    corrupt_payload["cat_spatial"]["spatial"]["anchors"][0]["residual_irradiance_validity"][0] += 0.001
    try:
        compare_payloads(payloads[0], corrupt_payload)
    except AssertionError:
        payload_rejected = True
    else:
        raise AssertionError("restored-payload negative control accepted")
    print(json.dumps({"status":"qualified static pixels and captured lighting PASS; animated pose equality unproved",
                      "envelope":envelope, "projection":projection, "image_comparisons":pixels,
                      "high_endpoints":endpoints, "matching_visual_snapshot":visual_snapshots[0],
                      "negative_controls":{"outside_envelope_pixel_rejected":outside_rejected,
                                           "restored_entity_payload_mismatch_rejected":payload_rejected},
                      "original_whole_image_failures_retained":2,
                      "pose_time_or_posed_buffer_trace_available":False}, indent=2))


if __name__ == "__main__":
    main()
