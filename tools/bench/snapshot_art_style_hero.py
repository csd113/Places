#!/usr/bin/env python3
"""Preserve one immutable local Art-style runnable milestone outside target.

Copies the player, optional diagnostic player/compiler, hero package, source,
camera manifest, catalog and hash-verified runtime dependencies. Never overwrites
an existing snapshot and never compiles or captures another map.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import shlex
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--package', required=True, type=Path)
    parser.add_argument('--revision', required=True, help='Verified implementation revision or explicit precommit base')
    parser.add_argument('--diagnostics', type=Path)
    parser.add_argument('--compiler', type=Path)
    parser.add_argument('--runtime-library', type=Path, action='append', default=[],
                        help='Native shared library to preserve beside the player')
    parser.add_argument('--runtime-asset', type=Path, action='append', default=[],
                        help='Additional assets-relative file needed by runtime-only spawns')
    parser.add_argument('--catalog', type=Path, default=ROOT/'assets/catalog.json',
                        help='Compatible catalog, including a preserved before-state catalog')
    parser.add_argument('--manifest', type=Path, default=ROOT/'docs/art-style/hero-manifest.json')
    args = parser.parse_args()
    out = args.out.resolve()
    if out.exists():
        parser.error('Snapshot already exists; preserve it and choose a new milestone')
    with zipfile.ZipFile(args.package) as archive:
        package_manifest = json.loads(archive.read('manifest.json'))
    cameras = json.loads(args.manifest.read_text())
    dependencies = package_manifest['dependencies']
    extra_assets = []
    for relative in args.runtime_asset:
        source = (ROOT/'assets'/relative).resolve()
        if not source.is_relative_to((ROOT/'assets').resolve()) or not source.is_file():
            parser.error('Runtime asset must be a real file within assets/: '+str(relative))
        extra_assets.append(source.relative_to((ROOT/'assets').resolve()))
    for dependency in dependencies:
        source = ROOT/'assets'/dependency['path']
        if source.stat().st_size != dependency['bytes'] or digest(source) != dependency['sha256']:
            parser.error('Asset differs from the package: '+str(source))
    out.mkdir(parents=True)
    copies = [('places', args.binary), ('assets/levels/'+args.package.name, args.package),
              ('hero-source.json', ROOT/cameras['source']), ('hero-manifest.original.json', args.manifest),
              ('assets/catalog.json', args.catalog),
              ('capture_art_style_hero.py', ROOT/'tools/bench/capture_art_style_hero.py')]
    if args.diagnostics:
        copies.append(('places-diagnostics', args.diagnostics))
    if args.compiler:
        copies.append(('places-compile', args.compiler))
    copies.extend((library.name, library) for library in args.runtime_library)
    copies.extend(('assets/'+dependency['path'], ROOT/'assets'/dependency['path']) for dependency in dependencies)
    copies.extend(('assets/'+str(relative), ROOT/'assets'/relative) for relative in extra_assets
                  if all(name != 'assets/'+str(relative) for name, _ in copies))
    # Embedded fallbacks remain in the player; keep their real source files too.
    for relative in ['core/textures/white_01.png', 'core/textures/missing_01.png']:
        source = ROOT/'assets'/relative
        if source.is_file() and all(name != 'assets/'+relative for name, _ in copies):
            copies.append(('assets/'+relative, source))
    files = []
    for name, source in copies:
        destination = out/name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        files.append(dict(path=name, bytes=destination.stat().st_size, sha256=digest(destination)))
    replay = dict(cameras, source=str(out/'hero-source.json'),
                  package=str(out/'assets/levels'/args.package.name))
    replay_path = out/'hero-manifest.json'
    replay_path.write_text(json.dumps(replay, indent=2)+'\n')
    files.append(dict(path=replay_path.name, bytes=replay_path.stat().st_size,
                      sha256=digest(replay_path)))
    quoted_out = shlex.quote(str(out))
    receipt = dict(stage_revision=args.revision,
                   additional_runtime_assets=[str(relative) for relative in extra_assets],
                   source_checkout_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
                   package_compiler_fingerprint=package_manifest['compiler_fingerprint'],
                   files=files, runtime_asset_root=str(out),
                   launch='DYLD_LIBRARY_PATH='+quoted_out+' PLACES_ASSET_ROOT='+quoted_out+
                   ' PLACES_LEVEL='+shlex.quote(cameras['level'])+' '+shlex.quote(str(out/'places')),
                   replay='DYLD_LIBRARY_PATH='+quoted_out+' python3 '+
                   shlex.quote(str(ROOT/'tools/bench/capture_art_style_hero.py'))+
                   ' --asset-root '+quoted_out+' --manifest '+shlex.quote(str(replay_path))+
                   ' --binary '+shlex.quote(str(out/'places'))+' --out NEW_OUTPUT_DIRECTORY',
                   native_runtime='macOS Metal; preserved native SDL3 library when supplied; not a cross-platform binary bundle')
    (out/'snapshot.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(dict(snapshot=str(out), files=len(files), bytes=sum(file['bytes'] for file in files))))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
