#!/usr/bin/env python3
"""Concrete native Beam outcome checks. No bake, no automatic CI integration."""
from pathlib import Path
import argparse,json

def audit(folder,case):
 samples=json.loads((folder/'snapshots.json').read_text());receipt=json.loads((folder/'receipt.json').read_text())
 checks=[]
 def require(ok,label):checks.append(dict(check=label,passed=bool(ok)))
 require(receipt['exit_code']==0 and receipt['shutdown'],'clean standalone native shutdown')
 for name,s in samples.items():
  require(not any('error' in d.lower() or 'budget exhausted' in d.lower() for d in s['script_diagnostics']),name+': no script error/budget refusal')
  require(s['audio']['errors']==s['audio']['dropped']==0,name+': no mixer error/drop')
  require(s['physics']['stats']['recovered_bodies']==s['physics']['stats']['suspended_bodies']==0,name+': no recovered/suspended item')
 actions=json.loads((folder/'actions.json').read_text())
 require(all(a['action']['kind'] in ['key','view','view_at','aim_entity','aim_item','capture','focus','quit'] for a in actions),'only ordinary player controls/camera and evidence actions; no pose/goal/flag activation')
 if case=='natural':
  s=samples['win'];require(s['outcome'] is not None and s['outcome'][0] is True,'ordinary exit interaction shows explicit win')
  require(s['physics']['held'] is None,'all gifts unheld at completion')
  require('3/3 gifts returned' in s['script_diagnostics'],'all three actual trigger objectives complete')
  require(all(5.8<=b['pose']['position'][0]<=8.2 and 13.9<=b['pose']['position'][2]<=14.9 and .96<=b['pose']['position'][1]<=1.4 for b in s['physics']['bodies']),'all real gift bodies rest on the exit counter')
  require(sum(v['looping'] for v in s['audio_voices'])==0,'store ambience stops at win')
  require(samples['hint-one-returned']['guidance'] is not None and samples['hint-one-returned']['guidance'][1][0].startswith('1 of 3'),'F1 reports current progress and next objective')
  states=[]
  for line in (folder/'trace.jsonl').read_text().splitlines():
   e=json.loads(line)
   if e['event']=='action_result':states.append(json.loads(e['detail']))
  require(any(s['player_body']['feet_y']>=2.99 for s in states),'real movement reaches upper floor')
  require(any(s['physics']['held'] is not None and s['player_body']['feet_y']>=2.99 for s in states),'gift actually carried on upper floor')
  require(any(s['doors'][1]['angle_degrees']>75 for s in states),'ordinary E opens real upper office leaf')
 elif case=='death':
  require(samples['reveal']['light_responses'][0]['active'] and samples['reveal']['light_responses'][0]['intensity']>.005,'beam genuinely reveals/holds the mannequin')
  for name in ['death','death-again']:require(samples[name]['outcome'] is not None and samples[name]['outcome'][0] is False,name+': explicit death feedback')
  for index,name in enumerate(['respawn-world','second-respawn'],1):
   s=samples[name];require(s['outcome'] is None and s['guidance'] is None and s['player_body']['reset_count']==index,name+': new clean playing run')
   require(s['physics']['held'] is None and not s['flashlight_on'],name+': hands/flashlight reset')
   require(s['light_responses'][0]['position']==[3.0,0.0,5.599999904632568] and not s['light_responses'][0]['active'],name+': creature pose and beam hold reset')
   require(all(d['phase']=='Closed' for d in s['doors']) and s['doors'][0]['locked'],name+': both doors reset')
   require(sum(v['looping'] for v in s['audio_voices'])==1,name+': authored ambience restored')
  if 'respawn-beam' in samples:
   resets=[json.loads(e['detail']) for e in map(json.loads,(folder/'trace.jsonl').read_text().splitlines()) if e['event']=='run_restarted']
   require(len(resets)==2,'two immediate native reset snapshots before subsequent ticks')
   for index,s in enumerate(resets,1):
    r=s['light_responses'][0];a=s['entity_simulation']['agents'][0];expected=[3.0,0.0,5.599999904632568]
    require(a['clock']==0 and a['speed_mps']==0 and a['position']==r['position']==r['script_position']==r['render_position']==expected,'immediate reset '+str(index)+': AI/render-frame/script/beam anchors agree before movement')
   for name in ['respawn-beam','second-respawn-beam']:
    s=samples[name];r=s['light_responses'][0]
    require(r['active'] and r['opacity']>0 and r['position']==r['render_position']==r['script_position']==s['entity_simulation']['agents'][0]['position'],name+': first real flashlight reacquisition samples the authored reset body')
 elif case=='edges':
  def counter(s):
   counts=[int(line[0]) for line in s['script_diagnostics'] if line.endswith('/3 gifts returned')]
   return counts[-1] if counts else 0
  for name,value in [('stars-first',1),('held-return-invalid',0),('return-reentry',1),('dropped-outstanding',0)]:
   require(counter(samples[name])==value,name+': physical enter/exit/unheld count')
  require(samples['early-exit-refused']['outcome'] is None and samples['early-exit-refused']['guidance'] is not None,'missing gift refuses win and offers current hint')
  s=samples['reset-items'];require(s['player_body']['reset_count']==1 and not s['flashlight_on'] and s['physics']['held'] is None,'ordinary restart sign resets run, hands and beam')
  require(counter(s)==0 and s['doors'][0]['locked'],'restart clears objectives and locks exit')
 elif case=='menu_reentry':
  require(samples['menu-death']['outcome'] is not None and not samples['menu-death']['outcome'][0],'ordinary death precedes menu/reentry')
  require(samples['outcome-menu']['state']=='MainMenu','outcome Esc returns to actual main menu')
  s=samples['reentry-world'];require(s['state']=='Playing' and s['outcome'] is None and s['guidance'] is None and s['physics']['held'] is None and not s['flashlight_on'],'ordinary menu play reenters clean gameplay')
  require(s['doors'][0]['locked'] and not s['script_diagnostics'],'reentry restores fresh objectives and locked exit')
 elif case=='held_death':
  require(samples['held-red']['physics']['held'] is not None,'real pickup before death')
  require(samples['death-holding']['outcome'] is not None and not samples['death-holding']['outcome'][0],'ordinary dark pursuit catches holding player')
  s=samples['held-reset'];require(s['physics']['held'] is None and s['player_body']['reset_count']==1 and s['outcome'] is None,'respawn releases held item')
  authored=[p['position'] for p in s['authored_placements'] if p['physics']]
  require(all(sum((a-b)**2 for a,b in zip(body['pose']['position'],position))<.001 for body,position in zip(s['physics']['bodies'],authored)),'all gifts restored to authored poses')
 elif case=='beam':
  def response(n):return samples[n]['light_responses'][0]
  require(0 < response('far-beam')['intensity'] < response('near-beam')['intensity'],'real distance attenuates beam between roughly 8.5m and 2.4m')
  require(response('beam-edge')['opacity']>0 and response('beam-edge')['intensity']<response('near-beam')['intensity'],'17-degree edge dims before 35-degree exclusion')
  require(response('near-beam')['active'] and response('near-beam')['intensity']>.035,'near beam reveals and holds')
  require(response('outside-beam')['opacity']==0 and response('outside-beam')['intensity']<.005,'outside beam hides immediately')
  require(response('power-off')['opacity']==0 and response('power-off')['active'],'power off hides immediately while release grace remains')
  require(not response('dark-movement')['active'],'darkness releases movement after grace')
  require(response('held-again')['active'] and response('held-again')['opacity']>0,'real F reacquires and holds')
  require(response('partition-occlusion')['opacity']==0,'physical partition occludes beam')
 elif case=='patrol':
  positions=[s['entity_simulation']['agents'][0]['position'] for s in samples.values()]
  require(max(p[0] for p in positions)-min(p[0] for p in positions)>4,'real authored route travels around furniture and partitions')
  require(any(p[1]>2.5 for p in positions),'shared planner reaches actual upper floor')
  require(any(s['doors'][1]['angle_degrees']>75 for s in samples.values()),'AI opens the physical upper door')
  require(all(s['outcome'] is None for s in samples.values()),'refuge avoids unavoidable attack')
 elif case in ['bounds','east_south','upper_bounds']:
  require(all(s['outcome'] is None for s in samples.values()),'bounds attempt remains live rather than blocked by outcome')
  require(all(.2<=s['player_position'][0]<=13.8 and .2<=s['player_position'][2]<=15.8 and s['player_body']['feet_y']>=-.01 for s in samples.values()),'ordinary push/jump/crouch cannot escape perimeter or floor')
 result=dict(case=case,passed=all(c['passed'] for c in checks),checks=checks,receipt=receipt)
 (folder/'acceptance.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps({'case':case,'passed':result['passed'],'failed':[c for c in checks if not c['passed']]}))
 return result['passed']
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('folder',type=Path);p.add_argument('case',choices=['natural','death','smoke','edges','beam','patrol','held_death','bounds','east_south','upper_bounds','menu_reentry']);a=p.parse_args();raise SystemExit(0 if audit(a.folder,a.case) else 1)
