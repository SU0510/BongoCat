#!/usr/bin/env python3
"""InSync pixel sprite generator.

Draws the original character "Momo" (and palette variants) frame by frame with
PIL, packs each animation into a horizontal sprite sheet and writes
``resources/insync/sprites/manifest.json``. Deterministic: same code, same
pixels. See docs/insync/sprite-assets.md for the production workflow.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field, replace
from pathlib import Path

from PIL import Image

REPO = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO / "resources" / "insync" / "sprites"
# `--out <dir>` redirects sheets/manifest/icons (used for pixel-drift checks
# that regenerate into a temp directory and compare decoded pixels).
OUT = Path(sys.argv[sys.argv.index("--out") + 1]) if "--out" in sys.argv else DEFAULT_OUT
SCALE = 4
FRAME_W, FRAME_H = 48, 64

# ---------------------------------------------------------------------------
# palette
# ---------------------------------------------------------------------------

PALETTES = {
    "momo": {
        "outline": "#4a3b3b",
        "skin": "#f7d7b8",
        "skin_shadow": "#eab890",
        "hair": "#f2a6b3",
        "hair_shadow": "#d97f92",
        "hair_light": "#ffd3dc",
        "hoodie": "#fff4e3",
        "hoodie_shadow": "#e8d5b8",
        "hoodie_deep": "#cdb494",
        "accent": "#f2a6b3",
        "pants": "#7fb5a3",
        "pants_shadow": "#5f9483",
        "shoe": "#6b5a52",
        "blush": "#f6b1b1",
        "eye": "#4a3b3b",
        "white": "#ffffff",
        "zzz": "#9db8d9",
        "star": "#ffd98a",
        "confetti_a": "#f2a6b3",
        "confetti_b": "#9fd8c5",
        "confetti_c": "#ffd98a",
        "paper": "#fffdf5",
        "paper_line": "#b9a98f",
        "wood": "#c9a678",
    },
    "momo-mint": {
        "outline": "#3b4a41",
        "skin": "#f7d7b8",
        "skin_shadow": "#eab890",
        "hair": "#9fd8c5",
        "hair_shadow": "#77b8a3",
        "hair_light": "#c8ecdd",
        "hoodie": "#fff4e3",
        "hoodie_shadow": "#e8d5b8",
        "hoodie_deep": "#cdb494",
        "accent": "#9fd8c5",
        "pants": "#d9a3ae",
        "pants_shadow": "#b87f8c",
        "shoe": "#5f6b62",
        "blush": "#f6b1b1",
        "eye": "#3b4a41",
        "white": "#ffffff",
        "zzz": "#9db8d9",
        "star": "#ffd98a",
        "confetti_a": "#f2a6b3",
        "confetti_b": "#9fd8c5",
        "confetti_c": "#ffd98a",
        "paper": "#fffdf5",
        "paper_line": "#b9a98f",
        "wood": "#c9a678",
    },
    # The second character: dark warm-brown twin braids with wispy center-parted
    # bangs, a dusty-pink sleeveless top, a black crossbody strap and jeans —
    # read off the reference photo, then brightened: the first pass read too
    # dark/yellow (near-black outline over a yellow skin), so the skin is a
    # warm ivory, the outline a soft warm brown and the top a rosier pink.
    # Same silhouette as momo; the style flags below drive the differences.
    "nana": {
        "outline": "#6b4f4a",
        "skin": "#ffe3c9",
        "skin_shadow": "#f3c7a1",
        "hair": "#7a5c50",
        "hair_shadow": "#523a34",
        "hair_light": "#a07d68",
        "hoodie": "#f2b7bd",
        "hoodie_shadow": "#dba0a8",
        "hoodie_deep": "#c48892",
        "accent": "#ef7280",
        "pants": "#6f89b8",
        "pants_shadow": "#54709f",
        "shoe": "#e8e2d6",
        "blush": "#f79fa4",
        "eye": "#5d443f",
        "white": "#ffffff",
        "zzz": "#9db8d9",
        "star": "#ffd98a",
        "confetti_a": "#f2a6b3",
        "confetti_b": "#9fd8c5",
        "confetti_c": "#ffd98a",
        "paper": "#fffdf5",
        "paper_line": "#b9a98f",
        "wood": "#c9a678",
        "strap": "#4a4038",
    },
    # The chibi variant keeps nana's exact colours so the two read as the same
    # person at a glance; only the proportions and the face styling differ.
    "nana-chibi": {
        "outline": "#6b4f4a",
        "skin": "#ffe3c9",
        "skin_shadow": "#f3c7a1",
        "hair": "#7a5c50",
        "hair_shadow": "#523a34",
        "hair_light": "#a07d68",
        "hoodie": "#f2b7bd",
        "hoodie_shadow": "#dba0a8",
        "hoodie_deep": "#c48892",
        "accent": "#ef7280",
        "pants": "#6f89b8",
        "pants_shadow": "#54709f",
        "shoe": "#e8e2d6",
        "blush": "#f79fa4",
        "eye": "#5d443f",
        "white": "#ffffff",
        "zzz": "#9db8d9",
        "star": "#ffd98a",
        "confetti_a": "#f2b7bd",
        "confetti_b": "#9fd8c5",
        "confetti_c": "#ffd98a",
        "paper": "#fffdf5",
        "paper_line": "#b9a98f",
        "wood": "#c9a678",
        "strap": "#4a4038",
    },
}


@dataclass(frozen=True)
class Style:
    """Structural traits that differ between characters.

    momo and its palette variants share the default; a new character flips the
    flags its reference calls for. The draw functions branch on these instead of
    on the character name, so a palette variant of nana needs no code.
    """

    braids: bool = False  # twin braids draping over the shoulders, in front
    center_part: bool = False  # wispy bangs with a gap at the center
    sleeveless: bool = False  # bare arms, no drawstrings or pocket
    strap: bool = False  # black crossbody strap over the top
    soft_face: bool = False  # melon-seed taper instead of the round bob jaw
    ahoge: bool = True  # the little curl on top
    chibi: bool = False  # two-head-tall variant with its own draw path


STYLES = {
    "momo": Style(),
    "momo-mint": Style(),
    "nana": Style(
        braids=True, center_part=True, sleeveless=True, strap=True,
        soft_face=True, ahoge=False,
    ),
    # The chibi take on nana: same traits (braids, curtain bangs, sleeveless
    # pink top, strap, jeans) on a two-head-tall body with big sparkly eyes,
    # after the chibi reference sheet. Own draw path, so nana herself is
    # untouched by any of it.
    "nana-chibi": Style(
        braids=True, center_part=True, sleeveless=True, strap=True,
        soft_face=True, ahoge=False, chibi=True,
    ),
}

# ---------------------------------------------------------------------------
# tiny drawing kit
# ---------------------------------------------------------------------------


class Grid:
    """A logical-resolution pixel canvas with binary alpha."""

    def __init__(self, w: int, h: int) -> None:
        self.w, self.h = w, h
        self.px: dict[tuple[int, int], str] = {}

    def plot(self, x: int, y: int, color: str) -> None:
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[(x, y)] = color

    def hline(self, x0: int, x1: int, y: int, color: str) -> None:
        for x in range(min(x0, x1), max(x0, x1) + 1):
            self.plot(x, y, color)

    def vline(self, x: int, y0: int, y1: int, color: str) -> None:
        for y in range(min(y0, y1), max(y0, y1) + 1):
            self.plot(x, y, color)

    def rect(self, x0: int, y0: int, x1: int, y1: int, color: str) -> None:
        for y in range(y0, y1 + 1):
            self.hline(x0, x1, y, color)

    def rect_outline(self, x0: int, y0: int, x1: int, y1: int, color: str) -> None:
        self.hline(x0, x1, y0, color)
        self.hline(x0, x1, y1, color)
        self.vline(x0, y0, y1, color)
        self.vline(x1, y0, y1, color)

    def rrect(self, x0: int, y0: int, x1: int, y1: int, color: str, r: int = 2) -> None:
        for y in range(y0, y1 + 1):
            inset = r
            if y < y0 + r:
                inset = r - (y - y0)
            elif y > y1 - r:
                inset = r - (y1 - y)
            self.hline(x0 + inset, x1 - inset, y, color)

    def disc(self, cx: int, cy: int, r: int, color: str) -> None:
        for y in range(cy - r, cy + r + 1):
            span = int((r * r - (y - cy) ** 2) ** 0.5 + 0.001)
            self.hline(cx - span, cx + span, y, color)

    def mirror_plot(self, x: int, y: int, color: str) -> None:
        self.plot(x, y, color)
        self.plot(47 - x, y, color)

    def translate(self, dx: int, dy: int) -> None:
        self.px = {(x + dx, y + dy): c for (x, y), c in self.px.items()}

    def to_image(self, pal: dict[str, str]) -> Image.Image:
        img = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        colors = {name: _hex(c) for name, c in pal.items()}
        unknown = set(self.px.values()) - set(colors)
        if unknown:
            raise ValueError(f"colors outside palette: {sorted(unknown)}")
        for (x, y), name in self.px.items():
            img.putpixel((x, y), colors[name])
        return img


def _hex(s: str) -> tuple[int, int, int, int]:
    s = s.lstrip("#")
    return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16), 255)


# ---------------------------------------------------------------------------
# pose model
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Pose:
    """Everything that may vary between frames of one animation."""

    bob: int = 0  # head+torso vertical offset (breathing)
    jump: int = 0  # whole-body vertical offset (jumping)
    lean: int = 0  # head horizontal offset
    eyes: str = "open"  # open|half|closed|happy|wide
    mouth: str = "smile"  # smile|open|small|grin|flat
    blush: bool = True
    arm_l: str = "down"  # down|up|wave|forward|flail|book|cup|stretch
    arm_r: str = "down"
    legs: str = "stand"  # stand|walk_a|walk_b|sit
    props: tuple[str, ...] = ()
    prop_phase: int = 0  # per-frame phase for animated props


# ---------------------------------------------------------------------------
# character parts (all coordinates hand-placed on the 48x64 logical canvas)
# ---------------------------------------------------------------------------


def draw_hair_back(g: Grid, p: Pose) -> None:
    dy = p.bob + p.jump
    # short bob behind the neck, just below the jaw
    for y in range(24 + dy, 30 + dy):
        g.hline(13, 34, y, "hair_shadow")
    g.hline(14, 33, 30 + dy, "hair_shadow")
    g.vline(13, 24 + dy, 29 + dy, "outline")
    g.vline(34, 24 + dy, 29 + dy, "outline")


def draw_head(g: Grid, p: Pose, style: Style) -> None:
    dy, dx = p.bob + p.jump, p.lean
    if style.soft_face:
        # melon-seed face: full width through the eyes, then a gentle
        # two-step taper to a soft rounded chin — no hard jaw corners
        for y in range(9 + dy, 18 + dy):
            g.hline(14 + dx, 33 + dx, y, "skin")
        g.hline(16 + dx, 31 + dx, 7 + dy, "skin")
        g.hline(15 + dx, 32 + dx, 8 + dy, "skin")
        for y, x0 in ((18, 15), (19, 15), (20, 16), (21, 16), (22, 17), (23, 17)):
            xr = 47 - x0
            g.hline(x0 + dx, xr + dx, y + dy, "skin")
            g.plot(x0 - 1 + dx, y + dy, "outline")
            g.plot(xr + 1 + dx, y + dy, "outline")
        # rounded chin
        g.plot(17 + dx, 24 + dy, "outline")
        g.plot(18 + dx, 24 + dy, "skin_shadow")
        g.hline(19 + dx, 28 + dx, 24 + dy, "skin")
        g.plot(29 + dx, 24 + dy, "skin_shadow")
        g.plot(30 + dx, 24 + dy, "outline")
        g.plot(19 + dx, 25 + dy, "outline")
        g.plot(20 + dx, 25 + dy, "skin_shadow")
        g.hline(21 + dx, 26 + dx, 25 + dy, "skin")
        g.plot(27 + dx, 25 + dy, "skin_shadow")
        g.plot(28 + dx, 25 + dy, "outline")
        g.hline(21 + dx, 26 + dx, 26 + dy, "outline")
        g.hline(23 + dx, 24 + dx, 27 + dy, "outline")
    else:
        # skull: x14..33, y8..26
        for y in range(9 + dy, 26 + dy):
            g.hline(14 + dx, 33 + dx, y, "skin")
        g.hline(16 + dx, 31 + dx, 7 + dy, "skin")
        g.hline(15 + dx, 32 + dx, 8 + dy, "skin")
        g.hline(15 + dx, 32 + dx, 26 + dy, "skin_shadow")
        # jaw outline
        g.hline(17 + dx, 30 + dx, 27 + dy, "outline")
        g.hline(16 + dx, 16 + dx, 26 + dy, "outline")
        g.hline(31 + dx, 31 + dx, 26 + dy, "outline")
    draw_hair_front(g, p, dx, dy, style)
    draw_face(g, p, dx, dy, style)


def draw_hair_front(g: Grid, p: Pose, dx: int, dy: int, style: Style) -> None:
    # top mass: rounded, y6..11
    g.hline(19 + dx, 28 + dx, 5 + dy, "hair")
    g.hline(17 + dx, 30 + dx, 6 + dy, "hair")
    for y in range(7 + dy, 12 + dy):
        g.hline(14 + dx, 33 + dx, y, "hair")
    if style.center_part:
        # curtain bangs combed from the center part along the hairline: the
        # sweeps fall diagonally to the temples, so the forehead reads as a
        # narrow triangle widening to the brow, like the reference photo
        g.hline(15 + dx, 22 + dx, 12 + dy, "hair")
        g.hline(25 + dx, 32 + dx, 12 + dy, "hair")
        g.hline(15 + dx, 21 + dx, 13 + dy, "hair")
        g.hline(26 + dx, 32 + dx, 13 + dy, "hair")
        g.hline(14 + dx, 19 + dx, 14 + dy, "hair")
        g.hline(28 + dx, 33 + dx, 14 + dy, "hair")
        g.hline(14 + dx, 17 + dx, 15 + dy, "hair")
        g.hline(30 + dx, 33 + dx, 15 + dy, "hair")
        # shadow along the inner edge of each sweep
        g.plot(21 + dx, 13 + dy, "hair_shadow")
        g.plot(26 + dx, 13 + dy, "hair_shadow")
        g.plot(19 + dx, 14 + dy, "hair_shadow")
        g.plot(28 + dx, 14 + dy, "hair_shadow")
        g.plot(17 + dx, 15 + dy, "hair_shadow")
        g.plot(30 + dx, 15 + dy, "hair_shadow")
        # soft shading where the skin meets the hairline
        g.plot(20 + dx, 14 + dy, "skin_shadow")
        g.plot(27 + dx, 14 + dy, "skin_shadow")
        g.plot(18 + dx, 15 + dy, "skin_shadow")
        g.plot(29 + dx, 15 + dy, "skin_shadow")
    else:
        # bangs bottom edge with fringe tips
        g.hline(15 + dx, 32 + dx, 12 + dy, "hair")
        for tip in (16, 20, 24, 28):
            g.vline(tip + dx, 13 + dy, 14 + dy, "hair")
        g.vline(32 + dx, 13 + dy, 14 + dy, "hair")
        # shadow right under the bangs
        g.hline(16 + dx, 31 + dx, 13 + dy, "hair_shadow")
        for tip in (16, 20, 24, 28):
            g.plot(tip + dx, 15 + dy, "skin_shadow")
        g.plot(32 + dx, 15 + dy, "skin_shadow")
    if style.braids:
        # side locks stop where the braids take over
        g.vline(13 + dx, 9 + dy, 17 + dy, "hair")
        g.vline(14 + dx, 11 + dy, 17 + dy, "hair")
        g.vline(34 + dx, 9 + dy, 17 + dy, "hair")
        g.vline(33 + dx, 11 + dy, 17 + dy, "hair")
    else:
        # side locks framing the face, ending at the jaw
        g.vline(13 + dx, 9 + dy, 22 + dy, "hair")
        g.vline(14 + dx, 11 + dy, 24 + dy, "hair")
        g.plot(15 + dx, 23 + dy, "hair")
        g.plot(15 + dx, 24 + dy, "hair")
        g.vline(34 + dx, 9 + dy, 22 + dy, "hair")
        g.vline(33 + dx, 11 + dy, 24 + dy, "hair")
        g.plot(32 + dx, 23 + dy, "hair")
        g.plot(32 + dx, 24 + dy, "hair")
    # bob silhouette outline
    g.hline(18 + dx, 29 + dx, 4 + dy, "outline")
    g.plot(17 + dx, 5 + dy, "outline")
    g.plot(30 + dx, 5 + dy, "outline")
    g.plot(16 + dx, 6 + dy, "outline")
    g.plot(31 + dx, 6 + dy, "outline")
    g.plot(15 + dx, 7 + dy, "outline")
    g.plot(32 + dx, 7 + dy, "outline")
    g.plot(14 + dx, 8 + dy, "outline")
    g.plot(33 + dx, 8 + dy, "outline")
    g.plot(13 + dx, 9 + dy, "outline")
    g.plot(34 + dx, 9 + dy, "outline")
    if style.braids:
        # the outline hands off to the braid edges below the ears
        g.vline(12 + dx, 10 + dy, 15 + dy, "outline")
        g.vline(35 + dx, 10 + dy, 15 + dy, "outline")
    else:
        g.vline(12 + dx, 10 + dy, 21 + dy, "outline")
        g.vline(35 + dx, 10 + dy, 21 + dy, "outline")
        g.plot(12 + dx, 22 + dy, "outline")
        g.plot(35 + dx, 22 + dy, "outline")
        g.hline(13 + dx, 13 + dx, 25 + dy, "outline")
        g.hline(34 + dx, 34 + dx, 25 + dy, "outline")
        g.plot(14 + dx, 24 + dy, "outline")
        g.plot(33 + dx, 24 + dy, "outline")
        g.plot(15 + dx, 25 + dy, "outline")
        g.plot(32 + dx, 25 + dy, "outline")
    # shine
    g.hline(18 + dx, 24 + dx, 7 + dy, "hair_light")
    g.hline(19 + dx, 23 + dx, 8 + dy, "hair_light")
    g.plot(33 + dx, 12 + dy, "hair_light")
    if style.ahoge:
        # ahoge: a little curl on top
        g.plot(24 + dx, 2 + dy, "outline")
        g.plot(25 + dx, 3 + dy, "outline")
        g.plot(23 + dx, 3 + dy, "outline")
        g.plot(24 + dx, 3 + dy, "outline")
        g.plot(24 + dx, 4 + dy, "outline")


def draw_braids(g: Grid, p: Pose) -> None:
    """Twin braids draping over the shoulders, in front of the chest.

    A short strand continues each side lock down beside the face, then the
    braid crosses the shoulder and hangs in front of the torso to a tie just
    above the hem. Drawn after the torso but before the arms and the strap:
    a raised arm passes in front of its strand, the strap's top end crosses
    over the drape, and the bag covers the tip; the head (chin, jaw) is drawn
    last over the strand's top. The twist reads from a shadow pixel that
    travels across the 3-wide strand on a 4-row cycle, with a highlight on
    the opposite side. In the walk cycle the drape follows the step by one
    pixel, opposite on the two sides.
    """
    sway = (p.prop_phase % 2) if p.legs in ("walk_a", "walk_b") else 0
    dx = p.lean
    dy = p.bob + p.jump
    tie_y = 40
    twist = [0, 1, 2, 1]  # shadow column index within the 3-wide strand
    for side in ("l", "r"):
        sx = dx + (sway if side == "l" else -sway)
        if side == "l":
            conn = (13 + dx, 14 + dx)  # beside the face, ear to jaw
            conn_wide = 15 + dx  # the lock widens where the jaw tapers in
            conn_edge = 12 + dx
            drape = (15 + sx, 16 + sx, 17 + sx)  # over the shoulder, in front
            edge = 14 + sx
        else:
            conn = (34 + dx, 33 + dx)
            conn_wide = 32 + dx
            conn_edge = 35 + dx
            drape = (32 + sx, 31 + sx, 30 + sx)
            edge = 33 + sx

        # strand down the side of the face, continuing the side lock
        for y in range(18, 24):
            for x in conn:
                g.plot(x, y + dy, "hair")
            if y >= 22:
                g.plot(conn_wide, y + dy, "hair")
        g.vline(conn_edge, 16 + dy, 23 + dy, "outline")
        # drape in front of the chest, down to the tie
        for i, y in enumerate(range(24, tie_y)):
            phase = i % 4
            for x in drape:
                g.plot(x, y + dy, "hair")
            g.plot(drape[twist[phase]], y + dy, "hair_shadow")
            if phase == 0:
                g.plot(drape[0], y + dy, "hair_light")
            elif phase == 2:
                g.plot(drape[2], y + dy, "hair_light")
            g.plot(edge, y + dy, "outline")
        # hair tie and the tapering tip
        for x in drape:
            g.plot(x, tie_y + dy, "accent")
            g.plot(x, tie_y + 1 + dy, "accent")
        g.plot(drape[1], tie_y + 2 + dy, "hair")
        g.plot(drape[1], tie_y + 3 + dy, "outline")


def draw_face(g: Grid, p: Pose, dx: int, dy: int, style: Style) -> None:
    eye_y = 17 + dy
    if p.eyes == "open":
        for ex in (17, 28):
            g.rect(ex + dx, eye_y, ex + 2 + dx, eye_y + 2, "eye")
            g.plot(ex + 2 + dx, eye_y, "white")
            g.plot(ex + 1 + dx, eye_y, "white")
            if style.soft_face:
                g.plot(ex + 2 + dx, eye_y + 1, "white")
    elif p.eyes == "wide":
        for ex in (17, 28):
            g.rect(ex + dx, eye_y - 1, ex + 2 + dx, eye_y + 3, "eye")
            g.plot(ex + 2 + dx, eye_y - 1, "white")
            g.plot(ex + 1 + dx, eye_y, "white")
    elif p.eyes == "half":
        for ex in (17, 28):
            g.rect(ex + dx, eye_y + 1, ex + 2 + dx, eye_y + 2, "eye")
            g.plot(ex + 2 + dx, eye_y + 1, "white")
    elif p.eyes == "closed":
        g.hline(17 + dx, 19 + dx, eye_y + 2, "eye")
        g.hline(28 + dx, 30 + dx, eye_y + 2, "eye")
    elif p.eyes == "happy":
        for ex in (17, 28):
            g.plot(ex + dx, eye_y + 2, "eye")
            g.plot(ex + 1 + dx, eye_y + 1, "eye")
            g.plot(ex + 2 + dx, eye_y + 2, "eye")
    mouth_y = 22 + dy
    if p.mouth == "smile":
        g.plot(23 + dx, mouth_y, "eye")
        g.plot(24 + dx, mouth_y + 1, "eye")
        g.plot(25 + dx, mouth_y, "eye")
    elif p.mouth == "open":
        g.rect(23 + dx, mouth_y, 24 + dx, mouth_y + 1, "eye")
    elif p.mouth == "small":
        g.plot(24 + dx, mouth_y, "eye")
    elif p.mouth == "flat":
        g.hline(23 + dx, 25 + dx, mouth_y, "eye")
    elif p.mouth == "grin":
        g.hline(22 + dx, 26 + dx, mouth_y + 1, "eye")
        g.plot(22 + dx, mouth_y, "eye")
        g.plot(26 + dx, mouth_y, "eye")
        g.plot(24 + dx, mouth_y + 1, "white")
    if p.blush:
        if style.soft_face:
            # on the tapering cheek, a row under each eye
            g.hline(16 + dx, 18 + dx, 20 + dy, "blush")
            g.hline(29 + dx, 31 + dx, 20 + dy, "blush")
        else:
            g.hline(15 + dx, 16 + dx, 21 + dy, "blush")
            g.hline(31 + dx, 32 + dx, 21 + dy, "blush")


def draw_torso(g: Grid, p: Pose, style: Style) -> None:
    dy = p.bob + p.jump
    # top body: x15..32, y27..47
    for y in range(28 + dy, 46 + dy):
        g.hline(15, 32, y, "hoodie")
    g.hline(15, 32, 27 + dy, "hoodie")
    g.hline(16, 31, 26 + dy, "hoodie_shadow")
    # shading on the right + hem
    g.vline(31, 29 + dy, 45 + dy, "hoodie_shadow")
    g.hline(15, 32, 45 + dy, "hoodie_shadow")
    if style.sleeveless:
        # scoop neckline: skin shows between the jaw and the collar
        g.hline(21, 26, 27 + dy, "skin")
        g.hline(21, 26, 28 + dy, "skin")
        g.plot(20, 28 + dy, "hoodie_shadow")
        g.plot(27, 28 + dy, "hoodie_shadow")
        g.hline(20, 27, 29 + dy, "hoodie_shadow")
    else:
        # hood collar behind the neck
        g.hline(16, 31, 26 + dy, "hoodie_shadow")
        g.hline(15, 32, 27 + dy, "hoodie")
        g.hline(16, 31, 28 + dy, "hoodie_shadow")
        # drawstrings
        g.vline(20, 29 + dy, 33 + dy, "accent")
        g.vline(27, 29 + dy, 33 + dy, "accent")
        g.plot(20, 34 + dy, "accent")
        g.plot(27, 34 + dy, "accent")
        # kangaroo pocket
        g.hline(18, 29, 39 + dy, "hoodie_deep")
        g.vline(18, 39 + dy, 43 + dy, "hoodie_deep")
        g.vline(29, 39 + dy, 43 + dy, "hoodie_deep")
        g.hline(19, 28, 43 + dy, "hoodie_deep")
    # body outline
    g.vline(14, 28 + dy, 45 + dy, "outline")
    g.vline(33, 28 + dy, 45 + dy, "outline")
    g.plot(14, 27 + dy, "outline")
    g.plot(33, 27 + dy, "outline")
    g.hline(15, 32, 46 + dy, "outline")
    g.hline(16, 31, 47 + dy, "outline")
    g.plot(15, 46 + dy, "outline")
    g.plot(32, 46 + dy, "outline")


def draw_arm(g: Grid, p: Pose, side: str, style: Style) -> None:
    dy = p.bob + p.jump
    pose = p.arm_l if side == "l" else p.arm_r
    if pose == "none":
        return
    # bare arms for the sleeveless style, with a fabric cap at the shoulder
    fill = "skin" if style.sleeveless else "hoodie"
    shade = "skin_shadow" if style.sleeveless else "hoodie_shadow"
    cap_start, cap_end = None, None
    if style.sleeveless:
        if pose == "down":
            cap_start, cap_end = 29, 31
        elif pose in ("up", "stretch"):
            cap_start, cap_end = 27, 29
    if pose == "down":
        x0, x1 = (11, 13) if side == "l" else (34, 36)
        for y in range(29 + dy, 41 + dy):
            g.hline(x0, x1, y, fill)
        g.hline(x0, x1, 41 + dy, shade)
        g.vline(x0 if side == "l" else x1, 29 + dy, 41 + dy, "outline")
        g.hline(x0 + 1 if side == "l" else x0, x1, 28 + dy, "outline")
        g.hline(x0, x1, 42 + dy, "outline")
        # hand
        g.rect(x0 + 1 if side == "l" else x0, 40 + dy, x1, 41 + dy, "skin")
        if cap_start is not None:
            g.hline(x0, x1, cap_start + dy, "hoodie")
            g.hline(x0, x1, cap_end + dy, "hoodie")
    elif pose == "up":
        x0, x1 = (11, 13) if side == "l" else (34, 36)
        for y in range(19 + dy, 29 + dy):
            g.hline(x0, x1, y, fill)
        g.vline(x0 if side == "l" else x1, 19 + dy, 29 + dy, "outline")
        g.hline(x0, x1, 29 + dy, "outline")
        g.hline(x0 + 1 if side == "l" else x0, x1, 18 + dy, "outline")
        g.rect(x0 + 1 if side == "l" else x0, 16 + dy, x1, 17 + dy, "skin")
        if cap_start is not None:
            g.hline(x0, x1, cap_start + dy, "hoodie")
            g.hline(x0, x1, cap_end + dy, "hoodie")
    elif pose == "wave":
        if side == "l":
            return
        phase = p.prop_phase % 2
        hy = 22 + dy - phase
        for y in range(hy, hy + 3):
            g.hline(33, 38, y, fill)
        g.hline(33, 38, hy + 3, shade)
        g.hline(33, 38, hy - 1, "outline")
        g.hline(34, 38, hy + 4, "outline")
        g.vline(33, hy, hy + 4, "outline")
        g.vline(38, hy, hy + 4, "outline")
        g.rect(37, hy - 3, 39, hy - 1, "skin")
        g.plot(39, hy - 4, "skin")
        g.hline(37, 39, hy - 4, "outline")
        g.plot(40, hy - 2, "outline")
    elif pose == "forward":
        if side == "l":
            for x in range(15, 21):
                g.hline(x, x, 34 + dy, fill)
            g.hline(15, 21, 33 + dy, fill)
            g.hline(15, 21, 35 + dy, shade)
            g.hline(15, 20, 32 + dy, "outline")
            g.hline(15, 20, 36 + dy, "outline")
            g.rect(20, 33 + dy, 21, 35 + dy, "skin")
        else:
            for x in range(27, 33):
                g.hline(x, x, 34 + dy, fill)
            g.hline(26, 32, 33 + dy, fill)
            g.hline(26, 32, 35 + dy, shade)
            g.hline(27, 32, 32 + dy, "outline")
            g.hline(27, 32, 36 + dy, "outline")
            g.rect(26, 33 + dy, 27, 35 + dy, "skin")
    elif pose == "book":
        if side == "l":
            for x in range(14, 20):
                g.hline(x, x, 36 + dy, fill)
            g.hline(14, 19, 35 + dy, fill)
            g.hline(14, 19, 37 + dy, shade)
            g.hline(14, 18, 34 + dy, "outline")
            g.hline(14, 18, 38 + dy, "outline")
            g.rect(18, 35 + dy, 19, 37 + dy, "skin")
        else:
            for x in range(28, 34):
                g.hline(x, x, 36 + dy, fill)
            g.hline(28, 33, 35 + dy, fill)
            g.hline(28, 33, 37 + dy, shade)
            g.hline(29, 33, 34 + dy, "outline")
            g.hline(29, 33, 38 + dy, "outline")
            g.rect(28, 35 + dy, 29, 37 + dy, "skin")
    elif pose == "flail":
        if side == "l":
            for x, y in [(13, 30 + dy), (12, 28 + dy), (11, 26 + dy), (11, 24 + dy)]:
                g.hline(x, x + 1, y, fill)
            g.rect(10, 21 + dy, 11, 23 + dy, "skin")
        else:
            for x, y in [(34, 30 + dy), (35, 28 + dy), (36, 26 + dy), (36, 24 + dy)]:
                g.hline(x - 1, x, y, fill)
            g.rect(36, 21 + dy, 37, 23 + dy, "skin")
    elif pose == "cup":
        if side == "l":
            return
        for x in range(29, 34):
            g.hline(x, x, 36 + dy, fill)
        g.hline(29, 33, 35 + dy, fill)
        g.hline(29, 33, 37 + dy, shade)
        g.rect(32, 38 + dy, 33, 39 + dy, "skin")
    elif pose == "stretch":
        x0, x1 = (11, 13) if side == "l" else (34, 36)
        for y in range(18 + dy, 29 + dy):
            g.hline(x0, x1, y, fill)
        g.vline(x0 if side == "l" else x1, 18 + dy, 29 + dy, "outline")
        g.hline(x0, x1, 29 + dy, "outline")
        g.hline(x0 + 1 if side == "l" else x0, x1, 17 + dy, "outline")
        g.rect(x0 + 1 if side == "l" else x0, 14 + dy, x1, 16 + dy, "skin")
        g.plot(x0 if side == "l" else x1 + 1, 14 + dy, "skin")
        g.plot(x0 if side == "l" else x1 + 1, 15 + dy, "skin")
        if cap_start is not None:
            g.hline(x0, x1, cap_start + dy, "hoodie")
            g.hline(x0, x1, cap_end + dy, "hoodie")


def draw_strap(g: Grid, p: Pose) -> None:
    """The black crossbody strap with its bag: over the right shoulder
    (viewer's), bag at the left hip, matching the reference.

    Drawn after the arms (the strap passes over the top) and before the head
    (the braids hang over it), following the torso's bob so it stays on the
    clothes.
    """
    dy = p.bob + p.jump
    for y in range(27, 44):
        x = 31 - (y - 27) * 16 // 17
        g.plot(x, y + dy, "strap")
        g.plot(x + 1, y + dy, "strap")
    # the bag at the strap's low end, hanging at the hip
    g.rect(14, 42 + dy, 19, 46 + dy, "strap")
    g.rect(14, 42 + dy, 19, 43 + dy, "hoodie_deep")
    g.rect_outline(14, 42 + dy, 19, 46 + dy, "outline")
    g.plot(16, 47 + dy, "outline")
    g.plot(17, 47 + dy, "outline")


def draw_legs(g: Grid, p: Pose) -> None:
    jl = p.jump
    if p.legs == "sit":
        g.hline(18, 29, 47 + jl, "pants")
        g.hline(18, 29, 48 + jl, "pants")
        g.hline(19, 28, 49 + jl, "pants_shadow")
        g.hline(17, 30, 50 + jl, "shoe")
        g.hline(17, 30, 51 + jl, "outline")
        g.vline(18, 47 + jl, 48 + jl, "outline")
        g.vline(29, 47 + jl, 48 + jl, "outline")
        return
    if p.legs == "walk_a":
        left = (19, 22, 47 + jl, 55 + jl)
        right = (25, 28, 48 + jl, 58 + jl)
    elif p.legs == "walk_b":
        left = (19, 22, 48 + jl, 58 + jl)
        right = (25, 28, 47 + jl, 55 + jl)
    else:
        left = (19, 22, 47 + jl, 57 + jl)
        right = (25, 28, 47 + jl, 57 + jl)
    for x0, x1, y0, y1 in (left, right):
        for y in range(y0, y1 + 1):
            g.hline(x0, x1, y, "pants")
        g.vline(x1, y0, y1, "pants_shadow")
        g.vline(x0, y0, y1, "outline")
        g.vline(x1, y0, y1, "outline")
        g.hline(x0, x1, y1 + 1, "shoe")
        g.hline(x0, x1, y1 + 2, "outline")
        g.plot(x0 - 1, y1 + 2, "outline")
        g.plot(x1 + 1, y1 + 2, "outline")


# ---------------------------------------------------------------------------
# chibi variant (nana-chibi): two-head-tall body on its own coordinate layout
# ---------------------------------------------------------------------------


def draw_chibi_head(g: Grid, p: Pose) -> None:
    dy, dx = p.bob + p.jump, p.lean
    # skull outline: top arc plus straight sides down to the jaw; the head
    # takes ~60% of the height so the body reads tiny, like the chibi reference
    g.hline(16 + dx, 31 + dx, 6 + dy, "outline")
    for x, y in ((15, 7), (32, 7), (14, 8), (33, 8), (13, 9), (34, 9),
                 (12, 10), (35, 10), (11, 11), (36, 11), (10, 12), (37, 12)):
        g.plot(x + dx, y + dy, "outline")
    g.vline(9 + dx, 13 + dy, 38 + dy, "outline")
    g.vline(38 + dx, 13 + dy, 38 + dy, "outline")
    # hair mass over the skull
    g.hline(16 + dx, 31 + dx, 7 + dy, "hair")
    g.hline(14 + dx, 33 + dx, 8 + dy, "hair")
    g.hline(12 + dx, 35 + dx, 9 + dy, "hair")
    for y in range(10 + dy, 17 + dy):
        g.hline(10 + dx, 37 + dx, y, "hair")
    # face, then a four-step taper to a small rounded chin
    for y in range(17 + dy, 34 + dy):
        g.hline(10 + dx, 37 + dx, y, "skin")
    for y, x0 in ((34, 12), (35, 14), (36, 16), (37, 18)):
        xr = 47 - x0
        g.hline(x0 + dx, xr + dx, y + dy, "skin")
        g.plot(x0 - 1 + dx, y + dy, "outline")
        g.plot(xr + 1 + dx, y + dy, "outline")
    g.hline(18 + dx, 29 + dx, 38 + dy, "outline")
    # close the silhouette between the chin and the braid strands so the
    # transparent canvas never shows through at the neck
    g.hline(13 + dx, 17 + dx, 38 + dy, "outline")
    g.hline(30 + dx, 34 + dx, 38 + dy, "outline")
    g.plot(13 + dx, 39 + dy, "outline")
    g.plot(34 + dx, 39 + dy, "outline")
    # side locks framing the face, continuing into the braid strands
    g.vline(10 + dx, 17 + dy, 38 + dy, "hair")
    g.vline(11 + dx, 17 + dy, 38 + dy, "hair")
    g.vline(36 + dx, 17 + dy, 38 + dy, "hair")
    g.vline(37 + dx, 17 + dy, 38 + dy, "hair")
    g.vline(12 + dx, 27 + dy, 38 + dy, "outline")
    g.vline(35 + dx, 27 + dy, 38 + dy, "outline")
    # curtain bangs combed from the center part along the hairline, falling
    # to just above the eyes so the chibi face stays round and full
    g.hline(10 + dx, 20 + dx, 17 + dy, "hair")
    g.hline(27 + dx, 37 + dx, 17 + dy, "hair")
    g.hline(10 + dx, 19 + dx, 18 + dy, "hair")
    g.hline(28 + dx, 37 + dx, 18 + dy, "hair")
    g.hline(10 + dx, 17 + dx, 19 + dy, "hair")
    g.hline(30 + dx, 37 + dx, 19 + dy, "hair")
    g.hline(10 + dx, 15 + dx, 20 + dy, "hair")
    g.hline(32 + dx, 37 + dx, 20 + dy, "hair")
    g.hline(10 + dx, 13 + dx, 21 + dy, "hair")
    g.hline(34 + dx, 37 + dx, 21 + dy, "hair")
    for x, y in ((20, 17), (27, 17), (19, 18), (28, 18), (17, 19),
                 (30, 19), (15, 20), (32, 20), (13, 21), (34, 21)):
        g.plot(x + dx, y + dy, "hair_shadow")
    # shine on the crown
    g.hline(15 + dx, 21 + dx, 10 + dy, "hair_light")
    g.hline(16 + dx, 20 + dx, 11 + dy, "hair_light")
    draw_chibi_face(g, p, dx, dy)


def draw_chibi_face(g: Grid, p: Pose, dx: int, dy: int) -> None:
    # tall oval eyes with a thick lash line and a big double highlight: the
    # chibi reference's eye recipe, each eye ~1/4 of the face width
    if p.eyes == "open":
        for ex in (13, 28):
            for y in range(22, 28):
                g.hline(ex + dx, ex + 6 + dx, y + dy, "eye")
            g.hline(ex + 1 + dx, ex + 5 + dx, 28 + dy, "eye")
            g.plot(ex + 4 + dx, 23 + dy, "white")
            g.plot(ex + 5 + dx, 23 + dy, "white")
            g.plot(ex + 4 + dx, 24 + dy, "white")
            g.plot(ex + 5 + dx, 24 + dy, "white")
            g.plot(ex + 1 + dx, 26 + dy, "white")
    elif p.eyes == "wide":
        for ex in (13, 28):
            g.hline(ex + 1 + dx, ex + 5 + dx, 21 + dy, "eye")
            for y in range(22, 29):
                g.hline(ex + dx, ex + 6 + dx, y + dy, "eye")
            g.hline(ex + 1 + dx, ex + 5 + dx, 29 + dy, "eye")
            g.plot(ex + 4 + dx, 23 + dy, "white")
            g.plot(ex + 5 + dx, 23 + dy, "white")
            g.plot(ex + 4 + dx, 24 + dy, "white")
            g.plot(ex + 5 + dx, 24 + dy, "white")
            g.plot(ex + 1 + dx, 27 + dy, "white")
    elif p.eyes == "half":
        for ex in (13, 28):
            for y in range(25, 28):
                g.hline(ex + dx, ex + 6 + dx, y + dy, "eye")
            g.hline(ex + 1 + dx, ex + 5 + dx, 28 + dy, "eye")
            g.plot(ex + 4 + dx, 25 + dy, "white")
    elif p.eyes == "closed":
        for ex in (13, 28):
            g.hline(ex + 1 + dx, ex + 5 + dx, 26 + dy, "eye")
            g.plot(ex + dx, 25 + dy, "eye")
            g.plot(ex + 6 + dx, 25 + dy, "eye")
    elif p.eyes == "happy":
        for ex in (13, 28):
            for x, y in ((0, 27), (1, 26), (2, 25), (3, 24), (4, 25),
                         (5, 26), (6, 27)):
                g.plot(ex + x + dx, y + dy, "eye")
    if p.mouth == "smile":
        # closed v-shaped smile: corner pixels over a two-pixel point
        g.plot(22 + dx, 32 + dy, "eye")
        g.plot(25 + dx, 32 + dy, "eye")
        g.hline(23 + dx, 24 + dx, 33 + dy, "eye")
    elif p.mouth == "open":
        g.hline(22 + dx, 25 + dx, 31 + dy, "eye")
        g.hline(21 + dx, 26 + dx, 32 + dy, "eye")
        g.hline(21 + dx, 26 + dx, 33 + dy, "eye")
        g.hline(22 + dx, 25 + dx, 34 + dy, "blush")
    elif p.mouth == "small":
        g.plot(24 + dx, 33 + dy, "eye")
    elif p.mouth == "flat":
        g.hline(23 + dx, 25 + dx, 33 + dy, "eye")
    elif p.mouth == "grin":
        g.hline(21 + dx, 26 + dx, 33 + dy, "eye")
        g.plot(21 + dx, 32 + dy, "eye")
        g.plot(26 + dx, 32 + dy, "eye")
        g.hline(23 + dx, 24 + dx, 34 + dy, "white")
    if p.blush:
        g.hline(12 + dx, 14 + dx, 30 + dy, "blush")
        g.hline(33 + dx, 35 + dx, 30 + dy, "blush")


def draw_chibi_braids(g: Grid, p: Pose) -> None:
    """Straight twin locks hanging beside the body, flower at each tip.

    The locks continue the head's side-lock columns straight down: no twist
    pattern and no drape over the shoulders. Sitting shortens them so the
    tips stay above the sit ground line. A hair tie and a tiny five-pixel
    flower close each tip after the chibi reference's floral accents.
    """
    dy, dx = p.bob + p.jump, p.lean
    end = 44 if p.legs == "sit" else 48
    for side in ("l", "r"):
        if side == "l":
            outer, inner, fill_a, fill_b, fx = 9, 12, 10, 11, 11
        else:
            outer, inner, fill_a, fill_b, fx = 38, 35, 36, 37, 36
        for y in range(39, end + 1):
            g.plot(outer + dx, y + dy, "outline")
            g.plot(fill_a + dx, y + dy, "hair")
            g.plot(fill_b + dx, y + dy, "hair")
            g.plot(inner + dx, y + dy, "outline")
        for x in (fill_a, fill_b):
            g.plot(x + dx, end + 1 + dy, "accent")
        g.plot(fx + dx, end + 2 + dy, "white")
        g.plot(fx - 1 + dx, end + 3 + dy, "white")
        g.plot(fx + 1 + dx, end + 3 + dy, "white")
        g.plot(fx + dx, end + 4 + dy, "white")
        g.plot(fx + dx, end + 3 + dy, "star")


def draw_chibi_torso(g: Grid, p: Pose) -> None:
    dy = p.bob + p.jump
    sit = p.legs == "sit"
    hem = 44 if sit else 48
    for y in range(40 + dy, hem + dy):
        g.hline(18, 29, y, "hoodie")
    g.hline(18, 21, 39 + dy, "outline")
    g.hline(26, 29, 39 + dy, "outline")
    g.hline(22, 25, 39 + dy, "skin")
    g.hline(22, 25, 40 + dy, "skin")
    g.hline(21, 28, 41 + dy, "hoodie_shadow")
    g.vline(28, 42 + dy, hem - 1 + dy, "hoodie_shadow")
    g.hline(18, 29, hem + dy, "hoodie_shadow")
    g.vline(17, 39 + dy, hem + dy, "outline")
    g.vline(30, 39 + dy, hem + dy, "outline")
    g.hline(18, 29, hem + 1 + dy, "outline")


def draw_chibi_arm(g: Grid, p: Pose, side: str) -> None:
    dy = p.bob + p.jump
    pose = p.arm_l if side == "l" else p.arm_r
    if pose == "none":
        return
    x0, x1 = (15, 16) if side == "l" else (31, 32)
    xo = 14 if side == "l" else 33
    if pose == "down":
        for y in range(43 + dy, 50 + dy):
            g.hline(x0, x1, y, "skin")
        g.hline(x0, x1, 41 + dy, "hoodie")
        g.hline(x0, x1, 42 + dy, "hoodie")
        g.vline(xo, 40 + dy, 49 + dy, "outline")
        g.hline(x0, x1, 40 + dy, "outline")
        g.hline(x0, x1, 50 + dy, "outline")
    elif pose in ("up", "stretch"):
        # the chibi head is wider than the shoulder line, so raised arms must
        # sit outside the head silhouette or they vanish behind it
        top = 26 if pose == "stretch" else 28
        if side == "l":
            x0, x1, xo = 6, 7, 5
        else:
            x0, x1, xo = 40, 41, 42
        for y in range(top + 2 + dy, 43 + dy):
            g.hline(x0, x1, y, "skin")
        g.rect(x0, top + dy, x1, top + 1 + dy, "skin")
        g.hline(x0, x1, 41 + dy, "hoodie")
        g.hline(x0, x1, 42 + dy, "hoodie")
        g.vline(xo, top + dy, 42 + dy, "outline")
        g.hline(x0, x1, top - 1 + dy, "outline")
        g.hline(x0, x1, 43 + dy, "outline")
        # sleeve bridge back to the torso; the braid strands drawn later
        # cover the middle so the sleeve passes behind the hair
        if side == "l":
            g.hline(6, 17, 41 + dy, "hoodie")
            g.hline(6, 17, 42 + dy, "hoodie")
            g.hline(6, 16, 43 + dy, "outline")
        else:
            g.hline(30, 41, 41 + dy, "hoodie")
            g.hline(30, 41, 42 + dy, "hoodie")
            g.hline(31, 41, 43 + dy, "outline")
    elif pose == "wave":
        if side == "l":
            return
        phase = p.prop_phase % 2
        hy = 30 + dy - phase
        for y in range(hy, hy + 3):
            g.hline(38, 43, y, "skin")
        g.hline(38, 43, hy + 3, "skin_shadow")
        g.hline(38, 43, hy - 1, "outline")
        g.hline(39, 43, hy + 4, "outline")
        g.vline(38, hy, hy + 4, "outline")
        g.vline(43, hy, hy + 4, "outline")
        g.rect(42, hy - 3, 44, hy - 1, "skin")
        g.plot(44, hy - 4, "skin")
        g.hline(42, 44, hy - 4, "outline")
        g.plot(45, hy - 2, "outline")
    elif pose in ("forward", "book"):
        ay = 45 if pose == "forward" else 46
        if side == "l":
            g.rect(15, ay + dy, 20, ay + 2 + dy, "skin")
            g.hline(15, 20, ay - 1 + dy, "outline")
            g.hline(15, 20, ay + 3 + dy, "outline")
        else:
            g.rect(27, ay + dy, 32, ay + 2 + dy, "skin")
            g.hline(27, 32, ay - 1 + dy, "outline")
            g.hline(27, 32, ay + 3 + dy, "outline")
    elif pose == "flail":
        if side == "l":
            for x, y in [(10, 40), (9, 38), (8, 36), (8, 34)]:
                g.hline(x, x + 1, y + dy, "skin")
            g.rect(7, 31 + dy, 8, 33 + dy, "skin")
        else:
            for x, y in [(37, 40), (38, 38), (39, 36), (39, 34)]:
                g.hline(x - 1, x, y + dy, "skin")
            g.rect(39, 31 + dy, 40, 33 + dy, "skin")
    elif pose == "cup":
        if side == "l":
            return
        g.rect(27, 44 + dy, 31, 46 + dy, "skin")
        g.hline(27, 31, 46 + dy, "skin_shadow")


def draw_chibi_strap(g: Grid, p: Pose) -> None:
    dy = p.bob + p.jump
    for y in range(39, 47):
        x = 27 - (y - 39)
        g.plot(x, y + dy, "strap")
    g.rect(14, 46 + dy, 18, 49 + dy, "strap")
    g.rect(14, 46 + dy, 18, 47 + dy, "hoodie_deep")
    g.rect_outline(14, 46 + dy, 18, 49 + dy, "outline")


def draw_chibi_legs(g: Grid, p: Pose) -> None:
    jl = p.jump
    if p.legs == "sit":
        g.hline(18, 29, 46 + jl, "pants")
        g.hline(18, 29, 47 + jl, "pants")
        g.hline(19, 28, 48 + jl, "pants_shadow")
        g.hline(17, 30, 49 + jl, "shoe")
        g.hline(17, 30, 50 + jl, "shoe")
        g.hline(17, 30, 51 + jl, "outline")
        g.vline(18, 46 + jl, 47 + jl, "outline")
        g.vline(29, 46 + jl, 47 + jl, "outline")
        return
    if p.legs == "walk_a":
        left = (20, 23, 50 + jl, 53 + jl)
        right = (25, 28, 50 + jl, 56 + jl)
    elif p.legs == "walk_b":
        left = (20, 23, 50 + jl, 56 + jl)
        right = (25, 28, 50 + jl, 53 + jl)
    else:
        left = (20, 23, 50 + jl, 56 + jl)
        right = (25, 28, 50 + jl, 56 + jl)
    for x0, x1, y0, y1 in (left, right):
        for y in range(y0, y1 + 1):
            g.hline(x0, x1, y, "pants")
        g.vline(x0, y0, y1, "outline")
        g.vline(x1, y0, y1, "outline")
        g.hline(x0, x1, y1 + 1, "shoe")
        g.hline(x0, x1, y1 + 2, "shoe")
        g.hline(x0 - 1, x1 + 1, y1 + 3, "outline")


def draw_chibi_frame(g: Grid, p: Pose) -> None:
    draw_chibi_legs(g, p)
    draw_chibi_torso(g, p)
    draw_chibi_arm(g, p, "l")
    draw_chibi_arm(g, p, "r")
    draw_chibi_braids(g, p)
    draw_chibi_strap(g, p)
    draw_chibi_head(g, p)
# props
# ---------------------------------------------------------------------------


def draw_props(g: Grid, p: Pose, style: Style) -> None:
    phase = p.prop_phase
    for prop in p.props:
        if prop == "zzz":
            if style.chibi:
                draw_z(g, 39, 5 - (phase % 2), 4, "zzz")
                draw_z(g, 43, 10 - (phase % 2), 2, "zzz")
            else:
                draw_z(g, 36, 8 - (phase % 2), 4, "zzz")
                draw_z(g, 41, 13 - (phase % 2), 2, "zzz")
        elif prop == "heart":
            draw_heart(g, 39 if style.chibi else 36, 3 if style.chibi else 6)
        elif prop == "sparkles":
            if style.chibi:
                draw_sparkle(g, 7, 14 + (phase % 2))
                draw_sparkle(g, 39, 20 - (phase % 2))
                draw_sparkle(g, 5, 4)
            else:
                draw_sparkle(g, 8, 12 + (phase % 2))
                draw_sparkle(g, 38, 18 - (phase % 2))
                draw_sparkle(g, 24, 3)
        elif prop == "exclaim":
            x = 39 if style.chibi else 38
            g.vline(x, 6, 9, "star")
            g.plot(x, 11, "star")
        elif prop == "envelope":
            y = 40 if style.chibi else 32
            draw_envelope(g, 19, y + p.bob + p.jump, phase)
        elif prop == "clock":
            draw_clock(g, 39 if style.chibi else 36, 8 + phase % 2)
        elif prop == "confetti":
            pts = [
                (10, 10), (36, 6), (14, 4), (40, 14), (7, 20), (42, 24),
                (12, 28), (38, 32), (20, 4), (30, 5), (6, 8), (43, 18),
            ]
            colors = ["confetti_a", "confetti_b", "confetti_c"]
            for i, (x, y) in enumerate(pts[(phase * 3) % 4:]):
                c = colors[(i + phase) % 3]
                g.plot(x, y + (phase % 2), c)
                if i % 3 == 0:
                    g.plot(x + 1, y + (phase % 2), c)
        elif prop == "teacup":
            y0 = (43 if style.chibi else 39) + p.bob + p.jump
            g.hline(27, 32, y0, "paper")
            g.hline(27, 32, y0 + 1, "paper")
            g.hline(28, 31, y0 + 2, "paper_line")
            g.plot(26, y0, "paper")
            g.plot(33, y0, "paper")
            g.hline(27, 32, y0 - 1, "outline")
            g.vline(27, y0, y0 + 1, "outline")
            g.vline(32, y0, y0 + 1, "outline")
            sx = 29 + (phase % 2)
            g.plot(sx, y0 - 3, "white")
            g.plot(sx + 1, y0 - 4, "white")
            g.plot(sx - 1 + phase % 2, y0 - 5, "white")
        elif prop == "notebook":
            y0 = (44 if style.chibi else 37) + p.bob + p.jump
            g.rect(18, y0, 29, y0 + 5, "paper")
            g.rect_outline(18, y0, 29, y0 + 5, "outline")
            g.vline(23, y0 + 1, y0 + 4, "paper_line")
            g.hline(19, 22, y0 + 2, "paper_line")
            g.hline(24, 28, y0 + 2, "paper_line")
            px = 25 + phase % 2
            g.vline(px, y0 + 3, y0 + 5, "wood")
            g.plot(px, y0 + 6, "outline")
        elif prop == "sandclock":
            x0, y0 = (38 if style.chibi else 37), 8 + phase % 2
            g.rect_outline(x0, y0, x0 + 4, y0 + 5, "star")
            g.plot(x0 + 2, y0 + 2, "star")
            g.plot(x0 + 2, y0 + 3, "star")
            g.hline(x0, x0 + 4, y0, "star")
            g.hline(x0, x0 + 4, y0 + 5, "star")


def draw_z(g: Grid, x: int, y: int, size: int, color: str) -> None:
    g.hline(x, x + size - 1, y, color)
    g.hline(x, x + size - 1, y + size - 1, color)
    for i in range(1, size - 1):
        g.plot(x + size - 1 - i, y + i, color)


def draw_heart(g: Grid, x: int, y: int, color: str = "confetti_a") -> None:
    rows = [
        ".#.#.",
        "#####",
        "#####",
        ".###.",
        "..#..",
    ]
    for j, row in enumerate(rows):
        for i, c in enumerate(row):
            if c == "#":
                g.plot(x + i, y + j, color)


def draw_sparkle(g: Grid, x: int, y: int, color: str = "star") -> None:
    g.plot(x, y - 1, color)
    g.plot(x - 1, y, color)
    g.plot(x, y, color)
    g.plot(x + 1, y, color)
    g.plot(x, y + 1, color)


def draw_envelope(g: Grid, x: int, y: int, phase: int) -> None:
    g.rect(x, y, x + 9, y + 6, "paper")
    g.rect_outline(x, y, x + 9, y + 6, "outline")
    for i in range(1, 5):
        g.plot(x + i, y + i, "paper_line")
        g.plot(x + 9 - i, y + i, "paper_line")
    g.plot(x + 4, y + 4, "paper_line")
    g.plot(x + 5, y + 4, "paper_line")
    g.plot(x + 4 + phase % 2, y - 2, "zzz")
    g.plot(x + 5 + phase % 2, y - 3, "zzz")


def draw_clock(g: Grid, x: int, y: int) -> None:
    g.disc(x + 3, y + 3, 3, "paper")
    g.disc(x + 3, y + 3, 3, "paper")
    g.rect_outline(x, y, x + 6, y + 6, "outline")
    g.plot(x + 3, y + 1, "outline")
    g.plot(x + 3, y + 5, "outline")
    g.plot(x + 1, y + 3, "outline")
    g.plot(x + 5, y + 3, "outline")
    g.vline(x + 3, y + 2, y + 3, "star")
    g.hline(x + 3, x + 4, y + 3, "star")


# ---------------------------------------------------------------------------
# animations
# ---------------------------------------------------------------------------

IDLE = Pose()

ANIMATIONS: dict[str, dict] = {
    "idle": {
        "fps": 3,
        "loop": True,
        "poses": [
            IDLE,
            replace(IDLE, bob=1),
            replace(IDLE, bob=1),
            IDLE,
        ],
    },
    "blink": {
        "fps": 8,
        "loop": False,
        "poses": [
            IDLE,
            replace(IDLE, eyes="half"),
            replace(IDLE, eyes="closed"),
            replace(IDLE, eyes="closed"),
            IDLE,
        ],
    },
    "wave": {
        "fps": 6,
        "loop": False,
        "poses": [
            replace(IDLE, arm_r="wave", prop_phase=0, mouth="open", blush=False),
            replace(IDLE, arm_r="wave", prop_phase=1, mouth="grin"),
            replace(IDLE, arm_r="wave", prop_phase=0, mouth="open", blush=False),
            replace(IDLE, arm_r="wave", prop_phase=1, mouth="grin"),
            IDLE,
            replace(IDLE, eyes="happy"),
        ],
    },
    "walk": {
        "fps": 8,
        "loop": True,
        "poses": [
            replace(IDLE, legs="walk_a", prop_phase=0),
            replace(IDLE, legs="walk_a", bob=1, prop_phase=0),
            replace(IDLE, legs="walk_b", prop_phase=1),
            replace(IDLE, legs="walk_b", bob=1, prop_phase=1),
            replace(IDLE, legs="walk_a", prop_phase=0),
            replace(IDLE, legs="walk_a", bob=1, prop_phase=0),
        ],
    },
    "sleep": {
        "fps": 2,
        "loop": True,
        "poses": [
            replace(IDLE, eyes="closed", mouth="small", lean=1, props=("zzz",),
                    prop_phase=0, blush=False),
            replace(IDLE, eyes="closed", mouth="small", lean=1, props=("zzz",),
                    prop_phase=1, blush=False),
            replace(IDLE, eyes="closed", mouth="small", lean=1, bob=1, props=("zzz",),
                    prop_phase=0, blush=False),
            replace(IDLE, eyes="closed", mouth="small", lean=1, bob=1, props=("zzz",),
                    prop_phase=1, blush=False),
        ],
    },
    "work": {
        "fps": 3,
        "loop": True,
        "poses": [
            replace(IDLE, arm_l="book", arm_r="book", eyes="half",
                    props=("notebook",), prop_phase=0),
            replace(IDLE, arm_l="book", arm_r="book", eyes="half",
                    props=("notebook",), prop_phase=1),
            replace(IDLE, arm_l="book", arm_r="book", eyes="half", bob=1,
                    props=("notebook",), prop_phase=0),
            replace(IDLE, arm_l="book", arm_r="book", eyes="half", bob=1,
                    props=("notebook",), prop_phase=1),
        ],
    },
    "stretch": {
        "fps": 5,
        "loop": False,
        "poses": [
            IDLE,
            replace(IDLE, arm_l="stretch", arm_r="stretch", bob=1),
            replace(IDLE, arm_l="stretch", arm_r="stretch", bob=-1, mouth="open"),
            replace(IDLE, arm_l="stretch", arm_r="stretch", bob=-1, mouth="open"),
            replace(IDLE, arm_l="stretch", arm_r="stretch", bob=0, eyes="happy"),
            IDLE,
        ],
    },
    "react": {
        "fps": 8,
        "loop": False,
        "poses": [
            replace(IDLE, eyes="wide", mouth="open", props=("exclaim",),
                    lean=-1, blush=False),
            replace(IDLE, eyes="wide", mouth="open", props=("exclaim",),
                    lean=-1, blush=False, bob=1),
            replace(IDLE, eyes="wide", mouth="small", props=("exclaim",), lean=0),
            IDLE,
        ],
    },
    "poke_react": {
        "fps": 8,
        "loop": False,
        "poses": [
            replace(IDLE, eyes="wide", mouth="open", lean=-1, blush=False,
                    props=("exclaim",)),
            replace(IDLE, eyes="closed", mouth="open", lean=-2, arm_l="flail",
                    arm_r="flail", props=("sparkles",), prop_phase=0, blush=False),
            replace(IDLE, eyes="closed", mouth="open", lean=-2, jump=-1,
                    arm_l="flail", arm_r="flail", props=("heart", "sparkles"),
                    prop_phase=1, blush=False),
            replace(IDLE, eyes="happy", mouth="open", lean=-1, jump=-1,
                    arm_l="up", arm_r="up", props=("heart",), prop_phase=0),
            replace(IDLE, eyes="happy", mouth="smile", lean=0,
                    props=("heart",), prop_phase=1),
            IDLE,
        ],
    },
    "message_react": {
        "fps": 6,
        "loop": False,
        "poses": [
            replace(IDLE, eyes="wide", mouth="small", props=("exclaim",),
                    blush=False),
            replace(IDLE, arm_l="forward", arm_r="forward", eyes="open",
                    mouth="smile", props=("envelope",), prop_phase=0),
            replace(IDLE, arm_l="forward", arm_r="forward", eyes="open",
                    mouth="open", props=("envelope",), prop_phase=1, bob=1),
            replace(IDLE, arm_l="forward", arm_r="forward", eyes="happy",
                    mouth="smile", props=("envelope",), prop_phase=0),
        ],
    },
    "invite_react": {
        "fps": 6,
        "loop": False,
        "poses": [
            replace(IDLE, eyes="wide", mouth="small", props=("sandclock",),
                    blush=False),
            replace(IDLE, eyes="happy", mouth="grin", bob=1,
                    props=("sandclock",), prop_phase=1),
            replace(IDLE, eyes="happy", mouth="grin",
                    props=("sandclock",), prop_phase=0),
            replace(IDLE, eyes="happy", mouth="grin", bob=1,
                    props=("sandclock",), prop_phase=1),
        ],
    },
    "celebrate": {
        "fps": 8,
        "loop": False,
        "poses": [
            replace(IDLE, arm_l="up", arm_r="up", eyes="happy", mouth="grin",
                    props=("confetti",), prop_phase=0),
            replace(IDLE, arm_l="up", arm_r="up", eyes="happy", mouth="open",
                    jump=-1, props=("confetti",), prop_phase=1),
            replace(IDLE, arm_l="up", arm_r="up", eyes="happy", mouth="grin",
                    jump=-2, props=("confetti",), prop_phase=2),
            replace(IDLE, arm_l="up", arm_r="up", eyes="happy", mouth="open",
                    jump=-1, props=("confetti",), prop_phase=3),
            replace(IDLE, arm_l="up", arm_r="up", eyes="happy", mouth="grin",
                    jump=-1, props=("confetti",), prop_phase=0),
            IDLE,
        ],
    },
    "rest": {
        "fps": 3,
        "loop": True,
        "poses": [
            replace(IDLE, legs="sit", arm_r="cup", eyes="happy", mouth="small",
                    props=("teacup",), prop_phase=0),
            replace(IDLE, legs="sit", arm_r="cup", eyes="closed", mouth="small",
                    props=("teacup",), prop_phase=1),
            replace(IDLE, legs="sit", arm_r="cup", eyes="happy", mouth="small",
                    bob=1, props=("teacup",), prop_phase=0),
            replace(IDLE, legs="sit", arm_r="cup", eyes="closed", mouth="small",
                    bob=1, props=("teacup",), prop_phase=1),
        ],
    },
}


# ---------------------------------------------------------------------------
# sheet packing
# ---------------------------------------------------------------------------


def render_frame(pose: Pose, pal: dict[str, str], style: Style) -> Image.Image:
    g = Grid(FRAME_W, FRAME_H)
    if style.chibi:
        draw_chibi_frame(g, pose)
    else:
        draw_hair_back(g, pose)
        draw_legs(g, pose)
        draw_torso(g, pose, style)
        if style.braids:
            draw_braids(g, pose)
        draw_arm(g, pose, "l", style)
        draw_arm(g, pose, "r", style)
        if style.strap:
            draw_strap(g, pose)
        draw_head(g, pose, style)
    draw_props(g, pose, style)
    g.translate(0, 2)
    return g.to_image(pal)


def build_sheets() -> dict:
    manifest = {"format_version": 1, "characters": {}}
    for char_name, pal in PALETTES.items():
        style = STYLES.get(char_name, Style())
        char_dir = OUT / char_name
        char_dir.mkdir(parents=True, exist_ok=True)
        anims: dict[str, dict] = {}
        for anim_name, spec in ANIMATIONS.items():
            frames = [render_frame(p, pal, style) for p in spec["poses"]]
            sheet = Image.new("RGBA", (FRAME_W * len(frames) * SCALE, FRAME_H * SCALE),
                              (0, 0, 0, 0))
            for i, fr in enumerate(frames):
                big = fr.resize((FRAME_W * SCALE, FRAME_H * SCALE), Image.NEAREST)
                sheet.paste(big, (i * FRAME_W * SCALE, 0))
            rel = f"{char_name}/{anim_name}.png"
            sheet.save(char_dir / f"{anim_name}.png")
            anims[anim_name] = {
                "file": rel,
                "frames": len(frames),
                "fps": spec["fps"],
                "loop": spec["loop"],
            }
        manifest["characters"][char_name] = {
            "frame_width": FRAME_W * SCALE,
            "frame_height": FRAME_H * SCALE,
            "animations": anims,
        }
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return manifest


def build_tray_icon() -> None:
    """32x32 tray icon cropped from the idle frame head."""
    frame = render_frame(IDLE, PALETTES["momo"], STYLES["momo"])
    head = frame.crop((10, 5, 38, 33)).resize((32, 32), Image.NEAREST)
    out = OUT.parent / "icons"
    out.mkdir(parents=True, exist_ok=True)
    head.save(out / "tray.png")


def build_contact_sheet() -> None:
    """One PNG per character with the first frame of every animation."""
    target = REPO / "target" / "insync-preview"
    target.mkdir(parents=True, exist_ok=True)
    cols = len(ANIMATIONS)
    for char_name, pal in PALETTES.items():
        style = STYLES.get(char_name, Style())
        sheet = Image.new("RGBA", (FRAME_W * cols * SCALE, FRAME_H * SCALE * 2),
                          (250, 245, 238, 255))
        for i, (name, spec) in enumerate(ANIMATIONS.items()):
            f0 = render_frame(spec["poses"][0], pal, style).resize(
                (FRAME_W * SCALE, FRAME_H * SCALE), Image.NEAREST)
            f1 = render_frame(spec["poses"][min(2, len(spec["poses"]) - 1)],
                              pal, style).resize(
                (FRAME_W * SCALE, FRAME_H * SCALE), Image.NEAREST)
            sheet.paste(f0, (i * FRAME_W * SCALE, 0))
            sheet.paste(f1, (i * FRAME_W * SCALE, FRAME_H * SCALE))
        sheet.save(target / f"contact-sheet-{char_name}.png")


def main() -> None:
    build_sheets()
    build_tray_icon()
    build_contact_sheet()
    print(f"wrote {len(ANIMATIONS)} animations x {len(PALETTES)} palettes to {OUT}")


if __name__ == "__main__":
    main()
