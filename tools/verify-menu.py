#!/usr/bin/env python3
"""Validate the final bundled menu against exported assets; never bake a map."""
from pathlib import Path
import argparse
import json
import os
import subprocess
import tempfile


def verify_menu(compiler: Path, assets: Path, project: Path) -> None:
    package = assets / 'shared/menu/scene.bin'
    environment = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    with tempfile.TemporaryDirectory(prefix='places-menu-validation-') as state:
        environment.update(PLACES_STATE_ROOT=state, PLACES_CONFIG_ROOT=state,
                           PLACES_CACHE_ROOT=str(Path(state) / 'cache'))
        flags = (subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS) if os.name == 'nt' else 0
        result = subprocess.run([str(compiler), 'validate', str(package), '--asset-root', str(assets), '--json'],
                                env=environment, text=True, capture_output=True, check=True, creationflags=flags)
        if not json.loads(result.stdout)['dependencies_intact']:
            raise ValueError('Bundled menu dependencies are not intact')
        subprocess.run([str(compiler), 'verify', str(project), '--level', 'places_menu',
                        '--package', str(package), '--asset-root', str(assets), '--require-current', '--json'],
                       env=environment, text=True, capture_output=True, check=True, creationflags=flags)
    print('Bundled menu validated against final payload and current source', flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', type=Path, required=True)
    parser.add_argument('--assets', type=Path, required=True)
    parser.add_argument('--project', type=Path, required=True)
    args = parser.parse_args()
    try:
        verify_menu(*(path.resolve() for path in (args.compiler, args.assets, args.project)))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        detail = error.stderr if isinstance(error, subprocess.CalledProcessError) else str(error)
        raise SystemExit(f'Bundled menu validation failed: {detail}') from error
