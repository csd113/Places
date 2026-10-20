#!/usr/bin/env python3
"""Read-only storage accounting for trusted compiler map/project outputs.

Counts physical ZIP records once, follows PPS1 shared segments, and separates
variant-unique payload from shared payload. This is not a package validator and
does not rewrite archives or estimate a hypothetical re-export's compression.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for child in value.values():
            yield from strings(child)
    elif isinstance(value, list):
        for child in value:
            yield from strings(child)


def inspect(path):
    with zipfile.ZipFile(path) as archive:
        infos = {entry.filename: entry for entry in archive.infolist()}
        if len(infos) != len(archive.infolist()):
            raise ValueError("duplicate archive names")

        def small(name, limit=8 * 1024 ** 2):
            if infos[name].file_size > limit:
                raise ValueError(f"metadata exceeds audit read budget: {name}")
            return archive.read(name)

        if "project.json" in infos:
            project = json.loads(small("project.json"))
            members = project["levels"]
            source_records = set(project["sources"].values())
        else:
            members = [dict(id=json.loads(small("manifest.json"))["id"],
                            records={name: name for name in infos})]
            source_records = set()

        def totals(names):
            return dict(records=len(names), logical_bytes=sum(infos[n].file_size for n in names),
                        compressed_bytes=sum(infos[n].compress_size for n in names))

        all_modes = {}
        role_records = {}
        maps = []
        logical_references = 0
        for member in members:
            records = member["records"]
            manifest = json.loads(small(records["manifest.json"]))
            declared = {entry["name"]: entry for entry in manifest["entries"]}
            if len(declared) != len(manifest["entries"]):
                raise ValueError("duplicate manifest entries")
            logical_references += sum(infos[name].file_size for name in set(records.values()))
            for name, entry in declared.items():
                role_records.setdefault(entry["role"], set()).add(records[name])

            def dependencies(value):
                references = set(strings(value))
                if any(name.startswith("blobs/") and name not in declared for name in references):
                    raise ValueError("undeclared variant blob reference")
                names = references & declared.keys()
                for name in list(names):
                    if not name.endswith(".props.recipe"):
                        continue
                    data = small(records[name])
                    if (data[:4] != b"PPS1" or len(data) < 82
                            or struct.unpack_from("<H", data, 4)[0] not in (1, 2)):
                        raise ValueError("unsupported PPS1 recipe")
                    count = struct.unpack_from("<I", data, 78)[0]
                    if count > 4096 or 82 + count * 72 > len(data):
                        raise ValueError("unbounded/truncated shared segment table")
                    for index in range(count):
                        offset = 82 + index * 72
                        expected_bytes = struct.unpack_from("<Q", data, offset)[0]
                        start = offset + 8
                        digest = data[start:start + 64].decode("ascii")
                        if any(character not in "0123456789abcdef" for character in digest):
                            raise ValueError("invalid shared prop digest")
                        shared = f"blobs/{digest}.props.shared"
                        if shared not in declared:
                            raise ValueError("missing shared prop dependency")
                        if (declared[shared]["role"] != "props-shared"
                                or expected_bytes != declared[shared]["bytes"]
                                or expected_bytes != infos[records[shared]].file_size):
                            raise ValueError("shared prop segment metadata disagrees")
                        names.add(shared)
                return {records[name] for name in names}

            modes = {v["lightmap_quality"]: dependencies(v["entries"])
                     for v in manifest["variants"]}
            for mode, names in modes.items():
                all_modes.setdefault(mode, set()).update(names)
            off, full = modes.get("off", set()), modes.get("full", set())
            atlas = []
            for variant in manifest["variants"]:
                entries = variant["entries"]
                name = entries.get("lightmaps_meta")
                if name:
                    # The production lightmap metadata ceiling is 64 MiB.
                    meta = json.loads(small(records[name], 64 * 1024 ** 2))
                    atlas.append(dict(mode=variant["lightmap_quality"],
                                      page_count=meta["page_count"], page_edge=meta["page_edge"],
                                      rgba16f_pair_base_bytes=meta["page_count"] * meta["page_edge"] ** 2 * 16,
                                      switchable_lights=len(meta.get("switchable_lights", []))))
            maps.append(dict(id=member["id"], physical_records=totals(set(records.values())),
                             variants={key: totals(value) for key, value in modes.items()},
                             static_unique_vs_full=totals(off - full),
                             full_unique_vs_static=totals(full - off),
                             shared_static_full=totals(off & full),
                             atlas_allocation_shape=atlas))
        off, full = all_modes.get("off", set()), all_modes.get("full", set())
        payload = sum(entry.compress_size for entry in infos.values())
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        return dict(package=str(path.resolve()), sha256=digest, archive_bytes=path.stat().st_size,
                    zip_payload=totals(set(infos)), zip_headers_and_directory_bytes=path.stat().st_size - payload,
                    member_logical_reference_bytes=logical_references,
                    sources=totals(source_records), roles={k: totals(v) for k, v in sorted(role_records.items())},
                    collection_static_unique_vs_full=totals(off - full),
                    collection_full_unique_vs_static=totals(full - off),
                    collection_shared_static_full=totals(off & full), maps=maps,
                    caveats=["Unique costs are existing ZIP payload bytes, excluding index/header changes in a hypothetical re-export.",
                             "Per-map totals overlap where content-addressed blobs are shared; collection sets count once.",
                             "Atlas shape describes RGBA16F pair base allocations, not measured total GPU memory or RSS.",
                             "External dependency assets are not embedded package bytes."])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packages", type=Path, nargs="+")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("choose a new report path")
    report = [inspect(path) for path in args.packages]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(args.out)


if __name__ == "__main__":
    main()
