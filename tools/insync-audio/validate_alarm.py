#!/usr/bin/env python3
"""Validate the committed alarm chime `resources/insync/sounds/alarm.wav`.

Parses the file independently of `generate_alarm.py` (its own reader, its own
expectations) and checks the properties that make the asset usable as a
notification sound rather than merely well-formed: it is the format
`bongocat-audio` can decode, it is loud enough to hear without clipping, it
starts and ends at zero so it does not click, and it really contains the two
bell tones it is meant to.

Usage: validate_alarm.py [<path>]
"""

from __future__ import annotations

import math
import struct
import sys
from pathlib import Path

DEFAULT_PATH = (
    Path(__file__).resolve().parents[2] / "resources" / "insync" / "sounds" / "alarm.wav"
)

EXPECTED_RATE = 44_100
MIN_SECONDS = 0.3
MAX_SECONDS = 3.0
MIN_PEAK = 0.5
MAX_PEAK = 0.98
MIN_RMS = 0.05
# Tones the chime must contain, with the window each one is expected in.
EXPECTED_TONES = ((1046.50, 0.02, 0.20), (783.99, 0.26, 0.44))

failures = 0


def fail(msg: str) -> None:
    global failures
    failures += 1
    print(f"FAIL {msg}")


def goertzel(samples: list[int], rate: int, frequency: float) -> float:
    """Single-bin power, so the tone check needs no FFT dependency."""
    omega = 2.0 * math.pi * frequency / rate
    coefficient = 2.0 * math.cos(omega)
    s1 = 0.0
    s2 = 0.0
    for value in samples:
        s0 = value + coefficient * s1 - s2
        s2, s1 = s1, s0
    return math.sqrt(max(0.0, s1 * s1 + s2 * s2 - coefficient * s1 * s2))


def read_wav(path: Path) -> tuple[int, int, list[int]]:
    data = path.read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("not a RIFF/WAVE file")
    if struct.unpack_from("<I", data, 4)[0] != len(data) - 8:
        raise ValueError("RIFF size does not match the file length")
    fmt = data.find(b"fmt ")
    if fmt < 0:
        raise ValueError("no fmt chunk")
    layout = struct.unpack_from("<IHHIIHH", data, fmt + 4)
    _, audio_format, channels, rate, byte_rate, block_align, bits = layout
    if (audio_format, channels, bits) != (1, 1, 16):
        raise ValueError(
            f"expected 16-bit mono PCM, got format={audio_format} "
            f"channels={channels} bits={bits}"
        )
    if (rate, byte_rate, block_align) != (EXPECTED_RATE, EXPECTED_RATE * 2, 2):
        raise ValueError(f"unexpected streaming parameters for {rate} Hz")
    mark = data.find(b"data")
    if mark < 0:
        raise ValueError("no data chunk")
    length = struct.unpack_from("<I", data, mark + 4)[0]
    if length % 2 or mark + 8 + length != len(data):
        raise ValueError("data chunk length does not match the file length")
    count = length // 2
    return rate, count, list(struct.unpack_from(f"<{count}h", data, mark + 8))


def main() -> int:
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_PATH
    if not path.exists():
        fail(f"{path} does not exist")
        return 1

    try:
        rate, count, samples = read_wav(path)
    except ValueError as error:
        fail(f"{path.name}: {error}")
        return 1

    seconds = count / rate
    if not MIN_SECONDS <= seconds <= MAX_SECONDS:
        fail(f"duration {seconds:.3f}s outside {MIN_SECONDS}-{MAX_SECONDS}s")
    else:
        print(f"OK format: 16-bit mono PCM {rate} Hz, {seconds:.3f}s")

    peak = max(abs(value) for value in samples)
    peak_fraction = peak / 32768.0
    if not MIN_PEAK <= peak_fraction <= MAX_PEAK:
        fail(f"peak {peak_fraction:.3f} FS outside {MIN_PEAK}-{MAX_PEAK} FS")
    rms = math.sqrt(sum(value * value for value in samples) / count) / 32768.0
    if rms < MIN_RMS:
        fail(f"rms {rms:.4f} FS below {MIN_RMS} FS (too quiet to notice)")
    if failures == 0:
        print(f"OK level: peak {peak_fraction:.3f} FS, rms {rms:.4f} FS")

    clipped = 0
    run = 0
    for value in samples:
        run = run + 1 if abs(value) >= 32767 else 0
        clipped = max(clipped, run)
    if clipped > 2:
        fail(f"{clipped} consecutive full-scale samples (clipping)")
    else:
        print("OK no clipping")

    if samples[0] != 0 or samples[-1] != 0:
        fail(f"starts/ends at {samples[0]}/{samples[-1]} instead of 0 (clicks)")
    else:
        print("OK silent edge samples")

    for frequency, start, end in EXPECTED_TONES:
        window = samples[int(start * rate) : int(end * rate)]
        spoken = goertzel(window, rate, frequency)
        neighbours = [
            goertzel(window, rate, frequency * ratio) for ratio in (0.5, 0.75, 1.5)
        ]
        if spoken <= max(neighbours):
            fail(f"{frequency} Hz is not the strongest tone in {start}-{end}s")
        else:
            print(f"OK {frequency} Hz present in {start}-{end}s")

    if failures == 0:
        print("OK alarm chime")
        return 0
    print(f"{failures} problem(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
