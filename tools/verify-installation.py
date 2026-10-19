#!/usr/bin/env python3
"""Explicit native path/import acceptance using a tiny existing project/package.

Copies an installation into a fresh evidence folder before changing permissions
or simulating missing files. Never launches through a desktop app handler.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(installation: Path, project: Path, package: Path, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    work = output / 'cases'
    work.mkdir()  # Never reuse, overwrite or remove an earlier acceptance run.
    installed = work / 'Installed Places ü'
    shutil.copytree(installation, installed)
    player = installed / ('places.exe' if sys.platform == 'win32' else 'places')
    compiler = installed / ('places-compile.exe' if sys.platform == 'win32' else 'places-compile')
    state = work / 'User Data ü'
    launch = work / 'empty launch'
    launch.mkdir()
    other_launch = work / 'another launch'
    other_launch.mkdir()
    source = work / 'project'
    shutil.copytree(project, source, ignore=shutil.ignore_patterns('packages', 'native-state', '*.placesmap'))
    incoming = work / 'incoming'
    incoming.mkdir()
    original = incoming / package.name
    shutil.copyfile(package, original)
    base = {key: value for key, value in os.environ.items() if not key.startswith('PLACES_')}
    base.pop('SDL_VIDEODRIVER', None)
    base['PLACES_STATE_ROOT'] = str(state)
    checks = []

    def run(label, args, *, changes=None, cwd=launch, expected=0, native=False):
        environment = {**base, **(changes or {})}
        if native:
            environment.update({'PLACES_BENCH': '1', 'PLACES_CAPTURE': str(output / (label + '.png')),
                                'PLACES_CAPTURE_FRAME': '3', 'PLACES_VERBOSE': '1'})
        result = subprocess.run(list(map(str, args)), cwd=cwd, env=environment,
                                text=True, capture_output=True, timeout=120, check=False)
        (output / (label + '.log')).write_text(result.stdout + result.stderr)
        assert result.returncode == expected, (label, result.returncode, result.stderr)
        if native:
            assert (output / (label + '.png')).is_file(), (label, 'missing native capture')
            assert 'fatal device error' not in result.stdout + result.stderr
        checks.append({'check': label, 'exit_code': result.returncode})
        return result

    try:
        paths = json.loads(run('paths-empty', [player, '--paths']).stdout)
        assert Path(paths['assets']) == (installed / 'assets').resolve()
        assert Path(paths['data']) == state
        for key in ('levels', 'import', 'projects', 'cache', 'saves'):
            assert Path(paths[key]).is_dir()
        assert run('empty-discovery', [player, '--list-levels']).stdout == ''
        run('native-empty-explore', [player], changes={'PLACES_SCREEN': 'explore'}, native=True)
        settings = state / 'settings.json'
        saved_settings = settings.read_bytes()
        save_file = Path(paths['saves']) / 'preservation-probe.save'
        save_file.write_bytes(b'user save data must survive first-run initialization\n')
        saved_data = save_file.read_bytes()
        run('paths-overrides', [player, '--paths'], changes={
            'PLACES_ASSET_ROOT': str(installed), 'PLACES_CONFIG_ROOT': str(work / 'Config ü'),
            'PLACES_CACHE_ROOT': str(work / 'Cache ü')})
        assert json.loads((output / 'paths-overrides.log').read_text())['config'] == str(work / 'Config ü')
        run('bad-asset-override', [player, '--paths'], changes={'PLACES_ASSET_ROOT': str(work / 'absent-assets')}, expected=1)
        run('empty-state-override', [player, '--paths'], changes={'PLACES_STATE_ROOT': ''}, expected=1)
        obstruction = work / 'state-is-file'
        obstruction.write_text('keep')
        run('invalid-user-root', [player, '--paths'], changes={'PLACES_STATE_ROOT': str(obstruction)}, expected=1)
        run('assets-as-user-root', [player, '--paths'], changes={'PLACES_STATE_ROOT': str(installed / 'assets')}, expected=1)
        run('compiler-explicit-precedence', [compiler, 'validate', original, '--asset-root', installed / 'assets', '--json'],
            changes={'PLACES_ASSET_ROOT': str(work / 'absent-assets')})
        run('compiler-invalid-explicit', [compiler, 'validate', original, '--asset-root', work / 'absent-assets'], expected=1)
        built = work / 'new-output.placesmap'
        run('installed-compiler-project', [compiler, 'build', source, '--out', built,
                                         '--variants', 'static', '--workers', '2', '--progress', 'off', '--json'])
        run('installed-compiler-verify', [compiler, 'verify', source, '--package', built, '--require-current', '--json'])
        shutil.copyfile(original, state / 'import' / original.name)
        actions = work / 'import-actions.json'
        actions.write_text(json.dumps([
            {'after': 'start', 'delay_ms': 500, 'action': {'kind': 'key', 'key': 'Return', 'pressed': True}},
            {'after': 'start', 'delay_ms': 600, 'action': {'kind': 'key', 'key': 'Return', 'pressed': False}},
        ]))
        run('native-inbox-import', [player], changes={'PLACES_SCREEN': 'explore', 'PLACES_BENCH_ACTIONS': str(actions)}, native=True)
        assert (state / 'levels' / original.name).is_file()
        run('import-initial', [player, '--import', original])
        installed_map = state / 'levels' / original.name
        imported_hash = digest(installed_map)
        run('import-repeat', [player, '--import', original])
        assert digest(installed_map) == imported_hash
        duplicate = incoming / 'duplicate.placesmap'
        shutil.copyfile(original, duplicate)
        run('import-duplicate-id', [player, '--import', duplicate], expected=1)
        original.write_bytes(b'not a package')
        run('import-corrupt-preserves', [player, '--import', original], expected=1)
        assert digest(installed_map) == imported_hash
        shutil.copyfile(package, original)
        discovery = run('restart-other-cwd', [player, '--list-levels'], cwd=other_launch).stdout
        level_id = discovery.split('\t')[0]
        assert str(installed_map) in discovery
        run('native-imported-play', [player], changes={'PLACES_LEVEL': level_id}, cwd=other_launch, native=True)
        assert settings.read_bytes() == saved_settings
        assert save_file.read_bytes() == saved_data
        assert not list((state / 'levels').glob('*.partial*'))
        assert not (launch / 'settings.json').exists()
        assert not (installed / 'settings.json').exists()
        # Remove one dependency in our private copy; restoration happens even
        # on failure. Existing user maps must remain untouched after rejection.
        catalog = json.loads((installed / 'assets/catalog.json').read_text())
        chair_resource = next(asset['model'] for asset in catalog['assets'] if asset['id'] == 'core:chair')
        missing = installed / 'assets' / chair_resource
        backup = work / 'chair.glb.saved'
        missing.rename(backup)
        try:
            run('missing-required-model', [player, '--paths'], expected=1)
            run('missing-dependency-validation', [compiler, 'validate', original, '--json'])
            report = json.loads((output / 'missing-dependency-validation.log').read_text())
            assert not report['dependencies_intact']
        finally:
            backup.rename(missing)
        bare = work / 'bare-binary'
        bare.mkdir()
        shutil.copyfile(player, bare / player.name)
        (bare / player.name).chmod(player.stat().st_mode)
        # Keep the relocated macOS loader dependency so this case reaches
        # Places' missing-assets error, rather than failing in dyld first.
        if (installed / 'lib').is_dir():
            shutil.copytree(installed / 'lib', bare / 'lib')
        run('missing-resource-payload', [bare / player.name, '--paths'], expected=1)
        if os.name == 'posix':
            owned = [installed, *installed.rglob('*')]
            modes = {path: stat.S_IMODE(path.stat().st_mode) for path in owned}
            try:
                for path, mode in modes.items():
                    path.chmod(mode & ~0o222)
                run('readonly-install-discovery', [player, '--list-levels'])
                run('native-readonly-install', [player], changes={'PLACES_LEVEL': level_id}, native=True)
                fresh_state = work / 'Readonly Install First Run ü'
                run('native-readonly-first-run', [player], cwd=installed, changes={
                    'PLACES_STATE_ROOT': str(fresh_state), 'PLACES_SCREEN': 'explore'}, native=True)
                fresh_paths = json.loads(run('readonly-first-run-paths', [player, '--paths'], cwd=installed,
                    changes={'PLACES_STATE_ROOT': str(fresh_state)}).stdout)
                assert (fresh_state / 'settings.json').is_file()
                for key in ('levels', 'import', 'projects', 'cache', 'saves'):
                    assert Path(fresh_paths[key]).is_dir()
                readonly = work / 'readonly-user'
                readonly.mkdir(mode=0o555)
                try:
                    run('unwritable-user-root', [player, '--paths'], changes={'PLACES_STATE_ROOT': str(readonly)}, expected=1)
                finally:
                    readonly.chmod(0o755)
            finally:
                for path, mode in modes.items():
                    path.chmod(mode)
        summary = {'platform': sys.platform, 'checks': checks,
                   'installation': str(installed), 'state': str(state),
                   'package_sha256': imported_hash, 'settings_preserved': True,
                   'save_file_preserved': True,
                   'limitations': ['explicit isolated user storage; default platform locations covered by pure path tests',
                                   'system frameworks remain OS-provided; no clean-OS installer acceptance',
                                   'no Windows/Linux native, installer/signing or performance acceptance']}
        (output / 'installation-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
        print(json.dumps(summary, indent=2))
    except BaseException:
        (output / 'completed-checks.json').write_text(json.dumps(checks, indent=2) + '\n')
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installation', type=Path, required=True, help='map-free flat payload exported with --with-compiler')
    parser.add_argument('--project', type=Path, required=True, help='tiny revision-one project directory')
    parser.add_argument('--package', type=Path, required=True, help='compiled tiny package with built-in chair dependency')
    parser.add_argument('--out', type=Path, required=True, help='evidence directory; cases/ must not exist')
    options = parser.parse_args()
    verify(*(path.resolve() for path in (options.installation, options.project, options.package, options.out)))
