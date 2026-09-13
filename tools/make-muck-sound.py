# -*- coding: utf-8 -*-
"""Generate `assets/sounds/muck.wav`, the sound of a hand mucked at a showdown.

D-050. PokerTH has no muck and therefore no sound for one, so this client makes
its own. The owner found the first one too aggressive -- it hit at once, and
most of its energy was a low tap under 300 Hz -- and chose, from four softer
candidates in the character of PokerTH's `dealtwocards.wav`, **B, a soft
flick**: the slow swell of a card sliding (a band of noise round 2.8 kHz), then
a gentle, darker flick at 75 ms, everything above 5 kHz rolled off, 0.22 s,
peaking at -12 dBFS. Made from nothing but a fixed seed, so the file is this
project's own work:

    python tools/make-muck-sound.py

Needs numpy. The seed's first draw belonged to candidate A, which was not
chosen, and is drawn and discarded so the chosen sound comes out as it was
heard. The format is PokerTH's action sounds' own: 16-bit stereo PCM at
44 100 Hz.
"""
import math
import os
import wave

import numpy as np

RATE = 44_100
SEED = 50_050
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'assets', 'sounds', 'muck.wav')


def bandpass(x, fc, q):
    """RBJ cookbook band-pass (constant 0 dB peak gain), sample by sample."""
    y = np.zeros_like(x)
    x1 = x2 = y1 = y2 = 0.0
    w0 = 2 * math.pi * fc / RATE
    alpha = math.sin(w0) / (2 * q)
    a0 = 1 + alpha
    b0, b2 = alpha / a0, -alpha / a0
    a1, a2 = -2 * math.cos(w0) / a0, (1 - alpha) / a0
    for i in range(len(x)):
        x0 = x[i]
        y0 = b0 * x0 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1, y2, y1 = x1, x0, y1, y0
        y[i] = y0
    return y


def lowpass(x, cutoff, width=800.0):
    X = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / RATE)
    return np.fft.irfft(X / (1.0 + np.exp((f - cutoff) / (width / 4))), n=len(x))


def swell(t, rise, fall, peak_at):
    up = np.clip(t / rise, 0, 1) ** 2
    return up * np.where(t > peak_at, np.exp(-(t - peak_at) / fall), 1.0)


def main():
    rng = np.random.default_rng(SEED)
    rng.uniform(-1, 1, int(RATE * 0.26))  # candidate A's draw, not chosen

    n = int(RATE * 0.22)
    t = np.arange(n) / RATE
    body = bandpass(rng.uniform(-1, 1, n), 2_800, 0.8) * swell(t, 0.06, 0.03, 0.07)
    after = np.clip(t - 0.075, 0, None)
    flick_env = np.where(t >= 0.075, (1 - np.exp(-after / 0.002)) * np.exp(-after / 0.022), 0.0)
    flick = bandpass(rng.uniform(-1, 1, n), 3_400, 1.4) * flick_env
    mono = lowpass(0.7 * body + 1.0 * flick, 5_000)

    # Fades so the file never clicks on or off, then -12 dBFS at the peak.
    fade_in, fade_out = int(RATE * 0.004), int(RATE * 0.012)
    mono[:fade_in] *= np.linspace(0, 1, fade_in)
    mono[-fade_out:] *= np.linspace(1, 0, fade_out)
    mono = mono / np.abs(mono).max() * 10 ** (-12.0 / 20)

    # A hair of width: the right channel a sample later.
    right = np.concatenate([[0.0], mono[:-1]])
    frames = np.stack([mono, right], axis=1)
    data = (np.clip(frames, -1, 1) * 32767).round().astype('<i2').tobytes()
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with wave.open(OUT, 'wb') as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(data)
    print(os.path.normpath(OUT), n, 'frames')


if __name__ == '__main__':
    main()
