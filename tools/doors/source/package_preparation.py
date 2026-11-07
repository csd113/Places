from pathlib import Path
import json,hashlib,zipfile
from PIL import Image,ImageDraw,ImageFont
R=Path(__file__).resolve().parents[1]
sha=lambda b:hashlib.sha256(b).hexdigest()
gate=json.load(open(R/'metadata/asset_gate_report.json'))
reopen=json.load(open(R/'metadata/blender_reopen_import_gate.json'))
assert gate['status']==reopen['status']=='PASS'and len(reopen['models'])==2
assert all(m['byte_identical_source_reexport']for m in reopen['models'])
try:font=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf',18)
except OSError:font=ImageFont.load_default()
canvas=Image.new('RGB',(1600,980),(32,36,38));draw=ImageDraw.Draw(canvas)
draw.text((18,10),'Hollow-metal exit doors | actual GLB re-import previews | not native Places acceptance',font=font,fill=(235,235,230))
images=[]
for row,var in enumerate(('single','double')):
 for col,view in enumerate(('closed_front','closed_back','open_90','hardware')):
  p=R/'qa'/(var+'_'+view+'.png');im=Image.open(p).convert('RGB');assert im.size==(1000,1100)
  im.thumbnail((392,432),Image.Resampling.LANCZOS)
  x=col*400+4;y=row*470+50
  canvas.paste(im,(x,y+25));draw.text((x+6,y),var.title()+' / '+view.replace('_',' '),font=font,fill=(235,235,230))
  images.append({'path':p.relative_to(R).as_posix(),'sha256':sha(p.read_bytes()),'runtime_sha256':next(m['sha256']for m in gate['models']if m['variant']==var),'review':'Inspected actual pixels: silhouette, reverse, open aperture, fitted hardware; neutral CPU preview only.'})
canvas.save(R/'qa/contact_sheet.png',optimize=True)
json.dump({'status':'REVIEWED OFFLINE','images':images,'native_acceptance':False},open(R/'metadata/visual_review.json','w'),indent=2)
# Retain only requested deliverables, editable sources and deterministic evidence.
# Exclude app caches, Python bytecode and automatic .blend1 backup snapshots.
files=[]
for folder in ('source','runtime','textures','metadata','qa','reference','research'):
 for p in sorted((R/folder).rglob('*')):
  if not p.is_file()or '__pycache__'in p.parts or p.suffix in ('.pyc','.blend1'):continue
  files.append(p)
files.append(R/'README.md')
manifest=[{'path':p.relative_to(R).as_posix(),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())}for p in sorted(files)]
(R/'SHA256SUMS').write_text(''.join(m['sha256']+'  '+m['path']+'\n'for m in manifest))
(R/'MANIFEST.json').write_text(json.dumps({'preparation_state':'ready_for_later_integration','files':manifest},indent=2)+'\n')
files.extend([R/'SHA256SUMS',R/'MANIFEST.json'])
zip_path=R.parent/'Places_hollow_metal_exit_doors_preparation_2026-11-07.zip'
with zipfile.ZipFile(zip_path,'w',zipfile.ZIP_DEFLATED,compresslevel=9)as z:
 for p in sorted(files):
  info=zipfile.ZipInfo('hollow_metal_exit_doors/'+p.relative_to(R).as_posix(),date_time=(2026,11,7,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16
  z.writestr(info,p.read_bytes())
with zipfile.ZipFile(zip_path)as z:
 assert z.testzip()is None
 for m in manifest:
  assert sha(z.read('hollow_metal_exit_doors/'+m['path']))==m['sha256']
receipt={'path':str(zip_path),'bytes':zip_path.stat().st_size,'sha256':sha(zip_path.read_bytes()),'files':len(files),'manifest_members_verified':len(manifest)}
(R.parent/(zip_path.name+'.receipt.json')).write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
