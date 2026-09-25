#!/usr/bin/env python3
"""Compare a captured canonical view set with the frozen renderer reference.

Exact PNG equality is required on the baseline host/display. Missing or extra
views fail as well as changed images. Does not overwrite either input.
"""
import argparse
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("current", type=Path)
    parser.add_argument("--reference", type=Path, default=Path("docs/renderer-baseline"))
    args = parser.parse_args()
    failures = 0
    for profile in ("high", "low"):
        reference = {p.name: p for p in (args.reference / profile).glob("*.png")}
        current = {p.name: p for p in (args.current / profile).glob("*.png")}
        if not reference or reference.keys() != current.keys():
            print(f"FAIL {profile}: missing/extra views or empty reference")
            failures += 1
            continue
        changed = [name for name, path in reference.items()
                   if path.read_bytes() != current[name].read_bytes()]
        if changed:
            print(f"FAIL {profile}: changed images: {', '.join(sorted(changed))}")
            failures += 1
        else:
            print(f"PASS {profile}: {len(reference)} byte-identical images")
    return int(failures != 0)


if __name__ == "__main__":
    raise SystemExit(main())
