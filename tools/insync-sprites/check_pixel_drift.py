#!/usr/bin/env python3
"""Pixel-level drift check: regenerate into a temp directory and compare the
decoded RGBA pixels (and manifest) against the committed assets.

Byte-level PNG comparison is unreliable across platforms because the zlib
encoder differs (macOS vs Ubuntu produce different deflate streams for the
same Pillow version). Decoded pixels are the actual product surface, so this
is both stricter in meaning and stable across platforms.

Usage: check_pixel_drift.py <regenerated-dir> [<committed-dir>]
Where <regenerated-dir> is the `--out` sprite dir from generate_sprites.py
(icons live in its sibling `icons/` directory).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from PIL import Image, ImageChops

failures = 0


def fail(msg: str) -> None:
    global failures
    failures += 1
    print(f"FAIL {msg}")


def compare_png(a: Path, b: Path) -> None:
    try:
        img_a = Image.open(a).convert("RGBA")
        img_b = Image.open(b).convert("RGBA")
    except Exception as exc:  # noqa: BLE001
        fail(f"{a.name}: unreadable ({exc})")
        return
    if img_a.size != img_b.size:
        fail(f"{a.name}: size {img_a.size} != {img_b.size}")
        return
    diff = ImageChops.difference(img_a, img_b)
    if diff.getbbox() is not None:
        fail(f"{a.name}: pixels differ (bbox {diff.getbbox()})")
    else:
        print(f"OK {a.name}")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    regenerated = Path(sys.argv[1])
    committed = Path(sys.argv[2]) if len(sys.argv) > 2 else (
        Path(__file__).resolve().parents[2] / "resources" / "insync" / "sprites"
    )

    # manifest: parsed-JSON equality (whitespace/key order irrelevant)
    manifest_a = json.loads((regenerated / "manifest.json").read_text("utf-8"))
    manifest_b = json.loads((committed / "manifest.json").read_text("utf-8"))
    if manifest_a != manifest_b:
        fail("manifest.json content differs")
    else:
        print("OK manifest.json")

    for char_dir in sorted(committed.iterdir()):
        if not char_dir.is_dir():
            continue
        for png in sorted(char_dir.glob("*.png")):
            gen = regenerated / char_dir.name / png.name
            if not gen.exists():
                fail(f"{char_dir.name}/{png.name}: missing from regeneration")
                continue
            compare_png(gen, png)

    # tray icon
    tray_committed = committed.parent / "icons" / "tray.png"
    tray_generated = regenerated.parent / "icons" / "tray.png"
    if tray_committed.exists() and tray_generated.exists():
        compare_png(tray_generated, tray_committed)
    else:
        fail("tray icon missing")

    if failures == 0:
        print("OK no pixel drift")
        return 0
    print(f"{failures} drift problem(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
