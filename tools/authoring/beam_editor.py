#!/usr/bin/env python3
"""Explicit native Beam authoring input recipes; no scene geometry is generated.
Uses visible controls and real editor input. Never a Cargo/CI dependency.
"""
from pathlib import Path
import json
import closing_time_editor as editor
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'local/verification/0.12.0-beam'
editor.OUT=BASE
Phase=editor.Phase

def generate():
 p=Phase('01-floor')
 p.select('Room',0);p.transform({0:0,2:0,3:14,4:2.8,5:16})
 p.asset('core:carpet_damp_01',True);p.finish()
 walls=[(0,0,14,.2), (0,.2,.2,15.6),(13.8,.2,.2,15.6),
        (0,15.8,6.3,.2),(7.7,15.8,6.3,.2),(6.3,15.8,1.4,.2,2.2,.6),
        (6.8,.2,.2,4.6),(6.8,7,.2,5.0)]
 for n in range(0,len(walls),4):
  p=Phase(f'{2+n//4:02}-walls')
  for row in walls[n:n+4]:
   x,z,w,d,*rest=row
   p.new('Wall');p.transform({0:x,1:rest[0] if rest else 0,2:z,3:w,4:rest[1] if rest else 2.8,5:d})
  p.finish()
 props=[('core:office_reception_desk',3,14.8,180),
 ('home:coffee_table',7,14.4),('core:exit_sign',7,15.68,180,2.28),
 ('core:desk',2.8,3),('core:chair',2.8,4.1,180),('core:office_filing_cabinet_tall',1,2),
 ('core:bookshelf',5.8,2.1,270),('core:plant',1,5),('core:table',3.2,7.8),
 ('core:chair',3.2,6.7),('core:chair',4.3,8.1,270),('core:armchair',1.3,10.4,90),
 ('core:couch',3.5,11.3),('core:lamp',1,12),('core:cardboard_box',5.8,10.7),
 ('core:bookshelf',12.8,2.7,270),('core:desk',10.8,3.8),('core:chair',10.8,5,180),
 ('core:couch',11,9.4),('home:coffee_table',11,10.9),('core:armchair',9.1,11,90),
 ('core:tv',12.7,7.6,270,.9),('core:cabinet',12.7,7.6,270),
 ('arcade:party_gift_red',2.8,3,0,.75),('arcade:party_gift_green',10.8,3.8,15,.75),
 ('arcade:party_gift_stars',11,10.9,-15,.46)]
 for n in range(0,len(props),3):
  p=Phase(f'{4+n//3:02}-furniture')
  for row in props[n:n+3]:p.prop(*row)
  p.finish()
 p=Phase('13-door')
 p.add('world_click',position=[7,0,14]);p.click('Focus')
 p.new('Door');p.transform({0:6.3,2:15.9,3:1.4,4:2.2,5:.045})
 p.finish()
 # Two modest, real fixtures keep the entrance and lateral landmark legible.
 fixtures=sorted(e['id'] for e in editor.CAT if e['asset_type']=='light')
 for n,(x,z,power,color) in enumerate([(7,14.4,.08,[.8,.65,.45]),(2,6,.015,[.38,.52,.7]),(11,7,.015,[.38,.52,.7])]):
  p=Phase(f'{14+n:02}-light')
  if n==0:p.add('scroll_scene',lines=-120);p.select('Light',0);p.click('Delete')
  p.new('Light');p.transform({0:x,1:2.798,2:z})
  p.click('Gameplay');p.click('GamePick(Fixture)')
  i=fixtures.index('core:fluorescent_panel_01')
  for _ in range(i//10):p.click('Page(true)')
  p.click(f'GameChoose(Fixture, {i})')
  for name,val in [('Brightness',power),('Range',5),('Red',color[0]),('Green',color[1]),('Blue',color[2]),('Emission',.15)]:p.setting(name,val)
  p.click('ClosePopup');p.finish()
 p=Phase('17-triggers')
 for _ in range(3):
  p.new('Trigger');p.transform({0:5.9,1:.4,2:13.6,3:2.2,4:.8,5:1.6})
 p.finish()
 p=Phase('18-actor')
 p.new('Entity');p.transform({0:3,1:0,2:7})
 p.asset('sheet-ghost')
 p.click('Gameplay');p.click('GamePick(EntityBehavior)');p.click('GameChoose(EntityBehavior, 1)');p.click('ClosePopup');p.finish()
 p=Phase('19-physics')
 scroll=0
 for i in (23,24,25):
  ordinal=9+i;p.add('scroll_scene',lines=scroll-ordinal);scroll=ordinal
  p.select('Prop',i);p.click('Gameplay');p.click('GameSetting(Physics)');p.click('ClosePopup')
 p.finish()

if __name__=='__main__':generate()
