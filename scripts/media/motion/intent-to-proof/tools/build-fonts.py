"""Instantiate static font cuts from the repo's OFL variable fonts.

The film renderer (Skia via @napi-rs/canvas) addresses fonts by family name,
so each weight/width we use becomes its own static TTF under .cache/fonts/.
Sources: scripts/media/motion/intent-to-impact/assets/fonts/ (OFL, committed).
"""
import pathlib
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

HERE = pathlib.Path(__file__).resolve().parent.parent
SRC = HERE.parent / "intent-to-impact" / "assets" / "fonts"
OUT = HERE / ".cache" / "fonts"
OUT.mkdir(parents=True, exist_ok=True)

# family alias -> (source file, axis location)
CUTS = {
    "Geist 300": ("geist-variable.woff2", {"wght": 300}),
    "Geist 400": ("geist-variable.woff2", {"wght": 400}),
    "Geist 500": ("geist-variable.woff2", {"wght": 500}),
    "Geist 600": ("geist-variable.woff2", {"wght": 600}),
    "Geist 700": ("geist-variable.woff2", {"wght": 700}),
    "Geist 800": ("geist-variable.woff2", {"wght": 800}),
    "MG 400": ("martian-grotesk-variable.woff2", {"wght": 400, "wdth": 100}),
    "MG 600": ("martian-grotesk-variable.woff2", {"wght": 600, "wdth": 100}),
    "MGW 300": ("martian-grotesk-variable.woff2", {"wght": 300, "wdth": 160}),
    "MGW 500": ("martian-grotesk-variable.woff2", {"wght": 500, "wdth": 160}),
    "MGW 700": ("martian-grotesk-variable.woff2", {"wght": 700, "wdth": 160}),
    "MGU 400": ("martian-grotesk-variable.woff2", {"wght": 400, "wdth": 200}),
    "MGU 800": ("martian-grotesk-variable.woff2", {"wght": 800, "wdth": 200}),
    "MGC 500": ("martian-grotesk-variable.woff2", {"wght": 500, "wdth": 75}),
    "MM 300": ("martian-mono-variable.woff2", {"wght": 300, "wdth": 100}),
    "MM 400": ("martian-mono-variable.woff2", {"wght": 400, "wdth": 100}),
    "MM 500": ("martian-mono-variable.woff2", {"wght": 500, "wdth": 100}),
    "MM 700": ("martian-mono-variable.woff2", {"wght": 700, "wdth": 100}),
}

for alias, (src, loc) in CUTS.items():
    dst = OUT / (alias.replace(" ", "-") + ".ttf")
    if dst.exists():
        continue
    font = TTFont(SRC / src)
    static = instancer.instantiateVariableFont(font, loc)
    # rename so Skia sees one family per cut
    name = static["name"]
    for rec in list(name.names):
        if rec.nameID in (1, 4, 16, 3, 6, 17, 21, 22):
            name.removeNames(nameID=rec.nameID)
    name.setName(alias, 1, 3, 1, 0x409)
    name.setName("Regular", 2, 3, 1, 0x409)
    name.setName(alias, 4, 3, 1, 0x409)
    name.setName(alias.replace(" ", "-"), 6, 3, 1, 0x409)
    name.setName(alias + " Regular", 3, 3, 1, 0x409)
    static.flavor = None
    static.save(dst)
    print("built", dst.name)
