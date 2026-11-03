"""Original offline elevator mechanism and arrival cues; repository MIT license."""
from array import array
import io
import math
import random
import sys
import wave

RATE = 44100


def render(name):
    seconds = 5.6 if name == 'elevator_shaft' else 1.5
    rng = random.Random(20261103)
    low = slow = 0.0
    values = []
    for frame in range(round(seconds * RATE)):
        t = frame / RATE
        low += 0.055 * (rng.uniform(-1, 1) - low)
        slow += 0.006 * (low - slow)
        if name == 'elevator_shaft':
            # A passing traction motor, pulley rumble and quiet cable resonance.
            envelope = math.sin(math.pi * t / seconds) ** 1.4
            motor = math.sin(math.tau * (43 * t + 1.8 * t * t))
            bearing = math.sin(math.tau * (127 * t + 0.8 * math.sin(t)))
            cable = sum(math.exp(-((t - at) / .10) ** 2) for at in (1.1, 2.7, 4.2))
            value = envelope * (.075 * motor + .018 * bearing + .72 * slow + .24 * (low - slow))
            value += .025 * cable * math.sin(math.tau * 670 * t) * envelope
        else:
            envelope = (1 - math.exp(-t * 90)) * math.exp(-t * 3.7)
            value = envelope * (.19 * math.sin(math.tau * 880 * t) + .055 * math.sin(math.tau * 1763 * t))
            value *= min(1.0, (seconds - t) / .12)
        values.append(value)
    pcm = array('h', (round(max(-.8, min(.8, v)) * 32767) for v in values))
    if sys.byteorder != 'little':
        pcm.byteswap()
    out = io.BytesIO()
    with wave.open(out, 'wb') as wav:
        wav.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        wav.writeframes(pcm.tobytes())
    return out.getvalue()
