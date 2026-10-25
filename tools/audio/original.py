"""Original Places ambience, rendered offline to committed PCM16 WAV assets.

No recordings, external music, runtime synthesis or external dependencies.
All frequencies/envelopes are deterministic. Copyright 2026 Places contributors;
distributed under the repository MIT license. Recipes are authoring tools only.
"""
from array import array
import io
import math
import random
import sys
import wave

RATE = 44_100


def render(name):
    seconds = {"fluorescent": 8, "pool": 24, "empty_room": 30}[name]
    rng = random.Random(1300)
    low = slow = 0.0
    values = []
    for frame in range(seconds * RATE):
        t = frame / RATE
        noise = rng.uniform(-1, 1)
        low += .06 * (noise - low)
        slow += .007 * (low - slow)
        if name == "fluorescent":
            # A quiet stable ballast hum, with no crackle or failing-light clicks.
            value = (.16 * math.sin(math.tau * 100 * t)
                     + .035 * math.sin(math.tau * 200 * t)
                     + .012 * math.sin(math.tau * 300 * t) + .035 * low)
            value *= .92 + .08 * math.sin(math.tau * t / seconds)
        elif name == "pool":
            # Soft surface wash and three rounded, irregular little splashes.
            wash = .10 + .045 * math.sin(math.tau * 3 * t / seconds)
            wash += .025 * math.sin(math.tau * 7 * t / seconds)
            splash = sum(.34 * math.exp(-((t - at) / width) ** 2)
                         for at, width in ((4.2, .20), (11.8, .32), (20.1, .24)))
            value = (low - slow) * (wash + splash) * 2.5 + slow * .2
        else:
            # Sparse, soft A-minor upper partials: warm/empty rather than subbass
            # drones, dissonance, dramatic rises or sudden musical stingers.
            envelope = sum(math.exp(-((t - at) / 3.5) ** 2) for at in (9.0, 23.0))
            value = envelope * sum(amp * math.sin(math.tau * round(hz * seconds) / seconds * t)
                                   for hz, amp in ((220, .075), (261.63, .055), (329.63, .035)))
            value += slow * .09
        values.append(value)
    # Circular overlap removes the noise seam without a recurring fade-to-zero.
    overlap = RATE // 2
    joined = [values[-overlap + i] * (1 - i / (overlap - 1))
              + values[i] * i / (overlap - 1) for i in range(overlap)]
    values = values[overlap:-overlap] + joined
    pcm = array("h", (round(max(-.9, min(.9, value)) * 32767) for value in values))
    if sys.byteorder != "little":
        pcm.byteswap()
    out = io.BytesIO()
    with wave.open(out, "wb") as wav:
        wav.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        wav.writeframes(pcm.tobytes())
    return out.getvalue()
