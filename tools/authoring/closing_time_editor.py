#!/usr/bin/env python3
"""Reproducible, opt-in Closing Time authoring through visible native UI controls.

Writes editor --verify-script input, never map geometry. Run each phase once in
order against the editable project. Retain checkpoints/video to prove operations.
The bounded driver refuses controls that are not visible. Not part of CI.
"""
from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'local/verification/0.12.0-closing-time'
CAT = json.loads((ROOT / 'assets/catalog.json').read_text())['assets']
MODELS = sorted(e['id'] for e in CAT if e['asset_type'] in ('prop', 'entity'))
MATS = sorted(e['id'] for e in CAT if e['asset_type'] == 'material')


class Phase:
    def __init__(self, name):
        self.name = name
        self.steps = []
        self.add('wait_idle', delay=1000)
        self.add('resize', width=1440, height=816)
        self.click('Snap')

    def add(self, kind, delay=120, **fields):
        self.steps.append({'delay_ms': delay, 'action': {'kind': kind, **fields}})

    def click(self, action, delay=150):
        self.add('control', action=action, delay=delay)

    def field(self, index, value):
        self.click(f'Field({index})')
        self.add('key', key='A', command=True)
        self.add('text', text=str(value))
        self.add('key', key='Return')

    def select(self, kind, index):
        # The scene list is scrolled only by actual wheel events. Prefer a
        # visible selection for initial rows; new objects select themselves.
        self.click(f'Select({kind}({index}))')

    def new(self, kind):
        if kind in ('Room', 'Wall', 'Ramp', 'FloorRegion'):
            self.click('Geometry')
        elif kind != 'Prop':
            self.click('Create')
        self.click(f'Add({kind})')
        if kind not in ('Room', 'Wall', 'Prop', 'Ramp', 'FloorRegion'):
            self.click('ClosePopup')

    def setting(self, name, value):
        self.click(f'GameSetting({name})')
        self.add('key', key='A', command=True)
        self.add('text', text=str(value))
        self.add('key', key='Return')

    def asset(self, asset, materials=False):
        if not materials:
            self.click('Gameplay')
        self.click('Materials' if materials else 'Assets')
        index = (MATS if materials else MODELS).index(asset)
        for _ in range(index // 10):
            self.click('Page(true)', delay=80)
        self.click(f'ChooseAsset({index})', delay=350)

    def transform(self, fields):
        for key, value in fields.items():
            self.field(key, value)

    def prop(self, model, x, z, yaw=0, y=0, scale=1):
        self.new('Prop')
        self.asset(model)
        self.transform({0:x, 1:y, 2:z, 6:yaw})
        if scale != 1:
            self.field(3, scale)
        self.add('wait_idle')

    def finish(self):
        self.click('Save')
        self.add('wait_idle')
        self.add('click', x=477, y=40)
        self.click('Focus')
        self.add('capture', name=self.name, delay=700)
        self.add('checkpoint', name=self.name)
        self.add('close', delay=1000)
        assert len(self.steps) <= 256, (self.name, len(self.steps))
        path = OUT / 'editor-scripts' / f'{self.name}.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({'output': str(OUT/'native'/self.name),
                                    'actions': self.steps}, indent=2)+'\n')
        print(self.name, len(self.steps), path)


def generate():
    p = Phase('01-floors')
    p.select('Room', 0)
    p.transform({0:0, 2:0, 3:10, 4:3.2, 5:12})
    p.asset('arcade:carpet_retro_swirl_01', True)
    for x,z,w,d,mat in [(0,12,10,6,'arcade:carpet_neon_confetti_01'),
                         (0,18,10,4,'arcade:carpet_geometric_01'),
                         (10,0,8,22,'arcade:carpet_cosmic_bowl_01')]:
        p.new('Room');p.transform({0:x,2:z,3:w,4:3.2,5:d});p.asset(mat,True)
    p.finish()


def environment():
    p=Phase('12-lane-surfaces')
    for x in (11.775,15.375):
        p.new('FloorRegion');p.transform({0:x,1:0,2:1,3:1.05,5:14.5})
        p.asset('arcade:lane_wood_01',True)
    p.new('Wall');p.transform({0:4.2,1:2.2,2:21.8,3:1.4,4:1.05,5:.2})
    p.asset('arcade:wall_navy_01',True)
    p.finish()
    lights=[(3,5,.30,[.6,.68,1],True),(6.5,8,.26,[.8,.3,.7],True),
            (4.5,15,.4,[1,.8,.62],True),(12,10,.33,[.65,.75,1],True),
            (16,10,.33,[.65,.75,1],True),(4.8,20,.12,[1,.65,.35],False),
            (7,17.7,.08,[.45,.58,.8],False)]
    fixtures=sorted(e['id'] for e in CAT if e['asset_type']=='light')
    for n in range(0,len(lights),3):
        p=Phase(f'{13+n//3:02}-lighting')
        if n==0:
            p.add('scroll_scene',lines=-64);p.select('Light',0);p.click('Delete')
        for x,z,power,color,switch in lights[n:n+3]:
            p.new('Light');p.transform({0:x,1:3.25,2:z})
            p.click('Gameplay');p.click('GamePick(Fixture)')
            i=fixtures.index('arcade:fluorescent_panel_01')
            for _ in range(i//10):p.click('Page(true)')
            p.click(f'GameChoose(Fixture, {i})')
            for name,val in [('Brightness',power),('Range',7),('Red',color[0]),('Green',color[1]),('Blue',color[2]),('Emission',.65)]:p.setting(name,val)
            if switch:p.click('GameSetting(Switchable)')
            p.click('ClosePopup')
        p.finish()
    p=Phase('16-return-volumes')
    for x,y,z,w,h,d in [(10.9,.65,18.45,1.0,.7,1.5),(7.05,.68,15.0,1.3,.85,1.3),(.65,.45,19.45,.7,.85,.7)]:
        p.new('Trigger');p.transform({0:x,1:y,2:z,3:w,4:h,5:d})
    p.finish()
    p=Phase('17-door-and-sound')
    # A temporary viewport target in the lobby keeps the new closed leaf clear.
    p.add('world_click',position=[4.8,0,20.5]);p.click('Focus')
    p.new('Door');p.transform({0:4.2,2:21.9,3:1.4,4:2.2,5:.045})
    for x,z,gain in [(4,5,.32),(4,15,.2),(14,10,.3),(4.8,20,.12)]:
        p.new('Audio');p.transform({0:x,1:1.5,2:z});p.click('Gameplay')
        p.setting('Gain',gain);p.click('GameSetting(Spatial)');p.click('ClosePopup')
    p.finish()
    p=Phase('18-luau')
    p.click('Scripts');p.click('ScriptNew');p.add('key',key='A',command=True)
    source=(ROOT/'projects/closing-time/scripts/closing_time_1.luau').read_text()
    for offset in range(0,len(source),240):p.add('text',text=source[offset:offset+240],delay=500)
    p.click('Save');p.click('ScriptCheck');p.add('capture',name='luau-source',delay=1000)
    p.add('checkpoint',name='luau-check');p.click('ClosePopup');p.finish()
    p=Phase('19-physics-and-fit')
    scroll=0
    for index in [12,14,23,24,25,31,32]:
        ordinal=13+index
        p.add('scroll_scene',lines=scroll-ordinal);scroll=ordinal
        p.select('Prop',index)
        if index in [12,14]:p.field(1,.502 if index==12 else .524)
        elif index in [31,32]:p.field(1,.5551)
        else:
            p.click('Gameplay');p.click('GameSetting(Physics)');p.click('ClosePopup')
    p.finish()
    for phase, walls in [('02-shell',[(0,0,18,.2),(0,.2,.2,21.6),(17.8,.2,.2,21.6),(0,21.8,4.2,.2),(5.6,21.8,12.2,.2)]),
                         ('03-partitions',[(9.8,.2,.2,6.8),(9.8,11,.2,10.8),(10.0,4.0,7.8,.2)])]:
        p=Phase(phase)
        for x,z,w,d in walls:
            p.new('Wall');p.transform({0:x,1:0,2:z,3:w,4:3.25,5:d})
            p.asset('arcade:wall_navy_01',True)
        p.finish()
    props = [
        ('arcade:cabinet_racing',1.1,2,0),('arcade:cabinet_shooter',2.5,2,0),
        ('arcade:cabinet_action',3.9,2,0),('arcade:cabinet_fighting',6.0,2,0),
        ('arcade:cabinet_scifi',7.6,2,0),('arcade:change_machine',8.9,5,270),
        ('arcade:pinball_cosmic',1.1,5.1,90),('arcade:pinball_comet',1.1,7.4,90),
        ('arcade:bench_three_seat_blue_red_blue',8.5,7.8,270),
        ('arcade:neon_bowl',5,0.25,0,2.1),
        ('arcade:vending_machine',1,10.4,90),
        ('arcade:party_table_rect',3.2,14.4,0),
        ('arcade:party_cloth_rect_confetti',3.2,14.4,0,.75),
        ('arcade:party_table_round',7.7,15.7,0),
        ('arcade:party_cloth_round_stars',7.7,15.7,0,.75),
        ('arcade:fixed_chair_red',3.0,13.2,180),('arcade:fixed_chair_blue',4.0,15.6,0),
        ('arcade:fixed_chair_blue',7.7,17,0),
        ('arcade:party_cake_unlit',2.8,14.4,0,.75),
        ('arcade:party_cup_confetti',3.7,14.4,0,.75),
        ('arcade:party_plate_confetti',3.3,14.7,0,.75),
        ('arcade:party_gift_red',7.45,15.7,0,.75),
        ('arcade:party_gift_green',7.8,15.4,15,.75),
        ('arcade:party_gift_stars',5.8,12.5,20),
        ('arcade:cat_plush',6.5,7.6,-20),
        ('arcade:bowling_ball_burgundy',12.5,14.5,0),
        ('arcade:ball_rack',11.4,19.2,90),
        ('arcade:ball_return',14,17.5,0),
        ('arcade:score_monitor',14,20.5,180),
        ('arcade:bench_three_seat_red_blue_red',16.8,19.4,270),
        ('arcade:pedestal_table',16.7,16.8,0),
        ('arcade:bowling_ball_green',11.4,19.4,0,.74),
        ('arcade:bowling_ball_cobalt',11.4,19.0,0,.74),
        ('core:cardboard_box',1.0,19.8,0),
        ('core:office_noticeboard',0.25,19.5,90,1.05),
        ('core:exit_sign',4.9,21.6,180,2.35),
        ('home:wall_switch',8.9,4.5,270,1.3),
        ('home:wall_switch',9.65,16.7,270,1.3),
        ('home:wall_switch',10.2,20.8,90,1.3),
    ]
    for n in range(0,len(props),6):
        p=Phase(f'{4+n//6:02}-furnish')
        for args in props[n:n+6]:p.prop(*args)
        p.finish()
    # Bowling pins are scenery; this is a lock-up mode, not a second bowling game.
    p=Phase('11-pin-decks')
    for x,z in [(12.3,2.0),(12.1,1.65),(12.5,1.65),(15.9,2.0),(15.7,1.65),(16.1,1.65)]:
        p.prop('arcade:bowling_pin',x,z)
    p.finish()


if __name__ == '__main__':
    generate()
    environment()
