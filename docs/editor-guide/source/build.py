#!/usr/bin/env python3
"""Build GitHub Markdown and the self-contained PDF from guide.json and annotated images.
Requires: reportlab, beautifulsoup4, markdownify, pillow.
Run from any directory: python3 path/to/editor-guide/source/build.py
"""
from pathlib import Path
import json,html,re,io,hashlib,tempfile
from bs4 import BeautifulSoup
from markdownify import markdownify
from PIL import Image as PILImage
from reportlab.pdfgen import canvas
from reportlab.platypus import BaseDocTemplate,PageTemplate,Frame,Paragraph,Spacer,PageBreak,Table,TableStyle,Flowable,KeepTogether,CondPageBreak
from reportlab.platypus.tableofcontents import TableOfContents
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.colors import HexColor,white
from reportlab.lib.pagesizes import A4
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.lib.utils import ImageReader
import argparse
B=Path(__file__).resolve().parent.parent
G=json.loads((B/'source/guide.json').read_text())
PDF_NAME='Beginners_Guide_to_Your_First_Places_Space.pdf'
URL='https://github.com/csd113/Places/releases/download/v0.13.1/'+PDF_NAME
CHAPTERS=[('01-start','Start here',1,5),('02-backrooms','Build Backrooms',6,11),('03-home','Build Home',12,17),('04-pool','Build Pool',18,25),('05-elevators','Connect the elevators',26,27),('06-play','Build, export and play',28,33),('07-help','Troubleshooting',34,34)]

def clean(s):
 return s.replace('⌘','Cmd ').replace('→',' > ').replace('↗','').replace('—',' - ').replace('–','-').replace('\u2011','-')
def md(s):
 soup=BeautifulSoup(s,'html.parser')
 for li in soup.find_all('li'):li.unwrap()
 return markdownify(str(soup),heading_style='ATX').strip()
def blocks(p):
 soup=BeautifulSoup(p['body'],'html.parser');lead=[];steps=[];tail=[];tables=[];seen=False
 for node in soup.contents:
  if not getattr(node,'name',None):continue
  if node.name in ['ol','ul'] and not steps:
   steps=[str(x) for x in node.find_all('li',recursive=False)];seen=True
  elif node.name=='table':tables.append(str(node))
  elif seen:tail.append(str(node))
  else:lead.append(str(node))
 return lead,steps,tail,tables

def figmd(f,n,j):
 out=f'![Figure {n}.{j}: {f["caption"]}](../{f["image"]})\n\n'
 if 'detail_box'in f:
  detail=Path(f['image']).with_name(Path(f['image']).stem+'-detail.jpg');PILImage.open(B/f['image']).crop(f['detail_box']).save(B/detail,quality=90,optimize=True,subsampling=0)
  out+=f'![Control detail for figure {n}.{j}](../{detail.as_posix()})\n\n'
 return out+f'*Figure {n}.{j}. {f["caption"]}*\n\n'

for ci,(slug,title,start,end) in enumerate(CHAPTERS):
 nav=f'[Guide contents](../README.md) · [Download the complete PDF]({URL})'
 out=f'# {ci+1}. {title}\n\n{nav}\n\n'
 pages=G['pages'][start-1:end]
 out+='In this chapter:\n\n'+''.join(f'- [{p["number"]}. {p["title"]}](#step-{p["number"]:02})\n' for p in pages)+'\n'
 for p in pages:
  n=p['number'];out+=f'<a id="step-{n:02}"></a>\n\n## {n}. {p["title"]}\n\n';lead,steps,tail,tables=blocks(p)
  out+='\n\n'.join(md(s) for s in lead+tables)+'\n\n'
  for si,step in enumerate(steps,1):
   out+=f'**{si}.** {md(step)}\n\n'
   for j,f in enumerate(p['figures'],1):
    if f['after_step']==si:out+=figmd(f,n,j)
  out+='\n\n'.join(md(s) for s in tail)+'\n\n'
  for j,f in enumerate(p['figures'],1):
   if not steps or f['after_step']==0:out+=figmd(f,n,j)
  if p['checkpoint']:out+='> **Checkpoint:** '+p['checkpoint']+'\n\n'
  out+='[Back to chapter contents](#'+str(ci+1)+'-'+title.lower().replace(',','').replace(' ','-')+')\n\n---\n\n'
 prev=f'[← {CHAPTERS[ci-1][1]}]({CHAPTERS[ci-1][0]}.md)' if ci else '[← Guide contents](../README.md)'
 nxt=f'[{CHAPTERS[ci+1][1]} →]({CHAPTERS[ci+1][0]}.md)' if ci+1<len(CHAPTERS) else '[Guide contents →](../README.md)'
 out+=prev+' · '+nxt+'\n';(B/'chapters'/(slug+'.md')).write_text(out)
intro=f'''# Beginner’s Guide to Your First Places Space

**[Download the complete illustrated PDF]({URL})** — one file with every screenshot embedded. Open it directly; no repository clone, folder download or ZIP extraction is needed.

Prefer to read here? Start with chapter 1 below. These Markdown chapters render directly on GitHub, with screenshots beside the relevant actions in the sequence. Each chapter includes a short contents list and Previous/Next links. Click a screenshot to see it larger.

You'll build a Backrooms office, a furnished Home room and a Pool room, then link them with elevators and play your finished space. No prior editor knowledge or coding is needed. The whole walkthrough uses the editor’s **Simple** workspace.

| Chapter | What you’ll do |
| --- | --- |
'''
summaries=['Open the right executable, create and name a project, learn the camera.','Draw walls and an open doorway; add pillars, desk, chair and lights.','Create the room, fit a door, choose surfaces and place the bed and kitchen.','Build the entire shell, cut a dry basin, add water, closet, bench, duck and lights.','Place three cabins and link their stops.','Test, save, build, export, import and walk the complete route.','Recover from common selection, placement, material and import mistakes.']
for i,((slug,title,start,end),summary) in enumerate(zip(CHAPTERS,summaries),1):intro+=f'| [{i}. {title}](chapters/{slug}.md) | {summary} |\n'
intro+='''
**Keep these habits:** save at each checkpoint; click empty floor before selecting a different prop model; check a placement preview before you click; leave space for doors to swing.

The finished example uses one level containing three rooms. The pool is shallow, the duck rests on the deck, and the kitchen is decorative. Room creation, the initial pillar, basin and water use the forms that Simple provides; the other construction uses visual placement and handles.

[Start chapter 1 →](chapters/01-start.md)

## Example project and editable source

The optional [example project](example-project/) is the completed fresh-project replay. You do not need it to follow the guide. Copy that whole folder to a writable location if you want to inspect it, then use the editor’s **Open** folder chooser.

[Source and validation notes](source/README.md) explain how the guide is regenerated and what was tested. The screenshots and native replay are from Places 0.13.1 on Mac; Windows executable naming was verified against packaging, but Windows was not run. The PDF has clickable contents and bookmarks; use those to return to a chapter.
'''
(B/'README.md').write_text(intro)
# PDF typography: use common installed fonts; caller may supply an alternate directory.
parser=argparse.ArgumentParser();parser.add_argument('--pdf',type=Path,default=B.parent/PDF_NAME);parser.add_argument('--font-dir',type=Path,default=Path('/System/Library/Fonts/Supplemental'));args=parser.parse_args()
for name,file in [('Guide','Arial.ttf'),('Guide-Bold','Arial Bold.ttf'),('Guide-Italic','Arial Italic.ttf')]:pdfmetrics.registerFont(TTFont(name,str(args.font_dir/file)))
pdfmetrics.registerFontFamily('Guide',normal='Guide',bold='Guide-Bold',italic='Guide-Italic',boldItalic='Guide-Bold')
ink=HexColor('#20352f');teal=HexColor('#246b5c');pale=HexColor('#e8efe6')
styles={'body':ParagraphStyle('body',fontName='Guide',fontSize=11,leading=15.5,textColor=ink,spaceAfter=9),'caption':ParagraphStyle('caption',fontName='Guide',fontSize=9.5,leading=12,textColor=ink),'h1':ParagraphStyle('h1',fontName='Guide-Bold',fontSize=23,leading=28,textColor=ink,spaceAfter=17,keepWithNext=True),'chapter':ParagraphStyle('chapter',fontName='Guide-Bold',fontSize=11,leading=15,textColor=teal,spaceAfter=10,keepWithNext=True),'check':ParagraphStyle('check',fontName='Guide',fontSize=11,leading=15,backColor=pale,borderPadding=12,spaceBefore=10,spaceAfter=14)}
W,H=A4;M=43;CW=W-2*M
CACHE=tempfile.TemporaryDirectory(prefix='places-guide-pdf-');PDF_IMAGES={}
for page in G['pages']:
 for f in page['figures']:
  path=B/f['image'];target=Path(CACHE.name)/path.name
  im=PILImage.open(path);im.thumbnail((1600,1600));im.save(target,quality=82,subsampling=0,optimize=True);PDF_IMAGES[str(path)]=target

def markup(s):
 soup=BeautifulSoup(clean(s),'html.parser')
 for t in soup.find_all(['p','li']):t.unwrap()
 for t in soup.find_all('strong'):t.name='b'
 for t in soup.find_all('kbd'):t.name='b'
 for t in soup.find_all('code'):t.name='font';t.attrs={'name':'Courier','size':'9'}
 return str(soup)
def para(s,style='body'):return Paragraph(markup(s),styles[style])
class Heading(Paragraph):
 def __init__(self,text,level,key):super().__init__(html.escape(clean(text)),styles['chapter' if level==0 else 'h1']);self.level=level;self.key=key;self.raw=clean(text)
class Figure(Flowable):
 def __init__(self,f,n,j):
  super().__init__();self.f=f;self.path=B/f['image'];self.width=CW;w,h=f['native_size'];self.ih=CW*h/w;self.caption=para((f'<b>Figure {n}.{j}.</b> ' if n else '')+html.escape(f['caption']),'caption');_,self.ch=self.caption.wrap(CW,100)
  self.dh=0
  if 'detail_box'in f:
   x,y,xx,yy=f['detail_box'];self.dh=CW*(yy-y)/(xx-x)
  self.height=self.ih+self.dh+self.ch+(18 if self.dh else 0)+18
 def draw(self):
  c=self.canv;top=self.height;c.drawImage(str(PDF_IMAGES[str(self.path)]),0,top-self.ih,width=CW,height=self.ih);top-=self.ih
  if self.dh:
   top-=14;c.setFont('Guide-Bold',8);c.setFillColor(teal);c.drawString(0,top+3,'CONTROL DETAIL');x,y,xx,yy=self.f['detail_box'];w,h=self.f['native_size'];factor=CW/(xx-x)
   detail=self.path.with_name(self.path.stem+'-detail.jpg');c.drawImage(str(detail),0,top-self.dh,width=CW,height=self.dh);top-=self.dh+4
  self.caption.drawOn(c,0,top-self.ch-9)
class Book(BaseDocTemplate):
 def __init__(self,path):
  super().__init__(str(path),pagesize=A4,leftMargin=M,rightMargin=M,topMargin=47,bottomMargin=42,title=G['title'],author='Places',pageCompression=1)
  self.chapter='';self.lesson='';self.addPageTemplates(PageTemplate(id='book',frames=Frame(M,42,CW,H-89,id='body',leftPadding=0,rightPadding=0,topPadding=0,bottomPadding=0),onPageEnd=self.page))
 def beforeDocument(self):
  self.chapter=''
 def page(self,c,d):
  c.setFillColor(ink);c.setFont('Guide',8);c.drawString(M,23,'PLACES · FIRST SPACE');c.drawRightString(W-M,23,str(d.page));c.setStrokeColor(HexColor('#cbd7ca'));c.line(M,35,W-M,35)
  if d.page>2:
   c.setFont('Guide',8);c.drawString(M,H-25,self.chapter or 'Guide contents');c.drawRightString(W-M,H-25,'Contents');c.linkRect('', 'contents',(W-M-60,H-29,W-M,H-17),relative=0)
 def afterFlowable(self,f):
  if isinstance(f,Heading):
   self.canv.bookmarkPage(f.key);self.canv.addOutlineEntry(f.raw,f.key,level=f.level,closed=False);self.notify('TOCEntry',(f.level,f.raw,self.page,f.key))
   if f.level==0:self.chapter=f.raw
story=[]
story+=[Spacer(1,25),para('PLACES 0.13.1 · SIMPLE WORKSPACE','chapter'),Paragraph('Beginner’s Guide to<br/>Your First Places Space',ParagraphStyle('cover',parent=styles['h1'],fontSize=34,leading=39)),para('From a blank project to three connected rooms you can play.'),Spacer(1,20)]
hero=next(f for p in G['pages'] for f in p['figures'] if f['shot']=='102-three-room-overview');story.append(Figure(hero,0,0));story+=[Spacer(1,8),para('Everything is in this file: instructions, screenshots, control close-ups and checkpoints. Use the clickable contents and PDF bookmarks to find your place.'),PageBreak()]
class Anchor(Flowable):
 def __init__(self,key):super().__init__();self.key=key;self.width=0;self.height=0
 def draw(self):self.canv.bookmarkPage(self.key)
story+=[Anchor('contents'),para('Contents','h1'),para('Read in order the first time. The numbers below match the sections in the GitHub guide.')]
toc=TableOfContents();toc.levelStyles=[ParagraphStyle('toc0',fontName='Guide-Bold',fontSize=11,leading=15,spaceBefore=9,textColor=teal),ParagraphStyle('toc1',fontName='Guide',fontSize=10,leading=14,leftIndent=12,textColor=ink)];story+=[toc]
for ci,(slug,title,start,end) in enumerate(CHAPTERS):
 for p in G['pages'][start-1:end]:
  n=p['number']
  if n==start:
   story.append(PageBreak());story.append(Heading(f'{ci+1}. {title}',0,slug))
  else:story.extend([CondPageBreak(250),Spacer(1,20)])
  story.append(Heading(f'{n}. '+p['title'],1,p['id']));lead,steps,tail,tables=blocks(p)
  for s in lead:story.append(para(s))
  for s in tables:
   rows=[]
   for tr in BeautifulSoup(s,'html.parser').find_all('tr'):rows.append([para(str(td),'caption') for td in tr.find_all(['td','th'])])
   t=Table(rows,colWidths=[CW*.38,CW*.62],hAlign='LEFT');t.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),pale),('VALIGN',(0,0),(-1,-1),'TOP'),('LINEBELOW',(0,0),(-1,-1),.4,HexColor('#cad6c8')),('TOPPADDING',(0,0),(-1,-1),7),('BOTTOMPADDING',(0,0),(-1,-1),7)]));story.extend([t,Spacer(1,12)])
  for si,step in enumerate(steps,1):
   story.append(para(f'<b>{si}.</b> '+markup(step)))
   for j,f in enumerate(p['figures'],1):
    if f['after_step']==si:story.extend([Figure(f,n,j),Spacer(1,12)])
  for s in tail:story.append(para(s))
  for j,f in enumerate(p['figures'],1):
   if not steps or f['after_step']==0:story.extend([Figure(f,n,j),Spacer(1,12)])
  if p['checkpoint']:story.append(para('<b>Checkpoint</b><br/>'+html.escape(p['checkpoint']),'check'))
args.pdf.parent.mkdir(parents=True,exist_ok=True);Book(args.pdf).multiBuild(story)
print(json.dumps({'pdf':str(args.pdf),'bytes':args.pdf.stat().st_size,'chapters':7,'screenshots':sum(len(p['figures']) for p in G['pages'])}))

CACHE.cleanup()
