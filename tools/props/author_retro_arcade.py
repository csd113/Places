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
    a=Art("cabinet_"+kind)
    # 4:3 screen; 4:1 marquee; 2:1 controls; square coin panel.
    a.box((0,0,128,96),(7,13,27))
    a.box((3,4,125,92),(12,23,45))
    a.box((0,96,128,128),accent)
    a.box((2,98,126,126),(24,26,41))
    a.text(title,(4,98,124,121),accent,15)
    a.text("INSERT COIN",(32,120,96,127),(218,216,200),4)
    a.box((0,128,128,192),(32,36,43))
    for x in (8,118):
        a.line([(x,133),(x,188)],accent,2)
    a.text(title,(8,132,120,145),accent,7)
    a.box((0,192,64,256),(37,43,50))
    a.box((8,206,56,246),(14,18,24),outline=(114,121,124),width=1)
    a.text("25¢",(18,213,46,227),(187,190,174),7)
    a.text("PUSH",(19,235,45,243),(187,190,174),4)
    for i,(name,color) in enumerate((('body',(26,30,38)),('metal',(151,153,154)),('dark',(7,10,17)),('white',(233,229,213)))):
        stock(a,(64+i%2*32,192+i//2*32,32,32),color,i)
    # Side art fills a true 1:2 region, on both sides without mirroring text.
    a.box((128,0,256,256),(19,23,37))
    a.poly([(129,0),(178,0),(256,154),(256,219)],accent)
    a.poly([(128,208),(173,176),(256,216),(256,256),(128,256)],tuple(round(v*.6) for v in accent))
    a.text(title.split()[0],(133,10,250,35),(235,226,190),20)
    a.text(" ".join(title.split()[1:]),(136,32,250,49),(235,226,190),13)
    if kind=='racing':
        a.box((5,4,123,44),(129,63,48))
        a.poly([(5,44),(123,44),(123,77),(5,77)],(111,84,50))
        a.poly([(50,44),(79,44),(120,91),(8,91)],(47,48,48))
        a.line([(64,49),(61,64),(58,84)],(210,193,135),2)
        a.poly([(51,70),(74,70),(81,85),(43,85)],(205,48,45))
        a.box((54,69,73,77),(60,100,128))
        a.text("LAP 02    186",(6,7,122,16),(243,221,155),6)
        a.poly([(141,126),(163,108),(219,109),(247,131),(245,149),(138,148)],(203,63,46))
        a.poly([(161,110),(176,86),(207,86),(222,112)],(52,115,150))
        for x in (160,226): a.oval((x-12,137,x+12,161),(14,18,25),outline=(174,175,163),width=2)
        a.text("GRAND TOUR",(135,176,250,196),(245,221,176),11)
    elif kind=='shooter':
        for i in range(22):
            x=8+(i*37)%112;y=17+(i*19)%65
            a.box((x,y,x+1,y+1),(191,205,204))
        for row in range(3):
            for col in range(5):
                x=21+col*19;y=24+row*13
                a.poly([(x-4,y+3),(x,y-2),(x+4,y+3),(x,y+1)],(78,174,192))
        a.poly([(59,84),(64,72),(69,84)],(217,224,205))
        a.text("P1 003850",(4,6,90,15),(146,203,213),6)
        a.poly([(145,158),(192,64),(235,158),(200,137),(192,180),(183,137)],(123,200,209))
        a.oval((174,105,210,141),(29,68,97))
        a.line([(150,168),(133,215)],accent,3)
        a.line([(231,168),(249,218)],accent,3)
    elif kind=='action':
        a.box((5,5,123,69),(45,41,63))
        for i in range(7):
            x=6+i*19;h=19+(i*7)%24
            a.box((x,68-h,x+16,72),(69,65,75))
            for yy in range(48,65,7): a.box((x+4,yy,x+6,yy+3),(148,126,86))
        a.box((5,72,123,91),(74,55,44))
        a.box((33,63,40,80),(210,171,55));a.box((35,57,39,64),(204,162,127))
        a.box((85,63,94,80),(140,50,48));a.box((88,57,93,64),(203,162,127))
        a.text("QUEST 03  ♥♥♥",(4,7,122,16),(232,188,80),6)
        a.poly([(159,194),(163,126),(177,100),(194,102),(210,140),(207,194)],(210,156,47))
        a.oval((174,69,200,101),(199,153,110))
        a.poly([(177,136),(193,132),(230,173),(223,181)],(198,156,109))
        a.text("THE CITY CALLS",(133,211,250,231),(228,203,145),10)
    elif kind=='fighting':
        a.box((5,7,123,70),(44,28,59))
        a.box((5,70,123,91),(80,59,69))
        for x,c in ((28,(183,56,69)),(89,(42,114,155))):
            a.oval((x-4,41,x+4,49),(212,168,126));a.box((x-6,49,x+6,67),c)
            a.line([(x-4,67),(x-9,80)],c,4);a.line([(x+3,67),(x+8,80)],c,4)
            a.line([(x,53),(x+14 if x==28 else x-14,56)],c,4)
        a.box((8,17,53,21),(198,57,66));a.box((76,17,120,21),(66,127,169))
        a.text("ROUND 2",(38,6,91,15),(230,211,164),6)
        for x,c in ((164,(181,58,65)),(224,(37,97,143))):
            a.oval((x-10,77,x+10,99),(202,155,114))
            a.poly([(x-15,104),(x+12,102),(x+18,154),(x-20,156)],c)
            a.line([(x-10,151),(x-20,203)],c,14);a.line([(x+9,151),(x+20,203)],c,14)
        a.text("CHOOSE YOUR SIDE",(133,218,250,237),(222,197,163),9)
    else:
        a.poly([(8,10),(37,40),(38,79),(8,91)],(28,57,71))
        a.poly([(120,10),(91,40),(90,79),(120,91)],(28,57,71))
        a.box((39,31,91,80),(18,37,48),outline=(56,107,126),width=1)
        for yy in range(41,76,8): a.line([(43,yy),(87,yy)],(39,83,105),1)
        a.oval((55,49,75,69),None,outline=(130,164,158),width=1)
        a.line([(64,46),(64,72)],(130,164,158),1)
        a.text("SECTOR 7   0440",(5,6,123,15),(155,190,171),6)
        a.oval((143,63,242,166),(39,64,77),outline=(135,175,172),width=3)
        a.poly([(171,185),(191,81),(215,185),(192,163)],(157,185,180))
        a.oval((187,120,202,145),(21,67,84))
        a.text("BEYOND THE EDGE",(133,214,250,235),(174,203,192),10)
    a.finish()
    if kind=='scifi':
        mask=Art('cabinet_scifi_uv_mask',(0,0,0))
        mask.oval((143,63,242,166),None,outline=(210,210,210),width=3)
        mask.poly([(171,185),(191,81),(215,185),(192,163)],(155,155,155))
        mask.oval((187,120,202,145),(0,0,0))
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
    a=Art('vending_machine')
    a.box((0,0,128,64),(21,92,159))
    a.poly([(0,60),(0,31),(87,0),(128,0),(128,17)],(28,119,185))
    a.poly([(9,64),(48,0),(65,0),(26,64)],(177,43,54))
    a.text('STRIKE',(9,10,119,36),(235,236,213),23)
    a.text('REFRESH YOUR GAME',(12,39,117,54),(196,217,218),7)
    a.box((0,64,128,192),(13,24,37))
    product_colors=((145,36,47),(35,99,145),(159,125,39),(29,110,93),(116,47,99))
    for row in range(4):
        yy=69+row*29
        a.box((5,yy+22,123,yy+25),(99,117,126))
        for col in range(5):
            x=11+col*23;c=product_colors[(col+row)%5]
            a.box((x,yy+1,x+14,yy+21),c,outline=(126,138,134),width=.4)
            a.box((x+1,yy+9,x+13,yy+14),(223,217,193))
            a.text(('COLA','SODA','POP','LIME','FIZZ')[col],(x,yy+9,x+14,yy+14),c,3)
            a.box((x+2,yy-1,x+12,yy+1),(162,169,167))
    a.box((0,192,128,224),(20,79,133))
    a.text('COLD DRINKS',(7,196,120,221),(223,225,205),14)
    a.box((0,224,128,256),(7,12,18))
    a.text('PUSH',(39,236,88,250),(102,110,112),6)
    a.box((128,0,192,192),(79,90,98))
    a.text('$1.00',(134,9,187,27),(227,222,191),9)
    a.box((144,35,178,39),(13,19,25))
    a.box((141,53,181,67),(25,29,34));a.text('BILLS',(142,54,180,66),(196,205,189),6)
    for y in range(88,173,17):
        a.box((139,y,181,y+12),(20,39,57),outline=(134,155,155),width=.6)
        a.box((143,y+3,163,y+9),(174,186,166));a.oval((172,y+3,178,y+9),(70,122,154))
    a.text('SELECT',(131,175,189,190),(193,201,191),7)
    for i,(name,color) in enumerate((('body',(24,62,114)),('trim',(94,108,120)),('dark',(10,15,22)),('metal',(158,164,163)))):
        stock(a,(192,i*64,64,64),color,i)
    a.box((128,192,192,256),(29,57,92))
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
