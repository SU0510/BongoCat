#!/usr/bin/env python3
"""Cut the panel's mascot images out of the character sprite sheets.

The hub panel shows the same character as the desk pet. Rendering a whole
192x256 animation frame at panel scale would be soft and mostly empty, so this
tool crops the pose to its alpha bounding box and rescales it to exactly the
size the panel draws (2x the logical size, so Retina screens stay crisp).

Run from the repository root:

    python3 tools/insync-ui/generate_ui_images.py
    python3 tools/insync-ui/generate_ui_images.py --check

`--check` regenerates in memory and compares decoded pixels against the committed
images instead of writing them (CI drift gate). Pixels rather than bytes, for the
same reason as the sprite sheets: the zlib encoder differs between macOS and
Ubuntu, and the decoded image is what the panel actually draws.

Outputs (checked in, embedded by `crates/insync-app/src/assets.rs`):

    resources/insync/ui/momo_portrait.png   head/bust crop, 96x96
    resources/insync/ui/momo_wave.png       waving full pose, 100x194
    resources/insync/ui/nana_portrait.png   head/bust crop, 96x96
    resources/insync/ui/nana_wave.png       waving full pose, 100x194
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[2]
SPRITES = ROOT / "resources" / "insync" / "sprites"
OUT = ROOT / "resources" / "insync" / "ui"

FRAME_W, FRAME_H = 192, 256

CHARACTERS = ("momo", "nana")

# Crop window measured from the momo idle sheet: the head/bust occupies the top
# square of the figure (the braids start at y=16, the shoulders at y=120). The
# characters share one silhouette, so the same window crops both.
PORTRAIT_CROP = (44, 12, 148, 120)
# The wave animation's frame 1 has the paw raised, which reads as a greeting.
WAVE_FRAME = 1

# Exact pixel sizes the panel draws, doubled for Retina.
PORTRAIT_SIZE = (96, 96)
WAVE_SIZE = (100, 194)


def frame(sheet: Image.Image, index: int) -> Image.Image:
    return sheet.crop(
        (index * FRAME_W, 0, (index + 1) * FRAME_W, FRAME_H)
    ).convert("RGBA")


def crop_to_ink(image: Image.Image) -> Image.Image:
    box = image.getbbox()
    if box is None:
        raise SystemExit("frame is fully transparent")
    return image.crop(box)


def harden_alpha(image: Image.Image, threshold: int = 96) -> Image.Image:
    """Restore the pixel art's hard silhouette after resampling.

    `Image.resize` blends alpha, which leaves a soft grey fringe around art
    that is drawn with a binary alpha channel. The colours keep the blend (it
    reads as anti-aliasing at panel size) but the silhouette goes back to fully
    opaque or fully clear.
    """
    red, green, blue, alpha = image.split()
    alpha = alpha.point(lambda value: 255 if value >= threshold else 0)
    return Image.merge("RGBA", (red, green, blue, alpha))


def scale(image: Image.Image, size: tuple[int, int]) -> Image.Image:
    return harden_alpha(image.resize(size, Image.LANCZOS))


def save(image: Image.Image, name: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    image.save(path, optimize=True)
    colours = image.convert("RGBA").getcolors(maxcolors=1 << 24) or []
    opaque = sum(count for count, pixel in colours if pixel[3] > 0)
    total = image.width * image.height
    print(
        f"{path.relative_to(ROOT)}  {image.width}x{image.height}  "
        f"{path.stat().st_size} bytes  ink {100 * opaque / total:.0f}%"
    )


def build() -> list[tuple[str, Image.Image]]:
    """Every panel image, in the order the summary prints them."""
    images = []
    for character in CHARACTERS:
        idle = Image.open(SPRITES / character / "idle.png")
        wave = Image.open(SPRITES / character / "wave.png")

        portrait = crop_to_ink(frame(idle, 0).crop(PORTRAIT_CROP))
        pose = crop_to_ink(frame(wave, WAVE_FRAME))
        images.append((f"{character}_portrait.png", scale(portrait, PORTRAIT_SIZE)))
        images.append((f"{character}_wave.png", scale(pose, WAVE_SIZE)))
    return images


def check(images: list[tuple[str, Image.Image]]) -> int:
    failures = 0
    for name, fresh in images:
        path = OUT / name
        if not path.exists():
            print(f"FAIL {name}: missing; run generate_ui_images.py")
            failures += 1
            continue
        committed = Image.open(path).convert("RGBA")
        if committed.size != fresh.size:
            print(f"FAIL {name}: size {committed.size} != {fresh.size}")
            failures += 1
            continue
        diff = ImageChops.difference(committed, fresh)
        if diff.getbbox() is None:
            print(f"OK {name} matches a fresh generation")
        else:
            print(f"FAIL {name}: pixels differ (bbox {diff.getbbox()})")
            failures += 1
    return 1 if failures else 0


def main() -> int:
    images = build()
    if "--check" in sys.argv[1:]:
        return check(images)
    for name, image in images:
        save(image, name)
    return 0


if __name__ == "__main__":
    sys.exit(main())
