#!/usr/bin/env python3
"""Sequential isolated lossless campaign; never bake or replace production maps.

Build package-repack first. Originals must be locally trusted. Native validation,
exact baseline repack, on-disk factoring reconstruction, deterministic native
codec outputs and original-file preservation are checked. Research ZIPs remain
unplayable; timing is offline, not a browser/runtime result.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import platform
import re
import os
import signal
import subprocess
import time

from map_delivery import digest

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packages', type=Path, nargs='+')
    parser.add_argument('--out', type=Path, required=True, help='new ignored campaign directory')
    parser.add_argument('--repack', type=Path, default=ROOT / 'target/release/examples/package-repack')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    repack = str(args.repack.resolve())
    compiler = str(ROOT / 'target/release/places-compile')
    report = dict(platform=platform.platform(), repacker_sha256=digest(Path(repack)),
                  compiler_sha256=digest(Path(compiler)), records=[],
                  note='Warm OS caches, sequential release offline work. RSS is whole process.')

    def run(label, command):
        stdout, stderr = out / f'{label}.stdout', out / f'{label}.log'
        timed = ['/usr/bin/time', '-l', *command] if platform.system() == 'Darwin' else command
        started = time.monotonic()
        with stdout.open('x') as output, stderr.open('x') as log:
            with subprocess.Popen(timed, cwd=ROOT, stdout=output, stderr=log,
                                  start_new_session=os.name == 'posix') as process:
                try:
                    code = process.wait(timeout=600)
                except subprocess.TimeoutExpired:
                    if os.name == 'posix':
                        os.killpg(process.pid, signal.SIGTERM)
                    else:
                        process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        if os.name == 'posix':
                            os.killpg(process.pid, signal.SIGKILL)
                        else:
                            process.kill()
                        process.wait()
                    raise
                if code:
                    raise subprocess.CalledProcessError(code, timed)
        text = stderr.read_text()
        rss = re.search(r'(\d+)\s+maximum resident set size', text)
        return dict(wall_seconds=time.monotonic()-started,
                    peak_rss_bytes=int(rss[1]) if rss else None,
                    output=json.loads(stdout.read_text()) if stdout.stat().st_size else None)

    for number, package in enumerate(args.packages):
        source = package.resolve()
        label = f'{number}-{source.stem}'
        identity = digest(source)
        row = dict(path=str(source), sha256=identity, original_bytes=source.stat().st_size)
        print(f'{label}: native validate, repack and factoring', flush=True)
        row['native_validation'] = run(label+'-validate', [compiler, 'validate', str(source), '--json'])
        if not row['native_validation']['output']['dependencies_intact']:
            raise ValueError('original dependencies are not intact; not a current runtime candidate')
        for level in (6, 9):
            candidate = out / f'{label}-{level}.placesmap'
            row[f'deflate{level}'] = run(label+f'-pack{level}', [repack, str(source), str(candidate), str(level)])
            if level == 6 and digest(candidate) != identity:
                raise ValueError('baseline native level-6 repack differs')
            row[f'read{level}'] = [run(label+f'-read{level}-{repeat}', [repack, '--read-only', str(candidate)])
                                   for repeat in range(3)]
            candidate.unlink()  # Task-owned temporary: receipts preserve evidence.
        store = out / f'{label}-factor-store.zip'
        emit = out / f'{label}-emit.json'
        row['factor_emit'] = run(label+'-factor-emit', ['python3', 'tools/bench/map_storage.py', str(source),
                                                     '--emit-factor', str(store), '--out', str(emit)])
        row['factoring'] = json.loads(emit.read_text())[0]['factor_prototype']
        candidates = []
        for repeat in range(2):
            candidate = out / f'{label}-factor{repeat}.zip'
            row[f'factor_pack{repeat}'] = run(label+f'-factor-pack{repeat}', [repack, str(store), str(candidate), '6'])
            proof = out / f'{label}-proof{repeat}.json'
            row[f'factor_verify{repeat}'] = run(label+f'-factor-verify{repeat}',
                                              ['python3', 'tools/bench/map_delivery.py', str(source),
                                               '--verify-factor', str(candidate), '--out', str(proof)])
            row[f'factor_proof{repeat}'] = json.loads(proof.read_text())
            candidates.append(candidate)
        if digest(candidates[0]) != digest(candidates[1]):
            raise ValueError('factoring codec output is not deterministic')
        candidates[1].unlink()
        store.unlink()
        if digest(source) != identity:
            raise ValueError('original changed during campaign')
        report['records'].append(row)
        (out / 'campaign.json').write_text(json.dumps(report, indent=2)+'\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
