#!/usr/bin/env python3
"""Attribute a .placesmap's exact archive bytes without compiling a level.

The optional shuffle experiment uses stdlib zlib, with a matching unshuffled
control. It reconstructs and hashes every original atlas byte. It is NOT a
runtime-loadable format or a production encoder. Output belongs in ignored
tools/bench/results/, never docs/. Native codec repacks use the package-repack tool binary
(`cargo build --release --bin package-repack`).
Inputs must first pass native places-compile validation; this diagnostic tool
does not duplicate all native archive/semantic security checks.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import time
import zipfile
import sys as _package_sys
_package_sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'levels'))
from package_zip import PackageZip

import zlib

MAX_TOTAL = 1024 * 1024 * 1024
MAX_ATLAS = 640 * 1024 * 1024 + 65536
MAX_META = 64 * 1024 * 1024


def checked_archive(path):
    archive = PackageZip(path)
    entries = archive.infolist()
    names = [entry.filename for entry in entries]
    if 'project.json' in names:
        archive.close()
        raise ValueError(
            'This ordinary-map storage/delivery diagnostic does not analyze shared project bundles; '
            'use places-compile validate and the collection acceptance tools for the complete project.'
        )
    if len(names) > 512 or len(names) != len(set(names)):
        raise ValueError("entry count or duplicate names")
    if sum(entry.file_size for entry in entries) > MAX_TOTAL:
        raise ValueError("aggregate exceeds existing 1 GiB limit")
    for entry in entries:
        name = entry.filename
        if (entry.is_dir() or name.startswith(('/', '\\')) or '\\' in name
                or ':' in name or '..' in Path(name).parts):
            raise ValueError("unsafe archive name")
        limit = MAX_ATLAS if name.endswith('.lightmaps.ktx2') else 256 * 1024 * 1024
        if name.endswith(('.props', '.mesh')):
            limit = 512 * 1024 * 1024
        if name.endswith('.lightmaps.json'):
            limit = MAX_META
        if name == 'manifest.json':
            limit = 2 * 1024 * 1024
        if name == 'semantics.json':
            limit = 64 * 1024 * 1024
        if name == 'materials.json':
            limit = 16 * 1024 * 1024
        if name.endswith(('.collision', '.navigation')):
            limit = 128 * 1024 * 1024
        if name.endswith('.png'):
            limit = 64 * 1024 * 1024
        if entry.file_size > limit:
            raise ValueError("entry exceeds existing typed limit")
        if entry.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, 93):
            raise ValueError("unsupported codec for this inspection tool")
    manifest = json.loads(archive.read('manifest.json'))
    declared = {entry['name']: entry for entry in manifest['entries']}
    if len(declared) != len(manifest['entries']) or set(declared) != set(names) - {'manifest.json'}:
        raise ValueError("declarations do not match archive")
    for entry in entries:
        if entry.filename != 'manifest.json' and declared[entry.filename]['bytes'] != entry.file_size:
            raise ValueError("declared length mismatch")
    return archive, manifest, declared


def inspect(path, verify=False):
    archive, manifest, declared = checked_archive(path)
    with archive:
        rows, resources, atlases = {}, [], []
        for entry in archive.infolist():
            role = declared.get(entry.filename, {}).get('role', 'manifest')
            row = rows.setdefault(role, dict(count=0, raw=0, compressed=0))
            row['count'] += 1
            row['raw'] += entry.file_size
            row['compressed'] += entry.compress_size
            resources.append(dict(name=entry.filename, role=role, raw=entry.file_size,
                                  compressed=entry.compress_size))
            if verify and entry.filename != 'manifest.json':
                digest, length = hashlib.sha256(), 0
                with archive.open(entry) as stream:
                    while block := stream.read(1024 * 1024):
                        length += len(block)
                        if length > entry.file_size:
                            raise ValueError("decompression exceeded declared length")
                        digest.update(block)
                if length != entry.file_size or digest.hexdigest() != declared[entry.filename]['sha256']:
                    raise ValueError("entry checksum/length mismatch")
        for variant in manifest['variants']:
            name = variant['entries'].get('lightmaps_meta')
            if not name:
                continue
            meta = json.loads(archive.read(name))
            rectangles = {}
            for chart in meta['charts']:
                rectangle = tuple(chart['chart'][key] for key in ('page', 'x', 'y', 'width', 'height'))
                # Mirror LightmapConfig::padding_for: model receivers use one
                # bilinear gutter texel; architecture retains configured padding.
                padding = min(meta['padding'], 1) if chart['patch']['kind'] == 'prop' else meta['padding']
                if rectangle in rectangles and rectangles[rectangle] != padding:
                    raise ValueError('conflicting padding for shared chart rectangle')
                rectangles[rectangle] = padding
            data = sum(width * height for _, _, _, width, height in rectangles)
            padded = sum((width + 2 * padding) * (height + 2 * padding)
                         for (_, _, _, width, height), padding in rectangles.items())
            capacity = meta['page_edge'] ** 2 * meta['page_count']
            if padded > capacity:
                raise ValueError('chart allocation exceeds atlas capacity')
            atlases.append(dict(quality=variant['lightmap_quality'], pages=meta['page_count'],
                               edge=meta['page_edge'], groups=1 + len(meta['switchable_lights']),
                               chart_references=len(meta['charts']), unique_rectangles=len(rectangles),
                               data_texels=data, gutter_texels=padded-data,
                               unallocated_texels=capacity-padded, capacity_texels=capacity,
                               stats=meta['stats']))
        compressed = sum(row['compressed'] for row in rows.values())
        return dict(path=str(path), id=manifest['id'], file_bytes=path.stat().st_size,
                    allocated_bytes=path.stat().st_blocks * 512,
                    uncompressed_bytes=sum(row['raw'] for row in rows.values()),
                    zip_structure_bytes=path.stat().st_size-compressed,
                    roles=rows, atlases=atlases,
                    largest=sorted(resources, key=lambda row: -row['compressed'])[:8],
                    variants=manifest['variants'], dependencies=manifest['dependencies'],
                    hashes_verified=verify)


def raw_deflate(data):
    compressor = zlib.compressobj(6, zlib.DEFLATED, -15)
    return compressor.compress(data) + compressor.flush()


def inflate_exact(encoded, expected):
    if not 0 <= expected <= MAX_ATLAS:
        raise ValueError("expanded length exceeds existing atlas bound")
    decoder = zlib.decompressobj(-15)
    decoded = decoder.decompress(encoded, expected + 1)
    if len(decoded) != expected or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:
        raise ValueError("truncated, overlong or trailing compressed payload")
    return decoded


def shuffle(data, block_bytes, stride=8):
    if len(data) % stride or block_bytes < stride:
        raise ValueError("shuffle requires whole records")
    block_bytes = block_bytes // stride * stride
    out = bytearray(len(data))
    for offset in range(0, len(data), block_bytes):
        block = data[offset:offset + block_bytes]
        length = len(block) // stride
        for lane in range(stride):
            start = offset + lane * length
            out[start:start + length] = block[lane::stride]
    return out


def unshuffle(data, block_bytes, stride=8):
    if len(data) % stride or block_bytes < stride:
        raise ValueError("invalid shuffled length")
    block_bytes = block_bytes // stride * stride
    out = bytearray(len(data))
    for offset in range(0, len(data), block_bytes):
        length = min(block_bytes, len(data)-offset) // stride
        for lane in range(stride):
            start = offset + lane * length
            out[offset + lane:offset + length * stride:stride] = data[start:start + length]
    return out


def atlas_experiment(path, block_bytes):
    archive, manifest, declared = checked_archive(path)
    results = []
    with archive:
        for name, entry in declared.items():
            if entry['role'] != 'lightmaps':
                continue
            original = archive.read(name)
            if hashlib.sha256(original).hexdigest() != entry['sha256']:
                raise ValueError("baseline atlas digest mismatch")
            offset, length, expanded = struct.unpack_from('<QQQ', original, 80)
            if offset + length != len(original) or length != expanded or length % 8:
                raise ValueError("unsupported KTX2 level shape")
            started = time.perf_counter()
            control = raw_deflate(original)
            control_seconds = time.perf_counter()-started
            started = time.perf_counter()
            reordered = original[:offset] + shuffle(original[offset:], block_bytes)
            encoded = raw_deflate(reordered)
            encode_seconds = time.perf_counter()-started
            started = time.perf_counter()
            decoded = inflate_exact(encoded, len(original))
            reconstructed = decoded[:offset] + unshuffle(decoded[offset:], block_bytes)
            decode_seconds = time.perf_counter()-started
            if reconstructed != original:
                raise ValueError("shuffle round trip changed atlas bytes")
            if raw_deflate(reordered) != encoded:
                raise ValueError("shuffle encoding is not deterministic")
            results.append(dict(name=name, raw=len(original), project_deflate6=archive.getinfo(name).compress_size,
                                stdlib_control=len(control), shuffled=len(encoded),
                                savings_vs_matching_control=len(control)-len(encoded),
                                control_pack_seconds=control_seconds, shuffle_pack_seconds=encode_seconds,
                                shuffle_decode_seconds=decode_seconds, exact_byte_equal=True))
    return dict(path=str(path), block_bytes=block_bytes, zlib_version=zlib.ZLIB_RUNTIME_VERSION,
                note='representation experiment only; no loadable package or decoder migration', atlases=results)


def self_test():
    # Byte patterns include both signed zeros and arbitrary half-float bits.
    original = bytes(range(256)) * 256
    for block_bytes in (8192, 65536, 1048576):
        reordered = shuffle(original, block_bytes)
        encoded = raw_deflate(reordered)
        assert unshuffle(inflate_exact(encoded, len(original)), block_bytes) == original
        assert raw_deflate(reordered) == encoded
        for bad, length in ((encoded[:-1], len(original)), (encoded + b'x', len(original)),
                            (encoded, len(original)-1), (encoded, MAX_ATLAS+1)):
            try:
                inflate_exact(bad, length)
            except (ValueError, zlib.error):
                pass
            else:
                raise AssertionError("invalid payload accepted")
    for mode, stride in enumerate((81, 55, 57)):
        prefix = b'PMP1\x01\x00' + struct.pack('<II', 1, 4) + b'test' + bytes(26)
        prefix += struct.pack('<II', 0, 2) + bytes([mode])
        if mode:
            prefix += struct.pack('<I', 1) + bytes(28)
        sample = prefix + bytes(range(stride)) * 2 + struct.pack('<IHHH', 3, 0, 1, 0)
        reordered = transform_props(sample, 65536)
        restored = bytearray(reordered)
        for start, end, region_stride in prop_regions(reordered):
            restored[start:end] = unshuffle(reordered[start:end], 65536, region_stride)
        assert restored == sample
        try:
            prop_regions(sample[:-1])
        except ValueError:
            pass
        else:
            raise AssertionError('truncated prop accepted')
    print('atlas/prop roundtrip, deterministic output, truncation, trailing bytes and bounds: PASS')


def prop_regions(data):
    """Locate PMP1 revision 1 vertex spans, without decoding or re-encoding a float."""
    if data[:6] != b'PMP1\x01\x00':
        raise ValueError('prop experiment requires PMP1 revision 1')
    cursor = 6

    def take(length):
        nonlocal cursor
        end = cursor + length
        if end > len(data):
            raise ValueError('truncated prop record')
        start = cursor
        cursor = end
        return start

    def u32():
        return struct.unpack_from('<I', data, take(4))[0]

    def option():
        marker = data[take(1)]
        if marker not in (0, 1):
            raise ValueError('invalid prop option')
        if marker:
            take(2)

    batches = u32()
    if batches > 1 << 20:
        raise ValueError('prop batch bound')
    regions = []
    for _ in range(batches):
        take(u32())  # UTF-8 model name
        take(1)     # caster flag
        control = data[take(1)]
        if control not in (0, 1):
            raise ValueError("invalid light controller option")
        if control:
            take(4)
        take(24)    # two XYZ bounds
        submeshes = u32()
        if submeshes > 4096:
            raise ValueError('prop submesh bound')
        for _ in range(submeshes):
            option()
            take(21)  # alpha mode/cutoff and emission
            option()
            take(20)  # specular, roughness, UV response strength
            option()  # UV response texture
            take(20)  # UV response tint and index range
        vertices = u32()
        if vertices > 1 << 24:
            raise ValueError('prop vertex bound')
        mode = data[take(1)]
        if mode not in (0, 1, 2):
            raise ValueError('unsupported prop vertex mode')
        if mode:
            frames = u32()
            if frames > vertices:
                raise ValueError('prop frame bound')
            take(frames * 28)
        stride = (81, 55, 57)[mode]
        start = take(vertices * stride)
        regions.append((start, cursor, stride))
        take(u32() * 2)
    if cursor != len(data):
        raise ValueError('trailing prop bytes')
    return regions


def transform_props(data, block_bytes):
    out = bytearray(data)
    for start, end, stride in prop_regions(data):
        reordered = shuffle(data[start:end], block_bytes, stride)
        if unshuffle(reordered, block_bytes, stride) != data[start:end]:
            raise ValueError('prop vertex roundtrip changed bits')
        out[start:end] = reordered
    return out


def emit_shuffle(path, destination, block_bytes, kind):
    """Emit an intentionally unplayable STORE ZIP for the native codec bench.

    Unchanged manifests retain ORIGINAL hashes: the current player MUST reject
    this input. It is a transform experiment, never a replacement .placesmap.
    """
    archive, manifest, declared = checked_archive(path)
    with archive, PackageZip(destination, 'x', compression=zipfile.ZIP_STORED) as output:
        for entry in archive.infolist():
            original = archive.read(entry)
            expected = declared.get(entry.filename, {}).get('sha256')
            if expected and hashlib.sha256(original).hexdigest() != expected:
                raise ValueError('baseline record hash mismatch')
            data = original
            role = declared.get(entry.filename, {}).get('role')
            if role == 'props' and kind in ('props', 'both'):
                data = transform_props(original, block_bytes)
            if role == 'lightmaps' and kind in ('atlas', 'both'):
                offset, length, expanded = struct.unpack_from('<QQQ', original, 80)
                if offset + length != len(original) or length != expanded or length % 8:
                    raise ValueError('unsupported atlas shape')
                data = original[:offset] + shuffle(original[offset:], block_bytes)
                if data[:offset] + unshuffle(data[offset:], block_bytes) != original:
                    raise ValueError('native benchmark input roundtrip failed')
            output.writestr(entry.filename, data)


def emit_factor(path, destination):
    """Exact cross-quality prop prefix/chunk sharing; unplayable research ZIP."""
    archive, manifest, declared = checked_archive(path)
    chunks, identities, recipes = [], {}, {}

    def references(data):
        refs = []
        for offset in range(0, len(data), 65520):
            chunk = bytes(data[offset:offset + 65520])
            digest = hashlib.sha256(chunk).digest()
            if digest in identities:
                index = identities[digest]
                if chunks[index] != chunk:
                    raise ValueError('chunk digest collision')
            else:
                index = len(chunks)
                identities[digest] = index
                chunks.append(chunk)
            refs.append(index)
        return refs

    def restore(parts, expected):
        out = bytearray()

        def append(data):
            if len(out) + len(data) > expected:
                raise ValueError('reconstruction exceeds original record bound')
            out.extend(data)

        for part in parts:
            if part[0] == 'raw':
                for index in part[1]:
                    append(chunks[index])
            else:
                _, stride, count, prefix_refs, tail_refs = part
                prefix = b''.join(chunks[index] for index in prefix_refs)
                tail = b''.join(chunks[index] for index in tail_refs)
                if len(prefix) != count * 36 or len(tail) != count * (stride-36):
                    raise ValueError('factored vertex length mismatch')
                for vertex in range(count):
                    append(prefix[vertex * 36:(vertex+1) * 36])
                    append(tail[vertex * (stride-36):(vertex+1) * (stride-36)])
        if len(out) != expected:
            raise ValueError('factored record is truncated')
        return out

    with archive:
        for entry in archive.infolist():
            if declared.get(entry.filename, {}).get('role') != 'props':
                continue
            original = archive.read(entry)
            if hashlib.sha256(original).hexdigest() != declared[entry.filename]['sha256']:
                raise ValueError('original prop digest mismatch')
            parts, cursor = [], 0
            for start, end, stride in prop_regions(original):
                parts.append(['raw', references(original[cursor:start])])
                vertices = original[start:end]
                prefix = b''.join(vertices[offset:offset+36] for offset in range(0, len(vertices), stride))
                tail = b''.join(vertices[offset+36:offset+stride] for offset in range(0, len(vertices), stride))
                parts.append(['vertices', stride, len(vertices)//stride, references(prefix), references(tail)])
                cursor = end
            parts.append(['raw', references(original[cursor:])])
            restored = restore(parts, len(original))
            if restored != original:
                raise ValueError('factored props changed original bytes')
            recipes[entry.filename] = dict(bytes=len(original), sha256=declared[entry.filename]['sha256'], parts=parts)
        locations, segments, segment = [], [], bytearray()
        for chunk in chunks:
            if len(segment) + len(chunk) > 384 * 1024 * 1024:
                segments.append(segment)
                segment = bytearray()
            locations.append([len(segments), len(segment), len(chunk)])
            segment.extend(chunk)
        segments.append(segment)
        recipe = json.dumps(dict(investigation_format='exact-prop-factoring-1',
                                 chunk_locations=locations, records=recipes), separators=(',', ':')).encode()
        unchanged = [entry for entry in archive.infolist()
                     if declared.get(entry.filename, {}).get('role') != 'props']
        total = sum(entry.file_size for entry in unchanged) + sum(map(len, segments)) + len(recipe)
        if total > MAX_TOTAL or len(unchanged) + len(segments) + 1 > 512:
            raise ValueError('factoring prototype exceeds existing archive bounds')
        with PackageZip(destination, 'x', compression=zipfile.ZIP_STORED) as output:
            for entry in unchanged:
                output.writestr(entry.filename, archive.read(entry))
            output.writestr('prototype-recipes.json', recipe)
            for index, segment in enumerate(segments):
                output.writestr(f'prototype-{index}.props', segment)
        return dict(runtime_loadable=False, original_prop_bytes=sum(row['bytes'] for row in recipes.values()),
                    unique_chunk_bytes=sum(map(len, chunks)), unique_chunks=len(chunks),
                    recipe_bytes=len(recipe), archive_uncompressed_bytes=total,
                    reconstructed_all_prop_bytes_exact=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packages', type=Path, nargs='*')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--verify', action='store_true', help='stream and hash every declared entry')
    parser.add_argument('--shuffle', type=int, metavar='BLOCK_BYTES', help='reversible atlas experiment')
    parser.add_argument('--emit-shuffle', type=Path,
                        help='one-package unplayable STORE ZIP for native repack measurement')
    parser.add_argument('--emit-kind', choices=('atlas', 'props', 'both'), default='atlas')
    parser.add_argument('--emit-factor', type=Path,
                        help='unplayable exact cross-quality prop factoring ZIP, one input')
    parser.add_argument('--out', type=Path, help='new ignored JSON receipt')
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.packages or (args.shuffle is not None and (args.shuffle <= 0 or args.shuffle % 8)):
        parser.error('provide packages and a positive whole-texel shuffle block size')
    factor = None
    if args.emit_factor:
        if len(args.packages) != 1 or args.emit_shuffle or args.shuffle or args.emit_factor.suffix != '.zip':
            parser.error('--emit-factor requires one package and a .zip destination')
        factor = emit_factor(args.packages[0], args.emit_factor)
    if args.emit_shuffle:
        if len(args.packages) != 1 or args.shuffle is None or args.emit_shuffle.suffix != '.zip':
            parser.error('--emit-shuffle requires one package, --shuffle, and a .zip destination')
        emit_shuffle(args.packages[0], args.emit_shuffle, args.shuffle, args.emit_kind)
    results = [atlas_experiment(path, args.shuffle) if args.shuffle and args.emit_kind != 'props'
               else inspect(path, args.verify)
               for path in args.packages]
    if factor is not None:
        results[0]['factor_prototype'] = factor
    output = json.dumps(results, indent=2) + '\n'
    if args.out:
        with args.out.open('x') as stream:
            stream.write(output)
    else:
        print(output, end='')


if __name__ == '__main__':
    main()
