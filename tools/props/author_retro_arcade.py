#!/usr/bin/env python3
"""Explicit offline authoring of original period Arcade fitted PNG artwork.

This command writes retained 1024-square masters, Lanczos 256-square graphic
natives and 128-square low-detail palette natives.
Normal model builds only read the committed natives. Coordinates below are
native atlas pixels; all packing and type overlays use four times that size.
Pinball illustrations are retained file-backed inputs from the built-in
imagegen tool; exact prompts are under assets/references/neon_bowling_arcade.
No reference-board pixels are copied, cropped or used as texture imagery.
"""
from __future__ import annotations

import argparse
import math
from pathlib import Path
import random

from PIL import Image, ImageDraw, ImageFont, ImageOps

ROOT = Path(__file__).resolve().parents[2] / "assets/environments/arcade/props"
SCALE = 4
COLORS = {
    "ivory": (232, 227, 212), "red": (160, 31, 48), "blue": (25, 67, 128),
    "grey": (116, 124, 127), "metal": (150, 156, 159), "dark": (20, 25, 35),
    "black": (15, 18, 25), "white": (241, 237, 224), "purple": (69, 44, 94),
    "burgundy": (83, 29, 51), "green": (29, 77, 62), "marble": (73, 45, 106),
    "bottle": (31, 44, 44), "label": (20, 86, 154), "yellow": (223, 174, 56),
    "paper": (198, 190, 168),
}
CABINETS = {
    "racing": ("TURBO COAST", (214, 93, 36)),
    "shooter": ("ORBIT PATROL", (24, 159, 202)),
    "action": ("STREET QUEST", (226, 177, 48)),
    "fighting": ("DUEL ARENA", (153, 60, 149)),
    "scifi": ("NOVA SIGNAL", (75, 120, 160)),
}


class Art:
    def __init__(self, name, background=(20, 25, 35)):
        self.name = name
        self.image = Image.new("RGB", (1024, 1024), background)
        self.draw = ImageDraw.Draw(self.image)

    def box(self, rect, color, outline=None, width=1):
        self.draw.rectangle(tuple(round(v*SCALE) for v in rect), fill=color,
                            outline=outline, width=max(1, round(width*SCALE)))

    def poly(self, points, color):
        self.draw.polygon([(round(x*SCALE), round(y*SCALE)) for x,y in points], fill=color)

    def line(self, points, color, width=1):
        self.draw.line([(round(x*SCALE), round(y*SCALE)) for x,y in points],
                       fill=color, width=max(1, round(width*SCALE)), joint="curve")

    def oval(self, rect, color, outline=None, width=1):
        self.draw.ellipse(tuple(round(v*SCALE) for v in rect), fill=color,
                          outline=outline, width=max(1, round(width*SCALE)))

    def text(self, text, rect, color, size=14, *, bold=True):
        x0,y0,x1,y1 = rect
        font_path = Path("/System/Library/Fonts/Supplemental/Arial Bold.ttf" if bold else
                         "/System/Library/Fonts/Supplemental/Arial.ttf")
        font = ImageFont.truetype(str(font_path), round(size*SCALE)) if font_path.exists() else ImageFont.load_default(size=round(size*SCALE))
        while font.getlength(text) > (x1-x0-3)*SCALE and size > 3:
            size -= .5
            font = ImageFont.truetype(str(font_path), round(size*SCALE)) if font_path.exists() else ImageFont.load_default(size=round(size*SCALE))
        bb = self.draw.textbbox((0,0), text, font=font)
        x = (x0+x1)*SCALE/2-(bb[2]-bb[0])/2
        y = (y0+y1)*SCALE/2-(bb[3]-bb[1])/2-bb[1]
        self.draw.text((round(x),round(y)), text, fill=color, font=font)

    def illustration(self, filename, rect):
        """Fit committed original bitmap art into an existing atlas region."""
        x,y,w,h=rect
        with Image.open(ROOT/'illustrations'/filename) as source:
            if source.mode != 'RGB':
                raise ValueError(f'{filename} must be an opaque RGB illustration')
            fitted=ImageOps.fit(source,(w*SCALE,h*SCALE),Image.Resampling.LANCZOS)
            self.image.paste(fitted,(x*SCALE,y*SCALE))

    def finish(self,native_size=256):
        ROOT.mkdir(parents=True, exist_ok=True)
        self.image.save(ROOT / (self.name+"_master.png"))
        self.image.resize((native_size,native_size), Image.Resampling.LANCZOS).save(ROOT / (self.name+".png"))

    def low_palette(self):
        self.image.save(ROOT / (self.name+"_128_master.png"))
        self.image.resize((128,128), Image.Resampling.LANCZOS).save(ROOT / (self.name+"_128.png"))


def stock(art, rect, color, seed=1):
    """Very quiet printed/plastic grain, confined to one fitted material cell."""
    x,y,w,h = rect
    art.box((x,y,x+w,y+h),color)
    rng=random.Random(seed)
    for _ in range(round(w*h*.16)):
        px,py=rng.uniform(x+1,x+w-1),rng.uniform(y+1,y+h-1)
        delta=rng.randint(-4,4)
        c=tuple(max(0,min(255,v+delta)) for v in color)
        art.box((px,py,px+.3,py+.3),c)


def palette():
    a=Art("bowling_palette")
    for i,(name,color) in enumerate(COLORS.items()):
        x,y=i%4*64,i//4*64
        stock(a,(x,y,64,64),color,i+20)
    # The marbled bowling ball has broad organic ribbons, not RGB noise.
    for j in range(5):
        points=[(192+i*4,135+j*10+math.sin(i*.36+j)*5) for i in range(17)]
        a.line(points,(102,75,134),2)
    a.box((68,199,124,249),(29,84,149))
    a.poly([(67,226),(91,201),(125,201),(102,226),(74,248),(66,248)],(166,44,50))
    a.text("STRIKE",(69,207,123,226),(236,232,213),10)
    a.text("COLA",(73,226,118,240),(236,232,213),8)
    a.text("LEAGUE",(196,196,251,207),(89,86,79),8)
    for y in range(211,250,5):
        a.line([(196,y),(249,y)],(145,140,123),.4)
    # Native label occupies the entire cell; U wraps once around the bottle.
    a.finish()
    a.low_palette()


def cabinet(kind, title, accent):
    """Compose original period prints and gameplay into the fixed cabinet atlas."""
    a=Art("cabinet_"+kind)
    # Original full-bleed gameplay, fitted once to the mandatory 4:3 glass.
    a.illustration('cabinet_'+kind+'_screen.png',(0,0,128,96))
    a.box((0,0,128,11),(8,12,27))
    ivory=(239,221,164);cyan=(126,214,227)
    if kind=='racing':
        a.text('LAP 02/03',(4,1,57,10),ivory,6)
        a.text('TIME 064',(76,1,124,10),ivory,6)
        a.text('186 KM/H',(6,82,56,91),ivory,6)
    elif kind=='shooter':
        a.text('P1 003850',(4,1,60,10),cyan,6)
        a.text('HI 013000',(70,1,124,10),ivory,6)
        for x in (7,13,19):a.poly([(x,82),(x-2,88),(x+2,88)],cyan)
    elif kind=='action':
        a.text('STAGE 03',(4,1,56,10),ivory,6)
        a.text('P1 03500',(72,1,124,10),ivory,6)
        for x in (7,13,19):a.box((x,83,x+3,87),(220,74,85))
    elif kind=='fighting':
        a.box((4,1,124,16),(11,12,28))
        for rect,color in (((5,7,52,11),(199,58,76)),((76,7,123,11),(65,158,194))):
            a.box(rect,color)
        a.text('P1',(4,0,17,7),ivory,4)
        a.text('P2',(111,0,124,7),ivory,4)
        a.text('99',(54,2,74,13),ivory,7)
        a.text('ROUND 2',(44,14,85,21),ivory,5)
    else:
        a.text('SECTOR 7',(4,1,55,10),cyan,6)
        a.text('SCORE 0440',(65,1,124,10),ivory,6)
        a.line([(60,48),(68,48)],cyan,.65)
        a.line([(64,44),(64,52)],cyan,.65)
        a.box((6,83,49,87),(25,45,54),outline=cyan,width=.6)
        a.box((7,84,37,86),(62,151,125))
        a.text('AMMO 024',(77,80,122,89),ivory,5)
    a.box((2,90,126,94),(8,12,27))
    a.text('CREDIT 00',(40,90,89,95),ivory,4)
    a.box((0,0,127,95),None,outline=(9,13,23),width=2)
    # Printed laminate deck, with large control legends and hardware surrounds.
    stock(a,(0,128,128,64),(51,39,58),31)
    a.poly([(2,185),(8,180),(120,180),(126,185),(126,190),(2,190)],(91,59,92))
    for x in (8,120):a.line([(x,134),(x,187)],accent,2)
    a.text(title,(12,132,116,145),accent,8)
    if kind=='racing':
        a.text('STEER / ACCELERATE',(10,167,118,179),(196,181,167),6)
        a.poly([(47,153),(64,146),(81,153),(64,160)],accent)
    elif kind=='scifi':
        a.oval((26,153,50,177),None,outline=accent,width=1.5)
        a.text('TRACKBALL',(17,178,59,187),(202,196,186),4)
        for x in (89,107):a.oval((x-5,151,x+5,161),(19,23,36),outline=accent,width=1)
    else:
        sticks=(24,90) if kind=='fighting' else (31,)
        for x in sticks:a.oval((x-7,156,x+7,170),(20,24,38),outline=accent,width=1)
        clusters=(63,113) if kind=='fighting' else (89,)
        for cx in clusters:
            for j in range(6 if kind=='fighting' else 3):
                x=cx+(j%3-1)*7.5;y=165-(j//3)*11
                a.oval((x-4.5,y-4.5,x+4.5,y+4.5),(20,24,38),outline=accent,width=.9)
        a.text('MOVE',(14,176,46,185),(209,193,173),4)
        a.text('ATTACK',(77,177,113,185),(209,193,173),4)
    stock(a,(0,192,64,64),(66,62,72),17)
    a.box((7,202,57,248),(16,20,29),outline=(126,123,133),width=1.5)
    a.box((13,210,24,218),(133,142,151))
    a.box((15,212,22,214),(8,12,20))
    a.text('25¢',(26,210,50,224),(221,208,179),8)
    a.text('PUSH TO REJECT',(11,231,53,238),(168,173,171),4)
    a.box((15,241,49,245),(78,83,94))
    for rect,color in (((64,192,96,224),(42,35,46)),((96,192,128,224),(151,153,154)),
                       ((64,224,96,256),(13,14,24)),((96,224,112,240),(181,41,63)),
                       ((112,224,128,240),(224,173,70)),((96,240,112,256),(51,149,186)),
                       ((112,240,128,256),(223,220,198))):
        a.box(rect,color)
    # Original text-free print input; no reference-board pixels are reused.
    illustration='cabinet_'+kind+'_side.png'
    a.illustration(illustration,(128,0,128,256))
    a.text(title.split()[0],(133,8,251,30),(241,216,165),21)
    a.text(' '.join(title.split()[1:]),(134,29,250,47),accent,15)
    a.text({'racing':'COAST TO COAST','shooter':'DEFEND THE ORBIT',
            'action':'THE CITY CALLS','fighting':'CHOOSE YOUR SIDE',
            'scifi':'BEYOND THE EDGE'}[kind],(134,236,250,249),(221,208,186),7)
    # Painted image under large ivory titles, inside the fixed 4:1 marquee.
    a.illustration(illustration,(0,96,128,32))
    overlay=Image.new('RGB',(128*SCALE,32*SCALE),(15,18,34))
    region=a.image.crop((0,96*SCALE,128*SCALE,128*SCALE))
    a.image.paste(Image.blend(region,overlay,.60),(0,96*SCALE))
    title_band=a.image.crop((3*SCALE,99*SCALE,125*SCALE,123*SCALE))
    quiet=Image.new('RGB',title_band.size,(12,15,29))
    a.image.paste(Image.blend(title_band,quiet,.40),(3*SCALE,99*SCALE))
    a.box((1,97,127,127),None,outline=accent,width=1)
    a.text(title,(4,99,124,122),(14,13,27),16)
    a.text(title,(3,98,123,121),(244,214,153),16)
    # A quiet lower strip replaces subtitle microtype that blurred in native
    # front views; the painted image remains visible around the main title.
    a.box((3,123,125,126),(12,15,29))
    a.line([(9,125),(119,125)],accent,.6)
    a.finish()
    if kind=='scifi':
        mask=Art('cabinet_scifi_uv_mask',(0,0,0))
        # Co-registered highlight pigment, independent of material emission.
        region=a.image.crop((128*SCALE,0,256*SCALE,256*SCALE))
        source=region.load();pixels=[]
        for y in range(region.height):
            for x in range(region.width):
                r,g,b=source[x,y]
                pigment=max(0,min(155,round((max(r,g,b)-58)*.8)))
                if b<r*.60 or max(r,g,b)-min(r,g,b)<15:pigment=0
                pixels.append((pigment,pigment,pigment))
        numeric=Image.new('RGB',region.size);numeric.putdata(pixels)
        mask.image.paste(numeric,(128*SCALE,0))
        mask.finish()


def score():
    a=Art('score_monitor')
    a.box((0,0,192,144),(16,51,129))
    a.box((6,7,186,25),(24,77,159))
    a.text('OPEN BOWL',(7,7,184,24),(218,229,211),11)
    for i in range(11): a.line([(8+i*16,34),(8+i*16,116)],(142,180,199),.7)
    for y in (34,53,79,105,116): a.line([(8,y),(184,y)],(142,180,199),.7)
    for i in range(10):
        a.text(str(i+1),(10+i*16,35,25+i*16,51),(223,225,181),8)
        a.text(('X','8 /','9 -','X','7 /','X','8 /','9 /','X','X X')[i],(9+i*16,56,25+i*16,73),(241,237,205),7)
        a.text(str((i+1)*18),(8+i*16,82,25+i*16,101),(236,234,214),6)
    a.text('PLAYER 1   180  •  YOUR TURN',(7,122,185,138),(228,209,139),8)
    a.box((0,144,128,240),(10,14,19))
    a.line([(4,192),(124,192)],(30,39,45),.6)
    for y in range(147,237,3): a.line([(4,y),(124,y)],(14,20,26),.5)
    a.text('NO SIGNAL',(20,184,109,199),(48,66,69),6)
    for i,(name,color) in enumerate((('body',(35,39,45)),('bezel',(18,22,28)),('metal',(117,126,130)),('white',(216,218,211)))):
        stock(a,(192,i*64,64,64),color,i)
    a.box((128,144,192,256),(23,28,34))
    a.finish()


def vending():
    """Fit original generated period art to the rebuilt machine's UV frame.

    This offline operation intentionally rebuilds its GLB alongside the atlas;
    no concept-board pixels become runtime textures. Preserve the retained
    illustration and the exact pixel rectangles declared in retro_arcade.py.
    """
    a=Art('vending_machine')
    with Image.open(ROOT/'illustrations/vending_bepsi_front.png') as source:
        source=source.convert('RGB')
        w,h=source.size
        def fit(rect, dest):
            x,y,width,height=dest
            crop=source.crop(tuple(round(v) for v in rect))
            a.image.paste(ImageOps.fit(crop,(width*SCALE,height*SCALE),Image.Resampling.LANCZOS),
                          (x*SCALE,y*SCALE))
        fit((0,0,w,h*.325),(0,0,128,80))
        # Two 2-column banks share six clear beverage designs in three rows.
        labels=[]
        for row,(lo,hi) in enumerate(((.327,.486),(.486,.631),(.631,.802))):
            for left,right in ((.012,.361),(.638,.987)):
                labels.append((left*w,lo*h,right*w,hi*h))
        order=(0,2,1,3,4,0,5,1,2,4,3,5)
        for i,source_index in enumerate(order):
            fit(labels[source_index],(i%4*32,80+i//4*32,32,32))
        fit((0,h*.80,w,h),(0,176,128,80))
    # Control/payment strip is ordinary silver hardware, never a light panel.
    a.box((128,0,192,192),(138,148,156))
    a.box((133,6,187,34),(29,42,39))
    a.text('$1.00',(134,8,186,31),(155,207,153),12)
    a.text('INSERT BILL',(131,36,189,48),(29,35,42),6)
    a.box((138,50,182,60),(10,16,24))
    a.text('COINS',(131,67,169,80),(26,34,40),6)
    a.box((173,66,179,86),(13,19,27))
    a.oval((140,105,166,131),(110,28,38),outline=(38,43,51),width=1)
    a.text('RETURN',(132,135,190,148),(27,33,41),6)
    a.box((149,151,174,161),(21,30,41))
    a.text('MAKE',(131,172,189,180),(30,37,46),6)
    a.text('SELECTION',(131,181,189,189),(30,37,46),6)
    a.box((128,192,192,224),(7,12,20))
    a.text('PUSH',(142,199,181,216),(86,104,118),9)
    for i,(name,color) in enumerate((('body',(18,53,113)),('trim',(79,96,114)),('dark',(8,12,19)),('metal',(144,159,172)))):
        stock(a,(192,i*64,64,64),color,i)
    a.finish()


def pinball(name, title, accent):
    a=Art(name)
    a.illustration(name+'_playfield.png',(0,0,128,256))
    a.box((20,5,108,18),(13,25,43))
    a.text(title,(21,5,107,18),accent,9)
    # Printed landing targets match the existing 0.62 x 1.14 m plate,
    # its 11.2-degree pitch and the retained two-pixel fitted UV inset.
    pitch=math.radians(11.2)
    for x,z in ((0,-.36),(-.17,-.16),(.17,-.16)):
        u=64+124*x/.62
        v=2+252*(.5+(z-.006*math.sin(pitch))/(1.14*math.cos(pitch)))
        a.oval((u-12,v-13,u+12,v+13),(13,25,43),outline=accent,width=1.5)
        a.oval((u-9.5,v-10.5,u+9.5,v+10.5),None,outline=(226,211,158),width=.7)
    # Quiet printed landing fields leave the real grey flippers legible.
    for x in (-.11,.11):
        u=64+124*x/.62
        v=2+252*(.5+(.39-.006*math.sin(pitch))/(1.14*math.cos(pitch)))
        a.oval((u-17,v-7,u+17,v+7),(13,25,43),outline=accent,width=.6)
    a.box((23,181,105,192),(13,25,43))
    a.text('ORBIT BONUS' if name.endswith('cosmic') else 'COMET BONUS',(24,181,104,192),accent,8)
    a.box((29,234,100,245),(13,25,43))
    a.text('SHOOT AGAIN',(30,234,99,245),(231,220,174),7)
    a.illustration(name+'_backglass.png',(128,0,128,80))
    a.text(title,(131,1,253,15),accent,13)
    a.box((128,80,256,112),(9,13,19))
    a.text('1  0254800',(134,83,250,107),(222,123,57),12)
    a.box((128,112,256,144),(23,36,55))
    a.text(title,(133,113,250,134),accent,10)
    a.text('2 PLAYERS   25¢',(146,133,242,142),(193,201,181),5)
    a.illustration(name+'_backglass.png',(128,144,128,64))
    a.box((139,147,245,160),(13,25,43))
    a.text(title,(140,147,244,160),(230,218,173),10)
    for i,c in enumerate(((39,49,61),(152,161,166),(11,16,25),(232,227,210))):
        stock(a,(128+i*32,208,32,48),c,i)
    a.finish()


def utility():
    a=Art('utility')
    names=('body','metal','dark','white','red','blue','yellow','tube')
    values=((105,113,119),(153,160,163),(11,15,23),(232,226,210),(209,40,53),(26,126,199),(225,191,56),(141,125,194))
    for i,c in enumerate(values): stock(a,(i%4*64,i//4*64,64,64),c,i)
    a.box((0,128,128,256),(106,115,120))
    a.text('CHANGE',(5,132,123,161),(235,230,210),19)
    a.text('$1  •  $5',(14,170,114,186),(37,47,51),13)
    a.text('INSERT BILL',(14,194,114,206),(222,217,193),8)
    a.box((24,210,104,217),(17,24,30))
    a.text('QUARTERS',(23,232,110,250),(230,225,205),10)
    a.box((128,128,256,256),(11,17,28))
    a.finish()
    a.low_palette()


def cat():
    a=Art('cat_plush')
    for i,c in enumerate(((23,25,32),(218,213,198),(48,45,47),(98,78,74))):
        stock(a,(i%2*128,i//2*128,128,128),c,i)
    # Stitch/seam hints belong to fabric; eyes are dark ordinary mesh pieces.
    a.line([(14,37),(20,63),(17,99)],(34,36,43),.7)
    a.finish(native_size=128)


def main():
    authors={'bowling_palette':palette,'score_monitor':score,'vending_machine':vending,
             'pinball_cosmic':lambda:pinball('pinball_cosmic','COSMIC ROLL',(46,164,183)),
             'pinball_comet':lambda:pinball('pinball_comet','COMET RUN',(211,137,45)),
             'utility':utility,'cat_plush':cat}
    for kind,(title,accent) in CABINETS.items():
        authors['cabinet_'+kind]=lambda k=kind,t=title,c=accent:cabinet(k,t,c)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--only',nargs='+',choices=tuple(authors),help='Author only these fitted PNG atlases')
    selected=parser.parse_args().only or tuple(authors)
    for name in selected:authors[name]()
    print('Authored '+', '.join(selected)+'; 1024-square masters retained')


if __name__ == '__main__':
    main()
