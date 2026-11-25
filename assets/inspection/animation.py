"""Read-only animation channel checks shared by shipped model contracts."""
import math

from . import glb
from .rig_pose import quat_slerp


def finite_rows(rows):
    return all(math.isfinite(v) for row in rows for v in row)


def animation_record(document, binary, animation):
    errors = []
    channels = []
    for channel in animation.get("channels", []):
        sampler = animation["samplers"][channel["sampler"]]
        target = channel["target"]
        times = [v[0] for v in glb._read_accessor(document, binary, sampler["input"])]
        values = glb._read_accessor(document, binary, sampler["output"])
        interpolation = sampler.get("interpolation", "LINEAR")
        if not times or not finite_rows(values) or not all(math.isfinite(t) for t in times):
            errors.append("empty or non-finite animation channel")
        if any(b <= a for a, b in zip(times, times[1:])) or (times and times[0] < 0):
            errors.append("animation times must be non-negative and strictly increasing")
        if len(times) != len(values) or interpolation not in ("LINEAR", "STEP"):
            errors.append("unsupported interpolation or channel length mismatch")
        if target.get("path") not in ("rotation", "translation", "scale"):
            errors.append("unsupported animation target")
        if not 0 <= target.get("node", -1) < len(document.get("nodes", [])):
            errors.append("invalid animation target node")
        if target.get("path") == "rotation" and any(abs(sum(v*v for v in row)-1) > 0.002 for row in values):
            errors.append("animation quaternion is not unit length")
        delta = None
        if values:
            delta = max(abs(a-b) for a, b in zip(values[0], values[-1]))
            if target.get("path") == "rotation":
                delta = min(delta, max(abs(a+b) for a, b in zip(values[0], values[-1])))
        channels.append({"node": target.get("node"), "path": target.get("path"),
                         "keys": len(times), "duration": max(times, default=0),
                         "interpolation": interpolation, "endpoint_delta": delta})
    return {"name": animation.get("name", ""), "channels": channels,
            "duration": max((c["duration"] for c in channels), default=0), "errors": errors}


def inspect_rigid_frames(document, binary, animation, duration):
    """Exercise rigid clips at 60 Hz, including their exact final frame."""
    tracks = []
    for channel in animation["channels"]:
        sampler = animation["samplers"][channel["sampler"]]
        tracks.append((channel["target"], sampler.get("interpolation", "LINEAR"),
                       [row[0] for row in glb._read_accessor(document, binary, sampler["input"])],
                       glb._read_accessor(document, binary, sampler["output"])))
    count = max(1, math.ceil(duration*60))
    for frame in range(count+1):
        time_ = duration*frame/count
        nodes = [dict(node) for node in document["nodes"]]
        for target, interpolation, times, values in tracks:
            right = next((i for i, value in enumerate(times) if value >= time_), len(times)-1)
            left = max(0, right-1)
            if interpolation == "STEP":
                value = values[left] if time_ < times[right] else values[right]
            else:
                t = 0 if right == left else (time_-times[left])/(times[right]-times[left])
                if target["path"] == "rotation":
                    value = quat_slerp(values[left], values[right], t)
                else:
                    value = tuple(a+(b-a)*t for a, b in zip(values[left], values[right]))
            nodes[target["node"]][target["path"]] = value
        posed = {**document, "nodes": nodes}
        matrices, _ = glb._node_rest_matrices(posed)
        for matrix in matrices:
            if not all(math.isfinite(v) for v in matrix):
                raise ValueError("non-finite posed rigid transform")
        for index, node in enumerate(nodes):
            if "mesh" not in node:
                continue
            for primitive in document["meshes"][node["mesh"]]["primitives"]:
                positions = glb._read_accessor(document, binary, primitive["attributes"]["POSITION"])
                if not finite_rows(glb._transform_point(matrices[index], p) for p in positions):
                    raise ValueError("non-finite rigid animated vertex")
    return count+1


NON_BEACH_CLIP_MODELS = (
    "rat", "feltling", "mannequin", "skeleton", "spooner-man",
    "carved-pumpkin", "pumpkin-skeleton", "sheet-ghost", "sheet-ghost-cat",
)
SPOONER_TRANSITIONS = (
    ("idle", "sit_down"), ("sit_down", "sit_idle"), ("sit_idle", "stand_up"),
    ("stand_up", "idle"), ("idle", "pounce"), ("pounce", "idle"),
)


LEGACY_ONE_SHOTS = {'sit_down', 'stand_up', 'pounce', 'dead', 'collapse_reassemble'}


def clip_is_looped(document, name):
    """Use declared boolean semantics; retain legacy unmarked one-shots."""
    marker = document.get('asset', {}).get('extras', {}).get('places_entity_clips', {})
    entries = marker.get('clips', []) if isinstance(marker, dict) else []
    for entry in entries if isinstance(entries, list) else []:
        if (isinstance(entry, dict) and isinstance(entry.get('name'), str)
                and entry['name'].lower() == name.lower()
                and isinstance(entry.get('loop'), bool)):
            return entry['loop']
    return name not in LEGACY_ONE_SHOTS


def endpoints(model, animation, last):
    result = {}
    for channel in animation['channels']:
        sampler = animation['samplers'][channel['sampler']]
        path = channel['target']['path']
        values = model._read_accessor(sampler['output'], 'VEC4' if path == 'rotation' else 'VEC3')
        result[channel['target']['node'], path] = values[-1 if last else 0]
    return result


def difference(a, b, rotation):
    direct = max(abs(x - y) for x, y in zip(a, b))
    return min(direct, max(abs(x + y) for x, y in zip(a, b))) if rotation else direct


def compare(model, a, b, label):
    for key in a.keys() | b.keys():
        node, path = key
        default = model.nodes[node].get(path, [0, 0, 0, 1] if path == 'rotation' else
                                        [1, 1, 1] if path == 'scale' else [0, 0, 0])
        if difference(a.get(key, default), b.get(key, default), path == 'rotation') > 1e-5:
            raise ValueError(f'{label}: discontinuous {model.nodes[node].get("name", node)} {path}')


def validate_clip_boundaries(model, name, transitions=()):
    """Check declared loops, skin weights and the selected transition pairs."""
    animations = {animation["name"]: animation for animation in model.animations}
    if not animations:
        raise ValueError(f"{name}: no exported animation clips")
    for clip, animation in animations.items():
        if clip_is_looped(model.document, clip):
            compare(model, endpoints(model, animation, False), endpoints(model, animation, True),
                    f"{name}/{clip} loop")
    for weights in model.vertex_weights:
        if any(not math.isfinite(w) or w < 0 for w in weights) or abs(sum(weights) - 1) > 1e-5:
            raise ValueError(f"{name}: invalid skin weights")
    for first, last in transitions:
        compare(model, endpoints(model, animations[first], True),
                endpoints(model, animations[last], False), f"{first} -> {last}")
    return len(animations)
