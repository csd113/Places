#!/usr/bin/env python3
"""Open every preserved map in native SDL/Metal and retain a capture/trajectory."""
import csv
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent

def main():
    captures = HERE / 'captures'
    captures.mkdir(exist_ok=True)
    results = []
    for item in json.loads((HERE / 'manifest.json').read_text()):
        image = captures / (item['id'] + '.png')
        command = [sys.executable, str(HERE / 'launch.py'), item['id'], '--capture', str(image)]
        try:
            with image.with_suffix('.log').open('w') as log:
                done = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=120, check=False)
            trajectory = image.with_suffix('.csv')
            rows = list(csv.reader(trajectory.open())) if trajectory.is_file() else []
            result = dict(id=item['id'], exit_code=done.returncode,
                          screenshot=image.is_file(), trajectory_samples=len(rows),
                          final_state=rows[-1] if rows else None)
        except subprocess.TimeoutExpired:
            result = dict(id=item['id'], exit_code='timeout', screenshot=False, trajectory_samples=0)
        results.append(result)
        print(item['id'], result, flush=True)
    (HERE / 'native-validation.json').write_text(json.dumps(results, indent=2) + '\n')
    return int(any(item['exit_code'] != 0 or not item['screenshot'] or item['trajectory_samples'] == 0 for item in results))

if __name__ == '__main__':
    raise SystemExit(main())
