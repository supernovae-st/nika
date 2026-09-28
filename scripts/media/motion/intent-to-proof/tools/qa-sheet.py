"""Assemble the exported QA stills into one labeled diagnostic sheet.

usage: python3 tools/qa-sheet.py  (after `npm run exports`)
reads .cache/dist/stills/t*.png → writes .cache/dist/qa-stills-sheet.png
"""
import pathlib

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).resolve().parent.parent
SRC = HERE / ".cache" / "dist" / "stills"
OUT = HERE / ".cache" / "dist" / "qa-stills-sheet.png"

files = sorted(SRC.glob("t*.png"), key=lambda p: float(p.stem[1:]))
cols, tw, th, pad, lab = 4, 640, 360, 8, 22
rows = (len(files) + cols - 1) // cols
sheet = Image.new("RGB", (cols * (tw + pad) + pad, rows * (th + lab + pad) + pad), (6, 8, 14))
draw = ImageDraw.Draw(sheet)
for i, f in enumerate(files):
    im = Image.open(f).convert("RGB").resize((tw, th), Image.LANCZOS)
    x = pad + (i % cols) * (tw + pad)
    y = pad + (i // cols) * (th + lab + pad)
    sheet.paste(im, (x, y))
    draw.text((x + 4, y + th + 4), f"{float(f.stem[1:]):.2f} s", fill=(159, 208, 255))
sheet.save(OUT, optimize=True)
print(OUT, len(files), "stills")
