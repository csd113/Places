#!/usr/bin/env python3
"""Explicit offline asset authoring; never called by runtime or Cargo."""
from pathlib import Path
import zipfile,wave,io,math,struct,json,hashlib
import argparse
parser=argparse.ArgumentParser(description='Convert the attributed MIT KEMAR diffuse archive into Places fixed filters.')
parser.add_argument('--archive',type=Path,required=True)
parser.add_argument('--out',type=Path,required=True)
args=parser.parse_args()
O=args.out
O.mkdir(parents=True,exist_ok=True)
z=zipfile.ZipFile(args.archive); records=[]
for name in sorted(z.namelist()):
 if not name.endswith('.wav'):continue
 stem=Path(name).stem; elevation=int(stem[1:stem.index('e')]); azimuth=int(stem[stem.index('e')+1:-1]);e=math.radians(elevation);a=math.radians(azimuth)
 with wave.open(io.BytesIO(z.read(name))) as w:
  assert (w.getframerate(),w.getnframes(),w.getnchannels(),w.getsampwidth())==(44100,128,2,2)
  raw=struct.unpack('<256h',w.readframes(128)); channels=[raw[ear::2] for ear in range(2)]
 # Windowed-sinc resampling of the complete 128-tap response, preserving interaural delay.
 taps=[]
 for j in range(140):
  t=j*44100/48000
  for ear in range(2):
   v=0
   for k in range(max(0,math.ceil(t-16)),min(128,math.floor(t+16)+1)):
    d=t-k; sinc=1 if abs(d)<1e-12 else math.sin(math.pi*d)/(math.pi*d)
    win=0.5+0.5*math.cos(math.pi*d/16)
    v+=channels[ear][k]/32768*sinc*win
   taps.append(v*44100/48000)
 records.append((math.sin(a)*math.cos(e),math.sin(e),math.cos(a)*math.cos(e),taps))
blob=b''.join(struct.pack('<283f',x,y,zz,*t) for x,y,zz,t in records)
(O/'kemar.bin').write_bytes(blob)
(O/'KEMAR.txt').write_text('MIT KEMAR measured head-related impulse responses\nBill Gardner and Keith Martin, MIT Media Laboratory, 1994.\nhttps://sound.media.mit.edu/resources/KEMAR.html\nhttps://sound.media.mit.edu/resources/KEMAR/diffuse.zip\n\nThis data is Copyright 1994 by the MIT Media Laboratory. It is provided free with no restrictions on use, provided the authors are cited when the data is used in any research or commercial application.\n\nPlaces uses the complete diffuse-field equalized 128-tap responses (44.1 kHz), resampled offline to 140 taps at 48 kHz using a 32-point Hann-windowed sinc. Azimuths 0..180 are mirrored by exchanging ears. Coordinates: +X right, +Y up, +Z forward. Linear weighted interpolation among three nearby measured directions happens on the game thread; coefficients crossfade in the callback. No personalized HRTF or below -40 degree measurement is claimed.\nSource archive SHA256: '+hashlib.sha256((args.archive).read_bytes()).hexdigest()+'\nDerived little-endian binary: '+str(len(records))+' records, each 3 direction floats and 140 stereo float taps.\nSHA256: '+hashlib.sha256(blob).hexdigest()+'\n')
print(len(records),len(blob),hashlib.sha256(blob).hexdigest())
