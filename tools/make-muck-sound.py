# -*- coding: utf-8 -*-
"""Generate `assets/sounds/muck.wav`, the sound of a hand mucked at a showdown.

D-050. PokerTH has no muck and therefore no sound for one, so this client makes
its own: two cards slid face down into the muck -- a short swish of filtered
noise whose band falls as it fades, and a soft tap when the cards land. Made
here from nothing but a fixed seed, so the file is this project's own work and
anybody can make the same bytes again:

    python tools/make-muck-sound.py

The format is PokerTH's action sounds' own: 16-bit stereo PCM at 44 100 Hz, and
about as loud as `fold.wav` (peak near -7 dBFS).
"""
import math
import os
import random
import struct
import wave

RATE = 44_100
LENGTH_S = 0.24
SEED = 50_050
PEAK_DBFS = -7.0

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'assets', 'sounds', 'muck.wav')


def bandpass(fc, q):
    """RBJ cookbook band-pass (constant 0 dB peak gain), as normalised coefficients."""
    w0 = 2 * math.pi * fc / RATE
    alpha = math.sin(w0) / (2 * q)
    b0, b1, b2 = alpha, 0.0, -alpha
    a0, a1, a2 = 1 + alpha, -2 * math.cos(w0), 1 - alpha
    return b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0


def main():
    rng = random.Random(SEED)
    n = int(RATE * LENGTH_S)
    x1 = x2 = y1 = y2 = 0.0
    swish = []
    for i in range(n):
        t = i / RATE
        # The band slides down from about 5.5 kHz to 1.8 kHz as the cards slide.
        fc = 1_800 + 3_700 * math.exp(-t / 0.07)
        b0, b1, b2, a1, a2 = bandpass(fc, 0.9)
        x0 = rng.uniform(-1.0, 1.0)
        y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1, y2, y1 = x1, x0, y1, y0
        attack = min(1.0, t / 0.006)
        decay = math.exp(-t / 0.055)
        swish.append(y0 * attack * decay)
    # The cards landing: a short, low, damped tap at 150 ms.
    tap = []
    for i in range(n):
        t = i / RATE - 0.150
        if t < 0:
            tap.append(0.0)
            continue
        tap.append(0.35 * math.sin(2 * math.pi * 190 * t) * math.exp(-t / 0.018))
    mono = [s + k for s, k in zip(swish, tap)]
    # A few milliseconds of fade at the end, so the file never clicks off.
    fade = int(RATE * 0.008)
    for i in range(fade):
        mono[n - 1 - i] *= i / fade
    peak = max(abs(s) for s in mono) or 1.0
    gain = (10 ** (PEAK_DBFS / 20)) / peak
    frames = bytearray()
    for i, s in enumerate(mono):
        v = int(round(max(-1.0, min(1.0, s * gain)) * 32_767))
        # A hair of width: the right channel a sample later.
        r = int(round(max(-1.0, min(1.0, mono[i - 1] * gain if i else 0.0)) * 32_767))
        frames += struct.pack('<hh', v, r)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with wave.open(OUT, 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(frames))
    print(os.path.normpath(OUT), n, 'frames')


if __name__ == '__main__':
    main()
