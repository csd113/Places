#!/usr/bin/env python3
"""Stage one levels-only release ZIP from current, explicitly allowlisted projects.

Never builds maps or uploads anything. Runtime assets and applications belong to
their platform distributions. Every map retains its editable project recovery.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
NATIVE_FLAGS = (subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS) if os.name == 'nt' else 0


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def collect(source: Path, compiler: Path, assets: Path) -> dict:
    inventory = json.loads((ROOT / 'tools/release-projects.json').read_text(encoding='utf8'))
    selected = {row['package'] for row in inventory['projects']}
    if len(selected) != len(inventory['projects']):
        raise ValueError('Release inventory contains duplicate package names')
    actual = {path.name for path in source.glob('*.placesmap')}
    if actual != selected:
        raise ValueError(f'Levels directory differs from release inventory: {sorted(actual ^ selected)}')
    records = {}
    members = set()
    for row in inventory['projects']:
        name = row['package']
        if Path(name).name != name or not name.endswith('.placesmap'):
            raise ValueError(f'Unsafe package filename: {name}')
        relative = Path(row['source'])
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError(f'Unsafe project source: {relative}')
        project = ROOT / relative
        maintained = json.loads((project / 'places.project.json').read_text(encoding='utf8'))
        package = source / name
        if package.is_symlink():
            raise ValueError(f'Refusing redirected map: {package}')
        # The native validator decodes every record and verifies runtime dependencies.
        validated = subprocess.run([str(compiler), 'validate', str(package), '--asset-root', str(assets), '--json'],
                                   check=True, text=True, capture_output=True, creationflags=NATIVE_FLAGS)
        report = json.loads(validated.stdout)
        if not report['dependencies_intact']:
            raise ValueError(f'{name}: dependencies are not intact')
        with zipfile.ZipFile(package) as archive:
            index = json.loads(archive.read('project.json'))
            expected = [level['id'] for level in maintained['levels']]
            if index['project'] != maintained or [level['id'] for level in index['levels']] != expected:
                raise ValueError(f'{name}: packaged project differs from maintained source')
            # Bootstrap manifests use Deflate; binary world records can use method 93.
            variants = {}
            for level in index['levels']:
                manifest = json.loads(archive.read(level['records']['manifest.json']))
                qualities = {value['lightmap_quality']: value for value in manifest['variants']}
                if set(qualities) != {'off', 'full'} or qualities['full'].get('lightmap_failure') or not qualities['full']['entries'].get('lightmaps'):
                    raise ValueError(f'{name}/{level["id"]}: requires intact Static and Full lighting')
                source_record = next(value for value in maintained['levels'] if value['id'] == level['id'])
                variants[level['id']] = dict(compiler_fingerprint=manifest['compiler_fingerprint'],
                                            source_sha256=digest(project / source_record['source']), qualities=['off', 'full'])
        for identity in expected:
            if identity in members:
                raise ValueError(f'Duplicate release level ID: {identity}')
            members.add(identity)
            subprocess.run([str(compiler), 'verify', str(project), '--level', identity, '--package', str(package),
                            '--asset-root', str(assets), '--require-current', '--json'],
                           check=True, text=True, capture_output=True, creationflags=NATIVE_FLAGS)
        records[name] = dict(bytes=package.stat().st_size, sha256=digest(package),
                             project=maintained['id'], members=variants)
    return dict(version=inventory['version'], scope='One cross-platform levels-only ZIP; no apps or shared runtime assets.',
                source_head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                source_worktree_status=subprocess.check_output(['git', 'status', '--porcelain=v1'], cwd=ROOT, text=True).splitlines(),
                compiler_sha256=digest(compiler), catalog_sha256=digest(assets / 'catalog.json'),
                excluded=inventory['excluded'], member_count=len(members), variant_count=2 * len(members), packages=records)


def package_levels(source: Path, output: Path, compiler: Path, assets: Path) -> dict:
    if output.exists() or output.is_symlink():
        raise ValueError(f'Output already exists; preserve it and choose a new path: {output}')
    receipt = collect(source, compiler, assets)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.places-levels-', dir=output.parent) as temporary:
        staged = Path(temporary) / 'levels.zip'
        with zipfile.ZipFile(staged, 'x', compression=zipfile.ZIP_STORED) as archive:
            for name in receipt['packages']:
                archive.write(source / name, 'Maps/' + name)
            archive.writestr('LEVELS.json', json.dumps(receipt, indent=2) + '\n')
            archive.writestr('README.txt',
                'Places ' + receipt['version'] + ' levels\n\n'
                'Use with the matching Windows or Mac application distribution.\n'
                'Extract this ZIP, then import the desired Maps/*.placesmap using Explore\n'
                'or places --import <package>. You may also copy them into the writable\n'
                'levels directory shown by places --paths. Keep shared runtime assets\n'
                'with the application. Do not extract the individual .placesmap archives.\n'
                'Each package contains recoverable editable project sources; use\n'
                'places-compile import-project <package> --out <new-project-folder>.\n'
                'The application already bundles its main menu. Annex and developer\n'
                'fixtures are excluded. This archive contains no application binaries.\n')
        # A hard-link publication is atomic and refuses a concurrent existing file.
        os.link(staged, output)
    return dict(path=str(output), bytes=output.stat().st_size, sha256=digest(output), **receipt)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('maps', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--compiler', type=Path, default=ROOT / ('target/release/places-compile.exe' if os.name == 'nt' else 'target/release/places-compile'))
    parser.add_argument('--asset-root', type=Path, default=ROOT / 'assets')
    args = parser.parse_args()
    print(json.dumps(package_levels(args.maps.resolve(), args.output.absolute(), args.compiler.resolve(), args.asset_root.resolve()), indent=2))
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        raise SystemExit(f'package-levels.py: {error}') from error
