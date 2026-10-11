#!/usr/bin/env python3
"""Read-only delivery accounting for already compiled, native-validated maps.

No browser or network benchmark: selected-variant figures are sums of existing
compressed ZIP members, plus the existing index/manifest overhead. Dependencies
are verified local bytes, not a new fetch implementation. Output is exclusive.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import zipfile

from map_storage import inspect

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1024 * 1024):
            value.update(block)
    return value.hexdigest()


def references(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from references(item)
    elif isinstance(value, list):
        for item in value:
            yield from references(item)


def delivery(paths, asset_root):
    maps, dependency_files, dependency_hashes, payloads = [], {}, {}, {}
    for path in paths:
        row = inspect(path, verify=True)
        row['sha256'] = digest(path)
        deps = {}
        for dependency in row['dependencies']:
            relative = Path(dependency['path'])
            if relative.is_absolute() or '..' in relative.parts:
                raise ValueError('unsafe dependency path')
            source = (asset_root / relative).resolve()
            if not source.is_relative_to(asset_root.resolve()):
                raise ValueError('dependency escapes asset root')
            size, identity = source.stat().st_size, digest(source)
            if size != dependency['bytes'] or identity != dependency['sha256']:
                raise ValueError(f'dependency identity mismatch: {source}')
            dependency_files[str(relative)] = size
            dependency_hashes[identity] = size
            deps[identity] = size
        row['dependency_unique_bytes'] = sum(deps.values())
        row['cold_all_variants_bytes'] = row['file_bytes'] + sum(deps.values())
        row['warm_dependencies_all_variants_bytes'] = row['file_bytes']
        with zipfile.ZipFile(path) as archive:
            manifest = json.loads(archive.read('manifest.json'))
            entries = {entry['name']: entry for entry in manifest['entries']}
            common = {'manifest.json', *(name for name in entries if not name.startswith('blobs/'))}
            for name, entry in entries.items():
                info = archive.getinfo(name)
                identity = entry['sha256']
                if identity in payloads and payloads[identity] != (info.file_size, info.compress_size):
                    raise ValueError('equal payload hash has different stored size')
                payloads[identity] = (info.file_size, info.compress_size)
            selected = []
            for variant in row['variants']:
                names = common | set(references(variant['entries']))
                if not names <= set(archive.namelist()):
                    raise ValueError('missing selected-variant member')
                encoded = sum(archive.getinfo(name).compress_size for name in names)
                decoded = sum(archive.getinfo(name).file_size for name in names)
                # Keep ALL original ZIP headers/index in the estimate. A real
                # range reader could use fewer headers, but that is unmeasured.
                transport = encoded + row['zip_structure_bytes']
                selected.append(dict(quality=variant['lightmap_quality'],
                                     member_count=len(names), decoded_bytes=decoded,
                                     compressed_payload_bytes=encoded,
                                     with_original_index_bytes=transport,
                                     cold_with_dependencies_bytes=transport + sum(deps.values())))
            row['selected_variant_estimates'] = selected
            semantics = json.loads(archive.read('semantics.json'))
            row['rooms'] = len(semantics.get('rooms', []))
            row['placed_props'] = len(semantics.get('props', []))
        maps.append(row)
    packages = sum(row['file_bytes'] for row in maps)
    deps_total = sum(row['dependency_unique_bytes'] for row in maps)
    return dict(note='Measured file/member/dependency bytes; hypothetical selected fetch, no browser implementation.',
                units='bytes; MB = 1000000 bytes; MiB = 1048576 bytes', maps=maps,
                collection=dict(package_bytes=packages,
                                dependencies_per_map_sum_bytes=deps_total,
                                installed_dependency_path_bytes=sum(dependency_files.values()),
                                content_unique_dependency_bytes=sum(dependency_hashes.values()),
                                repeated_dependencies_bytes=deps_total-sum(dependency_hashes.values()),
                                unique_payload_raw_bytes=sum(raw for raw, _ in payloads.values()),
                                unique_payload_compressed_bytes=sum(encoded for _, encoded in payloads.values()),
                                repeated_payload_compressed_bytes=sum(
                                    role['compressed'] for row in maps for name, role in row['roles'].items()
                                    if name != 'manifest')-sum(encoded for _, encoded in payloads.values()),
                                cold_all_maps_bytes=packages+sum(dependency_hashes.values())))


def verify_factor(original_path, prototype_path):
    """Verify the ON-DISK isolated factoring ZIP reconstructs every original bit.

    Diagnostic for locally generated inputs, not a production untrusted reader.
    Native bounds/validation must be applied to originals before this is used.
    """
    with zipfile.ZipFile(original_path) as original, zipfile.ZipFile(prototype_path) as prototype:
        recipe = json.loads(prototype.read('prototype-recipes.json'))
        if recipe['investigation_format'] != 'exact-prop-factoring-1':
            raise ValueError('unknown prototype layout')
        locations = recipe['chunk_locations']
        segments = {}

        def chunk(index):
            if not 0 <= index < len(locations):
                raise ValueError('chunk reference out of range')
            segment, offset, length = locations[index]
            if segment not in segments:
                segments[segment] = prototype.read(f'prototype-{segment}.props')
            data = segments[segment]
            if offset < 0 or length < 0 or offset + length > len(data):
                raise ValueError('chunk outside segment')
            return data[offset:offset+length]

        hashes = {}
        for entry in original.infolist():
            if entry.filename in recipe['records']:
                record = recipe['records'][entry.filename]
                restored = bytearray()
                for part in record['parts']:
                    if part[0] == 'raw':
                        restored.extend(b''.join(chunk(index) for index in part[1]))
                    elif part[0] == 'vertices':
                        _, stride, count, prefix_refs, tail_refs = part
                        prefix = b''.join(chunk(index) for index in prefix_refs)
                        tail = b''.join(chunk(index) for index in tail_refs)
                        if len(prefix) != count * 36 or len(tail) != count * (stride-36):
                            raise ValueError('vertex plane lengths mismatch')
                        for vertex in range(count):
                            restored.extend(prefix[vertex*36:(vertex+1)*36])
                            restored.extend(tail[vertex*(stride-36):(vertex+1)*(stride-36)])
                    else:
                        raise ValueError('unknown part kind')
                    if len(restored) > record['bytes']:
                        raise ValueError('expanded record exceeds original length')
                if len(restored) != record['bytes']:
                    raise ValueError('expanded length mismatch')
            else:
                restored = prototype.read(entry.filename)
            baseline = original.read(entry)
            if restored != baseline:
                raise ValueError(f'changed original bytes: {entry.filename}')
            hashes[entry.filename] = hashlib.sha256(restored).hexdigest()
        return dict(original_sha256=digest(original_path), prototype_sha256=digest(prototype_path),
                    runtime_loadable=False, all_original_entries_byte_equal=True,
                    original_entry_sha256=hashes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packages', type=Path, nargs='*')
    parser.add_argument('--asset-root', type=Path, default=ROOT / 'assets')
    parser.add_argument('--verify-factor', type=Path, help='verify isolated prototype against one original')
    parser.add_argument('--out', type=Path, required=True, help='new ignored JSON receipt')
    args = parser.parse_args()
    if args.verify_factor:
        if len(args.packages) != 1:
            parser.error('--verify-factor requires one original package')
        result = verify_factor(args.packages[0], args.verify_factor)
    else:
        paths = args.packages or [ROOT / 'assets/levels' / f'{name}.placesmap'
                                 for name in (ROOT / 'levels/lineup.txt').read_text().splitlines()
                                 if name and not name.startswith('#')]
        result = delivery(paths, args.asset_root)
    with args.out.open('x') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    main()
