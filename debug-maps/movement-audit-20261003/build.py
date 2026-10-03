#!/usr/bin/env python3
"""Rebuild and validate the durable movement fixtures with the current compiler."""
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]

def main():
    compiler = ROOT / 'target/release/places-compile'
    asset_root = HERE / 'asset-root/assets' if (HERE / 'asset-root/assets/catalog.json').is_file() else ROOT / 'assets'
    outcomes = []
    for item in json.loads((HERE / 'manifest.json').read_text()):
        source, package = HERE / item['source'], HERE / item['package']
        commands = [
            [str(compiler), 'build', str(source), '--out', str(package), '--variants', 'off', '--workers', '12', '--asset-root', str(asset_root)],
            [str(compiler), 'verify', str(source), '--package', str(package), '--require-current', '--asset-root', str(asset_root)],
            [str(compiler), 'validate', str(package)],
        ]
        results = []
        for command in commands:
            done = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
            results.append(dict(command=command, exit_code=done.returncode, stdout=done.stdout, stderr=done.stderr))
            if done.returncode:
                print(done.stderr or done.stdout, flush=True)
                break
        outcomes.append(dict(id=item['id'], results=results))
        print(item['id'], 'ok' if len(results) == 3 and all(result['exit_code'] == 0 for result in results) else 'FAILED', flush=True)
    (HERE / 'build-validation.json').write_text(json.dumps(outcomes, indent=2) + '\n')
    return int(any(len(item['results']) != 3 or any(result['exit_code'] for result in item['results']) for item in outcomes))

if __name__ == '__main__':
    raise SystemExit(main())
