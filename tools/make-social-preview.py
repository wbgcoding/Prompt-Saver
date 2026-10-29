"""Builds docs/social-preview.png (1280x640) from a main-window shot.

Run:  python tools/make-social-preview.py <shot.png> [out.png]

The window is centred on a dark background with a soft drop shadow and rounded
corners, everything inside a 40 px safe margin.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SIZE = (1280, 640)
MARGIN = 40
RADIUS = 14
BG = (18, 18, 20)
GLOW = (36, 64, 120)


def rounded(img, radius):
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, img.width - 1, img.height - 1), radius, fill=255)
    out = img.convert("RGBA")
    out.putalpha(mask)
    return out


def background():
    w, h = SIZE
    glow = Image.new("L", SIZE, 0)
    ImageDraw.Draw(glow).ellipse((w * 0.2, h * -0.1, w * 0.8, h * 1.1), fill=110)
    glow = glow.filter(ImageFilter.GaussianBlur(160))
    return Image.composite(Image.new("RGB", SIZE, GLOW), Image.new("RGB", SIZE, BG), glow).convert("RGBA")


def main(shot_path, out_path):
    shot = Image.open(shot_path).convert("RGB")
    box = (SIZE[0] - 2 * MARGIN, SIZE[1] - 2 * MARGIN)
    k = min(box[0] / shot.width, box[1] / shot.height)
    size = (round(shot.width * k), round(shot.height * k))
    win = rounded(shot.resize(size, Image.LANCZOS), RADIUS)
    pos = ((SIZE[0] - size[0]) // 2, (SIZE[1] - size[1]) // 2)
    shade = Image.new("RGBA", SIZE, (0, 0, 0, 0))
    x, y = pos
    ImageDraw.Draw(shade).rounded_rectangle((x, y + 8, x + size[0], y + size[1] + 8), RADIUS, fill=(0, 0, 0, 170))
    canvas = background()
    canvas.alpha_composite(shade.filter(ImageFilter.GaussianBlur(14)))
    canvas.alpha_composite(win, pos)
    canvas.convert("RGB").save(out_path, optimize=True)
    print(out_path, SIZE)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ROOT / "docs" / "social-preview.png")
