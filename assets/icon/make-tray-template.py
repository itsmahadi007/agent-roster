"""Draws the macOS menu bar template icon: icon-a's three jacks and cable, black on transparent.

Run: python3 assets/icon/make-tray-template.py  (needs Pillow)
"""
from pathlib import Path
from PIL import Image, ImageDraw

SIZE = 44          # 22 pt at 2x
SCALE = 8          # draw big, then downsample for smooth edges
OUT = Path(__file__).resolve().parents[2] / "app/src-tauri/icons/tray-template.png"


def u(v):
    return round(v * SCALE)


img = Image.new("RGBA", (u(SIZE), u(SIZE)), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
black = (0, 0, 0, 255)

# cable: left half of an ellipse joining the top and bottom jacks
cx, cy, rx, ry = 30, 22, 20, 14.5
d.arc([u(cx - rx), u(cy - ry), u(cx + rx), u(cy + ry)], 90, 270, fill=black, width=u(3.2))

# three jacks (rings) on the right
for y in (7.5, 22, 36.5):
    r = 6.2
    d.ellipse([u(cx - r), u(y - r), u(cx + r), u(y + r)], outline=black, width=u(2.8))

img.resize((SIZE, SIZE), Image.LANCZOS).save(OUT)
print(OUT)
