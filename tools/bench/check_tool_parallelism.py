#!/usr/bin/env python3
"""Compare preserved working-tree tooling with serial/parallel updated CLIs.

Requires a source+assets snapshot made BEFORE edits. Runs only isolated copies.
Usage: python3 tools/bench/check_tool_parallelism.py --baseline target/run09/baseline
Timings include process startup, serialization, merging and output writes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def digest_tree(root, pattern):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(root.glob(pattern)) if path.is_file()}


def measured_run(invocation, work, memory):
    start = time.perf_counter()
    peak_rss = peak_processes = 0
    if not memory:
        process = subprocess.run(invocation, cwd=work, capture_output=True, text=True)
        return process, time.perf_counter()-start, None, None
    with (work/'sampled-command.log').open('w+') as log, (work/'sampled-stderr.log').open('w+') as errors:
        process = subprocess.Popen(invocation, cwd=work, stdout=log, stderr=errors, text=True)
        try:
            while process.poll() is None:
                listing = subprocess.run(['ps', '-axo', 'pid=,ppid=,rss='], capture_output=True, text=True, check=True)
                rows = [tuple(map(int, row.split())) for row in listing.stdout.splitlines() if row.strip()]
                owned = {process.pid}
                for _ in range(8):
                    owned.update(pid for pid, parent, _ in rows if parent in owned)
                peak_rss = max(peak_rss, sum(rss for pid, _, rss in rows if pid in owned))
                peak_processes = max(peak_processes, sum(pid in owned for pid, _, _ in rows))
                time.sleep(.05)
            elapsed = time.perf_counter()-start
            log.seek(0)
            errors.seek(0)
            result = subprocess.CompletedProcess(invocation, process.returncode, log.read(), errors.read())
        except BaseException:
            process.send_signal(__import__('signal').SIGINT)
            process.wait(timeout=10)
            raise
    return result, elapsed, peak_rss, peak_processes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=ROOT/'target/tool-parallel-check')
    parser.add_argument('--memory', action='store_true', help='sample process-tree RSS; timing includes observer overhead')
    parser.add_argument('--only', nargs='+', help='workload names to run')
    parser.add_argument('--repeat', type=int, default=2)
    parser.add_argument('--workers', type=int, nargs='+', default=[1, 4, 8, 12])
    args = parser.parse_args()
    if args.repeat < 1 or any(workers < 1 for workers in args.workers):
        parser.error('repeat and worker counts must be positive')
    baseline, out = args.baseline.resolve(), args.out.resolve()
    if out.exists():
        parser.error('--out must be a new scratch directory')
    if not (baseline/'assets/catalog.json').is_file():
        parser.error('baseline must contain preserved tools and assets')
    out.mkdir(parents=True)
    results = []
    # Bounded real workloads, unchanged inputs and algorithmic coverage.
    jobs = [
        ('preview', ['tools/props/preview.py', '--all', '--out', 'output'], 'output/*.png'),
        ('textures', ['tools/textures/build.py'], 'assets/**/*.png'),
        ('props', ['tools/props/build.py', '--only', 'core:chair', 'core:table', 'home:fork', 'home:bowl'], 'assets/**/*.glb'),
        ('clips', ['tools/props/animate_spooner_man.py'], 'assets/entities/spooner-man/model/*.glb'),
        ('geometry', ['tools/props/repair_geometry.py', '--apply', '--report', 'output.json'], 'assets/**/*.glb'),
    ]
    jobs += [
        ('rat', ['tools/entities/build_rat.py', '--out', 'output/rat.glb'], 'output/*.glb'),
        ('contact', ['tools/entities/render_contact_sheets.py', '--workers', '1', '--blender-threads', '1', '--samples', '4', '--cell', '64', '--frames-per-clip', '1', '--out', 'output'], 'output/*.png'),
        ('entities', ['tools/entities/validate_entities.py', '--json'], 'entity-report.json'),
        ('zoo', ['tools/levels/build_model_zoo.py', '--no-cache', '--out', 'output.json'], 'output.json'),
        ('seams', ['tools/textures/seam_repair.py', '--repair', 'images/a.png', 'images/b.png', 'images/c.png', 'images/d.png'], 'images/*.png'),
        ('resize', ['tools/props/resize_embedded_textures.py', 'resize.glb', '--size', '256'], 'resize.glb'),
        ('holes', ['tools/bench/check_holes.py', '--max-fraction', '1', 'images'], None),
        ('compare', ['tools/bench/compare_captures.py', 'left', 'right'], None),
    ]
    if args.only:
        jobs = [job for job in jobs if job[0] in args.only]
        if {job[0] for job in jobs} != set(args.only):
            parser.error('unknown workload')
    for label, command, pattern in jobs:
        expected = None
        expected_exit = None
        for variant in ['original'] + args.workers:
            for repeat in range(args.repeat):
                work = out/f'{label}-{variant}-{repeat}'
                shutil.copytree(baseline, work)
                if variant != 'original':
                    # Preserve baseline painter/art inputs, including concurrent changes.
                    for source in ROOT.glob('tools/**/*.py'):
                        relative = source.relative_to(ROOT)
                        old = baseline/relative
                        if relative.as_posix() == 'tools/textures/office_art.py':
                            continue
                        shutil.copy2(source, work/relative)
                if label == 'geometry':
                    # Exercise a genuine repair, not only idempotent no-op audits.
                    import struct
                    path = next((work/'assets').glob('**/couch.glb'))
                    raw = bytearray(path.read_bytes())
                    length = struct.unpack_from('<I', raw, 12)[0]
                    document = json.loads(raw[20:20+length])
                    accessor = document['accessors'][document['meshes'][0]['primitives'][0]['indices']]
                    view = document['bufferViews'][accessor['bufferView']]
                    offset = 28+length+view.get('byteOffset',0)+accessor.get('byteOffset',0)
                    size = {5123:2,5125:4}[accessor['componentType']]
                    raw[offset+size:offset+2*size], raw[offset+2*size:offset+3*size] = raw[offset+2*size:offset+3*size], raw[offset+size:offset+2*size]
                    path.write_bytes(raw)
                if label in ('seams' , 'holes', 'compare'):
                    images = work/'images'
                    images.mkdir()
                    sources = sorted((baseline/'assets').glob('**/props/models/*.png'))[:4]
                    if len(sources) != 4:
                        raise RuntimeError('need four real source PNGs')
                    for key, source in zip('abcd', sources):
                        shutil.copy2(source, images/f'{key}.png')
                    if label == 'compare':
                        for side in ('left', 'right'):
                            shutil.copytree(images, work/side/'high')
                            shutil.copytree(images, work/side/'low')
                if label == 'resize':
                    sys.path.insert(0, str(ROOT/'tools/props'))
                    import resize_embedded_textures as codec
                    source = next((baseline/'assets').glob('**/couch.glb'))
                    document, binary = codec.read_glb(str(source))
                    image_view = document['images'][0]['bufferView']
                    rebuilt = bytearray()
                    for index, view in enumerate(document['bufferViews']):
                        old = view.get('byteOffset', 0)
                        payload = ((baseline/'icon.png').read_bytes() if index == image_view
                                   else binary[old:old+view['byteLength']])
                        rebuilt.extend(b'\0' * (-len(rebuilt) % 4))
                        view['byteOffset'] = len(rebuilt)
                        view['byteLength'] = len(payload)
                        rebuilt.extend(payload)
                    document['buffers'][0]['byteLength'] = len(rebuilt)
                    codec.write_glb(str(work/'resize.glb'), document, rebuilt)
                invocation = [sys.executable, *command]
                if variant != 'original':
                    invocation += ['--workers', str(variant)]
                process, elapsed, rss, processes = measured_run(invocation, work, args.memory)
                (work/'command.log').write_text(process.stdout+process.stderr)
                if process.returncode and not (label == 'seams' and process.returncode == 1):
                    raise RuntimeError(f'{label}/{variant} exited {process.returncode}; see {work}/command.log')
                if label == 'entities':
                    result = json.loads(process.stdout[process.stdout.index('\n{')+1:].replace(str(work), '<ROOT>'))
                    for field in ('requested_workers','workers_note','effective_workers','elapsed_s'):
                        result['report'].pop(field)
                    (work/'entity-report.json').write_text(json.dumps(result, sort_keys=True))
                hashes = (digest_tree(work, pattern) if pattern else
                          {'stdout': hashlib.sha256(process.stdout.replace(str(work), '<ROOT>').encode()).hexdigest()})
                if expected is None:
                    expected = hashes
                    expected_exit = process.returncode
                if hashes != expected or process.returncode != expected_exit:
                    mismatch = [key for key in set(hashes)|set(expected) if hashes.get(key)!=expected.get(key)]
                    raise RuntimeError(f'{label}/{variant} differs from original: {mismatch}')
                results.append(dict(tool=label, workers=variant, repeat=repeat, seconds=elapsed,
                                    files=len(hashes), equivalent=True, exit_code=process.returncode, sampled_peak_rss_kib=rss, peak_processes=processes))
                (out/'results.json').write_text(json.dumps(results, indent=2)+'\n')
                print(f'{label}: workers={variant} repeat={repeat} {elapsed:.3f}s exact {len(hashes)} files', flush=True)
                # Keep evidence/output but avoid duplicating assets for every repetition.
                shutil.rmtree(work/'assets')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
