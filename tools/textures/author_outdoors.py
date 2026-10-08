#!/usr/bin/env python3
"""Offline Outdoors concept artwork. Ordinary builders load committed PNGs.

Masters are 1024px; prop derivatives are 256px Lanczos. Surface derivatives
remain 512px, preserving the kit's established repeat scale and runtime path.
"""
from pathlib import Path
import math
import random
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2] / 'assets/environment/outdoor'
MODEL = ROOT / 'props/models'


def save(im, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    im.convert('RGBA').save(path, optimize=True)


def periodic(im):
    for x in range(im.width): im.putpixel((x, im.height-1), im.getpixel((x, 0)))
    for y in range(im.height): im.putpixel((im.width-1, y), im.getpixel((0, y)))
    return im


def tone(base, delta):
    return tuple(max(0, min(255, c + delta)) for c in base)


def finish(size, base, kind, seed):
    rng = random.Random(seed)
    im = Image.new('RGB', (size, size), base)
    d = ImageDraw.Draw(im)
    # Large low-contrast islands carry variation through mip reduction.
    for _ in range(140):
        x,y = rng.randrange(size),rng.randrange(size)
        r = rng.randrange(max(3,size//35),max(4,size//9))
        points=[(x+math.cos(i*math.tau/7)*r,y+math.sin(i*math.tau/7)*r*.7) for i in range(7)]
        for dx in (-size,0,size):
            for dy in (-size,0,size): d.polygon([(a+dx,b+dy) for a,b in points],fill=tone(base,rng.randint(-8,8)))
    if kind in ('grass','leaf'):
        for _ in range(size*size//120):
            x,y = rng.randrange(size),rng.randrange(size)
            w,h = rng.randrange(max(2,size//100),max(3,size//35)),rng.randrange(max(2,size//100),max(3,size//30))
            color=tone(base,rng.choice((-19,-12,-5,8,17,25)))
            d.polygon([(x,y+h*.3),(x+w*.4,y),(x+w,y+h*.2),(x+w*.8,y+h),(x+w*.1,y+h*.8)],fill=color)
    elif kind in ('dirt','rock','concrete'):
        for _ in range(size*size//350):
            x,y=rng.randrange(size),rng.randrange(size)
            r=rng.randrange(max(2,size//180),max(3,size//55))
            delta=rng.choice((-20,-13,10,18,28)) if kind=='dirt' else rng.randint(-10,10)
            points=[(x-r,y),(x-r*.3,y-r*.65),(x+r*.8,y-r*.4),(x+r,y+r*.6),(x-r*.6,y+r*.7)]
            # Chips crossing a tile edge continue on its opposite edge.
            shifts_x=[0]+([size] if kind=='dirt' and x-r<0 else [])+([-size] if kind=='dirt' and x+r>=size else [])
            shifts_y=[0]+([size] if kind=='dirt' and y-r<0 else [])+([-size] if kind=='dirt' and y+r>=size else [])
            for dx in shifts_x:
                for dy in shifts_y:
                    d.polygon([(a+dx,b+dy) for a,b in points],fill=tone(base,delta))
                    if kind=='dirt': d.line((x-r*.3+dx,y-r*.65+dy,x+r*.8+dx,y-r*.4+dy),fill=tone(base,delta+10),width=max(1,size//512))
    elif kind in ('bark','wood','clapboard'):
        for i in range(90):
            x=rng.randrange(size)
            points=[(x+math.sin(y*math.tau/size+i)*size*.007,y) for y in range(0,size+1,max(1,size//32))]
            if kind=='clapboard': points=[(b,a) for a,b in points]
            d.line(points,fill=tone(base,rng.choice((-30,-18,-8,8,15))),width=max(1,size//256))
        if kind=='clapboard':
            for row in range(13):
                y=round(row*size/13)
                d.line((0,y,size,y),fill=tone(base,-36),width=max(2,size//170))
                d.line((0,y+max(2,size//170),size,y+max(2,size//170)),fill=tone(base,16),width=max(1,size//300))
        if kind=='bark':
            for _ in range(32):
                x,y=rng.randrange(size),rng.randrange(size)
                d.ellipse((x,y,x+size//75,y+size//12),fill=tone(base,-25))
    elif kind=='shingle':
        step=size//8
        for row in range(-1,9):
            y=row*step+step//3
            d.line((0,y,size,y),fill=tone(base,-22),width=max(2,size//150))
            d.line((0,y+5,size,y+5),fill=tone(base,7),width=max(1,size//300))
            for col in range(-1,9):
                x=col*step+(row%2)*step//2
                d.line((x,y+6,x,y+step-2),fill=tone(base,-18),width=max(2,size//180))
    elif kind=='pane':
        d.rectangle((size*.05,size*.03,size*.95,size*.97),fill=tone(base,9))
        d.polygon([(size*.12,size*.9),(size*.2,size*.12),(size*.4,size*.12),(size*.3,size*.9)],fill=tone(base,18))
    return periodic(im)


def atlas(name, specs):
    im=Image.new('RGB',(1024,1024))
    for i,(base,kind) in enumerate(specs):
        im.paste(finish(512,base,kind,340+i),((i%2)*512,(i//2)*512))
    save(im,MODEL/(name+'_master.png'))
    save(im.resize((256,256),Image.Resampling.LANCZOS),MODEL/(name+'.png'))


def main():
    for name,base,kind in [('grass_ground',(92,110,66),'grass'),('dirt_gravel',(139,111,83),'dirt'),('concrete_pavement',(170,173,175),'concrete')]:
        im=finish(1024,base,kind,201)
        if kind=='concrete':
            d=ImageDraw.Draw(im)
            for xy in (256,768):
                d.line((xy,0,xy,1024),fill=(128,132,139),width=5)
                d.line((0,xy,1024,xy),fill=(128,132,139),width=5)
                d.line((xy+5,0,xy+5,1024),fill=(184,186,187),width=2)
        save(periodic(im),ROOT/f'textures/ground/{name}_master.png')
        save(periodic(im.resize((512,512),Image.Resampling.LANCZOS)),ROOT/f'textures/ground/{name}_01.png')
    bark=((102,77,53),'bark'); wood=((166,151,121),'wood')
    for name,leaf1,leaf2 in [('tree_01',(87,113,66),(110,132,75)),('tree_02',(100,124,69),(122,140,78)),('tree_03',(45,72,58),(65,91,62)),('bush_round',(97,119,65),(126,141,75)),('bush_low',(74,105,62),(97,120,68))]:
        atlas(name,[bark,wood,(leaf1,'leaf'),(leaf2,'leaf')])
    for name in ('lamp_stand','lamp_fence','lamp_wall','streetlight'):
        atlas(name,[((65,64,57),'metal'),((237,189,107),'pane'),((103,82,58),'wood'),((45,44,41),'metal')])
    for name in ('fence_two_rail','masonry_pier','porch_canopy','road_barrier','boundary_ridge','showcase_boulder','showcase_rock_face','showcase_rock_face_variant'):
        atlas(name,[((179,176,168),'concrete'),((184,174,151),'wood'),((60,68,85),'shingle'),((139,147,158),'rock')])
    # Original wall-module UV layout remains four quadrants.
    atlas('house_base', [((174,177,164),'clapboard'),((223,216,194),'wood'),((225,169,84),'pane'),((121,95,66),'wood')])
    # Family 02 keeps the exact seven-region atlas layout and both derivatives.
    layout={'siding':(0,0,1,.375),'trim':(0,.375,.375,.25),'glass':(.375,.375,.25,.25),'jamb':(.625,.375,.375,.25),'shingle':(0,.625,.5,.375),'deck':(.5,.625,.25,.375),'post':(.75,.625,.25,.375)}
    specs={'siding':((171,178,177),'wood'),'trim':((222,213,190),'wood'),'glass':((226,168,85),'pane'),'jamb':((114,91,66),'wood'),'shingle':((61,69,88),'shingle'),'deck':((170,162,145),'concrete'),'post':((224,216,193),'wood')}
    house=Image.new('RGB',(1024,1024))
    for name,(x,y,w,h) in layout.items():
        cell=finish(1024,*specs[name],301).resize((round(w*1024),round(h*1024)),Image.Resampling.LANCZOS)
        if name=='siding':
            d=ImageDraw.Draw(cell)
            for row in range(13):
                yy=round(row*cell.height/13)
                d.line((0,yy,cell.width,yy),fill=(125,137,143),width=3)
                d.line((0,yy+3,cell.width,yy+3),fill=(187,192,185),width=2)
        house.paste(cell,(round(x*1024),round(y*1024)))
    save(house,MODEL/'house_02_materials.png')
    for size in (128,256):save(house.resize((size,size),Image.Resampling.LANCZOS),MODEL/f'house_02_native_{size}.png')
    # New dirt is carried into the existing fitted feather roles and orientation.
    dirt=finish(1024,(139,111,83),'dirt',201).convert('RGBA')
    for kind in ('edge','end','corner'):
        w,h=(1024,512) if kind=='edge' else (512,512)
        im=dirt.resize((w,h),Image.Resampling.LANCZOS)
        pixels=[]
        for y in range(h):
            for x in range(w):
                u,v=x/(w-1),y/(h-1)
                if kind=='edge':
                    a=max(0,min(1,(.92-u)/.55))*min(1,v*12,(1-v)*12)
                elif kind=='end':a=max(0,min(1,(.48-math.hypot(u-.5,v-.5))/.19))
                else:a=max(0,min(1,(max(u,v)-.22)/.65))*min(1,u*10,v*10)
                a=a*a*(3-2*a)
                pixels.append((*im.getpixel((x,y))[:3],round(a*255)))
        im.putdata(pixels);save(im,ROOT/f'textures/decals/path_{kind}_master.png')
        save(im.resize((256,128) if kind=='edge' else (128,128),Image.Resampling.LANCZOS),ROOT/f'textures/decals/path_{kind}_01.png')
    # Fitted barrier face is a real red/cream striped sheet, behind actual stock.
    p=MODEL/'road_barrier_master.png';im=Image.open(p).convert('RGB')
    face=Image.new('RGB',(512,512),(226,220,202));d=ImageDraw.Draw(face)
    for x in range(-512,1024,150): d.polygon([(x,511),(x+220,0),(x+290,0),(x+70,511)],fill=(150,64,47))
    im.paste(face,(0,0))
    save(im,p);save(im.resize((256,256),Image.Resampling.LANCZOS),MODEL/'road_barrier.png')


if __name__=='__main__': main()
