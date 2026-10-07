#!/usr/bin/env python3
"""Capture and walk the same polished Winter scene in calm and severe weather.

Uses the real native player and existing held-control diagnostics. Compile both
packages first; the severe source retains Prompt 7's exact weather settings.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
from pathlib import Path
import subprocess

import capture_snowfall as native

ROOT = Path(__file__).resolve().parents[2]
EXTRA = {
    'walk-rock': ([5, -18, 90], [90, -8], 'forward@0-2,backward@2.2-2.8', 3),
    'walk-rail': ([15.2, -10, 90], [90, -8], 'forward@0-2,backward@2.2-2.8', 3),
    'walk-cottage-north': ([12.5, -21, 0], [0, -3], 'forward@0-2.2,backward@2.6-4.8', 5),
    'walk-cottage-south': ([-12.5, 13.1, 0], [0, -3], 'forward@0-2.2,backward@2.6-4.8', 5),
    'forest-rest': ([0, -30.7, -55], [-55, 2], '', 2),
    'shore-corner': ([7, -3, 0], [0, -24], '', 2),
}


def route_result(path: Path, name: str) -> dict:
    with path.open() as stream:
        points = [tuple(map(float, row[1:4])) for row in csv.reader(stream) if float(row[2]) != 0]
    if not points:
        raise RuntimeError(f'No ready player states: {path}')
    xs, ys, zs = zip(*points)
    checks = {
        'walk-lodge': max(ys) >= 2.19 and min(zs) < -12 and zs[-1] > -3.7,
        'walk-stairs': max(ys) >= 2.19 and max(xs) > -15.1 and xs[-1] < -18,
        'walk-pond': min(ys) <= 1.45 and max(ys) > 2.3 and max(xs) > 10.5 and xs[-1] < 4.2,
        'walk-forest': min(zs) < -34 and max(ys) < 1.61,
        'walk-ice-coast': 11.8 < xs[-1] < 12.6 and max(ys) < 1.45,
        'walk-snow-stop': 10.8 < xs[-1] < 11.15 and min(ys) > 1.59 and max(ys) < 1.61,
        'walk-ice-wall': 16.3 < xs[-1] < 16.55 and max(ys) < 1.61,
        'walk-rock': 5.8 < max(xs) < 6.2 and xs[-1] < 4.6,
        'walk-rail': 16.3 < max(xs) < 16.55 and xs[-1] < 15.5,
        'walk-cottage-north': min(zs) < -26 and zs[-1] > -21.3 and max(ys) < 1.61,
        'walk-cottage-south': min(zs) < 8 and zs[-1] > 12.8 and max(ys) < 1.61,
    }
    return {'passed': min(ys) >= 1.39 and checks[name],
            'bounds': [[min(axis), max(axis)] for axis in (xs, ys, zs)],
            'final': points[-1]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--qualities', default='high')
    parser.add_argument('--modes', default='calm,severe')
    parser.add_argument('--views', default='square,lodge,pond,forest,interior,entrance-snow,railing-snow,tree-lit-snow,string-cottage,shore-corner,forest-rest,walk-lodge,walk-stairs,walk-pond,walk-forest,walk-ice-coast,walk-snow-stop,walk-ice-wall,walk-rock,walk-rail,walk-cottage-north,walk-cottage-south')
    args = parser.parse_args()
    for view in json.loads((ROOT / 'tools/bench/winter_views.json').read_text()):
        native.VIEWS[view['name']] = (view['spawn'], view['camera'], view.get('move_script', ''),
                                      0 if view['name'] == 'overview' else view.get('capture_time', 1.5))
    native.VIEWS.update(EXTRA)
    for mode in args.modes.split(','):
        if mode not in ('calm', 'severe'):
            parser.error('mode must be calm or severe')
        level = 'winter' if mode == 'calm' else 'blizzard_review'
        source = ROOT / ('assets/levels/winter.json' if mode == 'calm' else
                         'debug-maps/blizzard-20261007/sources/blizzard_review.json')
        package = ROOT / ('assets/levels/winter.placesmap' if mode == 'calm' else
                          'levels/blizzard_review.placesmap')
        subprocess.run([str(ROOT / 'target/release/places-compile'), 'verify', str(source),
                        '--package', str(package), '--require-current'], check=True)
        for quality in args.qualities.split(','):
            out = args.out.resolve() / mode / quality
            out.mkdir(parents=True, exist_ok=True)
            summaries = []
            for name in args.views.split(','):
                summary = native.run(out, quality, name, level=level)
                if name.startswith('walk-'):
                    summary['traversal'] = route_result(out / (summary['label']+'-player.csv'), name)
                summaries.append(summary)
                manifest = {'quality': quality, 'level': level,
                            'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                            'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
                            'binary_sha256': hashlib.sha256((ROOT / 'target/release/places').read_bytes()).hexdigest(),
                            'runs': summaries}
                (out / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
                if not summary.get('traversal', {}).get('passed', True):
                    raise RuntimeError(f'Traversal failed: {name}: {summary["traversal"]}')


if __name__ == '__main__':
    main()
