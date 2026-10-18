#!/usr/bin/env python3
"""Export and validate curated distributions; never discover local review maps."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import zipfile

from curated_maps import ROOT, level_views, lineup, project_manifest, source_levels, validate_lighting


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=ROOT / 'target/release/places-compile')
    parser.add_argument('--output', type=Path, default=ROOT / 'assets/levels')
    parser.add_argument('--workers', type=int, default=8)
    parser.add_argument('--check', action='store_true', help='Require current full packages; do not rebuild')
    parser.add_argument('--map', dest='map_id', help='Build one distribution ID, including all project sub-maps')
    parser.add_argument('--list-json', action='store_true', help='Print validated distribution IDs for tooling')
    args = parser.parse_args()
    if not 1 <= args.workers <= 12:
        parser.error('--workers must be in 1..12')
    compiler = str(args.compiler.resolve())
    output = args.output.resolve()
    rows = lineup()
    if args.list_json:
        print(json.dumps([identity for identity, _ in rows]))
        return 0
    if args.map_id:
        rows = [row for row in rows if row[0] == args.map_id]
        if not rows:
            parser.error('--map must name an ID in levels/lineup.txt')
    if not args.check:
        output.mkdir(parents=True, exist_ok=True)
    for identity, source in rows:
        package = output / (identity + '.placesmap')
        if not args.check:
            operation = 'export-project' if project_manifest(source) else 'build'
            subprocess.run([compiler, operation, str(source), '--out', str(package),
                            '--asset-root', str(ROOT / 'assets'), '--workers', str(args.workers)],
                           cwd=ROOT, check=True)
        for level_id, _ in source_levels(source):
            selected = ['--level', level_id] if project_manifest(source) else []
            subprocess.run([compiler, 'verify', str(source), *selected, '--package', str(package),
                            '--asset-root', str(ROOT / 'assets'), '--require-current'], cwd=ROOT, check=True)
        subprocess.run([compiler, 'validate', str(package)], cwd=ROOT, check=True)
        with zipfile.ZipFile(package) as archive:
            views = level_views(archive)
            if [identity for identity, _ in views] != [identity for identity, _ in source_levels(source)]:
                raise ValueError(f'{package} does not contain every maintained level')
            for level_id, records in views:
                # Inspect every sub-map; the CLI's ordinary inspect selects the entry map.
                manifest = json.loads(archive.read(records['manifest.json']))
                if manifest['id'] != level_id:
                    raise ValueError(f'{package} has a mismatched manifest for {level_id}')
                validate_lighting(manifest, package)
    print(f'Validated {len(rows)} curated distributions in {output}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
