#!/usr/bin/env python3
"""Retain and index distinct playable compiler fixtures without removing inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import zipfile

HERE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--include", nargs="*", type=Path, default=[])
    args = parser.parse_args()
    manifest = HERE / "manifest.json"
    entries = json.loads(manifest.read_text()) if manifest.exists() else []
    indexed = {entry["alias"]: entry for entry in entries}
    for folder in [HERE / "regressions", *args.include]:
        for package in sorted(folder.rglob("*.placesmap")):
            try:
                with zipfile.ZipFile(package) as archive:
                    metadata = json.loads(archive.read("manifest.json"))
                    record = next(entry for entry in metadata["entries"] if entry["role"] == "semantics")
                    source_bytes = archive.read(record["name"])
                    level = json.loads(source_bytes)
            except (OSError, ValueError, KeyError, StopIteration, zipfile.BadZipFile):
                continue
            original = package.with_suffix(".json")
            source_kind = "decoded semantics snapshot"
            if original.is_file():
                try:
                    candidate = json.loads(original.read_bytes())
                    if candidate.get("id") == level["id"] and "rooms" in candidate:
                        source_bytes = original.read_bytes()
                        source_kind = "original authored source"
                except (ValueError, AttributeError):
                    pass
            package_hash = digest(package)
            alias = level["id"] + "-" + package_hash[:12]
            purpose = package.parent.name + "/" + package.stem
            if alias in indexed:
                origins = indexed[alias]["origins"]
                if str(package.resolve()) not in origins:
                    origins.append(str(package.resolve()))
                continue
            saved = HERE / "packages" / (alias + ".placesmap")
            source = HERE / "sources" / (alias + ".json")
            saved.parent.mkdir(exist_ok=True)
            source.parent.mkdir(exist_ok=True)
            shutil.copy2(package, saved)
            source.write_bytes(source_bytes)
            entry = dict(alias=alias, id=level["id"], name=level["name"], purpose=purpose,
                         package=str(saved.relative_to(HERE)), package_sha256=package_hash,
                         source=str(source.relative_to(HERE)), source_sha256=digest(source),
                         source_kind=source_kind, origins=[str(package.resolve())],
                         variants=[variant["lightmap_quality"] for variant in metadata["variants"]])
            if package.parent.name.startswith("catalogue-"):
                catalogue = (package.parent / "catalogue-before.json" if package.stem == "catalogue-before"
                             else package.parent / "asset-root/catalog.json")
                if catalogue.is_file():
                    destination = HERE / "catalogues" / (digest(catalogue) + ".json")
                    destination.parent.mkdir(exist_ok=True)
                    if not destination.exists():
                        shutil.copy2(catalogue, destination)
                    entry["catalogue"] = str(destination.relative_to(HERE))
                    entry["catalogue_sha256"] = digest(destination)
            indexed[alias] = entry
    entries = sorted(indexed.values(), key=lambda entry: entry["alias"])
    manifest.write_text(json.dumps(entries, indent=2) + "\n")
    print(f"Preserved {len(entries)} distinct compiled packages and their source snapshots")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
