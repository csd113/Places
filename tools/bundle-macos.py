#!/usr/bin/env python3
"""Relocate the host SDL runtime in private package stages, then ad-hoc sign.

Only the current SDL3 + Apple system-library linkage is supported. Unexpected
dependencies fail the export before publication instead of shipping a payload
that still needs a developer's machine. This is not notarization or release signing.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys


def run(*args: str | Path) -> str:
    return subprocess.check_output(list(map(str, args)), text=True, stderr=subprocess.STDOUT)


def dependencies(binary: Path) -> list[str]:
    # otool indents dependency lines; splitting at the version suffix preserves
    # paths with spaces. The first line names the inspected binary itself.
    return [line.strip().split(' (compatibility version', 1)[0]
            for line in run('otool', '-L', binary).splitlines()[1:] if line.strip()]


def system_library(path: str) -> bool:
    return path.startswith(('/usr/lib/', '/System/Library/'))


def version_key(version: str) -> tuple[int, ...]:
    return tuple(map(int, (version.split('.') + ['0'])[:3]))


def minimum_macos(binary: Path) -> str:
    versions = re.findall(r'^\s*minos (\d+\.\d+(?:\.\d+)?)\s*$', run('otool', '-l', binary), re.MULTILINE)
    if not versions:
        raise ValueError(f'{binary.name}: cannot determine minimum macOS from LC_BUILD_VERSION')
    return max(versions, key=version_key)


def inspect(binaries: list[Path]) -> tuple[Path, dict[Path, str]]:
    selected = {}
    for binary in binaries:
        external = [name for name in dependencies(binary) if not system_library(name)]
        if len(external) != 1:
            raise ValueError(f'{binary.name}: expected only SDL3 outside Apple libraries: {external}')
        name = external[0]
        path = Path(name)
        if not path.is_absolute() or path.name != 'libSDL3.0.dylib' or not path.is_file():
            raise ValueError(f'{binary.name}: unsupported SDL linkage {name}; rebuild with an absolute SDL3 library')
        selected[binary] = name
    sources = {Path(name).resolve() for name in selected.values()}
    if len(sources) != 1:
        raise ValueError('player and tools must use the same SDL3 runtime')
    source = sources.pop()
    # The first dylib entry is its install ID, not another dependency.
    external = [name for name in dependencies(source)[1:] if not system_library(name)]
    if external:
        raise ValueError(f'SDL3 has unsupported non-system dependencies: {external}')
    return source, selected


def bundle(flat: Path, app: Path) -> None:
    if sys.platform != 'darwin':
        raise ValueError('macOS runtime bundling must run on macOS')
    names = [name for name in ('places', 'places-compile', 'places-editor') if (flat / name).is_file()]
    source, originals = inspect([flat / name for name in names])
    minimum = max((minimum_macos(binary) for binary in [source, *originals]), key=version_key)
    supplied = os.environ.get('PLACES_SDL_LICENSE')
    license_path = (Path(supplied) if supplied else
                    source.parent.parent / 'share/licenses/sdl3/LICENSE.txt')
    if not license_path.is_file() or not 0 < license_path.stat().st_size <= 65536:
        raise ValueError('SDL3 license missing/invalid; set PLACES_SDL_LICENSE to its LICENSE.txt')
    with license_path.open('rb') as license_file:
        license_bytes = license_file.read(65537)
    if len(license_bytes) > 65536 or b'Sam Lantinga' not in license_bytes:
        raise ValueError('expected the SDL3 license, at most 64 KiB')
    receipt = {'runtime': 'SDL3', 'file': 'libSDL3.0.dylib',
               'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
               'minimum_macos': minimum,
               'executables': names, 'signing': 'ad-hoc; not notarized'}
    for binary_root, library_root, resource_root in (
        (flat, flat / 'lib', flat),
        (app / 'Contents/MacOS', app / 'Contents/Frameworks', app / 'Contents/Resources'),
    ):
        library_root.mkdir()
        library = library_root / 'libSDL3.0.dylib'
        shutil.copyfile(source, library)
        library.chmod(0o755)
        run('install_name_tool', '-id', '@rpath/libSDL3.0.dylib', library)
        run('codesign', '--force', '--sign', '-', library)
        run('codesign', '--verify', '--strict', library)
        relative = os.path.relpath(library, binary_root)
        for name in names:
            binary = binary_root / name
            run('install_name_tool', '-change', originals[flat / name],
                f'@executable_path/{relative}', binary)
            # Editing Mach-O load commands invalidates Rust's linker signature.
            run('codesign', '--force', '--sign', '-', binary)
            run('codesign', '--verify', '--strict', binary)
            external = [path for path in dependencies(binary) if not system_library(path)]
            if external != [f'@executable_path/{relative}']:
                raise ValueError(f'{name}: unrelocated dependencies: {external}')
        (resource_root / 'SDL3-LICENSE.txt').write_bytes(license_bytes)
        (resource_root / 'MACOS_RUNTIME.json').write_text(json.dumps(receipt, indent=2) + '\n')
    plist_path = app / 'Contents/Info.plist'
    with plist_path.open('rb') as file:
        plist = plistlib.load(file)
    plist['LSMinimumSystemVersion'] = minimum
    with plist_path.open('wb') as file:
        plistlib.dump(plist, file)
    run('codesign', '--force', '--sign', '-', app)
    run('codesign', '--verify', '--strict', app)
    print(f'Bundled SDL3; effective minimum macOS {minimum}; ad-hoc signed', flush=True)


if __name__ == '__main__':
    try:
        if len(sys.argv) != 3:
            raise ValueError('usage: bundle-macos.py <staged-flat-directory> <staged-app>')
        bundle(*(Path(value).resolve(strict=True) for value in sys.argv[1:]))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'package.sh: macOS runtime bundling failed: {error}', file=sys.stderr)
        sys.exit(2)
