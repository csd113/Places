#!/usr/bin/env python3
"""Record actual native editor display pixels alongside bounded input recipes."""
from pathlib import Path
import json,os,signal,subprocess,sys,shutil
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'local/verification/0.12.0-beam'
PAYLOAD=Path(os.environ.get('PLACES_EDITOR_PAYLOAD',str(ROOT/'local/verification/0.12.0-closing-time/payload-final')))
def run(name):
 recipe=BASE/'editor-scripts'/f'{name}.json'
 out=BASE/'video'/f'{name}-raw.mov'
 if out.exists():
  import time
  out=BASE/'video'/f'{name}-raw-{time.time_ns()}.mov'
 env={**os.environ,'PLACES_ASSET_ROOT':str(ROOT/'assets'),'PLACES_STATE_ROOT':str(BASE/'editor-state')}
 (BASE/'recorded-inputs').mkdir(exist_ok=True)
 shutil.copy2(recipe,BASE/'recorded-inputs'/recipe.name)
 with (BASE/f'{name}.log').open('w') as log:
  record=subprocess.Popen(['/usr/sbin/screencapture','-v','-D','1',str(out)],stdout=log,stderr=log)
  try:
   result=subprocess.run([str(PAYLOAD/'places-editor'),str(Path(os.environ.get('PLACES_EDITOR_PROJECT',str(ROOT/'projects/beam')))),'--compiler',str(PAYLOAD/'places-compile'),'--game',str(PAYLOAD/'places'),'--verify-script',str(recipe)],env=env,stdout=log,stderr=subprocess.STDOUT,timeout=900)
  finally:record.send_signal(signal.SIGINT);record.wait(timeout=20)
 receipt={'phase':name,'exit_code':result.returncode,'video':str(out),'input':str(recipe),'method':'Actual visible native controls through ordinary input handlers; genuine screen capture.'}
 (BASE/f'{name}-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
 print(json.dumps(receipt),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
if __name__=='__main__':
 for phase in sys.argv[1:]:run(phase)
