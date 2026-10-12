#!/usr/bin/env python3
"""Validate explicitly selected external maps before a staged export is published."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile


def export_maps(source: Path, payload: Path, compiler: Path) -> None:
    packages = sorted(source.glob('*.placesmap'))
    env = dict(os.environ)
    # Validate against the actual exported runtime assets, with isolated state.
    env['PLACES_ASSET_ROOT'] = str(payload)
    with tempfile.TemporaryDirectory(prefix='places-export-state-') as state:
        env['PLACES_STATE_ROOT'] = state
        for package in packages:
            result = subprocess.run([str(compiler), 'validate', str(package), '--json'],
                                    env=env, text=True, capture_output=True, check=True)
            report = json.loads(result.stdout)
            if not report['dependencies_intact']:
                raise ValueError(f"{package.name}: {'; '.join(report['warnings'])}")
            shutil.copyfile(package, payload / 'assets/levels' / package.name)
        # Ordinary player discovery also checks catalog provenance and semantic
        # compatibility. A stale, duplicate or malformed optional map must not
        # silently disappear from an export that explicitly selected it.
        discovered = subprocess.run([str(payload / 'places'), '--list-levels'],
                                    env=env, text=True, capture_output=True, check=True)
        rows = discovered.stdout.splitlines()
        if len(rows) != len(packages):
            raise ValueError(f'Only {len(rows)} of {len(packages)} selected maps are usable: '
                             f'{discovered.stderr.strip()}')
    print(f'Included {len(packages)} external map package(s)', flush=True)


if __name__ == '__main__':
    try:
        export_maps(*(Path(argument).resolve() for argument in sys.argv[1:]))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'package.sh: external map validation failed: {error}', file=sys.stderr)
        sys.exit(2)
