#!/usr/bin/env python3
"""Compile and validate the curated source lineup; never discover local dev maps."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def lineup(root: Path = ROOT) -> list[tuple[str, Path]]:
    ids = (root / 'levels/lineup.txt').read_text().splitlines()
    if not ids or len(ids) != len(set(ids)):
        raise ValueError('levels/lineup.txt must contain unique map IDs')
    rows = []
    for identity in ids:
        if not identity or not all(c.isascii() and (c.isalnum() or c == '_') for c in identity):
            raise ValueError(f'Invalid curated map ID: {identity!r}')
        source = root / 'levels' / (identity + '.json')
        if json.loads(source.read_text())['id'] != identity:
            raise ValueError(f'{source} does not author ID {identity}')
        rows.append((identity, source))
    actual = {p.stem for p in (root / 'levels').glob('*.json')}
    if actual != set(ids):
        raise ValueError(f'Curated sources differ from lineup: {sorted(actual ^ set(ids))}')
    return rows


def validate_lighting(manifest: dict, package: Path) -> None:
    variants = {variant['lightmap_quality']: variant for variant in manifest['variants']}
    if set(variants) != {'off', 'full'}:
        raise ValueError(f'{package} does not contain both Static and Full lighting variants')
    full = variants['full']
    if full.get('lightmap_failure') or not full.get('entries', {}).get('lightmaps'):
        raise ValueError(f'{package} Full lighting fell back instead of retaining its baked atlas')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=ROOT / 'target/release/places-compile')
    parser.add_argument('--output', type=Path, default=ROOT / 'assets/levels')
    parser.add_argument('--workers', type=int, default=8)
    parser.add_argument('--check', action='store_true', help='Require current full packages; do not rebuild')
    parser.add_argument('--map', dest='map_id', help='Build one ID from the curated lineup')
    parser.add_argument('--list-json', action='store_true', help='Print validated lineup IDs for CI')
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
            subprocess.run([compiler, 'build', str(source), '--out', str(package),
                            '--asset-root', str(ROOT / 'assets'), '--workers', str(args.workers)],
                           cwd=ROOT, check=True)
        subprocess.run([compiler, 'verify', str(source), '--package', str(package),
                        '--asset-root', str(ROOT / 'assets'), '--require-current'], cwd=ROOT, check=True)
        subprocess.run([compiler, 'validate', str(package)], cwd=ROOT, check=True)
        manifest = json.loads(subprocess.check_output([compiler, 'inspect', str(package), '--json'],
                                                     cwd=ROOT, text=True))
        # inspect emits the manifest directly; no reduced-quality package can pass export.
        validate_lighting(manifest, package)
    print(f'Validated {len(rows)} curated map packages in {output}', flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
