"""Offline GIMP 3 Python batch: original crowbar albedo and translucent HUD art.

Run through python-fu-eval with PLACES_ART_ROOT pointing at the repository.
Uses GIMP selections/layers/export, never runtime texture generation.
"""
from pathlib import Path
import math
import os
import random
from gi.repository import Gimp, Gegl, Gio

ROOT = Path(os.environ['PLACES_ART_ROOT'])
OUT = ROOT / 'assets/shared/tools'
SOURCE = ROOT / 'tools/player/source'
SOURCE.mkdir(exist_ok=True)

def color(rgb):
    return Gegl.Color.new('#%02x%02x%02x' % tuple(rgb))

def layer(image, name, opacity=100):
    value = Gimp.Layer.new(image, name, image.get_width(), image.get_height(),
                          Gimp.ImageType.RGBA_IMAGE, opacity, Gimp.LayerMode.NORMAL)
    image.insert_layer(value, None, 0)
    return value

def fill(image, drawable, rgb):
    Gimp.context_set_foreground(color(rgb))
    drawable.edit_fill(Gimp.FillType.FOREGROUND)

def rect(image, drawable, x, y, w, h, rgb):
    image.select_rectangle(Gimp.ChannelOps.REPLACE, x, y, w, h)
    fill(image, drawable, rgb)

def polygon(image, drawable, points, rgb):
    image.select_polygon(Gimp.ChannelOps.REPLACE, [float(v) for p in points for v in p])
    fill(image, drawable, rgb)

def png(image, path):
    Gimp.Selection.none(image)
    proc = Gimp.get_pdb().lookup_procedure('file-png-export')
    config = proc.create_config()
    for key, value in [('run-mode', Gimp.RunMode.NONINTERACTIVE), ('image', image),
                       ('file', Gio.File.new_for_path(str(path))), ('compression', 9),
                       ('include-color-profile', False)]:
        config.set_property(key, value)
    assert proc.run(config).index(0) == Gimp.PDBStatusType.SUCCESS

Gimp.context_set_antialias(True)
Gimp.context_set_feather(False)
image = Gimp.Image.new(512, 512, Gimp.ImageBaseType.RGB)
base = layer(image, 'Forged steel and oxblood enamel')
rect(image, base, 0., 0., 512., 384., (90, 30, 27))
rect(image, base, 0., 384., 512., 128., (112, 119, 119))
grain = layer(image, 'Rolled steel grain and enamel variation', 48)
rng = random.Random(14001110)
for _ in range(1800):
    x, y = rng.randrange(512), rng.randrange(512)
    c = (90, 30, 27) if y < 384 else (112, 119, 119)
    d = rng.choice([-15, -9, 9, 15])
    rect(image, grain, float(x), float(y), float(rng.randrange(1, 3)),
         float(min(rng.randrange(2, 9), 512-y)), tuple(v+d for v in c))
wear = layer(image, 'Restrained longitudinal handling wear', 72)
for _ in range(80):
    x, y = rng.randrange(5, 507), rng.randrange(10, 370)
    polygon(image, wear, [(x,y),(x+1,y-4),(x+2,y+5),(x,y+rng.randrange(6,14))], (117,116,107))
for x in (2, 64, 128, 192, 256, 320, 384, 448, 509):
    rect(image, wear, float(x), 0., 1., 382., (115,58,48))
Gimp.Selection.none(image)
assert Gimp.file_save(Gimp.RunMode.NONINTERACTIVE, image, Gio.File.new_for_path(str(SOURCE/'crowbar.xcf')))
png(image, OUT/'crowbar.png')
image.delete()

# Symmetric original heart; thin opaque rim around a translucent white body.
# Tint and HP splitting remain in the UI, so partial hearts stay exact.
heart = Gimp.Image.new(256, 256, Gimp.ImageBaseType.RGB)
points = []
for i in range(256):
    t = i * math.tau / 256
    points.append((128 + 6.5*16*math.sin(t)**3,
                   113 - 6.5*(13*math.cos(t)-5*math.cos(2*t)-2*math.cos(3*t)-math.cos(4*t))))
rim = layer(heart, 'Fine luminous edge', 88)
polygon(heart, rim, points, (255,255,255))
heart.select_polygon(Gimp.ChannelOps.REPLACE, [v for p in points for v in p])
Gimp.Selection.shrink(heart, 7)
rim.edit_clear()
body = layer(heart, 'Translucent centre', 34)
fill(heart, body, (255,255,255))
Gimp.Selection.none(heart)
assert Gimp.file_save(Gimp.RunMode.NONINTERACTIVE, heart, Gio.File.new_for_path(str(SOURCE/'health-heart.xcf')))
png(heart, SOURCE/'health-heart-master.png')
heart.scale(64,64)
icon = ROOT/'assets/shared/ui/health-heart.png'
png(heart, icon)
heart.delete()
# Pack the exported cell with pack_health_icon.py. GIMP compositing can
# normalize RGB under transparent pixels in unrelated font cells.
print('GIMP_TOOLS_COMPLETE', flush=True)
