#!/usr/bin/env python3
"""Measure the hub panel as it is actually drawn.

The panel is a visual surface and this environment has no eyes: the checks here
read the rendered window instead. The window is captured through the window
server and then reduced to numbers — which section's tile is selected, how much
of the surface each palette colour covers, how many rows of content the section
drew, and whether anything is painted against the window edge (which is what
clipped content looks like). Read against a known-good run, the numbers say
whether a change moved the layout.

A section is reached the way the product supports it: the panel opens on the
section it was last left on, so writing that section into the development window
state and starting the app puts the real panel on screen. Nothing is stubbed.

Usage:
  verify_panel.py --capture out.png        measure the panel already on screen
  verify_panel.py --all /tmp/panels        one capture per section, from a launch each
"""

import argparse
import json
import os
import pathlib
import subprocess
import sys
import time
from collections import Counter

import Quartz
from PIL import Image

# Tile grid geometry in window points: eight tiles over two rows, four per row.
# Measured from a capture, not from the layout code.
TILE_SECTIONS = [
    ["chat", "letters", "focus", "todos"],
    ["notes", "anniversaries", "alarms", "settings"],
]
TILE_X = [(12, 102), (107, 197), (203, 293), (298, 388)]
TILE_Y = [(107, 155), (161, 209)]

# Both product palettes, as drawn: a Retina capture through the window server
# lands a shade off the authored hex values, so the matches are approximate.
PALETTES = {
    "cream": {
        "canvas": (255, 249, 243),
        "muted": (246, 233, 222),
        "card": (255, 255, 255),
        "rose": (234, 140, 161),
        "border": (229, 206, 191),
        "ink": "dark",
    },
    "cocoa": {
        "canvas": (27, 23, 24),
        "muted": (53, 44, 46),
        "card": (42, 35, 38),
        "rose": (237, 146, 165),
        "border": (69, 58, 61),
        "ink": "light",
    },
}

DEFAULT_APP = "target/debug/insync-app"
DEV_ROOT = pathlib.Path.home() / "Library/Application Support/com.ayangweb.insync/development"


def luminance(pixel) -> float:
    return 0.299 * pixel[0] + 0.587 * pixel[1] + 0.114 * pixel[2]


def near(pixel, target, tolerance=14) -> bool:
    return sum(abs(a - b) for a, b in zip(pixel, target)) <= tolerance


def hub_window():
    """The panel window: InSync's large window, not the pet overlay."""
    options = (
        Quartz.kCGWindowListOptionOnScreenOnly
        | Quartz.kCGWindowListExcludeDesktopElements
    )
    for window in Quartz.CGWindowListCopyWindowInfo(options, Quartz.kCGNullWindowID):
        owner = window.get("kCGWindowOwnerName", "") or ""
        bounds = window.get("kCGWindowBounds", {})
        if "insync" in owner.lower() and bounds.get("Width", 0) > 200:
            return window.get("kCGWindowNumber")
    raise SystemExit("no panel window on screen — is the app running?")


def capture(path: str) -> None:
    ident = hub_window()
    result = subprocess.run(
        ["screencapture", "-x", "-o", "-l", str(ident), path], capture_output=True, text=True
    )
    if result.returncode != 0 or not os.path.exists(path):
        raise SystemExit(f"screencapture failed: {result.stderr.strip()}")


def launch_on(app: str, section: str) -> subprocess.Popen:
    """Start the app with the panel opening on `section`."""
    state = DEV_ROOT / "window-state.json"
    existing = json.loads(state.read_text()) if state.exists() else {"schema_version": 1}
    existing["hub_tab"] = section
    state.parent.mkdir(parents=True, exist_ok=True)
    state.write_text(json.dumps(existing))
    process = subprocess.Popen(
        [app], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True
    )
    for _ in range(40):
        time.sleep(0.5)
        try:
            hub_window()
            time.sleep(2.5)  # let the first frame settle
            return process
        except SystemExit:
            continue
    process.kill()
    raise SystemExit(f"{app} did not open its panel")


def stop(process: subprocess.Popen) -> None:
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()


def measure(path: str) -> dict:
    image = Image.open(path).convert("RGB")
    width, height = image.size
    px = image.load()
    counts = Counter(image.getdata())
    total = width * height
    scale = 2  # Retina: two pixels per point

    mode, palette = None, None
    for name, candidate in PALETTES.items():
        if near(counts.most_common(1)[0][0], candidate["canvas"], 20):
            mode, palette = name, candidate
            break
    if palette is None:
        raise SystemExit(f"{path}: the canvas is neither product palette")

    shares = {
        name: sum(n for colour, n in counts.items() if near(colour, target)) / total
        for name, target in palette.items()
        if isinstance(target, tuple)
    }

    # The selected tile is the only rose-filled surface in the grid.
    selected, most = "none", 0
    for row, (y0, y1) in enumerate(TILE_Y):
        for column, (x0, x1) in enumerate(TILE_X):
            filled = sum(
                1
                for y in range(y0 * scale, y1 * scale, scale)
                for x in range(x0 * scale, x1 * scale, scale)
                if near(px[x, y], palette["rose"], 34)
            )
            if filled > most:
                selected, most = TILE_SECTIONS[row][column], filled

    # Content is counted in rows that carry ink, below the tile grid. Ink is
    # dark in the cream palette and light in cocoa.
    ink_rows = []
    for y in range(216 * scale, height, scale):
        row = [px[x, y] for x in range(0, width, 4 * scale)]
        hits = sum(1 for pixel in row if near_ink(pixel, palette["ink"]))
        if hits >= 2:
            ink_rows.append(y)
    bands = []
    for y in ink_rows:
        if bands and y - bands[-1][1] <= 12:
            bands[-1][1] = y
        else:
            bands.append([y, y])

    # Nothing of ours may touch the window edge: the only palette colours that
    # belong there are the canvas, the section dividers and the rounded corners.
    stray = 0
    for y in range(120, height - 24, scale):
        for x in (0, 1, 2, 3, 4, width - 5, width - 4, width - 3, width - 2, width - 1):
            pixel = px[x, y]
            if any(
                near(pixel, value) for value in palette.values() if isinstance(value, tuple)
            ):
                continue
            stray += 1

    return {
        "file": os.path.basename(path),
        "mode": mode,
        "selected": selected,
        "content_rows": len(bands),
        "canvas": shares["canvas"],
        "content": 1.0 - shares["canvas"],
        "rose": shares["rose"],
        "card": shares["card"],
        "stray_edge_pixels": stray,
    }


def near_ink(pixel, kind: str) -> bool:
    level = luminance(pixel)
    return level < 150 if kind == "dark" else level > 150


def show(row: dict) -> None:
    print(
        f"{row['file']:24s} {row['mode']:6s} selected={row['selected']:14s} "
        f"content_rows={row['content_rows']:2d} canvas={row['canvas']:.2f} "
        f"content={row['content']:.2f} rose={row['rose']:.3f} "
        f"card={row['card']:.2f} stray_edge={row['stray_edge_pixels']}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", metavar="PNG")
    parser.add_argument("--all", metavar="DIR")
    parser.add_argument("--app", default=DEFAULT_APP, help=f"app binary (default {DEFAULT_APP})")
    args = parser.parse_args()

    if args.all:
        os.makedirs(args.all, exist_ok=True)
        for sections in TILE_SECTIONS:
            for section in sections:
                process = launch_on(args.app, section)
                try:
                    path = os.path.join(args.all, f"{section}.png")
                    capture(path)
                finally:
                    stop(process)
                row = measure(path)
                assert row["selected"] == section, (
                    f"opened {section!r} but {row['selected']!r} is the selected tile"
                )
                show(row)
        return

    if args.capture:
        capture(args.capture)
        show(measure(args.capture))
        return
    parser.print_help()
    sys.exit(2)


if __name__ == "__main__":
    main()
