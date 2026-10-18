#!/usr/bin/env python3
"""Opt-in shadow gallery authoring through visible native editor controls.

Emits input recipes, never map geometry. Start phase 01 with --new PROJECT,
then open that same project for each remaining phase. Animation routes and
named interaction/scripts are explicit advanced source edits after these steps.
"""
from pathlib import Path
import closing_time_editor as editor

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'local/shadow-only-0.12.0'
editor.OUT=OUT
class Phase(editor.Phase):
    def __init__(self,name):
        super().__init__(name)
        self.steps.insert(0,{'delay_ms':200,'action':{'kind':'focus'}})
    def finish(self):
        self.add('focus')
        super().finish()


def generate():
    p=Phase('01-room')
    p.select('Room',0)
    p.transform({0:0,2:0,3:12,4:3.6,5:32})
    p.asset('home:tile_home_01',True)
    p.finish()
    walls=[(-.2,-.2,12.4,.2),(-.2,32,12.4,.2),(-.2,0,.2,32),
           (12,0,.2,32),(1,19,7,.2),(8,23,3,.25)]
    for n in range(0,len(walls),2):
        p=Phase(f'{2+n//2:02}-walls')
        for x,z,w,d in walls[n:n+2]:
            p.new('Wall');p.transform({0:x,1:0,2:z,3:w,4:3.6,5:d})
            p.asset('home:wall_paint_offwhite_01',True)
        p.finish()
    actors=[(5,23),(6.7,22),(9,21),(6,2),(2.5,22)]
    for i,(x,z) in enumerate(actors):
        p=Phase(f'{5+i:02}-actor')
        p.new('Entity');p.transform({0:x,1:0,2:z})
        p.asset('mannequin');p.finish()
    p=Phase('10-light')
    p.add('scroll_scene',lines=-64);p.select('Light',0)
    p.transform({0:6,1:3.59,2:26})
    p.click('Gameplay')
    for name,val in [('Brightness',.07),('Range',12),('Emission',.12)]:p.setting(name,val)
    p.click('ClosePopup');p.finish()
    # New executable: verify every Boolean through the real gameplay sheet.
    p=Phase('11-shadow-property')
    p.add('scroll_scene',lines=-7)
    for i in range(4):
        p.select('Prop',i);p.click('Gameplay');p.click('GameSetting(ShadowOnly)');p.click('ClosePopup')
    p.finish()


if __name__=='__main__':generate()
