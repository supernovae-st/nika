#!/usr/bin/env python3
"""Nika · From intent to proof — procedural score and sound design.

Reads the SAME timeline the picture uses (.cache/timeline.json, dumped by
render.mjs from src/timeline.mjs) and synthesizes every sound at the exact
cue time, so picture and sound cannot drift. 120 BPM, D minor; the film
lifts to D major when the real world changes.

Sound grammar:
  intelligence  glassy FM, high register, short
  proof         precise micro-clicks, rising per check
  unknown       warm amber divergence (a tritone beating), never an alarm
  closure       three detuned tones phase-locking into unison
  human         soft, low, matte electric-piano tones — distinct from machines
  runtime       dry mechanical steps and clacks on the grid
  effect        one heavy, elegant hit — exactly once

usage: score.py <timeline.json> <out.wav>
"""
import json
import sys

import numpy as np
from scipy import signal
from scipy.io import wavfile

SR = 48000
rng = np.random.default_rng(20260928)


def n(sec):
    return int(round(sec * SR))


class Bus:
    def __init__(self, dur):
        self.x = np.zeros((n(dur) + SR * 4, 2), dtype=np.float64)

    def add(self, t0, mono, gain=1.0, pan=0.0):
        """Add a mono signal at time t0 with constant-power pan (-1..1)."""
        i = n(t0)
        if i < 0:
            mono = mono[-i:]
            i = 0
        j = min(i + len(mono), len(self.x))
        if j <= i:
            return
        a = (pan + 1) * np.pi / 4
        self.x[i:j, 0] += mono[: j - i] * gain * np.cos(a)
        self.x[i:j, 1] += mono[: j - i] * gain * np.sin(a)

    def add_st(self, t0, st, gain=1.0):
        i = n(t0)
        j = min(i + len(st), len(self.x))
        self.x[i:j] += st[: j - i] * gain


# ── oscillators & envelopes ────────────────────────────────────────────
def tt(dur):
    return np.arange(n(dur)) / SR


def env_ad(dur, a=0.005, d=None, curve=4.0):
    t = tt(dur)
    d = d or dur
    atk = np.clip(t / max(a, 1e-4), 0, 1)
    dec = np.exp(-curve * np.clip(t - a, 0, None) / d)
    return atk * dec


def env_asr(dur, a, r):
    t = tt(dur)
    e = np.clip(t / max(a, 1e-4), 0, 1) * np.clip((dur - t) / max(r, 1e-4), 0, 1)
    return e ** 1.5


def saw(f, dur, detune=0.0, harmonics=24):
    t = tt(dur)
    out = np.zeros_like(t)
    f = f * (1 + detune)
    for k in range(1, harmonics + 1):
        if f * k > 9000:
            break
        out += np.sin(2 * np.pi * f * k * t + rng.uniform(0, 6.28)) / k
    return out * 0.6


def tri(f, dur):
    t = tt(dur)
    return 2 / np.pi * np.arcsin(np.sin(2 * np.pi * f * t))


def fm(fc, ratio, index, dur, idx_decay=6.0):
    t = tt(dur)
    I = index * np.exp(-idx_decay * t)
    return np.sin(2 * np.pi * fc * t + I * np.sin(2 * np.pi * fc * ratio * t))


def noise(dur):
    return rng.standard_normal(n(dur))


def bp(x, lo, hi, order=2):
    sos = signal.butter(order, [lo, hi], btype="band", fs=SR, output="sos")
    return signal.sosfilt(sos, x)


def hp(x, f, order=2):
    return signal.sosfilt(signal.butter(order, f, btype="high", fs=SR, output="sos"), x)


def lp(x, f, order=2):
    return signal.sosfilt(signal.butter(order, f, btype="low", fs=SR, output="sos"), x)


def sweep_lp(x, f0, f1, block=256):
    """Time-varying low-pass (exponential cutoff sweep), block-wise."""
    out = np.zeros_like(x)
    nb = int(np.ceil(len(x) / block))
    zi = None
    for b in range(nb):
        u = b / max(1, nb - 1)
        fc = f0 * (f1 / f0) ** u
        sos = signal.butter(2, min(fc, SR * 0.45), btype="low", fs=SR, output="sos")
        if zi is None:
            zi = signal.sosfilt_zi(sos) * 0
        seg = x[b * block:(b + 1) * block]
        y, zi = signal.sosfilt(sos, seg, zi=zi)
        out[b * block:(b + 1) * block] = y
    return out


def sweep_bp(x, f0, f1, q=2.0, block=256):
    out = np.zeros_like(x)
    nb = int(np.ceil(len(x) / block))
    zi = None
    for b in range(nb):
        u = b / max(1, nb - 1)
        fc = f0 * (f1 / f0) ** u
        lo, hi = fc / (1 + 1 / q), min(fc * (1 + 1 / q), SR * 0.45)
        sos = signal.butter(1, [lo, hi], btype="band", fs=SR, output="sos")
        if zi is None:
            zi = signal.sosfilt_zi(sos) * 0
        y, zi = signal.sosfilt(sos, x[b * block:(b + 1) * block], zi=zi)
        out[b * block:(b + 1) * block] = y
    return out


# ── instruments ────────────────────────────────────────────────────────
def kick(amp=1.0):
    dur = 0.6
    t = tt(dur)
    f = 52 + 95 * np.exp(-t / 0.04)
    ph = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(ph) * np.exp(-t / 0.16)
    click = hp(noise(0.004), 2500) * 0.5
    body[: len(click)] += click
    return np.tanh(body * 1.6) * amp * 0.62


def hat(amp=1.0, open_=False):
    dur = 0.25 if open_ else 0.06
    x = hp(noise(dur), 7500, 4) * env_ad(dur, 0.0005, 0.12 if open_ else 0.022, 1.0)
    return x * amp * 0.5


# D minor pentatonic, the score's key: every pitched effect snaps into it
KEY_PCS = (2, 5, 7, 9, 0)  # D F G A C


def key_hz(freq):
    m = 69 + 12 * np.log2(freq / 440.0)
    best = min((round(m) + d for d in range(-3, 4)), key=lambda k: (k % 12 not in KEY_PCS, abs(k - m)))
    return 440.0 * 2 ** ((best - 69) / 12)


def tick(freq=4200, amp=1.0, dur=0.06):
    # a soft, pitched "tink" in key: rounded 1.5 ms attack, no bare
    # broadband click, so repeated ticks sit under the music
    t = tt(dur)
    atk = 0.5 - 0.5 * np.cos(np.pi * np.clip(t / 0.0015, 0, 1))
    rel = np.clip((dur - t) / 0.004, 0, 1)  # taper the tail: a short dur must not end on a click
    x = np.sin(2 * np.pi * key_hz(freq) * t) * np.exp(-t / 0.012)
    x += lp(hp(noise(dur), 2500), 7000) * np.exp(-t / 0.003) * 0.12
    return x * atk * rel * amp


def pluck(freq, amp=1.0, dur=0.9, bright=1.0):
    x = fm(freq, 2.0, 2.2 * bright, dur, 9) * env_ad(dur, 0.002, dur * 0.5, 3.5)
    x += 0.35 * np.sin(2 * np.pi * freq * 2 * tt(dur)) * env_ad(dur, 0.002, 0.12, 3)
    return x * amp


def bell(freq, amp=1.0, dur=2.2):
    x = fm(freq, 3.5, 3.0, dur, 3.0) * env_ad(dur, 0.002, dur * 0.45, 4)
    x += 0.4 * fm(freq * 2.01, 1.41, 1.0, dur, 4.0) * env_ad(dur, 0.002, dur * 0.25, 4)
    return x * amp


def epiano(freq, amp=1.0, dur=1.6):
    """Human timbre: soft, low-index FM with a slow tine — matte, warm."""
    x = fm(freq, 1.0, 1.2, dur, 2.5) * env_ad(dur, 0.006, dur * 0.6, 3)
    x += 0.25 * fm(freq * 4, 1.0, 0.6, dur, 8) * env_ad(dur, 0.002, 0.2, 4)
    return lp(x, 3200) * amp


def pad(freqs, dur, amp=1.0, a=0.6, r=0.8, cutoff=(400, 2600)):
    out = np.zeros(n(dur))
    for f in freqs:
        for d in (-0.004, 0.0, 0.0045):
            out += saw(f, dur, d, 16)
    out /= len(freqs) * 3
    out = sweep_lp(out, cutoff[0], cutoff[1])
    return out * env_asr(dur, a, r) * amp


def sub(freq, dur, amp=1.0, a=0.02, r=0.3):
    t = tt(dur)
    x = np.sin(2 * np.pi * freq * t) + 0.18 * np.sin(2 * np.pi * freq * 2 * t)
    return np.tanh(x * 1.2) * env_asr(dur, a, r) * amp


def bassnote(freq, dur, amp=1.0):
    x = saw(freq, dur, 0, 12)
    x = lp(x, 520)
    return np.tanh(x * 2) * env_ad(dur, 0.004, dur * 0.8, 2.5) * amp


def whoosh(dur, up=True, amp=1.0):
    x = noise(dur)
    x = sweep_bp(x, 300 if up else 6000, 6000 if up else 300, 1.5)
    e = env_asr(dur, dur * (0.75 if up else 0.15), dur * (0.2 if up else 0.8))
    return x * e * amp * 1.8


def impact(amp=1.0, dur=2.6, f0=62, f1=30):
    f1 = max(f1, 40)
    f0 = max(f0, 68)
    t = tt(dur)
    f = f1 + (f0 - f1) * np.exp(-t / 0.35)
    boom = 0.72 * np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-t / 0.75)
    crack = lp(noise(dur), 2200) * np.exp(-t / 0.12) * 0.6
    air = hp(noise(dur), 5000) * np.exp(-t / 0.05) * 0.15
    return np.tanh((boom * 1.2 + crack + air) * 1.1) * amp


def reverse_swell(dur, amp=1.0):
    x = bp(noise(dur), 800, 9000) * (tt(dur) / dur) ** 3
    x += 0.5 * np.sin(2 * np.pi * np.linspace(220, 880, n(dur)).cumsum() / SR) * (tt(dur) / dur) ** 4
    return x * amp


def high_shelf(f0, gain_db, q=0.707):
    """RBJ cookbook high-shelf biquad."""
    A = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * f0 / SR
    alpha = np.sin(w0) / (2 * q)
    cw = np.cos(w0)
    b0 = A * ((A + 1) + (A - 1) * cw + 2 * np.sqrt(A) * alpha)
    b1 = -2 * A * ((A - 1) + (A + 1) * cw)
    b2 = A * ((A + 1) + (A - 1) * cw - 2 * np.sqrt(A) * alpha)
    a0 = (A + 1) - (A - 1) * cw + 2 * np.sqrt(A) * alpha
    a1 = 2 * ((A - 1) - (A + 1) * cw)
    a2 = (A + 1) - (A - 1) * cw - 2 * np.sqrt(A) * alpha
    return np.array([b0, b1, b2]) / a0, np.array([1, a1 / a0, a2 / a0])


# ── reverb ─────────────────────────────────────────────────────────────
def reverb_ir(rt60=2.4, pre=0.018):
    dur = rt60 * 1.1
    t = tt(dur)
    decay = np.exp(-6.9 * t / rt60)
    irl = rng.standard_normal(len(t)) * decay
    irr = rng.standard_normal(len(t)) * decay
    # darker tail
    irl, irr = lp(irl, 5200), lp(irr, 5200)
    pad_ = np.zeros(n(pre))
    ir = np.stack([np.concatenate([pad_, irl]), np.concatenate([pad_, irr])], axis=1)
    return ir / np.sqrt(np.sum(ir ** 2))


# ── the score ──────────────────────────────────────────────────────────
D, F, A, C5 = 587.33, 698.46, 880.0, 1046.5
SCALE = [587.33, 698.46, 783.99, 880.0, 1046.5, 1174.66, 1396.9, 1567.98]
CHORDS = {
    "Dm": [146.83, 174.61, 220.0, 293.66],
    "Bb": [116.54, 146.83, 174.61, 233.08],
    "F": [130.81, 174.61, 220.0, 261.63],
    "C": [130.81, 164.81, 196.0, 261.63],
    "D": [146.83, 185.0, 220.0, 293.66, 329.63],
}
ROOT = {"Dm": 73.42, "Bb": 58.27, "F": 87.31, "C": 65.41, "D": 73.42}
# one chord per bar (15 bars of 2 s)
BARS = ["Dm", "Dm", "Dm", "Bb", "F", "C", "Dm", "Bb", "F", "C", "Dm", "F", "C", "Bb", "D"]


def main(tl_path, out_path):
    tl = json.load(open(tl_path))
    dur = tl["duration"]
    T = tl["T"]
    beat = 60 / tl["bpm"]
    dry, wet, bed = Bus(dur), Bus(dur), Bus(dur)

    # ── harmonic bed: pads per bar, sub pedal (sidechained to the kick) ──
    # levels per bar: a quiet cold open, a lift for the plan, a breath for
    # the human moment, full weight for execution and the final chord
    LEVEL = [0.035, 0.06, 0.13, 0.15, 0.17, 0.22, 0.14, 0.13, 0.13, 0.15, 0.06, 0.17, 0.19, 0.2, 0.26]
    SUB = [0, 0, 0.1, 0.1, 0.12, 0.13, 0.12, 0.08, 0.1, 0.1, 0, 0.14, 0.14, 0.13, 0.17]
    for b, ch in enumerate(BARS):
        t0 = b * 2
        amp = LEVEL[b]
        cut = (300, 1200) if b < 2 else (500, 3000) if b in (5, 12, 13, 14) else (420, 2200)
        p = pad(CHORDS[ch], 2.4 if b < 14 else 3.9, amp, a=0.35 if b else 1.2, r=0.6 if b < 14 else 2.8, cutoff=cut)
        wet.add(t0, p, 0.5, -0.25)
        bed.add(t0, p, 0.5, 0.25)
        if SUB[b]:
            bed.add(t0, sub(ROOT[ch] if ch != "D" else 73.42, 2.0 if b < 14 else 3.2, SUB[b], 0.03, 0.4), 1.0)
    kicks = []

    # ── rhythm bed ──────────────────────────────────────────────────────
    def section(name):
        for s in tl["sections"]:
            if s["id"] == name:
                return s["a"], s["b"]
        return 0, 0

    grid = np.arange(0, dur, beat / 4)  # 16th grid
    for g in grid:
        bi = int(round(g / beat * 4))  # 16th index
        on_beat = bi % 4 == 0
        in_ = lambda nm: section(nm)[0] <= g < section(nm)[1]
        if in_("observe") or in_("plan"):
            if bi % 8 == 0:
                dry.add(g, kick(0.85))
                kicks.append(g)
            if bi % 2 == 0:
                dry.add(g, hat(0.18 if bi % 4 else 0.1), 1, 0.3 if bi % 8 else -0.3)
            if in_("plan") and bi % 4 == 2:
                dry.add(g, bassnote(ROOT[BARS[int(g // 2)]] * 2, 0.22, 0.22), 1, -0.1)
        elif in_("prove"):
            if on_beat and not (T["unknown"] <= g < T["returned"]):
                dry.add(g, kick(0.8))
                kicks.append(g)
            dry.add(g, hat(0.12 if bi % 2 else 0.2), 1, 0.35 if bi % 2 else -0.35)
        elif in_("lower"):
            if bi % 8 == 0 and g < T["closure"]:
                dry.add(g, kick(0.8))
                kicks.append(g)
        elif in_("run"):
            if on_beat:
                dry.add(g, kick(0.95))
                kicks.append(g)
            dry.add(g, hat(0.22 if bi % 2 else 0.12, bi % 8 == 6), 1, 0.4 if bi % 2 else -0.4)
            if bi % 2 == 0 and g < T["reveal"]:
                dry.add(g, bassnote(ROOT[BARS[int(g // 2)]] * 2, 0.2, 0.26), 1, 0.0)
        elif in_("reveal"):
            # an accelerating roll into the final impact
            if bi % 2 == 0 or g > T["title"] - 1.0:
                u = (g - section("reveal")[0]) / 2.0
                dry.add(g, hat(0.08 + 0.2 * u), 1, 0.3 * np.sin(g * 9))
            if on_beat:
                dry.add(g, kick(0.55 + 0.3 * (g - section("reveal")[0]) / 2))
                kicks.append(g)

    # ── events (the same cues as the picture) ───────────────────────────
    for ev in tl["cues"]:
        t0, k = ev["t"], ev["kind"]
        i = ev.get("i", ev.get("step", 0))
        if k == "impact_low":
            dry.add(t0, impact(0.42, 2.2, 58, 30))
            wet.add(t0, impact(0.25, 2.2, 58, 30))
        elif k == "data_tone":
            x = np.sin(2 * np.pi * 2793.8 * tt(2.0)) * env_ad(2.0, 0.3, 1.2, 3) * 0.05
            wet.add(t0 + 0.05, x, 1, 0.4)
        elif k == "word_tick":
            dry.add(t0, tick(3600 + 180 * (i % 5), 0.085), 1, -0.3 + 0.12 * (i % 6))
            wet.add(t0, tick(3600 + 180 * (i % 5), 0.03), 1, -0.3 + 0.12 * (i % 6))
        elif k == "chirp":
            wet.add(t0, bell(1567.98, 0.12, 1.0), 1, 0.3)
            dry.add(t0, tick(5200, 0.2), 1, 0.3)
        elif k == "scan_sweep":
            x = whoosh(ev["dur"], True, 0.14)
            dry.add(t0, x, 1, -0.5)
            wet.add(t0, x, 0.6, 0.5)
        elif k == "reverse_swell":
            wet.add(t0, reverse_swell(ev["dur"], 0.3), 1, 0)
        elif k == "shatter":
            for s in range(16):
                f = rng.uniform(2200, 7200)
                dt = rng.uniform(0, 0.28) ** 1.6
                wet.add(t0 + dt, bell(f, 0.05, 0.6), 1, rng.uniform(-0.9, 0.9))
                dry.add(t0 + dt, tick(f, 0.1, 0.03), 1, rng.uniform(-0.9, 0.9))
            dry.add(t0, impact(0.35, 1.2, 90, 40))
            dry.add(t0, hp(noise(0.4), 3000) * env_ad(0.4, 0.001, 0.08, 1) * 0.25)
        elif k == "pluck":
            f = SCALE[i % len(SCALE)]
            dry.add(t0, pluck(f, 0.32), 1, -0.5 + 0.2 * i)
            wet.add(t0, pluck(f, 0.2), 1, -0.5 + 0.2 * i)
        elif k == "chain_draw":
            wet.add(t0, bell(2349.3, 0.05, 1.2), 1, 0.2)
        elif k == "riser":
            x = whoosh(ev["dur"] + 0.1, True, 0.5)
            dry.add(t0, x, 1, 0)
            wet.add(t0, x, 0.5, 0)
        elif k == "impact_mid":
            dry.add(t0, impact(0.5, 1.8, 80, 36))
            wet.add(t0, impact(0.25, 1.8, 80, 36))
        elif k == "data_storm":
            d = ev["dur"]
            for s in range(int(d * 90)):
                dt = rng.uniform(0, d)
                dry.add(t0 + dt, tick(rng.uniform(1800, 6500), rng.uniform(0.03, 0.09), 0.02), 1, rng.uniform(-1, 1))
        elif k == "lock":
            dry.add(t0, tick(3000 + 400 * i, 0.3), 1, -0.3 + 0.2 * i)
            wet.add(t0, pluck(SCALE[i + 1] / 2, 0.22, 0.7), 1, -0.3 + 0.2 * i)
        elif k == "amber_hint":
            x = (tri(415.3, 1.2) * 0.6 + tri(293.66, 1.2) * 0.6) * env_ad(1.2, 0.08, 0.6, 3) * 0.08
            wet.add(t0, lp(x, 1800), 1, 0.2)
        elif k == "ghost":
            x = np.sin(2 * np.pi * 1318.5 * tt(0.4)) * env_ad(0.4, 0.01, 0.2, 3) * (1 + 0.5 * np.sin(2 * np.pi * 23 * tt(0.4)))
            wet.add(t0, x * 0.06, 1, 0.5)
        elif k == "deny":
            x = np.concatenate([np.sin(2 * np.pi * 466.16 * tt(0.07)), np.sin(2 * np.pi * 440 * tt(0.12))])
            dry.add(t0, lp(x, 2000) * env_ad(len(x) / SR, 0.002, 0.08, 2) * 0.16, 1, 0.5)
        elif k in ("whoosh_up", "whoosh"):
            x = whoosh(0.55, True, 0.3)
            dry.add(t0 - 0.35, x, 1, 0.3 if k == "whoosh" else -0.3)
        elif k == "shimmer":
            d = ev["dur"]
            for s in range(int(d * 8)):
                f = SCALE[(s * 3) % len(SCALE)] * 2
                wet.add(t0 + s / 8, bell(f, 0.035, 0.9), 1, np.sin(s * 1.7) * 0.8)
        elif k == "rank_tick":
            dry.add(t0, tick(2400 + 150 * i, 0.18), 1, 0.5)
        elif k == "select":
            dry.add(t0, tick(5000, 0.35), 1, 0.5)
            wet.add(t0, pluck(1174.66, 0.25, 0.8), 1, 0.5)
        elif k == "socket":
            f = [587.33, 698.46, 880.0, 1046.5, 1174.66][i]
            dry.add(t0, pluck(f, 0.3, 0.9), 1, -0.6 + 0.3 * i)
            wet.add(t0, bell(f * 2, 0.07, 1.4), 1, -0.6 + 0.3 * i)
            dry.add(t0, tick(4400, 0.15), 1, -0.6 + 0.3 * i)
        elif k == "swell":
            d = ev["dur"]
            x = pad([293.66, 440.0, 587.33, 659.25], d, 0.18, a=d * 0.6, r=0.6, cutoff=(700, 5200))
            wet.add(t0, x, 1, 0)
        elif k == "spin_up":
            d = ev["dur"]
            f = np.linspace(80, 420, n(d))
            x = np.sin(2 * np.pi * np.cumsum(f) / SR) * env_asr(d, d * 0.8, 0.1) * 0.12
            x += bp(noise(d), 1500, 4000) * env_asr(d, d * 0.9, 0.05) * 0.08
            dry.add(t0, x, 1, 0)
        elif k == "check":
            # ten in a row: they count, they must not sparkle
            dry.add(t0, tick(2600 + 260 * i, 0.17), 1, -0.6 + 0.13 * i)
            wet.add(t0, tick(2600 + 260 * i, 0.06), 1, -0.6 + 0.13 * i)
            wet.add(t0, lp(bell(key_hz(1760 + 110 * i), 0.03, 0.5), 5000), 1, -0.6 + 0.13 * i)
        elif k == "check_amber":
            x = (tri(311.13, 0.5) + 0.7 * tri(329.63, 0.5)) * env_ad(0.5, 0.004, 0.22, 3) * 0.14
            dry.add(t0, lp(x, 2400), 1, 0)
        elif k == "amber_divergence":
            d = ev["dur"]
            t_ = tt(d)
            vib = 1 + 0.004 * np.sin(2 * np.pi * 4.3 * t_)
            x = np.sin(2 * np.pi * np.cumsum(293.66 * vib) / SR) + 0.8 * np.sin(2 * np.pi * np.cumsum(415.3 * vib * (1 + 0.003 * t_)) / SR)
            x = np.tanh(x * 0.9) * env_asr(d, 0.35, 0.3) * 0.1
            wet.add(t0, x, 1, -0.2)
            dry.add(t0, x, 0.5, 0.2)
        elif k == "ask":
            wet.add(t0, bell(880.0, 0.1, 1.4), 1, 0.4)
            wet.add(t0 + 0.12, bell(1244.5, 0.07, 1.4), 1, 0.4)  # a question: unresolved interval
        elif k in ("human_tap", "human_text"):
            dry.add(t0, epiano(293.66 if k == "human_tap" else 220.0, 0.3, 1.4), 1, -0.2)
            wet.add(t0, epiano(440.0, 0.12, 1.4), 1, -0.2)
        elif k == "resolve":
            for j, f in enumerate([293.66, 369.99, 440.0]):
                wet.add(t0 + j * 0.03, bell(f * 2, 0.08, 1.8), 1, -0.3 + 0.3 * j)
        elif k == "verified":
            wet.add(t0, bell(1760.0, 0.12, 1.4), 1, 0.2)
            wet.add(t0 + 0.09, bell(2349.3, 0.1, 1.6), 1, -0.2)
            dry.add(t0, tick(6000, 0.25), 1, 0)
        elif k == "tunnel":
            x = whoosh(ev["dur"] + 0.2, True, 0.55)
            dry.add(t0, x, 1, 0)
        elif k == "compress":
            d = ev["dur"]
            f = np.geomspace(1400, 60, n(d))
            x = np.sin(2 * np.pi * np.cumsum(f) / SR) * env_asr(d, 0.02, d * 0.5) * 0.18
            x += whoosh(d, False, 0.25)
            dry.add(t0, x, 1, 0)
        elif k == "type_click":
            c = lp(hp(noise(0.014), 1800), 7000) * env_ad(0.014, 0.0006, 0.004, 1)
            dry.add(t0, c * 0.2, 1, rng.uniform(-0.5, 0.5))
        elif k == "check_soft":
            dry.add(t0, tick(3200 + 200 * i, 0.15), 1, 0.4)
            wet.add(t0, tick(3200 + 200 * i, 0.05), 1, 0.4)
        elif k == "phase_lock":
            d = ev["dur"]
            t_ = tt(d + 0.8)
            p = np.clip(t_ / d, 0, 1)
            x = np.zeros_like(t_)
            for base, det in ((587.33, 0.018), (587.33, -0.013), (880.0 * 0.667, 0.02)):
                f = base * (1 + det * (1 - p) ** 2)
                x += np.sin(2 * np.pi * np.cumsum(f) / SR)
            x *= (0.25 + 0.75 * p) * env_asr(d + 0.8, 0.1, 0.8) * 0.07
            wet.add(t0, x, 1, 0)
            dry.add(t0, x, 0.6, 0)
        elif k == "lock_final":
            dry.add(t0, impact(0.4, 1.4, 70, 38))
            wet.add(t0, bell(587.33, 0.16, 2.2), 1, -0.2)
            wet.add(t0, bell(880.0, 0.12, 2.2), 1, 0.2)
            dry.add(t0, tick(4800, 0.3), 1, 0)
        elif k == "print":
            d = ev["dur"]
            for s in range(int(d / (beat / 8))):
                dry.add(t0 + s * beat / 8, tick(2000 + 90 * (s % 7), 0.1, 0.02), 1, 0.3)
        elif k == "stamp":
            dry.add(t0, impact(0.55, 1.6, 70, 34))
            dry.add(t0, hp(noise(0.08), 1200) * env_ad(0.08, 0.001, 0.03, 1) * 0.3)
            wet.add(t0, bell(1174.66, 0.08, 1.6), 1, 0.3)
        elif k == "gate_hold":
            x = lp(noise(0.3), 300) * env_ad(0.3, 0.004, 0.08, 1) * 0.5
            dry.add(t0, x, 1, 0.5)
            dry.add(t0, sub(73.42, 0.4, 0.3, 0.005, 0.3), 1, 0.4)
        elif k == "soft_tick":
            dry.add(t0, tick(2800, 0.12), 1, 0.2)
        elif k == "consent":
            for j, f in enumerate([146.83, 220.0, 293.66, 369.99, 440.0]):
                dry.add(t0 + j * 0.018, epiano(f, 0.2, 2.6), 1, -0.4 + 0.2 * j)
                wet.add(t0 + j * 0.018, epiano(f, 0.12, 2.6), 1, -0.4 + 0.2 * j)
        elif k == "gate_open":
            x = whoosh(0.9, False, 0.4)
            wet.add(t0, x, 1, 0.5)
            dry.add(t0, sub(55.0, 1.0, 0.3, 0.01, 0.8), 1, 0)
        elif k == "wave":
            dry.add(t0, tick(1600 + 220 * i, 0.3, 0.05), 1, -0.6 + 0.25 * i)
            c = bp(noise(0.08), 900, 3500) * env_ad(0.08, 0.0005, 0.02, 1)
            dry.add(t0, c * 0.5, 1, -0.6 + 0.25 * i)
        elif k == "effect":
            dry.add(t0, impact(0.95, 2.4, 70, 30))
            wet.add(t0, impact(0.45, 2.4, 70, 30))
            wet.add(t0, bell(1174.66, 0.18, 2.4), 1, 0.3)
            wet.add(t0, bell(1760.0, 0.1, 2.4), 1, -0.3)
            dry.add(t0, hp(noise(0.3), 2500) * env_ad(0.3, 0.001, 0.07, 1) * 0.35)
        elif k == "result_chord":
            for j, f in enumerate(CHORDS["D"]):
                wet.add(t0 + j * 0.022, bell(f * 2, 0.07, 2.4), 1, -0.5 + 0.25 * j)
            wet.add(t0, pad([293.66, 369.99, 440.0, 587.33], 1.8, 0.2, a=0.05, r=1.2, cutoff=(1800, 4200)), 1, 0)
        elif k == "receipt_tick":
            dry.add(t0, tick(3000 + 300 * i, 0.14), 1, 0.5)
            wet.add(t0, lp(pluck(SCALE[i % 6] * 2, 0.06, 0.5), 6000), 1, 0.5)
        elif k == "seal":
            dry.add(t0, impact(0.5, 1.8, 80, 36))
            wet.add(t0, bell(587.33 * 2, 0.14, 2.0), 1, 0)
            wet.add(t0 + 0.04, bell(880.0 * 2, 0.1, 2.0), 1, 0)
        elif k == "reveal":
            d = ev["dur"]
            x = whoosh(d, True, 0.35)
            dry.add(t0, x, 1, 0)
            f = np.geomspace(110, 880, n(d))
            s_ = np.sin(2 * np.pi * np.cumsum(f) / SR) * (tt(d) / d) ** 2 * 0.06
            wet.add(t0, s_, 1, 0)
        elif k == "principle":
            f = [587.33, 698.46, 880.0, 1174.66][i]
            dry.add(t0, pluck(f, 0.28, 1.0), 1, -0.45 + 0.3 * i)
            wet.add(t0, bell(f * 2, 0.06, 1.6), 1, -0.45 + 0.3 * i)
        elif k == "impact_final":
            dry.add(t0, impact(1.0, 3.2, 60, 26))
            wet.add(t0, impact(0.6, 3.2, 60, 26))
            for j, f in enumerate([146.83, 220.0, 293.66, 369.99, 440.0, 659.25]):
                wet.add(t0 + j * 0.015, bell(f * 2, 0.07, 3.6), 1, -0.5 + 0.2 * j)
            dry.add(t0, hp(noise(0.5), 3000) * env_ad(0.5, 0.001, 0.12, 1) * 0.3)
        else:
            print("unhandled cue", k, file=sys.stderr)

    # ── mix ─────────────────────────────────────────────────────────────
    # sidechain: the bed ducks under each kick (groove + low-end clarity)
    duck = np.ones(len(bed.x))
    tk = np.arange(len(bed.x)) / SR
    for k0 in kicks:
        i0 = n(k0)
        i1 = min(len(duck), i0 + n(0.4))
        duck[i0:i1] = np.minimum(duck[i0:i1], 1 - 0.55 * np.exp(-(tk[i0:i1] - k0) / 0.11))
    bedx = bed.x * duck[:, None]
    # never reverberate the sub: high-pass the send
    wetx = np.stack([hp(wet.x[:, c], 180, 2) for c in range(2)], axis=1)
    ir = reverb_ir(2.6)
    w = np.stack([signal.fftconvolve(wetx[:, c], ir[:, c])[: len(wetx)] for c in range(2)], axis=1)
    mix = dry.x + bedx + w * 0.9 + wetx * 0.35
    # gentle bus glue: RMS envelope compressor (ratio ~2:1 above -18 dBFS)
    rms = np.sqrt(signal.sosfilt(signal.butter(1, 8, fs=SR, output="sos"), np.mean(mix ** 2, axis=1)) + 1e-12)
    threshold = 10 ** (-18 / 20)
    gain = np.where(rms > threshold, (threshold / rms) ** 0.5, 1.0)
    mix *= gain[:, None]
    mix = mix[: n(dur)]
    # master high-pass: nothing below 32 Hz (rumble, not music)
    mix = np.stack([hp(mix[:, c], 32, 4) for c in range(2)], axis=1)
    # gentle high shelf (-3 dB above ~4 kHz): precise, not harsh
    b_, a_ = high_shelf(4000, -3.0)
    mix = np.stack([signal.lfilter(b_, a_, mix[:, c]) for c in range(2)], axis=1)
    # fade the very end to silence (the picture fades to black for the loop)
    fade = np.clip((dur - np.arange(len(mix)) / SR) / 0.35, 0, 1) ** 1.5
    mix *= fade[:, None]
    mix = np.tanh(mix * 1.1) / 1.1
    peak = np.max(np.abs(mix))
    mix = mix / peak * 10 ** (-1.5 / 20)
    wavfile.write(out_path, SR, mix.astype(np.float32))
    print(f"score: {out_path} · {dur:.2f}s · peak -1.5 dBFS · {len(tl['cues'])} cues")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
