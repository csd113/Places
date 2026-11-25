"""Pure source checks for authored physical asset parameters."""
import math
import re
from typing import List


def _bounded_physical_number(value: object, low: float, high: float) -> bool:
    return (isinstance(value, (int, float)) and not isinstance(value, bool)
            and low <= value <= high and math.isfinite(value))


def _is_reset_group(value: object) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value) is not None


def _validate_physics_shape(shape: object, where: str, scale: float, compound: bool = True) -> List[str]:
    if not isinstance(shape, dict) or not isinstance(shape.get("kind"), str):
        return [f"{where}: physics shape must be an object with a string kind"]
    kind = shape["kind"]
    if kind == "compound":
        parts = shape.get("parts")
        if (not compound or set(shape) != {"kind", "parts"} or
                not isinstance(parts, list) or not 1 <= len(parts) <= 8):
            return [f"{where}: compound requires 1..8 non-recursive primitive parts"]
        errors = []
        for index, part in enumerate(parts):
            context = f"{where} compound part {index}"
            if not isinstance(part, dict) or set(part) != {"offset", "shape"}:
                errors.append(f"{context}: malformed part or unknown fields")
                continue
            offset = part["offset"]
            if (not isinstance(offset, list) or len(offset) != 3 or
                    not all(_bounded_physical_number(value, -100.0, 100.0) and
                            _bounded_physical_number(value * scale, -100.0, 100.0) for value in offset)):
                errors.append(f"{context}: offsets must be finite and within +/-100 local/scaled metres")
            errors.extend(_validate_physics_shape(part["shape"], context, scale, compound=False))
        return errors
    if kind == "convex_hull":
        points = shape.get("points")
        if (set(shape) != {"kind", "points"} or not isinstance(points, list)
                or not 4 <= len(points) <= 96 or any(not isinstance(point, list) or len(point) != 3
                or not all(_bounded_physical_number(value, -100.0, 100.0) and
                _bounded_physical_number(value * scale, -100.0, 100.0) for value in point) for point in points)):
            return [f"{where}: hull requires 4..96 finite points within +/-100 local/scaled metres"]
        extents = [max(point[axis] for point in points) - min(point[axis] for point in points) for axis in range(3)]
        if not all(_bounded_physical_number(extent, 0.001, 100.0) and
                _bounded_physical_number(extent * scale, 0.001, 100.0) for extent in extents):
            return [f"{where}: hull extents must be finite in 0.001..100 local/scaled metres"]
        origin = points[0]
        vectors = [[point[axis] - origin[axis] for axis in range(3)] for point in points[1:]]
        first = max(vectors, key=lambda vector: sum(value * value for value in vector))
        cross = lambda a, b: [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
        normal = max((cross(first, vector) for vector in vectors), key=lambda vector: sum(value*value for value in vector))
        if max(abs(sum(normal[axis] * vector[axis] for axis in range(3))) for vector in vectors) <= 1e-12:
            return [f"{where}: hull points must span a nondegenerate 3D volume"]
        return []
    expected = {"cuboid": {"kind", "half_extents"}, "sphere": {"kind", "radius"},
                "cylinder": {"kind", "half_height", "radius"}}.get(kind)
    if expected is None or set(shape) != expected:
        return [f"{where}: malformed physics shape or unknown fields"]
    dimensions = shape.get("half_extents") if kind == "cuboid" else [
        shape[field] for field in sorted(expected - {"kind"})]
    if (not isinstance(dimensions, list) or (kind == "cuboid" and len(dimensions) != 3) or
            not all(_bounded_physical_number(value, 0.001, 100.0) and
                    _bounded_physical_number(value * scale, 0.001, 100.0) for value in dimensions)):
        return [f"{where}: shape dimensions must be finite in 0.001..100 local/scaled metres"]
    return []


def validate_physics_profile(profile: object, where: str, scale: float = 1.0) -> List[str]:
    """Mirror the strict PhysicsDef/ShapeDef authoring contract in Rust."""
    errors = []
    allowed = {"motion", "shape", "mass_kg", "friction", "restitution",
               "linear_damping", "angular_damping", "pickup", "reset_group", "water"}
    if not isinstance(profile, dict) or set(profile) - allowed:
        return [f"{where}: invalid physics object or unknown fields"]
    if not _bounded_physical_number(scale, 0.001, 100.0):
        return [f"{where}: physics scale must be finite in 0.001..100"]
    motion = profile.get("motion", "fixed")
    if motion not in ("fixed", "dynamic", "kinematic"):
        errors.append(f"{where}: invalid physics motion")
    pickup = profile.get("pickup", False)
    if not isinstance(pickup, bool) or (pickup and motion != "dynamic"):
        errors.append(f"{where}: physics pickup requires dynamic motion and a boolean")
    group = profile.get("reset_group")
    if group is not None and (not _is_reset_group(group) or motion not in ("dynamic", "kinematic")):
        errors.append(f"{where}: reset_group requires movable motion and an ASCII 1..64 identifier")
    for field, low, high, default in (
        ("mass_kg", 0.001, 1000.0, 1.0), ("friction", 0.0, 2.0, 0.6),
        ("restitution", 0.0, 1.0, 0.05), ("linear_damping", 0.0, 100.0, 0.1),
        ("angular_damping", 0.0, 100.0, 0.2),
    ):
        if not _bounded_physical_number(profile.get(field, default), low, high):
            errors.append(f"{where}: physics {field} must be finite in {low}..{high}")
    if profile.get("shape") is not None:
        errors.extend(_validate_physics_shape(profile["shape"], where, scale))
    water = profile.get("water")
    if water is not None:
        if (not isinstance(water, dict)
                or set(water) - {"displaced_volume_m3", "linear_drag", "angular_drag"}):
            errors.append(f"{where}: invalid water physics object or unknown fields")
        else:
            if motion != "dynamic":
                errors.append(f"{where}: water physics requires dynamic motion")
            volume = water.get("displaced_volume_m3")
            if volume is not None and (
                    not _bounded_physical_number(volume, 1e-9, 1000.0)
                    or not _bounded_physical_number(volume * scale ** 3, 1e-9, 1000.0)):
                errors.append(f"{where}: displaced_volume_m3 must be finite in 1e-9..1000 before and after scale cubed")
            elif volume is not None and _bounded_physical_number(profile.get("mass_kg", 1.0), 0.001, 1000.0):
                if volume * scale ** 3 > profile.get("mass_kg", 1.0) / 10.0:
                    errors.append(f"{where}: displaced volume requires density of at least 10 kg/m3 after scale")
            for field, default in (("linear_drag", 3.0), ("angular_drag", 2.0)):
                if not _bounded_physical_number(water.get(field, default), 0.0, 100.0):
                    errors.append(f"{where}: water {field} must be finite in 0..100")
    return errors
