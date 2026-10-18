#!/usr/bin/env python3
"""Opt-in two-floor Beam native acceptance. Direct executable, real input.
Natural routes use held W/E/F and camera-only aiming; diagnostics are labeled.
Never run by Cargo tests or CI, never bake a shipped map implicitly.
"""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,math,signal
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'local/verification/0.12.0-beam'
def write(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
def action(ms,kind,**fields):return dict(after='ready:beam',delay_ms=round(ms),action=dict(kind=kind,**fields))

class Walk:
 def __init__(self):self.steps=[];self.ms=400;self.pos=[7,13.1];self.walking=False;self.carrying=False
 def add(self,kind,**fields):self.steps.append(action(self.ms,kind,**fields))
 def view(self,yaw,pitch=0):self.add('view',yaw=yaw,pitch=pitch)
 def move(self,x,z,stop=True):
  dx,dz=x-self.pos[0],z-self.pos[1]
  if self.carrying and self.walking:self.halt()
  self.view(math.degrees(math.atan2(dx,-dz)),-5)
  if self.carrying:self.ms+=1000
  if not self.walking:self.add('key',key='W',pressed=True);self.walking=True
  self.ms+=math.hypot(dx,dz)/3*1000;self.pos=[x,z]
  if stop:self.halt()
 def halt(self):self.add('key',key='W',pressed=False);self.walking=False;self.ms+=100
 def aim(self,x,y,z):self.add('view_at',position=[x,y,z]);self.ms+=100
 def item(self,id):self.add('aim_item',id=id);self.ms+=100
 def press(self,key='E'):
  self.add('key',key=key,pressed=True);self.ms+=45;self.add('key',key=key,pressed=False);self.ms+=160
 def capture(self,name):self.add('capture',name=name);self.ms+=160
 def enter(self):self.press('Return')
 def deposit(self,name,x=7):
  self.move(x,13.7)
  self.aim(x,1.25,14.4);self.ms+=1000;self.press();self.carrying=False;self.ms+=650
 def finish(self):
  for at in range(0,round(self.ms)+1,7000):self.steps.append(action(at,'focus',focused=True))
  self.add('quit');assert self.ms<60000 and len(self.steps)<=128,(self.ms,len(self.steps))
  return sorted(self.steps,key=lambda s:s['delay_ms'])

def natural():
 p=Walk();p.capture('opening');p.enter();p.press('F');p.capture('entry')
 p.move(5,12.8,False);p.move(5,3.1);p.move(4.4,3.1);p.item('gift_red');p.press();p.carrying=True
 p.move(5,3.1);p.move(5,12.8);p.move(6.8,13.1);p.deposit('red',6.2)
 p.press('F1');p.ms+=200;p.capture('hint-one-returned');p.enter()
 p.move(10.5,12.2);p.item('gift_stars');p.press();p.carrying=True
 p.move(7.6,13.1);p.deposit('stars',7.8)
 p.move(8.5,13.1,False);p.move(12.8,13.1,False);p.move(12.8,5.6)
 p.aim(12.8,4.1,4.9);p.press();p.ms+=850;p.move(12.8,4.2)
 p.move(12.8,2.2);p.move(11,2.2);p.item('gift_green');p.press();p.carrying=True
 p.move(12.8,2.2);p.move(12.8,4.2);p.move(12.8,13.4);p.ms+=900;p.move(8.2,13.4);p.move(7.4,13.1);p.deposit('green',7)
 p.move(8.7,13.2,False);p.move(8.7,15.4,False);p.move(7,15.4)
 p.aim(7,1.1,15.9);p.press();p.ms+=300;p.capture('win');p.ms+=600
 return p.finish()

def death():
 p=Walk();p.enter();p.press('F');p.move(5,12.7,False);p.move(5,7);p.move(3.8,7)
 p.add('aim_entity',id='visitor');p.ms+=300;p.capture('reveal');p.ms+=700;p.press('F');p.ms+=6500;p.capture('death')
 p.enter();p.ms+=350;p.capture('respawn-opening');p.enter();p.ms+=300;p.capture('respawn-world');p.press('F');p.add('aim_entity',id='visitor');p.ms+=200;p.capture('respawn-beam');p.press('F')
 p.pos=[7,13.1];p.move(5,12.7,False);p.move(5,7);p.move(3.8,7);p.ms+=7500;p.capture('death-again')
 p.enter();p.ms+=250;p.enter();p.ms+=250;p.capture('second-respawn');p.press('F');p.add('aim_entity',id='visitor');p.ms+=200;p.capture('second-respawn-beam');p.ms+=200
 return p.finish()

def patrol():
 p=Walk();p.enter()
 for at,name in [(1000,'authored'),(11000,'lower-route'),(24000,'around-furniture'),(39000,'upper-route'),(51000,'upper-search')]:
  p.ms=at;p.add('aim_entity',id='visitor');p.capture(name)
 p.ms=53500
 return p.finish()

def beam():
 p=Walk();p.enter();p.press('F');p.add('aim_entity',id='visitor');p.ms+=250;p.capture('far-beam')
 p.move(5,12.7,False);p.move(5,7);p.add('aim_entity',id='visitor');p.ms+=250;p.capture('near-beam')
 p.add('aim_entity',id='visitor',yaw_offset_degrees=17);p.ms+=200;p.capture('beam-edge')
 p.add('aim_entity',id='visitor',yaw_offset_degrees=35);p.ms+=80;p.capture('outside-beam');p.ms+=600
 p.add('aim_entity',id='visitor');p.ms+=5000;p.press('F');p.ms+=80;p.capture('power-off');p.ms+=1500;p.capture('dark-movement')
 p.press('F');p.add('aim_entity',id='visitor');p.ms+=200;p.capture('held-again')
 # Normal S+D diagonal and A strafe, with camera-only aim updates. The
 # creature stays lit until the real partition blocks the view.
 for key in ['S','D']:p.add('key',key=key,pressed=True)
 for _ in range(7):p.ms+=130;p.add('aim_entity',id='visitor')
 for key in ['S','D']:p.add('key',key=key,pressed=False)
 p.ms+=120;p.add('key',key='A',pressed=True)
 for _ in range(7):p.ms+=130;p.add('aim_entity',id='visitor')
 p.add('key',key='A',pressed=False);p.ms+=100;p.capture('partition-occlusion');p.ms+=200
 return p.finish()

def edges():
 p=Walk();p.enter();p.press('F');p.move(10.5,12.2);p.item('gift_stars');p.press();p.carrying=True
 p.move(7.8,13.1);p.deposit('stars',7.8);p.capture('stars-first')
 p.item('gift_stars');p.press();p.carrying=True;p.ms+=150;p.capture('held-return-invalid')
 p.aim(7.8,1.25,14.4);p.ms+=800;p.press();p.carrying=False;p.ms+=500;p.capture('return-reentry')
 p.item('gift_stars');p.press();p.carrying=True;p.move(9.7,13.1);p.aim(10.7,.2,12.8);p.ms+=900;p.press();p.carrying=False;p.ms+=800;p.capture('dropped-outstanding')
 p.press('F1');p.ms+=150;p.capture('hint-outstanding');p.enter()
 p.move(9.7,15.4,False);p.move(7,15.4);p.aim(7,1.1,15.9);p.press();p.ms+=150;p.capture('early-exit-refused');p.enter()
 p.aim(7,2.4,15.68);p.press();p.ms+=250;p.capture('normal-sign-restart');p.enter();p.ms+=200;p.capture('reset-items')
 return p.finish()

def held_death():
 p=Walk();p.enter();p.move(5,12.7,False);p.move(5,3.1);p.move(4.4,3.1);p.item('gift_red');p.press();p.carrying=True;p.capture('held-red')
 p.ms+=20000;p.capture('death-holding');p.enter();p.ms+=300;p.enter();p.ms+=250;p.capture('held-reset')
 return p.finish()

def bounds():
 p=Walk();p.enter();p.view(270);p.add('key',key='W',pressed=True);p.ms+=5000;p.halt();p.press('Space');p.capture('west')
 p.view(0);p.add('key',key='W',pressed=True);p.ms+=5000;p.halt();p.press('C');p.press('Space');p.capture('north');p.press('C')
 return p.finish()

def east_south():
 p=Walk();p.enter();p.view(90);p.add('key',key='W',pressed=True);p.ms+=5000;p.halt();p.press('Space');p.capture('east')
 p.view(180);p.add('key',key='W',pressed=True);p.ms+=3000;p.halt();p.press('C');p.press('Space');p.capture('south');p.press('C')
 return p.finish()

def upper_bounds():
 p=Walk();p.enter();p.press('F');p.move(12.8,13.1);p.move(12.8,5.6);p.aim(12.8,4.1,4.9);p.press();p.ms+=850;p.move(12.8,4.2);p.move(12.8,2.2)
 for yaw,name in [(0,'upper-north'),(270,'upper-west'),(90,'upper-east')]:
  p.view(yaw);p.add('key',key='W',pressed=True);p.ms+=3200;p.halt();p.press('Space');p.capture(name)
 return p.finish()

def menu_reentry():
 p=Walk();p.enter();p.move(5,12.7,False);p.move(5,7);p.move(3.8,7);p.ms+=10000;p.capture('menu-death')
 p.press('Escape');p.ms+=200;p.capture('outcome-menu');p.enter();p.ms+=200;p.capture('menu-play');p.enter();p.ms+=2500;p.capture('reentry-opening');p.enter();p.ms+=200;p.capture('reentry-world')
 return p.finish()

def smoke():
 p=Walk();p.capture('opening');p.enter();p.press('F');p.capture('entry');p.add('aim_entity',id='visitor');p.ms+=500;p.capture('beam');p.ms+=500
 return p.finish()

def run(name, package, binary, steps, lighting='full', performance=False, record=False):
    out=BASE/name
    out.mkdir()
    write(out/'settings.json',dict(window_width=960,window_height=540,
        window_mode='windowed',quality='high',lightmaps=lighting,reflections='off',
        bloom=True,vsync=False,developer_console=True))
    write(out/'actions.json',steps)
    env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_') and k!='SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(ROOT),PLACES_STATE_ROOT=str(out),
               PLACES_LEVEL='beam',PLACES_BENCH='1',PLACES_VERBOSE='1',
               PLACES_VSYNC='off',PLACES_BENCH_FRAMES='6000' if performance else '0',
               PLACES_BENCH_ACTIONS=str(out/'actions.json'),
               PLACES_LOAD_TRACE=str(out/'trace.jsonl'))
    if performance:
        env.update(PLACES_BENCH_WARMUP='1200',PLACES_BENCH_OUT=str(out/'frames.csv'))
    else:
        env['PLACES_AUDIO_CAPTURE']='1'
    write(out/'environment.json',{k:v for k,v in env.items() if k.startswith('PLACES_')})
    with (out/'import.log').open('w') as log:
        imported=subprocess.run([str(binary),'--import',str(package)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
    if imported.returncode:raise RuntimeError((out/'import.log').read_text())
    with (out/'native.log').open('w') as log:
        recorder=subprocess.Popen(['/usr/sbin/screencapture','-v','-D','1',str(out/'gameplay-raw.mov')],stdout=log,stderr=log) if record else None
        try:
            child=subprocess.run([str(binary)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=150)
        finally:
            if recorder:
                recorder.send_signal(signal.SIGINT);recorder.wait(timeout=20)
    events=[json.loads(line) for line in (out/'trace.jsonl').read_text().splitlines()]
    samples={e['event'].split(':',1)[1]:json.loads(e['detail']) for e in events if e['event'].startswith('native_capture:')}
    write(out/'snapshots.json',samples)
    receipt=dict(exit_code=child.returncode,binary=str(binary),package=str(package),
        binary_sha256=hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),
        package_sha256=hashlib.file_digest(package.open('rb'),'sha256').hexdigest(),
        launch='direct executable, isolated imported package',captures=list(samples),
        shutdown=bool(events) and events[-1]['event']=='shutdown_complete')
    write(out/'receipt.json',receipt)
    print(json.dumps(receipt),flush=True)
    return out,samples

if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('name');parser.add_argument('--package',type=Path,required=True)
 parser.add_argument('--binary',type=Path,default=BASE/'payload/places');parser.add_argument('--actions',type=Path)
 parser.add_argument('--case',choices=['smoke','natural','death','patrol','beam','edges','held_death','bounds','east_south','upper_bounds','menu_reentry'],default='smoke');parser.add_argument('--lighting',default='full');parser.add_argument('--performance',action='store_true');parser.add_argument('--record',action='store_true')
 args=parser.parse_args();steps=json.loads(args.actions.read_text()) if args.actions else {'smoke':smoke,'natural':natural,'death':death,'patrol':patrol,'beam':beam,'edges':edges,'held_death':held_death,'bounds':bounds,'east_south':east_south,'upper_bounds':upper_bounds,'menu_reentry':menu_reentry}[args.case]()
 if args.performance:steps=[action(0,'focus',focused=True),action(100,'key',key='Return',pressed=True),action(150,'key',key='Return',pressed=False),action(200,'key',key='F',pressed=True),action(245,'key',key='F',pressed=False),action(300,'aim_entity',id='visitor')]
 run(args.name,args.package.resolve(),args.binary.resolve(),steps,args.lighting,args.performance,args.record)
