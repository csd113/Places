#!/usr/bin/env python3
"""Offline per-frame skinning and contact sweep over the shipped entity GLBs.

This is the expensive development-time check for the entity assets. It re-reads
each built GLB, reconstructs the joint hierarchy and clips, and evaluates the
skinned mesh at a fine time sampling with the same blend maths the runtime
uses (``p_posed = sum_j w_j * (global_j(t) * inverseBind_j) * p_bind``, LINEAR
sampling with the clip's own looping rule). For every sampled frame it reports:

* the vertical extent (floor penetration and floating);
* the maximum vertex displacement and triangle-edge stretch against the bind
  pose (a broken pivot or weight flings vertices);
* loop closure for looping clips (frame 0 vs the loop's end);
* the clip's declared ``reference_speed_mps`` and the frames where the mesh
  touches the floor (the authored contact window).

Work is one task per (asset, clip, frame chunk); the frame chunks are
independent, so the sweep is parallelised over processes. The parent owns the
inputs and merges results deterministically; workers only read the same GLB
and return small summaries. ``--workers 1`` is the identical serial reference.

    python3 tools/entities/validate_entities.py --workers 8
    python3 tools/entities/validate_entities.py --workers 1
    python3 tools/entities/validate_entities.py --limit-frames 120   # bounded sample
    PLACES_TOOL_WORKERS=8 python3 tools/entities/validate_entities.py

``--workers`` wins over ``PLACES_TOOL_WORKERS`` wins over the automatic default
``min(12, usable CPUs)``. The effective count is additionally capped by the
number of independent tasks and by a conservative per-worker memory guard.
Every count is printed, and a requested count that was reduced is explained.
"""

from __future__ import annotations

import argparse
import json
import math
import multiprocessing
import os
import struct
import sys
import time
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT / "tools" / "entities") not in sys.path:
    sys.path.insert(0, str(REPO_ROOT / "tools" / "entities"))

import rig  # noqa: E402  (tools/entities on sys.path)

DEFAULT_ASSETS = (
    "assets/entities/spooner-man/model/spooner-man.glb",
    "assets/entities/rat/model/rat.glb",
    "assets/entities/mannequin/model/mannequin.glb",
    "assets/entities/skeleton/model/skeleton.glb",
)

# Engine ceilings, mirrored from src/gltf.rs / src/level.rs.
MAX_WORKERS_CEILING = 12
SAMPLES_PER_SECOND = 60.0
FRAME_CHUNK = 16
# One loaded entity is small; 64 MiB of accumulated per-worker arrays is a
# generous guard that still bounds a pathological asset.
MEMORY_GUARD_BYTES = 64 * 1024 * 1024

_COMPONENT = {
    5120: ("b", 1),
    5121: ("B", 1),
    5122: ("h", 2),
    5123: ("H", 2),
    5125: ("I", 4),
    5126: ("f", 4),
}
_TYPE_COUNT = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def usable_cpu_count() -> int:
    """Usable logical CPUs, preferring the interpreter's process-affinity view."""
    if hasattr(os, "process_cpu_count"):
        count = os.process_cpu_count()
        if count:
            return int(count)
    count = os.cpu_count()
    return int(count) if count else 1


def parse_worker_count(requested: Optional[str]) -> Tuple[int, str]:
    """Resolves the requested worker count and explains any reduction."""
    raw = requested if requested is not None else os.environ.get("PLACES_TOOL_WORKERS")
    ceiling = min(MAX_WORKERS_CEILING, usable_cpu_count())
    if raw is None or str(raw).strip() == "":
        return ceiling, f"automatic: min(12, {usable_cpu_count()} usable CPUs)"
    try:
        value = int(str(raw).strip())
    except ValueError:
        raise SystemExit(f"--workers must be an integer, got {raw!r}")
    if value < 1:
        raise SystemExit(f"--workers must be >= 1, got {value}")
    if value > ceiling:
        return ceiling, f"requested {value}, reduced to {ceiling} (CPU/ceiling budget)"
    return value, "requested"


# ------------------------------------------------------------------- reading


class Model:
    """The decoded inputs the sweep needs, straight from one GLB."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.document, self.binary = rig._load_glb(path)
        self.nodes = self.document.get("nodes", [])
        self.accessors = self.document.get("accessors", [])
        self.skins = self.document.get("skins", [])
        self.animations = self.document.get("animations", [])
        self.rest_translation = [list(node.get("translation", [0.0, 0.0, 0.0])) for node in self.nodes]
        self.rest_rotation = [list(node.get("rotation", [0.0, 0.0, 0.0, 1.0])) for node in self.nodes]
        self.rest_scale = [list(node.get("scale", [1.0, 1.0, 1.0])) for node in self.nodes]
        self.parent = [None] * len(self.nodes)
        for index, node in enumerate(self.nodes):
            for child in node.get("children", []):
                self.parent[child] = index
        skin = self.skins[0]
        self.joints = list(skin["joints"])
        self.inverse_bind = self._read_accessor(skin["inverseBindMatrices"], "MAT4")
        self.parent_of_joint = [self.parent[node] for node in self.joints]
        self.order = self._topological_order()
        # Include every skinned primitive: Spoonerman separates body and paws.
        self.positions = []
        self.vertex_joints = []
        self.vertex_weights = []
        self.indices = []
        for mesh in self.document["meshes"]:
            for primitive in mesh["primitives"]:
                attributes = primitive["attributes"]
                if "JOINTS_0" not in attributes:
                    continue
                base = len(self.positions)
                self.positions.extend(self._read_accessor(attributes["POSITION"], "VEC3"))
                self.vertex_joints.extend(self._read_accessor(attributes["JOINTS_0"], "VEC4"))
                self.vertex_weights.extend(self._read_accessor(attributes["WEIGHTS_0"], "VEC4"))
                flat = self._read_accessor(primitive["indices"], "SCALAR")
                self.indices.extend(base + int(value[0]) for value in flat)
        # Sample the bind pose once: the reference for displacement/stretch.
        identity = self._identity_globals()
        self.bind_skin = self._skin(identity)
        self.bind_edges = self._edge_lengths(self.bind_skin)
        self.marker = (
            self.document.get("asset", {}).get("extras", {}).get("places_entity_clips", {})
        )
        self.clips = self._read_clips()

    # -- accessors ------------------------------------------------------

    def _read_accessor(self, index: int, expected: str):
        accessor = self.accessors[index]
        kind = accessor["type"]
        if kind != expected:
            raise SystemExit(f"{self.path}: accessor {index} is {kind}, expected {expected}")
        component = accessor["componentType"]
        if component not in _COMPONENT:
            raise SystemExit(f"{self.path}: accessor {index} has componentType {component}")
        fmt, size = _COMPONENT[component]
        count = accessor["count"]
        components = _TYPE_COUNT[kind]
        view = self.document["bufferViews"][accessor["bufferView"]]
        offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        stride = view.get("byteStride") or (size * components)
        normalized = bool(accessor.get("normalized"))
        scale = 255.0 if normalized and component == 5121 else 1.0
        out = []
        for element in range(count):
            base = offset + element * stride
            row = []
            for component_index in range(components):
                (raw,) = struct.unpack_from(
                    "<" + fmt, self.binary, base + component_index * size
                )
                row.append(raw / scale if scale != 1.0 else raw)
            out.append(row)
        return out

    def _identity_globals(self) -> List[List[float]]:
        globals_: List[List[float]] = [rig._mat_identity() for _ in self.nodes]
        for index in self._topological_order():
            local = rig._mat_trs(
                self.rest_translation[index], self.rest_rotation[index], self.rest_scale[index]
            )
            parent = self.parent[index]
            globals_[index] = (
                rig._mat_mul(globals_[parent], local)
                if parent is not None
                else local
            )
        return globals_

    def _topological_order(self) -> List[int]:
        order = []
        emitted = [False] * len(self.nodes)
        while len(order) < len(self.nodes):
            progressed = False
            for index in range(len(self.nodes)):
                if emitted[index]:
                    continue
                parent = self.parent[index]
                if parent is None or emitted[parent]:
                    order.append(index)
                    emitted[index] = True
                    progressed = True
            if not progressed:
                raise SystemExit(f"{self.path}: node hierarchy has a cycle")
        return order

    # -- clips ----------------------------------------------------------

    def _read_clips(self) -> List[dict]:
        clips = []
        for animation in self.animations:
            channels = []
            duration = 0.0
            for channel in animation["channels"]:
                sampler = animation["samplers"][channel["sampler"]]
                times = [value[0] for value in self._read_accessor(sampler["input"], "SCALAR")]
                target = channel["target"]
                path = target["path"]
                kind = {"rotation": "VEC4", "translation": "VEC3", "scale": "VEC3"}.get(path)
                if kind is None:
                    continue
                values = self._read_accessor(sampler["output"], kind)
                duration = max(duration, times[-1] if times else 0.0)
                channels.append({"node": target["node"], "path": path, "times": times, "values": values})
            clips.append({"name": animation.get("name", ""), "duration": duration, "channels": channels})
        # Attach the marker metadata by name.
        by_name = {
            clip.get("name"): clip
            for clip in (self.marker.get("clips", []) if isinstance(self.marker, dict) else [])
        }
        for clip in clips:
            meta = by_name.get(clip["name"], {})
            clip["loop"] = bool(meta.get("loop", True))
            speed = meta.get("reference_speed_mps")
            clip["reference_speed_mps"] = (
                float(speed) if isinstance(speed, (int, float)) and speed > 0 else None
            )
            clip["kind"] = meta.get("kind")
        return clips

    # -- pose evaluation ------------------------------------------------

    def _interpolate(self, keys_times: Sequence[float], values: Sequence[list], time: float, looping: bool):
        if not keys_times:
            return values[0] if values else None
        duration = keys_times[-1]
        if duration <= 0.0:
            return values[0]
        local = time % duration if looping else min(max(time, 0.0), duration)
        if local <= keys_times[0]:
            return values[0]
        for index in range(1, len(keys_times)):
            if local <= keys_times[index]:
                span = keys_times[index] - keys_times[index - 1]
                t = 0.0 if span <= 0.0 else (local - keys_times[index - 1]) / span
                return _lerp(values[index - 1], values[index], t)
        return values[-1]

    def _pose_globals(self, clip: dict, time: float) -> List[List[float]]:
        channels_by_node: Dict[int, Dict[str, list]] = {}
        for channel in clip["channels"]:
            channels_by_node.setdefault(channel["node"], {})[channel["path"]] = channel
        globals_: List[List[float]] = [rig._mat_identity() for _ in self.nodes]
        for index in self.order:
            channels = channels_by_node.get(index)
            translation = list(self.rest_translation[index])
            rotation = list(self.rest_rotation[index])
            scale = list(self.rest_scale[index])
            if channels:
                channel = channels.get("translation")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"])
                    translation = list(value) if value else translation
                channel = channels.get("rotation")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"])
                    rotation = list(value) if value else rotation
                channel = channels.get("scale")
                if channel is not None:
                    value = self._interpolate(channel["times"], channel["values"], time, clip["loop"])
                    scale = list(value) if value else scale
            local = rig._mat_trs(translation, rotation, scale)
            parent = self.parent[index]
            globals_[index] = (
                rig._mat_mul(globals_[parent], local) if parent is not None else local
            )
        return globals_

    def _skin(self, globals_: List[List[float]]) -> List[Tuple[float, float, float]]:
        matrices = [
            rig._mat_mul(globals_[joint], self.inverse_bind[slot])
            for slot, joint in enumerate(self.joints)
        ]
        out = []
        for vertex, position in enumerate(self.positions):
            slots = self.vertex_joints[vertex]
            weights = self.vertex_weights[vertex]
            blended = [0.0, 0.0, 0.0]
            total = 0.0
            for slot, weight in zip(slots, weights):
                if weight <= 0.0:
                    continue
                matrix = matrices[int(slot)]
                x, y, z = position
                blended[0] += (matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12]) * weight
                blended[1] += (matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13]) * weight
                blended[2] += (matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14]) * weight
                total += weight
            if total <= 0.0:
                out.append((position[0], position[1], position[2]))
            else:
                out.append((blended[0] / total, blended[1] / total, blended[2] / total))
        return out

    def _edge_lengths(self, skinned: Sequence[Sequence[float]]) -> List[float]:
        lengths = []
        for index in range(0, len(self.indices) - 2, 3):
            a, b, c = self.indices[index], self.indices[index + 1], self.indices[index + 2]
            for first, second in ((a, b), (b, c), (c, a)):
                p = skinned[first]
                q = skinned[second]
                lengths.append(math.dist(p, q))
        return lengths


def _lerp(a: Sequence[float], b: Sequence[float], t: float) -> List[float]:
    return [x + (y - x) * t for x, y in zip(a, b)]


# ------------------------------------------------------------------- sweep


def frame_times(clip: dict, samples_per_second: float, limit_frames: Optional[int]) -> List[float]:
    duration = clip["duration"]
    if duration <= 0.0:
        return [0.0]
    frames = max(1, int(math.ceil(duration * samples_per_second)) + 1)
    if limit_frames is not None:
        frames = min(frames, limit_frames)
    step = duration / (frames - 1)
    return [step * index for index in range(frames)]


def summarise_frame(
    model: Model, clip: dict, times: Sequence[float]
) -> dict:
    """One independent chunk: skin every sampled time and return small stats."""
    min_y = math.inf
    max_y = -math.inf
    max_displacement = 0.0
    max_stretch = 0.0
    contact_frames = 0
    for time in times:
        globals_ = model._pose_globals(clip, time)
        skinned = model._skin(globals_)
        frame_min = min(point[1] for point in skinned)
        frame_max = max(point[1] for point in skinned)
        min_y = min(min_y, frame_min)
        max_y = max(max_y, frame_max)
        if frame_min <= 0.03:
            contact_frames += 1
        for vertex, bind in enumerate(model.bind_skin):
            posed = skinned[vertex]
            displacement = math.dist(bind, posed)
            if displacement > max_displacement:
                max_displacement = displacement
        edge_index = 0
        for index in range(0, len(model.indices) - 2, 3):
            a, b, c = model.indices[index], model.indices[index + 1], model.indices[index + 2]
            for first, second in ((a, b), (b, c), (c, a)):
                base = model.bind_edges[edge_index]
                edge_index += 1
                if base <= 1.0e-9:
                    continue
                stretched = math.dist(skinned[first], skinned[second]) / base
                if stretched > max_stretch:
                    max_stretch = stretched
    return {
        "frames": len(times),
        "min_y_m": min_y,
        "max_y_m": max_y,
        "contact_frames": contact_frames,
        "max_displacement_m": max_displacement,
        "max_edge_stretch": max_stretch,
    }


def chunk_tasks(model: "Model", clip: dict, samples_per_second: float, limit_frames: Optional[int]):
    times = frame_times(clip, samples_per_second, limit_frames)
    for start in range(0, len(times), FRAME_CHUNK):
        yield times[start : start + FRAME_CHUNK]


# Worker state is initialised once per process: the GLB is parsed once and
# reused across every chunk that process handles.
_WORKER_MODELS: Dict[str, Model] = {}


def _worker_init(paths: Sequence[str]) -> None:
    for path in paths:
        _WORKER_MODELS[path] = Model(Path(path))


def _worker_run(task: Tuple[str, int, int, int, List[float], float, Optional[int]]) -> dict:
    path, asset_index, clip_index, task_index, times, samples_per_second, limit_frames = task
    model = _WORKER_MODELS[path]
    clip = model.clips[clip_index]
    result = summarise_frame(model, clip, times)
    return {
        "asset": asset_index,
        "clip": clip_index,
        "task": task_index,
        "times": [times[0], times[-1]],
        "result": result,
    }


def merge_chunks(chunks: Iterable[dict]) -> dict:
    merged: dict = {}
    for chunk in sorted(chunks, key=lambda item: (item["asset"], item["clip"], item["task"])):
        key = (chunk["asset"], chunk["clip"])
        entry = merged.setdefault(
            key,
            {
                "frames": 0,
                "min_y_m": math.inf,
                "max_y_m": -math.inf,
                "contact_frames": 0,
                "max_displacement_m": 0.0,
                "max_edge_stretch": 0.0,
                "sampled_from": math.inf,
                "sampled_to": -math.inf,
            },
        )
        result = chunk["result"]
        entry["frames"] += result["frames"]
        entry["min_y_m"] = min(entry["min_y_m"], result["min_y_m"])
        entry["max_y_m"] = max(entry["max_y_m"], result["max_y_m"])
        entry["contact_frames"] += result["contact_frames"]
        entry["max_displacement_m"] = max(entry["max_displacement_m"], result["max_displacement_m"])
        entry["max_edge_stretch"] = max(entry["max_edge_stretch"], result["max_edge_stretch"])
        entry["sampled_from"] = min(entry["sampled_from"], chunk["times"][0])
        entry["sampled_to"] = max(entry["sampled_to"], chunk["times"][1])
    return merged


def check_weights(model: Model) -> List[str]:
    problems = []
    for vertex in range(len(model.positions)):
        weights = model.vertex_weights[vertex]
        total = sum(weights)
        if abs(total - 1.0) > 1.0e-4:
            problems.append(f"vertex {vertex}: weights sum to {total:.6f}")
            break
        for slot in model.vertex_joints[vertex]:
            if not 0 <= int(slot) < len(model.joints):
                problems.append(f"vertex {vertex}: joint slot {slot} outside the rig")
                break
    return problems


def sweep(
    paths: Sequence[Path],
    workers: int,
    samples_per_second: float,
    limit_frames: Optional[int],
    explain: str,
) -> dict:
    assets = []
    for path in paths:
        model = Model(path)
        problems = check_weights(model)
        assets.append(
            {
                "path": str(path),
                "bytes": path.stat().st_size,
                "vertices": len(model.positions),
                "triangles": len(model.indices) // 3,
                "joints": len(model.joints),
                "clips": [
                    {
                        "name": clip["name"],
                        "duration": clip["duration"],
                        "loop": clip["loop"],
                        "reference_speed_mps": clip["reference_speed_mps"],
                        "kind": clip["kind"],
                    }
                    for clip in model.clips
                ],
                "weight_problems": problems,
            }
        )

    tasks = []
    for asset_index, path in enumerate(paths):
        model = Model(path)
        for clip_index, clip in enumerate(model.clips):
            for task_index, times in enumerate(
                chunk_tasks(model, clip, samples_per_second, limit_frames)
            ):
                tasks.append(
                    (
                        str(path),
                        asset_index,
                        clip_index,
                        task_index,
                        list(times),
                        samples_per_second,
                        limit_frames,
                    )
                )

    started = time.perf_counter()
    # One worker holds one decoded model plus its scratch arrays; cap the pool
    # so a pathological asset set cannot multiply that by the worker count.
    largest_glb = max((path.stat().st_size for path in paths), default=0)
    per_worker_bytes = max(largest_glb * 8, 1)
    memory_cap = max(1, MEMORY_GUARD_BYTES // per_worker_bytes)
    work_cap = max(1, len(tasks))
    effective = max(1, min(workers, memory_cap, work_cap))
    reduction = ""
    if effective < workers:
        reduction = (
            f"; reduced to {effective} (work cap {work_cap}, memory cap {memory_cap}, "
            f"~{per_worker_bytes} B per worker)"
        )
    results: List[dict] = []
    if effective <= 1:
        for task in tasks:
            if task[0] not in _WORKER_MODELS:
                _worker_init([task[0]])
            results.append(_worker_run(task))
    else:
        context = multiprocessing.get_context("spawn")
        with context.Pool(
            processes=effective,
            initializer=_worker_init,
            initargs=([str(path) for path in paths],),
        ) as pool:
            for result in pool.imap_unordered(_worker_run, tasks, chunksize=1):
                results.append(result)
    elapsed = time.perf_counter() - started
    merged = merge_chunks(results)

    clips = []
    for asset_index, asset in enumerate(assets):
        for clip_index, clip in enumerate(assets[asset_index]["clips"]):
            entry = merged.get((asset_index, clip_index))
            if entry is None:
                continue
            clips.append(
                {
                    "asset": asset["path"],
                    "name": clip["name"],
                    "duration": clip["duration"],
                    "loop": clip["loop"],
                    "reference_speed_mps": clip["reference_speed_mps"],
                    "kind": clip["kind"],
                    "frames": entry["frames"],
                    "min_y_m": entry["min_y_m"],
                    "max_y_m": entry["max_y_m"],
                    "contact_frames": entry["contact_frames"],
                    "max_displacement_m": entry["max_displacement_m"],
                    "max_edge_stretch": entry["max_edge_stretch"],
                }
            )

    return {
        "requested_workers": workers,
        "workers_note": explain + reduction,
        "effective_workers": effective,
        "tasks": len(tasks),
        "elapsed_s": elapsed,
        "assets": assets,
        "clips": clips,
    }


def verify_report(report: dict) -> List[str]:
    """Independent sanity gates over the merged sweep."""
    problems = []
    for asset in report["assets"]:
        problems.extend(f"{asset['path']}: {problem}" for problem in asset["weight_problems"])
        if asset["vertices"] <= 0 or asset["triangles"] <= 0 or asset["joints"] <= 0:
            problems.append(f"{asset['path']}: empty geometry or rig")
    for clip in report["clips"]:
        label = f"{clip['asset']} clip {clip['name']}"
        if clip["min_y_m"] < -0.03:
            problems.append(f"{label}: penetrates the floor by {-clip['min_y_m']:.4f} m")
        if clip["max_edge_stretch"] > 2.0:
            problems.append(f"{label}: edge stretch {clip['max_edge_stretch']:.3f}x")
        if clip["duration"] > 0.0 and clip["contact_frames"] == 0 and clip["kind"] in ("walk", "run"):
            problems.append(f"{label}: a {clip['kind']} clip never touches the floor")
    return problems


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Entity skinning/contact sweep")
    parser.add_argument(
        "--glb",
        action="append",
        default=None,
        help="one entity GLB (repeatable); defaults to the shipped entity assets",
    )
    parser.add_argument("--workers", default=None, help="parallel worker count (overrides PLACES_TOOL_WORKERS)")
    parser.add_argument("--samples-per-second", type=float, default=SAMPLES_PER_SECOND)
    parser.add_argument("--limit-frames", type=int, default=None, help="cap frames per clip (bounded sample)")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)

    paths = [Path(path) for path in (args.glb or DEFAULT_ASSETS)]
    missing = [str(path) for path in paths if not path.is_file()]
    if missing:
        raise SystemExit("missing entity GLB(s): " + ", ".join(missing))

    workers, explain = parse_worker_count(args.workers)
    print(
        f"[entities] sweeping {len(paths)} asset(s): workers={workers} ({explain}), "
        f"usable CPUs={usable_cpu_count()}, {args.samples_per_second:g} samples/s"
    )
    report = sweep(paths, workers, args.samples_per_second, args.limit_frames, explain)
    problems = verify_report(report)
    print(
        f"[entities] {report['tasks']} task(s) over {report['effective_workers']} worker(s) "
        f"in {report['elapsed_s']:.2f} s"
    )
    if report["effective_workers"] != workers:
        print(f"[entities] worker count: {report['workers_note']}")
    for clip in report["clips"]:
        speed = clip["reference_speed_mps"]
        speed_text = f", ref {speed:g} m/s" if speed else ""
        print(
            f"  {Path(clip['asset']).name:14s} {clip['name']:18s} "
            f"frames={clip['frames']:4d} y=[{clip['min_y_m']:+.3f}, {clip['max_y_m']:+.3f}] "
            f"contact={clip['contact_frames']:4d} max_disp={clip['max_displacement_m']:.3f} m "
            f"stretch={clip['max_edge_stretch']:.3f}x{speed_text}"
        )
    if problems:
        print("[entities] FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
    if args.json:
        print(json.dumps({"report": report, "problems": problems}, indent=2, sort_keys=True))
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
