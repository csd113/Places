#!/usr/bin/env python3
"""Stage a Windows x64 portable payload from existing binaries; never bake maps.

Use --with-editor to include both editor and compiler. An explicit --maps directory
is validated using the staged runtime resources. Existing products are refused.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
DLLS = ('SDL3.dll', 'dxcompiler.dll', 'dxil.dll', 'msvcp140.dll',
        'vcruntime140.dll', 'vcruntime140_1.dll')


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def reparse(path: Path) -> bool:
    return path.is_symlink() or bool(getattr(path.lstat(), 'st_file_attributes', 0)
                                    & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def destination(path: Path) -> Path:
    requested = path.absolute()
    for part in (requested, *requested.parents):
        if part.exists() and reparse(part):
            raise ValueError(f'Refusing a redirected destination: {part}')
    output = requested.resolve()
    for protected in (ROOT.resolve(), Path.home().resolve()):
        if output == protected or output in protected.parents:
            raise ValueError(f'Refusing source, home or ancestor destination: {output}')
    if output.exists() and not output.is_dir():
        raise ValueError(f'Output is not a directory: {output}')
    if (output/'Places').exists() or (output/'Places').is_symlink():
        raise ValueError('Places already exists; choose a new output root to preserve it')
    return output


def x64_pe(path: Path) -> None:
    with path.open('rb') as stream:
        if stream.read(2) != b'MZ':
            raise ValueError(f'Not a Windows executable: {path.name}')
        stream.seek(0x3c)
        offset = struct.unpack('<I', stream.read(4))[0]
        stream.seek(offset)
        if stream.read(6) != b'PE\0\0\x64\x86':
            raise ValueError(f'Not an x64 PE file: {path.name}')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--bin-dir', type=Path, default=ROOT/'target/release')
    parser.add_argument('--runtime-dir', type=Path,
                        help='Directory containing the six required Windows runtime DLLs')
    parser.add_argument('--sdl-license', type=Path,
                        help='License from the exact SDL distribution supplying SDL3.dll')
    parser.add_argument('--runtime-notices', type=Path,
                        help='Windows SDK/VC runtime notices; defaults to <runtime-dir>/licenses')
    parser.add_argument('--with-compiler', action='store_true')
    parser.add_argument('--with-editor', action='store_true')
    parser.add_argument('--maps', type=Path)
    parser.add_argument('--build-receipt', type=Path,
                        help='Optional exact build/source identity receipt, copied into payload')
    parser.add_argument('--check-destination', action='store_true')
    args = parser.parse_args()
    output = destination(args.output)
    if args.check_destination:
        print(f'Destination is safe: {output}')
        return 0
    if os.name != 'nt':
        raise ValueError('Run the Windows packager on Windows to validate native discovery')
    if not args.runtime_dir or not args.sdl_license:
        parser.error('--runtime-dir and --sdl-license are required')
    notices = args.runtime_notices or args.runtime_dir/'licenses'
    if not notices.is_dir() or not any(p.is_file() for p in notices.iterdir()):
        raise ValueError('Windows runtime notices are missing; supply --runtime-notices or <runtime-dir>/licenses')
    binaries = ['places.exe']
    if args.with_compiler or args.with_editor:
        binaries.append('places-compile.exe')
    if args.with_editor:
        binaries.append('places-editor.exe')
    if args.maps and not (args.bin_dir/'places-compile.exe').is_file():
        raise ValueError('--maps requires the matching compiler in --bin-dir')
    inputs = [(name, args.bin_dir/name) for name in binaries]
    inputs.extend((name, args.runtime_dir/name) for name in DLLS)
    for _, source in inputs:
        x64_pe(source)
    if not args.sdl_license.is_file():
        raise ValueError('SDL license is missing')
    if args.maps and not args.maps.is_dir():
        raise ValueError('--maps must be an existing directory')
    version = re.search(r'^version = "([^"]+)"', (ROOT/'Cargo.toml').read_text(), re.M).group(1)
    # Every cleanup target below is our own freshly allocated temporary directory.
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.places-windows-stage-', dir=output) as scratch:
        stage = Path(scratch)/'Places'
        stage.mkdir()
        for name, source in inputs:
            shutil.copyfile(source, stage/name)
        for name in ('README.md', 'LICENSE', 'THIRD_PARTY_LICENSES.txt'):
            shutil.copyfile(ROOT/name, stage/name)
        shutil.copyfile(args.sdl_license, stage/'SDL3-LICENSE.txt')
        shutil.copytree(notices, stage/'licenses/windows-runtime')
        (stage/'docs').mkdir()
        for name in ['INSTALLATION.md'] + (['EDITOR.md'] if args.with_editor else []):
            shutil.copyfile(ROOT/'docs'/name, stage/'docs'/name)
        assets = stage/'assets'
        (assets/'levels').mkdir(parents=True)
        for name in ('catalog.json', 'gameplay.json'):
            shutil.copyfile(ROOT/'assets'/name, assets/name)
        for name in ('audio', 'shared', 'environments', 'entities', 'diagnostic', 'janitor_closet'):
            shutil.copytree(ROOT/'assets'/name, assets/name,
                            ignore=shutil.ignore_patterns('*_master.png', '*.blend', '*.blend1',
                                                         'CABINET_ARTWORK_PROMPTS.md', '__pycache__'))
        if args.with_editor:
            shutil.copytree(ROOT/'assets/editor', assets/'editor')
        branding = assets/'shared/branding'
        if branding.is_dir():
            shutil.rmtree(branding)
        if args.maps:
            spec = importlib.util.spec_from_file_location('places_export_maps', ROOT/'tools/export-maps.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            module.export_maps(args.maps.resolve(), stage, (args.bin_dir/'places-compile.exe').resolve())
        git_head = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
        dirty = subprocess.check_output(['git','status','--porcelain=v1'], cwd=ROOT, text=True).splitlines()
        provenance = dict(version=version, platform='windows-x86_64', source_head=git_head,
                          source_worktree_status=dirty,
                          scope='Existing optimized binaries and validated maps; packaging performs no bake.',
                          runtime_notices={p.relative_to(notices).as_posix():dict(bytes=p.stat().st_size,sha256=digest(p))
                                           for p in sorted(notices.rglob('*')) if p.is_file()},
                          inputs={name:dict(bytes=source.stat().st_size,sha256=digest(source))
                                  for name,source in inputs})
        if args.build_receipt:
            shutil.copyfile(args.build_receipt, stage/'BUILD_RECEIPT.json')
            provenance['build_receipt_sha256'] = digest(args.build_receipt)
        (stage/'WINDOWS_RUNTIME.json').write_text(json.dumps(provenance, indent=2)+'\n', encoding='utf8')
        files = {path.relative_to(stage).as_posix():dict(bytes=path.stat().st_size,sha256=digest(path))
                 for path in sorted(stage.rglob('*')) if path.is_file()}
        (stage/'PAYLOAD_SHA256.json').write_text(json.dumps(files, indent=2)+'\n', encoding='utf8')
        # A concurrently created destination is never replaced.
        stage.rename(output/'Places')
    print(f'Packaged Places {version}: {output/"Places"} ({len(files)+1} files)', flush=True)
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'package-windows.py: {error}') from error
