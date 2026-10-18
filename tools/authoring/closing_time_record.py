#!/usr/bin/env python3
"""Record genuine desktop pixels while the native editor executes a UI phase.

No synthesized video frames or replacement UI. Source edits happen separately.
Raw .mov files are retained before honest trimming/cropping/condensation.
"""
from pathlib import Path
import json
import os
import signal
import subprocess
import sys
import shutil

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = ROOT/'local/verification/0.12.0-closing-time'
PAYLOAD = Path(os.environ.get('PLACES_EDITOR_PAYLOAD', str(EVIDENCE/'payload-final')))


def run(name):
    script=EVIDENCE/'editor-scripts'/f'{name}.json'
    retained=EVIDENCE/'recorded-inputs'
    retained.mkdir(exist_ok=True)
    shutil.copy2(script,retained/f'{name}.json')
    env={**os.environ,'PLACES_ASSET_ROOT':str(ROOT/'assets'),
         'PLACES_STATE_ROOT':str(EVIDENCE/'editor-state')}
    video=EVIDENCE/'video'/f'{name}-raw.mov'
    with (EVIDENCE/f'{name}.log').open('w') as log:
        recording=subprocess.Popen(['/usr/sbin/screencapture','-v','-D','1',str(video)],
                                   stdout=log,stderr=log)
        try:
            result=subprocess.run([str(PAYLOAD/'places-editor'),str(ROOT/'projects/closing-time'),
                '--compiler',str(PAYLOAD/'places-compile'),'--game',str(PAYLOAD/'places'),
                '--verify-script',str(script)],env=env,stdout=log,stderr=log,timeout=900)
        finally:
            recording.send_signal(signal.SIGINT)
            recording.wait(timeout=15)
    receipt={'phase':name,'native_editor_exit':result.returncode,'video':str(video),
             'driver':str(script),'method':'Scripted visible controls through ordinary native editor input handlers; genuine screen recording.'}
    (EVIDENCE/f'{name}-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt),flush=True)
    if result.returncode:
        print((EVIDENCE/f'{name}.log').read_text(),flush=True)
        raise SystemExit(result.returncode)


if __name__=='__main__':
    for phase in sys.argv[1:]:run(phase)
