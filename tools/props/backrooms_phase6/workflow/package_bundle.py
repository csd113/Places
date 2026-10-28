import os,json,glob,hashlib,zipfile,shutil,math
from pathlib import Path
B=Path(os.environ.get('PLACES_PHASE6_ROOT','/workspace/shared/places-phase6'))
report=json.load(open(B/'qa/glb_validation.json'));assert report['passed'] and len(report['assets'])==25
cat=[];ledger={};anchors={};materials=json.load(open(B/'metadata/texture_contract.json'))
for r in report['assets']:
 name=r['name'];id=r['id'];family='HVAC'if name.startswith('hvac')else'Furniture'if 'table_'in name or'chair_'in name else'Utility'if 'cabinet'in name or'breaker'in name else'Fixtures'
 solid=family in ['HVAC','Furniture','Utility'] or name.startswith('utility_')
 entry={'id':id,'display_name':name.replace('_',' ').title(),'asset_class':'environment','asset_type':'prop','source':'file','model':'environments/backrooms/props/'+name+'.glb','size':r['size'],'color':'#c0c3ad','category':family,'solid':solid,'theme':'backrooms','authoring':{'tool':'completed','source':'completed-models.json'},'description':'Blender/GIMP authored institutional utility kit. Metres, +Y up, +Z front, centred base origin. See phase6 manifest for mounts and connector anchors.'};cat.append(entry)
 ledger[entry['model']]={'sha256':r['sha256'],'triangles':r['triangles_measured'],'texture_edge':256,'identities':[id],'bounds_min':r['bounds_min'],'bounds_max':r['bounds_max']}
 if name.startswith('hvac'):
  if 'straight'in name or'short'in name:
   L=2.4 if'straight'in name else 1.2;raw=[([-L/2,.2,0],[-1,0,0]),([L/2,.2,0],[1,0,0])]
  elif'vertical'in name:raw=[([0,0,0],[0,-1,0]),([0,2.4,0],[0,1,0])]
  elif'horizontal'in name:raw=[([-.9,.2,0],[-1,0,0]),([0,.2,.9],[0,0,1])]
  else:
   s=-1 if'down'in name else 1;y=.2 if s>0 else 1.1;raw=[([-.9,y,0],[-1,0,0]),([0,y+s*.9,0],[0,s,0])]
  shift=r['origin_translation_game'];anchors[id]=[{'position':[round(v+shift[i],6)for i,v in enumerate(p)],'outward_normal':n}for p,n in raw]
json.dump({'assets':cat},open(B/'catalog_entries.json','w'),indent=2);json.dump(ledger,open(B/'completed_model_entries.json','w'),indent=2);json.dump(anchors,open(B/'hvac_connectors.json','w'),indent=2)
reused=['janitor:janitor_cart','janitor:cleaner_jug','janitor:detergent_bottle','janitor:spray_cleaner_bottle','janitor:cardboard_supply_box','janitor:perforated_supply_crate','janitor:mop_wringer_bucket','janitor:string_mop','janitor:paper_towel_roll','janitor:folded_towel']
manifest={'title':'Places 0.13.1 institutional utility and HVAC kit','source_commit_audited':'81d1893514f1409c02243f4b41448b05d99c3f82','authoring_tools':{'Blender':'4.3.2','GIMP':'3.0.4'},'new_models':25,'passes_per_model':3,'runtime_texture':'textures/liminal_utility_256.png','master_texture':'textures/liminal_utility_master.png','editable_texture':'sources/liminal_utility_materials.xcf','new_decoded_texture_bytes':report['new_texture_decoded_rgba_bytes'],'coordinate_contract':'+Y up, +Z front, metres, XZ-centred, occupied base at Y=0','materials':materials,'hvac_connectors':anchors,'reused_existing_catalog_ids':reused,'validation':'qa/glb_validation.json','actual_roundtrip_renders':['qa/pass1_contact.png','qa/pass2_contact.png','qa/pass3_contact.png','qa/final_detail_sheet.png'],'scope_limit':'Asset authoring only; editor/game integration and native engine validation remain with integration executor. No audio authored or runtime wiring changed.','assets':report['assets']}
json.dump(manifest,open(B/'manifest.json','w'),indent=2)
readme='''# Places 0.13.1 — institutional utility + HVAC kit

25 new static props, authored in the cloud with **Blender 4.3.2 and GIMP 3.0.4**. Low-poly institutional metal, restrained sage/cream, square manufactured stock, mild age; no bubbly forms or unrelated props. Existing source audited at `81d1893514f1409c02243f4b41448b05d99c3f82`.

## Delivered scope
- HVAC: 2.4 m and 1.2 m horizontal straight stock, 2.4 m vertical stock, 90-degree horizontal/up/down elbows. All ducts are genuinely hollow with open ends, 12 mm sheet stock, external flange rings and folded seams. Ordinary yaw gives either horizontal direction.
- Electrical: floor cabinet and wall breaker panel, each closed and visibly hinged-open.
- Doors: single utility and double wired-glass institutional doors. Separate stiles/rails surround a real glazing aperture. Opaque milked wired glass is deliberate, not a transparency claim. Door models are static, not animated or interactable doors.
- Suspended ceilings: missing, crooked, stained and exposed T-grid; 1.2 m stock modules, 0.6 m bays, real empty apertures.
- Fluorescents: recessed, surface, cable-hung, damaged and missing-diffuser, 1.2 x 0.6 m footprint.
- Wall clock and PA speaker; laminate folding table and stackable steel chair.
- Maintenance cart/supplies are **reused, not duplicated**: existing `janitor:janitor_cart`, bottle/mop/box/cloth stock. The accepted cart was fetched read-only and actually rendered in `qa/detail_janitor_cart_existing.png`; its angular utility style already fits. No existing model was modified.

## Source and export layout
- `models/`: final self-contained GLB files.
- `sources/`: individual editable packed Blender files, retained pass1/pass2 sources, and layered GIMP XCF.
- `textures/`: 1024-square master, 256-square native fitted atlas, optional 128-square derivative. Every GLB embeds its own copy of the 256-square PNG; keep the standalone equivalent beside models on import.
- `workflow/`: actual Blender/GIMP construction, export, round-trip rendering, validation and packaging scripts.
- `passes/` and `metadata/`: the first two exported construction stages and measured metadata for all three stages.
- `qa/`: actual exported-GLB round-trip renders, close-ups, validation report and review notes. Contact-sheet ceiling/fixture assemblies are flipped **only for inspection** to expose their undersides. This is not an export transform. Close-up hanging-fixture cables therefore appear below its displayed underside, but extend above the housing in the delivered model.

## Coordinates, mounts and connections
All final GLBs: **1 unit = 1 metre, +Y up, +Z front, X/Z centred, occupied base at Y=0**. `manifest.json` gives exact sizes and hashes. Wall items mount by their rear measured bound; ceiling pieces mount by their measured top Y. Crooked/damaged pieces include their displaced stock in the bounds. Do not assume the bounding box is the snapping grid.

HVAC raw passages are 0.60 x 0.40 m. Horizontal X stock has Z width .60/Y height .40; vertical stock has Z width .60/X depth .40 to mate with vertical bends. Match positions and opposite outward normals in `hvac_connectors.json`, then apply ordinary world placement/yaw. Small flange/seam projection is included in measured bounds. The ceiling stock grid is exactly 1.2 m even though small tabs project beyond it. Use a 1.2 m placement increment, not the occupied-envelope width.

## Materials and image contract
One 4x4 fitted GIMP atlas, 256 native, 1024 master; UVs stay in 0..1, clamp, no runtime image generation. Roles and region order are in `metadata/texture_contract.json`. Metal uses scalar metal .55/roughness .48; other roles matte. All materials are opaque, and all are non-emissive. The wire pattern is milked wired glass. Lens/tube artwork is albedo, not a real light source. No material normal, baked AO or directional-light texture is required. Closed sheet stock casts ordinary opaque shadows; genuine holes contain no surface.

## Import into Places
1. Copy `models/*.glb` to `assets/environments/backrooms/props/` and `textures/liminal_utility_256.png` beside them. Retain master/XCF/editable .blend files under the chosen authoring-source location.
2. Merge `catalog_entries.json` into `assets/catalog.json` and `completed_model_entries.json` into `assets/completed-models.json`. These are **fragments**, not replacements for those files. Reuse existing IDs listed in the manifest for cart/supplies; do not overwrite or duplicate them.
3. Recompute the complete pack budget. New images add **6,553,600 decoded RGBA bytes (6.25 MiB)** across 25 embedded copies. The audited old spec says about104.4/106 MiB; assess/increase that outdated aggregate budget with justification rather than dropping image detail silently.
4. Default catalog collision is conservative. Add narrow compound collision for duct walls/doors/legs if required; do not fill a hollow passage or exposed grid merely because its visual bounds are a box. Open cabinet covers are fixed geometry, not an interaction state machine.
5. Fluorescent props are passive models: integrate switched runtime lights with the corresponding model/fixture if desired. Do not make their metal housings emissive. Clock is fixed10:08; PA speaker has no automatic sound.
6. HVAC audio must be runtime proximity-only to these six duct IDs (including edited/rotated placements), not the old generic ceiling grille. This bundle deliberately does not wire sounds or modify game code. Integration must validate walk-away fade/stop, no ducts/no sound, up/down runs, map switches and editor delete/move cases.
7. Run repository content/asset validation plus the authorized native editor/game checks. Cloud authoring validation is **not** a substitute for native engine acceptance.

## Reproduce
Set `PLACES_PHASE6_ROOT` to this unpacked bundle directory. Run GIMP Python batch `workflow/paint_atlas_gimp.py` in an isolated writable GIMP profile, then `blender -b -t 10 --python workflow/build_assets.py -- --pass 1 --render`, repeat with `--pass 2` and `--pass 3`, and run `python3 workflow/validate_glb.py`. Construction scripts import only the bundled helpers. Python validation/contact composition uses Pillow. Blender renders use Cycles CPU24 samples with denoising disabled because this Blender build has no OpenImageDenoise support. The workflow has no paid/network asset dependency.
'''
(B/'README.md').write_text(readme)
# Deliver only intentional public engineering artifacts, not connector state or scratch input.
include=[]
for folder in ['models','sources','textures','workflow','passes','metadata']:
 include += [p for p in (B/folder).rglob('*') if p.is_file() and '__pycache__' not in p.parts and not p.name.endswith('.blend1')]
include += [p for p in (B/'qa').glob('*.png')]+[B/'qa/glb_validation.json',B/'qa/REVIEW.md']
include += [B/p for p in ['README.md','manifest.json','catalog_entries.json','completed_model_entries.json','hvac_connectors.json']]
archive=B.parent/'Places_0.13.1_Utility_HVAC_Cloud_Assets.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=7)as z:
 for p in include:
  if p.exists():z.write(p,'Places_0.13.1_Utility_HVAC/'+str(p.relative_to(B)))
with zipfile.ZipFile(archive)as z:assert z.testzip()is None;print('BUNDLE',str(archive),archive.stat().st_size,'files',len(z.infolist()))
