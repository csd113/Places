#!/usr/bin/env python3
"""Explicit local package/import adversarial campaign; never runs in Cargo/CI.

The supplied finished packages are read only. Mutations and recovered sources
stay in a newly created output directory; no fixture allocates a claimed large
payload. Every compiler child has a timeout and receives the explicit asset root.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import time
import warnings
import zipfile
import sys as _package_sys
_package_sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'levels'))
from package_zip import PackageZip



def write_zip(path, records):
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        with PackageZip(path, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, data in records:
                archive.writestr(name, data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True, type=Path)
    parser.add_argument("--asset-root", required=True, type=Path)
    parser.add_argument("--package", required=True, action="append", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=60)
    args = parser.parse_args()
    compiler = args.compiler.resolve(strict=True)
    assets = args.asset_root.resolve(strict=True)
    packages = [path.resolve(strict=True) for path in args.package]
    out = args.out.absolute()
    out.mkdir(exist_ok=False, parents=True)
    fixtures = out / "fixtures"
    fixtures.mkdir()
    environment = os.environ.copy()
    environment.pop("PLACES_ASSET_ROOT", None)
    results = []

    def run(name, command, expected):
        started = time.monotonic()
        try:
            completed = subprocess.run(
                [str(compiler), *map(str, command)], cwd=out, env=environment,
                capture_output=True, text=True, timeout=args.timeout, check=False,
            )
            passed = (completed.returncode == 0) == expected
            result = dict(name=name, passed=passed, returncode=completed.returncode,
                          stdout=completed.stdout[-12000:], stderr=completed.stderr[-12000:])
        except subprocess.TimeoutExpired:
            result = dict(name=name, passed=False, timeout=True)
        result["seconds"] = round(time.monotonic() - started, 3)
        result["command"] = [str(compiler), *map(str, command)]
        results.append(result)
        print(f"{'PASS' if result['passed'] else 'FAIL'} {name}", flush=True)
        (out / "receipt.json").write_text(json.dumps(dict(results=results), indent=2) + "\n")
        return result

    def validate(name, path, expected=False):
        return run(name, ["validate", path, "--asset-root", assets, "--json"], expected)

    small = fixtures / "small.placesmap"
    write_zip(small, [("payload", b"x")])
    raw = small.read_bytes()
    directory = raw.index(b"PK\x01\x02")
    end = raw.rindex(b"PK\x05\x06")
    mutations = {
        "aggregate-limit": (directory + 24, struct.pack("<I", 1024**3 + 1)),
        "entry-count": (end + 8, struct.pack("<HH", 8193, 8193)),
        "disk-count-mismatch": (end + 8, struct.pack("<H", 2)),
        "encrypted": (directory + 8, struct.pack("<H", 1)),
        "symlink": (directory + 38, struct.pack("<I", 0o120777 << 16)),
        "eocd-comment-length": (end + 20, struct.pack("<H", 1)),
        "directory-offset": (end + 16, struct.pack("<I", directory + 1)),
        "zip64": (end + 8, struct.pack("<HH", 65535, 65535)),
        "control-name": (directory + 46, b"bad\x00key"),
    }
    for name, (offset, replacement) in mutations.items():
        altered = bytearray(raw)
        altered[offset:offset + len(replacement)] = replacement
        path = fixtures / f"{name}.placesmap"
        path.write_bytes(altered)
        validate(name, path)
    for name, records in {
        "traversal": [("../escape", b"x")],
        "absolute": [("/tmp/places-escape", b"x")],
        "windows-drive": [("C:/escape", b"x")],
        "backslash": [(r"..\escape", b"x")],
        "directory": [("folder/", b"")],
        "duplicate": [("manifest.json", b"{}"), ("manifest.json", b"{}")],
    }.items():
        path = fixtures / f"{name}.placesmap"
        write_zip(path, records)
        validate(name, path)
    if hasattr(os, "mkfifo"):
        pipe = fixtures / "named-pipe.placesmap"
        os.mkfifo(pipe, 0o600)
        validate("special-file-fifo", pipe)

    for number, package in enumerate(packages):
        prefix = f"game-{number + 1}"
        validate(f"{prefix}-finished-valid", package, True)
        with PackageZip(package) as archive:
            records = {name: archive.read(name) for name in archive.namelist()}
        index = json.loads(records["project.json"])
        recovered = out / f"{prefix}-recovered"
        recovery = run(f"{prefix}-source-recovery", ["import-project", package, "--out", recovered,
                       "--asset-root", assets, "--progress", "off"], True)
        if recovery["passed"]:
            mismatches = [name for name, blob in index["sources"].items()
                          if (recovered / name).read_bytes() != records[blob]]
            results.append(dict(name=f"{prefix}-source-byte-identity", passed=not mismatches,
                                mismatches=mismatches, files=len(index["sources"])))
            run(f"{prefix}-explicit-root-source-check",
                ["check-project", recovered, "--asset-root", assets], True)
            marker = recovered / "preserve-existing.txt"
            marker.write_text("existing destination must survive\n")
            run(f"{prefix}-existing-target-refused", ["import-project", package, "--out", recovered,
                "--asset-root", assets, "--progress", "off"], False)
            results.append(dict(name=f"{prefix}-existing-target-preserved",
                                passed=marker.read_text() == "existing destination must survive\n"))
        for mutation in ["undeclared-record", "unsafe-source", "unowned-script", "script-mismatch", "bad-hash", "source-size"]:
            current = dict(records)
            modified = json.loads(records["project.json"])
            if mutation == "undeclared-record":
                current["undeclared"] = b"unexpected"
            elif mutation in {"unsafe-source", "unowned-script", "script-mismatch"}:
                name = {"unsafe-source": "../escaped.luau", "unowned-script": "scripts/unowned.luau"}.get(mutation)
                if mutation == "script-mismatch":
                    name = next(path for path in modified["sources"] if path.endswith(".luau"))
                data = b"return {}\n"
                blob = "blobs/" + hashlib.sha256(data).hexdigest() + ".record"
                current[blob] = data
                modified["sources"][name] = blob
                referenced = {"project.json", *modified["sources"].values()}
                for level in modified["levels"]:
                    referenced.update(level["records"].values())
                current = {key: value for key, value in current.items() if key in referenced}
            elif mutation == "bad-hash":
                blob = modified["sources"]["places.project.json"]
                current[blob] = b"X" * len(current[blob])
            elif mutation == "source-size":
                # Large declared metadata only; fixture never creates the claimed bytes.
                name = next(path for path in modified["sources"] if path.endswith(".luau"))
                blob = modified["sources"][name]
            current["project.json"] = json.dumps(modified, separators=(",", ":")).encode()
            path = fixtures / f"{prefix}-{mutation}.placesmap"
            write_zip(path, current.items())
            if mutation == "source-size":
                altered = bytearray(path.read_bytes())
                cursor = 0
                while True:
                    cursor = altered.find(b"PK\x01\x02", cursor)
                    if cursor < 0:
                        raise RuntimeError("source blob central entry not found")
                    length = struct.unpack_from("<H", altered, cursor + 28)[0]
                    if altered[cursor + 46:cursor + 46 + length].decode() == blob:
                        struct.pack_into("<I", altered, cursor + 24, 65537)
                        break
                    cursor += 46 + length
                path.write_bytes(altered)
            validate(f"{prefix}-{mutation}", path)
        # A failed package import must leave a new target absent and all existing sources intact.
        target = out / f"{prefix}-bad-recovery"
        bad = fixtures / f"{prefix}-unowned-script.placesmap"
        run(f"{prefix}-malformed-recovery-refused", ["import-project", bad, "--out", target,
            "--asset-root", assets, "--progress", "off"], False)
        results.append(dict(name=f"{prefix}-no-partial-publication", passed=not target.exists()
                            and not list(out.glob(target.name + ".partial*"))))

    receipt = dict(schema="places-package-safety-v1", compiler=str(compiler), assets=str(assets),
                   packages=[dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
                             for path in packages], results=results,
                   passed=all(result["passed"] for result in results))
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    raise SystemExit(0 if receipt["passed"] else 1)


if __name__ == "__main__":
    main()
