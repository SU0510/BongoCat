#!/usr/bin/env python3
"""Generate the InSync alarm chime: `resources/insync/sounds/alarm.wav`.

The chime is a two-note bell (C6 then G5), produced from scratch so the
binary asset is reproducible from this file alone.

Determinism, not accuracy, is why the sine and the exponential here are
hand-written polynomials instead of `math.sin`/`math.exp`: the C library
versions are allowed to differ by an ULP between platforms, and a single
ULP moves one sample across a `round()` boundary, which changes the byte
stream. Everything below is IEEE-754 double arithmetic and `math.fmod` /
`math.floor` / `math.ldexp`, all of which are exactly specified, so the
same input yields the same bytes on macOS, Linux and Windows.

Usage:
    generate_alarm.py [--out <path>] [--check]
    --check regenerates in memory and byte-compares against the committed
    file instead of writing it (CI drift gate).
"""

from __future__ import annotations

import math
import struct
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO_ROOT / "resources" / "insync" / "sounds" / "alarm.wav"

SAMPLE_RATE = 44_100
DURATION_SECONDS = 0.9
PEAK_FRACTION = 0.85
ATTACK_SECONDS = 0.004
FADE_OUT_SECONDS = 0.03
LOG2_E = 1.4426950408889634
LN_2 = 0.6931471805599453
TWO_PI = 6.283185307179586
PI = 3.141592653589793
HALF_PI = 1.5707963267948966

# (start seconds, frequency Hz, decay tau, amplitude)
# Frequencies are exact equal-tempered values; the amplitudes keep the two
# notes audible together where they overlap.
NOTES = (
    (0.0, 1046.5019837325143, 0.20, 0.60),  # C6
    (0.24, 783.9908544035725, 0.26, 0.55),  # G5
)
# A bell is not a pure tone: the upper partials are what make it read as a
# chime rather than a test signal.
PARTIALS = ((1.0, 1.0), (2.0, 0.28), (3.0, 0.10))


def sin(x: float) -> float:
    """Deterministic sine: exact range reduction plus a Taylor series."""
    x = math.fmod(x, TWO_PI)
    if x > PI:
        x -= TWO_PI
    elif x < -PI:
        x += TWO_PI
    # exp(-x) style Horner evaluation of sum((-1)^n x^(2n+1)/(2n+1)!).
    x2 = x * x
    term = x
    total = x
    for n in range(1, 14):
        term *= -x2 / ((2 * n) * (2 * n + 1))
        total += term
    return total


def exp(x: float) -> float:
    """Deterministic exp(x). Callers only pass x <= 0."""
    k = math.floor(x * LOG2_E)
    r = x - k * LN_2
    term = 1.0
    total = 1.0
    for n in range(1, 12):
        term *= r / n
        total += term
    return math.ldexp(total, int(k))


def envelope(t: float) -> float:
    """Attack ramp, exponential decay, and a final fade that avoids a click."""
    if t < 0.0:
        return 0.0
    attack = min(1.0, t / ATTACK_SECONDS)
    tail = DURATION_SECONDS - t
    if tail <= 0.0:
        return 0.0
    release = min(1.0, tail / FADE_OUT_SECONDS)
    return attack * release


def render() -> list[int]:
    """The chime as 16-bit mono samples."""
    count = int(SAMPLE_RATE * DURATION_SECONDS)
    raw: list[float] = []
    for index in range(count):
        t = index / SAMPLE_RATE
        total = 0.0
        for start, frequency, tau, amplitude in NOTES:
            local = t - start
            if local < 0.0:
                continue
            decay = exp(-local / tau)
            voice = 0.0
            for multiple, weight in PARTIALS:
                voice += weight * sin(TWO_PI * frequency * multiple * local)
            total += amplitude * decay * voice
        raw.append(total * envelope(t))

    peak = max(abs(sample) for sample in raw)
    scale = (PEAK_FRACTION * 32767.0) / peak
    samples: list[int] = []
    for sample in raw:
        value = int(round(sample * scale))
        samples.append(max(-32768, min(32767, value)))
    return samples


def encode_wav(samples: list[int]) -> bytes:
    """A canonical 44-byte-header PCM WAV, written without the `wave` module."""
    data = struct.pack(f"<{len(samples)}h", *samples)
    header = b"".join(
        (
            b"RIFF",
            struct.pack("<I", 36 + len(data)),
            b"WAVE",
            b"fmt ",
            struct.pack("<IHHIIHH", 16, 1, 1, SAMPLE_RATE, SAMPLE_RATE * 2, 2, 16),
            b"data",
            struct.pack("<I", len(data)),
        )
    )
    return header + data


def main() -> int:
    args = sys.argv[1:]
    check = "--check" in args
    out = DEFAULT_OUT
    if "--out" in args:
        out = Path(args[args.index("--out") + 1])

    encoded = encode_wav(render())

    if check:
        committed = out.read_bytes() if out.exists() else b""
        if committed == encoded:
            print(f"OK {out.name} is byte-identical to a fresh generation")
            return 0
        print(
            f"FAIL {out}: {len(committed)} committed byte(s) vs "
            f"{len(encoded)} generated"
        )
        return 1

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(encoded)
    seconds = len(encoded) - 44
    print(f"wrote {out} ({len(encoded)} bytes, {seconds // 2} samples)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
