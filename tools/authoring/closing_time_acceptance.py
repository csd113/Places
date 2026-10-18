#!/usr/bin/env python3
"""Explicit native Closing Time evidence; never run by Cargo tests or CI.

Direct import and player launch, real framebuffer/audio/trace evidence. Diagnostic
pose/item scenarios are kept separate from a controls-only natural playthrough.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import math

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT/'local/verification/0.12.0-closing-time'


def write(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')


def action(ms, kind, **fields):
    return dict(after='ready:closing_time', delay_ms=ms,
                action=dict(kind=kind, **fields))


def smoke():
    return [action(0,'focus',focused=True),action(300,'capture',name='entry'),
            action(800,'player',position=[4.8,1.6,11],yaw=0,pitch=-8),
            action(1000,'capture',name='arcade'),
            action(1600,'player',position=[14,1.6,19.2],yaw=0,pitch=-8),
            action(1800,'capture',name='bowling'),
            action(2400,'player',position=[5.4,1.6,16.8],yaw=315,pitch=-12),
            action(2600,'capture',name='party'),action(3100,'quit')]


class Walk:
    """Plan ordinary held W/E input; View changes orientation only."""
    def __init__(self):
        self.steps=[];self.ms=400;self.pos=[4.8,20.5];self.walking=False;self.carrying=False

    def add(self,kind,**fields):self.steps.append(action(round(self.ms),kind,**fields))

    def view(self,yaw,pitch=0):self.add('view',yaw=yaw,pitch=pitch)

    def move(self,x,z,stop=True):
        dx,dz=x-self.pos[0],z-self.pos[1]
        if self.carrying and self.walking:self.halt()
        self.view(math.degrees(math.atan2(dx,-dz)),-5)
        if self.carrying:self.ms+=800
        if not self.walking:self.add('key',key='W',pressed=True);self.walking=True
        self.ms+=math.hypot(dx,dz)/3*1000
        self.pos=[x,z]
        if stop:self.halt()

    def halt(self):
        self.add('key',key='W',pressed=False);self.walking=False;self.ms+=90

    def aim(self,x,y,z):
        self.add('view_at',position=[x,y,z])
        self.ms+=90

    def item(self,id):self.add('aim_item',id=id);self.ms+=100

    def press(self,key='E'):
        self.add('key',key=key,pressed=True);self.ms+=40
        self.add('key',key=key,pressed=False);self.ms+=180

    def capture(self,name):
        # Every input already retains an actual action_result world snapshot.
        # Avoid PNG stalls changing an uninterrupted movement trial's timing.
        if name=='natural-win':self.add('capture',name=name)
        self.ms+=70

    def finish(self):
        for at in range(0,round(self.ms)+1,5000):self.steps.append(action(at,'focus',focused=True))
        self.add('quit')
        assert self.ms<60000 and len(self.steps)<=128,(self.ms,len(self.steps))
        return sorted(self.steps,key=lambda s:s['delay_ms'])


def natural():
    p=Walk();p.capture('shift-start')
    p.move(5.8,13.4);p.item('loose_gift');p.press();p.carrying=True;p.capture('gift-held')
    p.move(6.8,14.8);p.aim(7.7,1.25,15.7);p.ms+=1400;p.press();p.carrying=False;p.ms+=600;p.capture('gift-returned')
    p.move(6.8,14.3,False);p.move(8.8,14.3,False);p.move(8.8,16.7)
    p.aim(9.7835,1.36,16.7);p.press();p.capture('party-closed')
    p.move(8.8,14.3,False);p.move(7.6,9.3,False);p.move(7.6,6.4)
    p.aim(8.64,1.26,5);p.press();p.capture('arcade-closed')
    p.press('F')
    p.move(6.5,6.7);p.item('loose_plush');p.press();p.carrying=True;p.capture('plush-held')
    p.move(5.5,11,False);p.move(5.5,18.7,False);p.move(1.6,19.8)
    p.aim(1,1.1,19.8);p.ms+=1400;p.press();p.carrying=False;p.ms+=600;p.capture('plush-returned')
    p.move(5.5,18.7,False);p.move(5.5,11,False);p.move(8.8,9,False);p.move(10.8,9,False);p.move(12.5,13.6)
    p.item('loose_ball');p.press();p.carrying=True;p.capture('ball-held')
    p.move(12.1,19.65);p.aim(11.4,1.05,19.65);p.ms+=1400;p.press();p.carrying=False;p.ms+=600;p.capture('ball-returned')
    p.move(12.6,20.4,False);p.move(11.2,20.4);p.aim(10.0165,1.36,20.8);p.press();p.capture('bowling-closed')
    p.move(12.6,20.4,False);p.move(12.6,9,False);p.move(8.8,9,False);p.move(5.5,11,False);p.move(5.5,18.7,False);p.move(4.9,20.5)
    p.aim(4.9,1.1,21.9);p.ms+=120;p.press();p.ms+=300;p.capture('natural-win');p.ms+=2000
    return p.finish()


def edge_cases():
    steps=[]
    def add(ms,kind,**fields):steps.append(action(ms,kind,**fields))
    def pose(ms,id,position):add(ms,'item_pose',id=id,position=position,rotation=[0,0,0,1])
    add(0,'focus',focused=True);add(200,'capture',name='authored')
    add(400,'activate',id='front_door');add(600,'capture',name='early-lock-refused')
    add(800,'activate',id='bowling_switch');add(1000,'activate',id='bowling_switch');add(1300,'capture',name='repeat-bowling')
    pose(1600,'loose_gift',[8.1,.75,15.8]);pose(1800,'loose_plush',[1,.5,19.8]);pose(2000,'loose_ball',[11.4,.5551,19.65])
    add(2700,'capture',name='returns-first');add(2800,'activate',id='party_switch');add(3000,'activate',id='arcade_switch');add(3500,'capture',name='all-ready')
    pose(3800,'loose_gift',[5.8,0,12.5]);add(4200,'activate',id='front_door');add(4500,'capture',name='removed-item-refuses-win')
    add(4800,'player',position=[2.1,1.6,19.8],yaw=270,pitch=-30);add(4900,'aim_item',id='loose_plush')
    add(5100,'key',key='E',pressed=True);add(5150,'key',key='E',pressed=False);add(5500,'capture',name='held-return-invalid')
    add(5700,'activate',id='exit_sign');add(6400,'capture',name='reset-from-held')
    pose(6800,'loose_ball',[11.4,.5551,19.65]);pose(7000,'loose_gift',[8.1,.75,15.8]);pose(7200,'loose_plush',[1,.5,19.8])
    add(7400,'activate',id='arcade_switch');add(7600,'activate',id='bowling_switch');add(7800,'activate',id='party_switch')
    add(8400,'view',yaw=180,pitch=-18);add(8600,'key',key='E',pressed=True);add(8650,'key',key='E',pressed=False);add(9000,'capture',name='win')
    add(9400,'key',key='E',pressed=True);add(9450,'key',key='E',pressed=False);add(10100,'capture',name='restart-after-win');add(10500,'quit')
    for ms in range(0,11000,2000):add(ms,'focus',focused=True)
    return sorted(steps,key=lambda s:s['delay_ms'])


def collisions():
    steps=[]
    def add(ms,kind,**fields):steps.append(action(ms,kind,**fields))
    for index,(name,position,yaw,jump,crouch) in enumerate([
        ('north',[5,1.6,1],0,True,False),
        ('entrance',[4.9,1.6,20.5],180,True,True),
        ('south',[8,1.6,20.5],180,True,False),
        ('west',[1,1.6,12],270,True,False),
        ('east',[17,1.6,12],90,True,False),
        ('southwest',[1,1.6,21],225,True,True),
        ('cabinet',[3.9,1.6,4],0,False,False),
        ('party-table',[3.2,1.6,16.5],0,False,False)]):
        ms=index*2200
        add(ms,'focus',focused=True);add(ms+50,'player',position=position,yaw=yaw,pitch=0)
        if crouch:add(ms+100,'key',key='C',pressed=True);add(ms+140,'key',key='C',pressed=False)
        add(ms+180,'key',key='W',pressed=True)
        if jump:add(ms+300,'key',key='Space',pressed=True);add(ms+350,'key',key='Space',pressed=False)
        add(ms+1600,'key',key='W',pressed=False);add(ms+1800,'capture',name=name)
    add(17800,'item_pose',id='loose_ball',position=[16.5,0,14.5],rotation=[0,0,0,1])
    add(17900,'item_velocity',id='loose_ball',linear=[20,0,0],angular=[0,0,0])
    add(18100,'capture',name='ball-wall-contact');add(18800,'capture',name='ball-wall-settled')
    add(19200,'quit')
    return sorted(steps,key=lambda s:s['delay_ms'])


def run(name, package, binary, steps, lighting='full', performance=False):
    out=BASE/name
    out.mkdir()
    write(out/'settings.json',dict(window_width=960,window_height=540,
        window_mode='windowed',quality='high',lightmaps=lighting,reflections='off',
        bloom=True,vsync=False,developer_console=True))
    write(out/'actions.json',steps)
    env={k:v for k,v in os.environ.items() if not k.startswith('PLACES_') and k!='SDL_VIDEODRIVER'}
    env.update(PLACES_ASSET_ROOT=str(ROOT),PLACES_STATE_ROOT=str(out),
               PLACES_LEVEL='closing_time',PLACES_BENCH='1',PLACES_VERBOSE='1',
               PLACES_VSYNC='off',PLACES_BENCH_FRAMES='1200' if performance else '0',
               PLACES_BENCH_ACTIONS=str(out/'actions.json'),
               PLACES_LOAD_TRACE=str(out/'trace.jsonl'))
    if performance:
        env.update(PLACES_BENCH_WARMUP='120',PLACES_BENCH_OUT=str(out/'frames.csv'))
    else:
        env['PLACES_AUDIO_CAPTURE']='1'
    write(out/'environment.json',{k:v for k,v in env.items() if k.startswith('PLACES_')})
    with (out/'import.log').open('w') as log:
        imported=subprocess.run([str(binary),'--import',str(package)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
    if imported.returncode:raise RuntimeError((out/'import.log').read_text())
    with (out/'native.log').open('w') as log:
        child=subprocess.run([str(binary)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=150)
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
    parser=argparse.ArgumentParser()
    parser.add_argument('name');parser.add_argument('--package',type=Path,required=True)
    parser.add_argument('--binary',type=Path,default=BASE/'payload-final/places')
    parser.add_argument('--lighting',default='full')
    parser.add_argument('--actions',type=Path)
    parser.add_argument('--performance',action='store_true')
    parser.add_argument('--case',choices=['smoke','edges','natural','collisions'],default='smoke')
    args=parser.parse_args()
    steps=json.loads(args.actions.read_text()) if args.actions else {'smoke':smoke,'edges':edge_cases,'natural':natural,'collisions':collisions}[args.case]()
    if args.performance:
        steps=[action(0,'focus',focused=True)]+[action(100,'item_velocity',id=id,linear=[0,0,0],angular=[0,0,0]) for id in ['loose_gift','loose_plush','loose_ball']]
    run(args.name,args.package.resolve(),args.binary.resolve(),steps,args.lighting,args.performance)
