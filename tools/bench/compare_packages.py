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
import sys as _package_sys
_package_sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'levels'))
from package_zip import BLOCK_BYTES, PackageZip, hash_stream



def archive_equal(before, after):
    with before.raw_stream() as left, after.raw_stream() as right:
        while True:
            a, b = left.read(BLOCK_BYTES), right.read(BLOCK_BYTES)
            if a != b:
                return False
            if not a:
                return True


def compare_record(left, right, name):
    before_hash, after_hash = hashlib.sha256(), hashlib.sha256()
    size, equal = 0, True
    with left.open(name) as before, right.open(name) as after:
        while True:
            a, b = before.read(BLOCK_BYTES), after.read(BLOCK_BYTES)
            if not a and not b:
                break
            size += len(a)
            equal &= a == b
            # Drain both streams even after a mismatch, for their hashes and CRCs.
            before_hash.update(a)
            after_hash.update(b)
    return size, equal, before_hash.hexdigest(), after_hash.hexdigest()


def compare(before: Path, after: Path, allow_stage_key=False, asset_root=None):
    with PackageZip(before) as left, PackageZip(after) as right:
        left_names, right_names = set(left.namelist()), set(right.namelist())
        result = dict(before=str(before), after=str(after),
                      archive_byte_equal=archive_equal(left, right),
                      missing=sorted(left_names - right_names),
                      added=sorted(right_names - left_names), differing_records=[],
                      stage_key_change=None, input_key_changes=None, roles={})
        manifest_bytes = left.read("manifest.json")
        manifest = json.loads(manifest_bytes)
        roles = {entry["name"]: entry["role"] for entry in manifest["entries"]}
        for name in sorted(left_names & right_names):
            size, equal, before_hash, after_hash = compare_record(left, right, name)
            role = roles.get(name, "manifest")
            summary = result["roles"].setdefault(role, dict(records=0, bytes=0, equal=0))
            summary["records"] += 1
            summary["bytes"] += size
            if equal:
                summary["equal"] += 1
                continue
            if name == "manifest.json" and (allow_stage_key or asset_root):
                original, updated = json.loads(manifest_bytes), json.loads(right.read(name))
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
                        with path.open("rb") as stream:
                            size, digest = hash_stream(stream)
                        valid &= size == dependency["bytes"] and digest == dependency["sha256"]
                    if original == updated and valid:
                        result["input_key_changes"] = dict(keys=keys, verified_added_dependencies=added)
                        continue
                elif original == updated:
                    result["stage_key_change"] = dict(before=old_key, after=new_key)
                    continue
            result["differing_records"].append(dict(
                name=name, role=role, before_sha256=before_hash, after_sha256=after_hash))
        result["quality_equal"] = not any(result[key] for key in ("missing", "added", "differing_records"))
        left.verify_source_identity()
        right.verify_source_identity()
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
