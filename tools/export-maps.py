#!/usr/bin/env python3
"""Validate explicitly selected external maps before a staged export is published."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def export_maps(source: Path, payload: Path, compiler: Path) -> None:
    packages = sorted(source.glob('*.placesmap'))
    archived = json.loads((ROOT / 'tools/release-projects.json').read_text(encoding='utf8'))['archived_projects']
    archived_names = {row['package'] for row in archived}
    archived_projects = {row['project_id'] for row in archived}
    archived_levels = {identity for row in archived for identity in row['level_ids']}
    if any(package.name in archived_names for package in packages):
        raise ValueError('Archived project packages are excluded from new exports')
    env = dict(os.environ)
    # Validate against the actual exported runtime assets, with isolated state.
    env['PLACES_ASSET_ROOT'] = str(payload)
    expected_ids = []
    with tempfile.TemporaryDirectory(prefix='places-export-state-') as state:
        env['PLACES_STATE_ROOT'] = state
        env['PLACES_CONFIG_ROOT'] = str(Path(state) / 'config')
        env['PLACES_CACHE_ROOT'] = str(Path(state) / 'cache')
        for package in packages:
            result = subprocess.run([str(compiler), 'validate', str(package), '--json'],
                                    env=env, text=True, capture_output=True, check=True)
            report = json.loads(result.stdout)
            if not report['dependencies_intact']:
                raise ValueError(f"{package.name}: {'; '.join(report['warnings'])}")
            # Native validation above admits the bounded index first. Discovery
            # lists every project member, even with an entry-only Explore row.
            with zipfile.ZipFile(package) as archive:
                if 'project.json' in archive.namelist():
                    index = json.loads(archive.read('project.json'))
                    identities = [level['id'] for level in index['levels']]
                    if index['project']['id'] in archived_projects:
                        raise ValueError(f'{package.name}: archived project is excluded from new exports')
                else:
                    identities = [json.loads(archive.read('manifest.json'))['id']]
                if archived_levels.intersection(identities):
                    raise ValueError(f'{package.name}: archived level is excluded from new exports')
                expected_ids.extend(identities)
            shutil.copyfile(package, payload / 'assets/levels' / package.name)
        # Ordinary player discovery also checks catalog provenance and semantic
        # compatibility. A stale, duplicate or malformed optional map must not
        # silently disappear from an export that explicitly selected it.
        discovered = subprocess.run([str(payload / ('places.exe' if os.name == 'nt' else 'places')), '--list-levels'],
                                    env=env, text=True, capture_output=True, check=True)
        rows = discovered.stdout.splitlines()
        discovered_ids = [row.split('\t', 1)[0] for row in rows]
        if sorted(discovered_ids) != sorted(expected_ids):
            raise ValueError(f'Discovered maps {discovered_ids} differ from selected members {expected_ids}: '
                             f'{discovered.stderr.strip()}')
    print(f'Included {len(packages)} external map package(s)', flush=True)


if __name__ == '__main__':
    try:
        export_maps(*(Path(argument).resolve() for argument in sys.argv[1:]))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'package.sh: external map validation failed: {error}', file=sys.stderr)
        sys.exit(2)
