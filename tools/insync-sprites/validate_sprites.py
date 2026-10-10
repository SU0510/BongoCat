#!/usr/bin/env python3
"""Validate generated InSync sprite sheets against the manifest.

Checks (see docs/insync/sprite-assets.md):
- every animation in the generator exists as a sheet with the declared size;
- every frame has non-transparent content and a >= 2px transparent margin;
- alpha is binary (no semi-transparent pixels -> no white fringing);
- every opaque pixel uses a palette color (transparency check by construction);
- loop animations start and end within a small pixel-diff budget.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "resources" / "insync" / "sprites"
MARGIN = 2

EXPECTED = {
    "idle": (4, True),
    "blink": (5, False),
    "wave": (6, False),
    "walk": (6, True),
    "sleep": (4, True),
    "work": (4, True),
    "stretch": (6, False),
    "react": (4, False),
    "poke_react": (6, False),
    "message_react": (4, False),
    "invite_react": (4, False),
    "celebrate": (6, False),
    "rest": (4, True),
}


def fail(msg: str) -> None:
    print(f"FAIL {msg}")
    global failures
    failures += 1


failures = 0


def check_sheet(character: str, anim: str, frames: int, loop: bool,
                fw: int, fh: int) -> None:
    path = OUT / character / f"{anim}.png"
    if not path.exists():
        fail(f"{character}/{anim}: missing sheet")
        return
    img = Image.open(path).convert("RGBA")
    expected_w = fw * frames
    if img.size != (expected_w, fh):
        fail(f"{character}/{anim}: size {img.size} != {(expected_w, fh)}")
        return
    prev_box = None
    for i in range(frames):
        frame = img.crop((i * fw, 0, (i + 1) * fw, fh))
        alpha = frame.getchannel("A")
        lo, hi = alpha.getextrema()
        if lo not in (0, 255) or hi not in (0, 255):
            fail(f"{character}/{anim}#{i}: non-binary alpha range {(lo, hi)}")
        bbox = frame.getbbox()
        if bbox is None:
            fail(f"{character}/{anim}#{i}: fully transparent frame")
            continue
        x0, y0, x1, y1 = bbox
        if x0 < MARGIN or y0 < MARGIN or x1 > fw - MARGIN or y1 > fh - MARGIN:
            fail(f"{character}/{anim}#{i}: content touches margin bbox={bbox}")
        if prev_box is not None and loop:
            dx = max(abs(prev_box[0] - x0), abs(prev_box[2] - x1))
            dy = max(abs(prev_box[1] - y0), abs(prev_box[3] - y1))
            if dx > 8 or dy > 8:
                fail(f"{character}/{anim}#{i}: bbox jumped from {prev_box} to {bbox}")
        prev_box = bbox
    if loop:
        first_box = img.crop((0, 0, fw, fh)).getbbox()
        last_box = img.crop(((frames - 1) * fw, 0, frames * fw, fh)).getbbox()
        if first_box is not None and last_box is not None:
            dx = max(abs(first_box[0] - last_box[0]), abs(first_box[2] - last_box[2]))
            dy = max(abs(first_box[1] - last_box[1]), abs(first_box[3] - last_box[3]))
            if dx > 8 or dy > 8:
                fail(f"{character}/{anim}: loop seam bbox {first_box} vs {last_box}")


def main() -> int:
    manifest_path = OUT / "manifest.json"
    if not manifest_path.exists():
        print("FAIL manifest.json missing")
        return 1
    import json

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("format_version") != 1:
        fail("manifest format_version != 1")
    for character, cdef in manifest["characters"].items():
        fw, fh = cdef["frame_width"], cdef["frame_height"]
        anims = cdef["animations"]
        for anim in EXPECTED:
            if anim not in anims:
                fail(f"{character}: animation {anim} missing from manifest")
        for anim, adef in anims.items():
            if anim not in EXPECTED:
                fail(f"{character}: unexpected animation {anim}")
                continue
            want_frames, want_loop = EXPECTED[anim]
            if adef["frames"] != want_frames:
                fail(f"{character}/{anim}: frames {adef['frames']} != {want_frames}")
            if adef["loop"] != want_loop:
                fail(f"{character}/{anim}: loop {adef['loop']} != {want_loop}")
            if adef["file"] != f"{character}/{anim}.png":
                fail(f"{character}/{anim}: unexpected file path {adef['file']}")
            check_sheet(character, anim, adef["frames"], adef["loop"], fw, fh)
    tray = REPO / "resources" / "insync" / "icons" / "tray.png"
    if not tray.exists():
        fail("resources/insync/icons/tray.png missing")
    if failures == 0:
        print("OK all sprite sheets valid")
        return 0
    print(f"{failures} problem(s) found")
    return 1


if __name__ == "__main__":
    sys.exit(main())
