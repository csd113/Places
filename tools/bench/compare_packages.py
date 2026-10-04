#!/usr/bin/env python3
"""Compare every map record, including numeric lighting bytes and probe mips.

Byte equality is stronger than a numeric tolerance: no geometry, collision,
navigation, HDR texel, probe coefficient, material reference or mip can change.
Exceptions are explicit: stage-key upgrades, or compiler input-key upgrades
with independently verified added texture dependencies. Other metadata differs
only by failing the comparison.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def compare(before: Path, after: Path, allow_stage_key=False, asset_root=None):
    with zipfile.ZipFile(before) as left, zipfile.ZipFile(after) as right:
        left_names, right_names = set(left.namelist()), set(right.namelist())
        result = dict(before=str(before), after=str(after),
                      archive_byte_equal=before.read_bytes() == after.read_bytes(),
                      missing=sorted(left_names - right_names),
                      added=sorted(right_names - left_names), differing_records=[],
                      stage_key_change=None, input_key_changes=None, roles={})
        manifest = json.loads(left.read("manifest.json"))
        roles = {entry["name"]: entry["role"] for entry in manifest["entries"]}
        for name in sorted(left_names & right_names):
            before_bytes, after_bytes = left.read(name), right.read(name)
            role = roles.get(name, "manifest")
            summary = result["roles"].setdefault(role, dict(records=0, bytes=0, equal=0))
            summary["records"] += 1
            summary["bytes"] += len(before_bytes)
            if before_bytes == after_bytes:
                summary["equal"] += 1
                continue
            if name == "manifest.json" and (allow_stage_key or asset_root):
                original, updated = json.loads(before_bytes), json.loads(after_bytes)
                old_key, new_key = original.pop("lighting_fingerprint", None), updated.pop("lighting_fingerprint", None)
                if asset_root:
                    keys = dict(lighting_fingerprint=dict(before=old_key, after=new_key),
                                compiler_fingerprint=dict(before=original.pop("compiler_fingerprint", None),
                                                          after=updated.pop("compiler_fingerprint", None)))
                    old_dependencies = original.pop("dependencies", [])
                    new_dependencies = updated.pop("dependencies", [])
                    added = [dependency for dependency in new_dependencies if dependency not in old_dependencies]
                    root = Path(asset_root).resolve()
                    valid = all(dependency in new_dependencies for dependency in old_dependencies)
                    for dependency in added:
                        path = (root / dependency["path"]).resolve()
                        if dependency["kind"] != "texture" or not path.is_relative_to(root) or not path.is_file():
                            valid = False
                            break
                        data = path.read_bytes()
                        valid &= len(data) == dependency["bytes"] and hashlib.sha256(data).hexdigest() == dependency["sha256"]
                    if original == updated and valid:
                        result["input_key_changes"] = dict(keys=keys, verified_added_dependencies=added)
                        continue
                elif original == updated:
                    result["stage_key_change"] = dict(before=old_key, after=new_key)
                    continue
            result["differing_records"].append(dict(
                name=name, role=role, before_sha256=hashlib.sha256(before_bytes).hexdigest(),
                after_sha256=hashlib.sha256(after_bytes).hexdigest()))
        result["quality_equal"] = not any(result[key] for key in ("missing", "added", "differing_records"))
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--allow-stage-key", action="store_true")
    parser.add_argument("--allow-input-keys", type=Path, metavar="ASSET_ROOT",
                        help="allow only revised keys and verified added texture identities")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    result = compare(args.before, args.after, args.allow_stage_key, args.allow_input_keys)
    text = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.write_text(text)
    print(text, end="")
    return 0 if result["quality_equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
