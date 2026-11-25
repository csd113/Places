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
import struct
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent

NATIVE_FLAGS = (subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS) if os.name == 'nt' else 0


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def check_recoverable_sources(project: Path, index: dict) -> None:
    """After native validation, compare verified record identities to current files."""
    for relative, record in index['sources'].items():
        path = (project / relative).resolve(strict=True)
        if not path.is_relative_to(project.resolve()) or record != f'blobs/{digest(path)}.record':
            raise ValueError(f'{project.name}: recoverable source differs: {relative}')


def retains_weather(actual, authored) -> bool:
    """Compare authored values at engine f32 precision, allowing resolved defaults."""
    if isinstance(authored, dict):
        return isinstance(actual, dict) and all(
            key in actual and retains_weather(actual[key], value) for key, value in authored.items())
    if isinstance(authored, list):
        return (isinstance(actual, list) and len(actual) == len(authored)
                and all(retains_weather(stored, value) for stored, value in zip(actual, authored, strict=True)))
    if type(authored) in (int, float):
        return type(actual) in (int, float) and struct.pack('<f', actual) == struct.pack('<f', authored)
    return type(actual) is type(authored) and actual == authored


def check_packaged_climate(archive: zipfile.ZipFile, level: dict, source: Path) -> None:
    """Read only one native-validated semantic record, never repack prepared blobs."""
    authored = json.loads(source.read_text(encoding='utf-8'))
    record = archive.getinfo(level['records']['semantics.json'])
    if record.file_size > 64 * 1024 * 1024:
        raise ValueError(f'{level["id"]}: semantics exceed the native size limit')
    packaged = json.loads(archive.read(record))
    for field in ('weather', 'weather_alternate', 'weather_cycle'):
        if not retains_weather(packaged.get(field), authored.get(field)):
            raise ValueError(f'{level["id"]}: packaged {field} differs from authored source')
    # Non-Winter source tests already establish the right sky and traction.
    # Keep those choices in the runtime semantic record as well as in recovery.
    sky = (authored.get('sky') or {}).get('texture')
    if (packaged.get('sky') or {}).get('texture') != sky:
        raise ValueError(f'{level["id"]}: packaged sky texture differs from authored source')
    floor = authored.get('defaults', {}).get('floor')
    if floor is not None and packaged.get('defaults', {}).get('floor') != floor:
        raise ValueError(f'{level["id"]}: packaged default floor differs from authored source')
    def materials(doc, field):
        return [surface.get('material') for surface in doc.get(field, [])]

    for field in ('rooms', 'floor_regions', 'ramps', 'stairs'):
        if materials(packaged, field) != materials(authored, field):
            raise ValueError(f'{level["id"]}: packaged {field} ground materials differ from authored source')


def collect(source: Path, compiler: Path, assets: Path) -> dict:
    inventory = json.loads((ROOT / 'release/release-projects.json').read_text(encoding='utf8'))
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
        # ProjectManifest materializes the documented editable-source default.
        # Compare its value without changing the original recoverable source.
        maintained.setdefault('package_version', '1.0.0')
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
            check_recoverable_sources(project, index)
            # Compiler-authored manifests and semantics are Deflate records;
            # native validation handles large prepared records (including method 93).
            variants = {}
            for level in index['levels']:
                manifest = json.loads(archive.read(level['records']['manifest.json']))
                qualities = {value['lightmap_quality']: value for value in manifest['variants']}
                if set(qualities) != {'off', 'full'} or qualities['full'].get('lightmap_failure') or not qualities['full']['entries'].get('lightmaps'):
                    raise ValueError(f'{name}/{level["id"]}: requires intact Static and Full lighting')
                source_record = next(value for value in maintained['levels'] if value['id'] == level['id'])
                authored_level = project / source_record['source']
                check_packaged_climate(archive, level, authored_level)
                variants[level['id']] = dict(compiler_fingerprint=manifest['compiler_fingerprint'],
                                            source_sha256=digest(authored_level), qualities=['off', 'full'])
        for identity in expected:
            if identity in members:
                raise ValueError(f'Duplicate release level ID: {identity}')
            members.add(identity)
        subprocess.run([str(compiler), 'verify-project', str(project), '--package', str(package),
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
                'places-map-compiler import-project <package> --out <new-project-folder>.\n'
                'The application already bundles its main menu. Archived projects and\n'
                'developer fixtures are excluded. This archive contains no application binaries.\n')
        # A hard-link publication is atomic and refuses a concurrent existing file.
        os.link(staged, output)
    return dict(path=str(output), bytes=output.stat().st_size, sha256=digest(output), **receipt)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('maps', type=Path)
    parser.add_argument('output', nargs='?', type=Path)
    parser.add_argument('--check-only', action='store_true', help='Verify prepared levels without creating a ZIP')
    parser.add_argument('--compiler', type=Path, default=ROOT / ('target/release/places-map-compiler.exe' if os.name == 'nt' else 'target/release/places-map-compiler'))
    parser.add_argument('--asset-root', type=Path, default=ROOT / 'assets')
    args = parser.parse_args()
    if args.check_only and args.output is not None:
        parser.error('--check-only does not take an output path')
    if not args.check_only and args.output is None:
        parser.error('supply an output ZIP path or --check-only')
    source, compiler, assets = (path.resolve() for path in (args.maps, args.compiler, args.asset_root))
    receipt = collect(source, compiler, assets) if args.check_only else package_levels(
        source, args.output.absolute(), compiler, assets)
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        raise SystemExit(f'package-levels.py: native verification failed: {error.stderr.strip()}') from error
    except (OSError, ValueError, KeyError, OverflowError, struct.error, NotImplementedError, zipfile.BadZipFile) as error:
        raise SystemExit(f'package-levels.py: {error}') from error
