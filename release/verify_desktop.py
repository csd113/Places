#!/usr/bin/env python3
"""Verify authored sources or an existing flat desktop payload without baking."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SUFFIX = '.exe' if os.name == 'nt' else ''


def run(command, *, cwd: Path, env: dict[str, str]) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, encoding='utf-8',
                            capture_output=True, check=False)
    if result.returncode:
        detail = '\n'.join(output.strip() for output in (result.stdout, result.stderr) if output.strip())
        raise ValueError(f'{result.args[0]} exited {result.returncode}: {detail}')
    return result.stdout


def isolated(scratch: Path) -> dict[str, str]:
    env = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    env.update(PLACES_STATE_ROOT=str(scratch), PLACES_CONFIG_ROOT=str(scratch / 'config'),
               PLACES_CACHE_ROOT=str(scratch / 'cache'))
    return env


def check_sources(compiler: Path, assets: Path) -> None:
    """Use the production Luau/check-project gate for every shipped editor source."""
    inventory = json.loads((ROOT / 'release/release-projects.json').read_text(encoding='utf-8'))
    project_root = (ROOT / 'projects').resolve()
    projects = [(ROOT / row['source']).resolve(strict=True) for row in inventory['projects']]
    if not projects or any(not project.is_relative_to(project_root) for project in projects):
        raise ValueError('Release inventory must name projects inside the checkout')
    projects.append(project_root / 'menu')
    with tempfile.TemporaryDirectory(prefix='places-desktop-source-') as temporary:
        scratch = Path(temporary)
        command = [sys.executable, ROOT / 'tests/release_luau.py', '--compiler', compiler,
                   '--asset-root', assets]
        for project in projects:
            command.extend(('--project', project))
        print(run(command, cwd=scratch, env=isolated(scratch)).strip())
        for project in projects:
            print(f'Source validated without changes: {project.relative_to(ROOT)}')


def smoke_payload(payload: Path) -> None:
    """Exercise the three packaged CLIs outside the checkout with no asset override."""
    payload = payload.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='places-desktop-install-') as temporary:
        scratch = Path(temporary)
        env = isolated(scratch)
        for name, flag in [('places', '--list-levels'), ('places-editor', '--help'),
                           ('places-map-compiler', '--help')]:
            run([payload / (name + SUFFIX), flag], cwd=scratch, env=env)
            print(f'Installed {name} {flag}: passed')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    profiles = parser.add_subparsers(dest='profile', required=True)
    source = profiles.add_parser('source', help='Validate the maintained projects and production Luau boundary')
    source.add_argument('--compiler', type=Path, default=ROOT / 'target/release' / ('places-map-compiler' + SUFFIX))
    source.add_argument('--asset-root', type=Path, default=ROOT / 'assets')
    install = profiles.add_parser('install', help='Smoke-test the installed flat payload without using source assets')
    install.add_argument('payload', type=Path)
    args = parser.parse_args()
    if args.profile == 'source':
        check_sources(args.compiler.resolve(strict=True), args.asset_root.resolve(strict=True))
    else:
        smoke_payload(args.payload)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(f'verify_desktop.py: {error}') from error
