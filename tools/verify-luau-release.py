#!/usr/bin/env python3
"""Protect Luau's foreign-unwind boundary in the actual production executable.

Cargo's test harness uses unwind even when release binaries use abort. This
unique regression runs tiny deliberately invalid modules through the direct
release compiler, before any geometry/asset bake. It costs three tiny source files
and three bounded process starts, not a GPU or any shipped map preparation.
"""
import argparse
import json
import subprocess
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, default=Path('target/release/places-compile'))
    parser.add_argument('--asset-root', type=Path, default=Path('assets'))
    args = parser.parse_args()
    compiler, assets = args.compiler.resolve(), args.asset_root.resolve()
    cases = [
        ('while true do end', 'execution budget exhausted'),
        ("local huge = string.rep('x', 4000000); return {}", 'not enough memory'),
        ("error('release refusal fixture')", 'release refusal fixture'),
    ]
    with tempfile.TemporaryDirectory(prefix='places-luau-release-') as directory:
        project = Path(directory)
        (project / 'levels').mkdir()
        (project / 'scripts').mkdir()
        (project / 'places.project.json').write_text(json.dumps({
            'schema': 'places-project', 'project_format': 1, 'id': 'release_guard',
            'name': 'Release guard', 'entry_level': 'entry',
            'levels': [{'id': 'entry', 'source': 'levels/entry.json'}],
        }))
        (project / 'levels/entry.json').write_text(json.dumps({
            'schema': 'places-level', 'format_version': 1, 'id': 'entry',
            'name': 'Release guard', 'spawn': {'x': 0, 'z': 0},
            'scripts': ['scripts/guard.luau'],
        }))
        output = project / 'levels/entry.placesmap'
        for source, expected in cases:
            (project / 'scripts/guard.luau').write_text(source)
            result = subprocess.run([
                str(compiler), 'build', str(project), '--asset-root', str(assets),
                '--progress', 'off', '--json',
            ], capture_output=True, text=True, timeout=15, check=False)
            combined = result.stdout + result.stderr
            if result.returncode != 1 or expected not in combined or output.exists():
                raise SystemExit(f'Luau release refusal failed: exit={result.returncode}, expected={expected!r}\n{combined[:4096]}')
    print('Luau production error boundary: 3 bounded refusals passed; no map baked')


if __name__ == '__main__':
    main()
