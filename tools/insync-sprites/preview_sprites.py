#!/usr/bin/env python3
"""Render GIF previews for every InSync animation into target/insync-preview/."""

from __future__ import annotations

import json
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "resources" / "insync" / "sprites"
PREVIEW = REPO / "target" / "insync-preview"
VIEW_SCALE = 2  # preview at 2x logical for comfortable human review


def main() -> None:
    manifest = json.loads((OUT / "manifest.json").read_text(encoding="utf-8"))
    PREVIEW.mkdir(parents=True, exist_ok=True)
    for character, cdef in manifest["characters"].items():
        fw, fh = cdef["frame_width"], cdef["frame_height"]
        out_dir = PREVIEW / character
        out_dir.mkdir(parents=True, exist_ok=True)
        for anim, adef in cdef["animations"].items():
            sheet = Image.open(OUT / adef["file"]).convert("RGBA")
            frames = []
            for i in range(adef["frames"]):
                fr = sheet.crop((i * fw, 0, (i + 1) * fw, fh))
                bg = Image.new("RGBA", fr.size, (250, 245, 238, 255))
                bg.alpha_composite(fr)
                frames.append(
                    bg.resize((fw * VIEW_SCALE // 4 * 2, fh * VIEW_SCALE // 4 * 2),
                              Image.NEAREST)
                )
            duration = int(1000 / adef["fps"])
            frames[0].save(
                out_dir / f"{anim}.gif",
                save_all=True,
                append_images=frames[1:] + ([frames[0]] if adef["loop"] else []),
                duration=duration,
                loop=0 if adef["loop"] else 1,
                disposal=2,
            )
    print(f"previews in {PREVIEW}")


if __name__ == "__main__":
    main()
