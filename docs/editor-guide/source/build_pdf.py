#!/usr/bin/env python3
"""Rebuild the guide: python -m pip install reportlab pillow; python build_pdf.py.
Original native PNGs remain unmodified. Crops and red callouts are PDF vectors.
"""
from pathlib import Path
import json, math
from PIL import Image
from reportlab.pdfgen import canvas
from reportlab.lib.colors import HexColor, white
from reportlab.platypus import Paragraph, Table, TableStyle
from reportlab.lib.styles import ParagraphStyle
ROOT=Path(__file__).resolve().parents[1]
PAGES=json.loads((ROOT/'source/pages.json').read_text())
OUT=ROOT.parent/'Beginners_Guide_to_Your_First_Places_Space.pdf'
W,H=900,675
INK=HexColor('#23313a'); TEAL=HexColor('#007f7a'); PALE=HexColor('#e9f3f0'); GRAY=HexColor('#5a6870'); RED=HexColor('#df242e')
styles={k:ParagraphStyle(k,fontName='Helvetica',fontSize=s,leading=l,textColor=INK,spaceAfter=7) for k,s,l in [('body',12,17),('small',10,14),('caption',9,12),('intro',14,20)]}
c=canvas.Canvas(str(OUT),pagesize=(W,H),pageCompression=1)
c.setTitle('Beginner’s Guide to Your First Places Space');c.setAuthor('Places');c.setSubject('A tested native editor walkthrough for Places 0.13.1')
def para(t,x,top,width,style='body'):
 p=Paragraph(t,styles[style]);_,h=p.wrap(width,1000);p.drawOn(c,x,top-h);return top-h-8

def shot(spec,x,y,w,h):
 path=ROOT/'screenshots'/(spec['name']+'.png');im=Image.open(path);iw,ih=im.size
 crop=spec.get('crop',[0,0,1,1]);l,t,r,b=[crop[0]*iw,crop[1]*ih,crop[2]*iw,crop[3]*ih]
 scale=min(w/(r-l),h/(b-t));dw=(r-l)*scale;dh=(b-t)*scale;ox=x+(w-dw)/2;oy=y+(h-dh)/2
 c.saveState();p=c.beginPath();p.rect(ox,oy,dw,dh);c.clipPath(p,stroke=0);c.drawImage(str(path),ox-l*scale,oy-(ih-b)*scale,width=iw*scale,height=ih*scale)
 for n,mark in enumerate(spec.get('marks',[]),1):
  if 'action' in mark:
   data=json.loads((ROOT/'screenshots'/(spec['name']+'.json')).read_text());q=next(q for q in data['visible_controls'] if q['action']==mark['action']);ux,uy=q['center'];base=min(iw/480,ih/272);px=(iw-480*base)/2+ux*base;py=(ih-272*base)/2+uy*base
  else:px,py=mark['at'][0]*iw,mark['at'][1]*ih
  xx=ox+(px-l)*scale;yy=oy+(b-py)*scale;rx=mark.get('rx',.037)*iw*scale;ry=mark.get('ry',.020)*ih*scale
  c.setStrokeColor(RED);c.setLineWidth(2.3);c.ellipse(xx-rx,yy-ry,xx+rx,yy+ry,stroke=1,fill=0)
  dx,dy=mark.get('offset',[-38,30]);sx,sy=xx+dx,yy+dy;ex,ey=xx+dx*.25,yy+dy*.25
  c.line(sx,sy,ex,ey);a=math.atan2(ey-sy,ex-sx);p=c.beginPath();p.moveTo(ex,ey);p.lineTo(ex-8*math.cos(a-.45),ey-8*math.sin(a-.45));p.lineTo(ex-8*math.cos(a+.45),ey-8*math.sin(a+.45));p.close();c.setFillColor(RED);c.drawPath(p,fill=1,stroke=0)
 c.restoreState();c.setStrokeColor(HexColor('#c6d1d4'));c.setLineWidth(.6);c.rect(ox,oy,dw,dh,fill=0,stroke=1)
 return oy

def table(data,x,top,width):
 rows=[[Paragraph(str(v),styles['small']) for v in row] for row in data]
 widths=None
 if len(rows[0])==2:widths=[width*.30,width*.70]
 t=Table(rows,colWidths=widths or [width/len(rows[0])]*len(rows[0]),hAlign='LEFT')
 t.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),PALE),('VALIGN',(0,0),(-1,-1),'TOP'),('LINEBELOW',(0,0),(-1,0),.7,TEAL),('LINEBELOW',(0,1),(-1,-1),.3,HexColor('#d8e0e1')),('LEFTPADDING',(0,0),(-1,-1),8),('RIGHTPADDING',(0,0),(-1,-1),8),('TOPPADDING',(0,0),(-1,-1),5),('BOTTOMPADDING',(0,0),(-1,-1),5)]));_,th=t.wrap(width,1000);t.drawOn(c,x,top-th);return top-th
for i,page in enumerate(PAGES,1):
 c.setFillColor(HexColor('#fafbf8'));c.rect(0,0,W,H,fill=1,stroke=0)
 c.setFillColor(TEAL);c.rect(36,H-38,32,4,fill=1,stroke=0);c.setFont('Helvetica-Bold',9);c.drawString(78,H-38,'PLACES  /  FIRST SPACE  /  0.13.1')
 c.setFillColor(INK);c.setFont('Helvetica-Bold',25);c.drawString(36,H-79,page['title'])
 c.bookmarkPage(str(i));c.addOutlineEntry(page['title'],str(i),level=0)
 if page.get('intro'):para(page['intro'],36,H-96,828,'small')
 top=H-135
 if 'shot' in page:
  shot(page['shot'],36,238,540,302)
  para(page.get('caption','Actual Places editor capture. Red callouts identify the controls used here.'),36,227,540,'caption')
  y=top
  for item in page.get('steps',[]):y=para(item,600,y,264)
  if y<222:raise ValueError(f'Page {i}: steps overflow {y}')
 elif 'shots' in page:
  for j,sp in enumerate(page['shots']):
   shot(sp,36+j*420,285,408,250);para(sp.get('caption',''),36+j*420,274,408,'caption')
  y=235
  for item in page.get('steps',[]):y=para(item,36,y,828)
 else:
  y=top
  for item in page.get('steps',[]):y=para(item,36,y,828,'intro')
 bottomtop=page.get('bottom_top',196)
 if page.get('table'):
  end=table(page['table'],36,bottomtop,828)
  if end<48:raise ValueError(f'Page {i}: table overflow {end}')
 if page.get('note'):
  nt=page.get('note_top',bottomtop if not page.get('table') else 73)
  para('<b>CHECKPOINT</b>  '+page['note'],330 if page.get('detail') else 36,nt,534 if page.get('detail') else 828,'small')
 if page.get('detail'):
  shot(page['detail'],36,50,270,145)
  para(page['detail_text'],330,118,430,'small')
 c.setStrokeColor(HexColor('#d0dbdb'));c.line(36,37,864,37);c.setFillColor(GRAY);c.setFont('Helvetica',8);c.drawString(36,22,'Beginner’s Guide to Your First Places Space');c.drawRightString(864,22,f'{i:02d} / {len(PAGES):02d}')
 c.showPage()
c.save();print(OUT)
