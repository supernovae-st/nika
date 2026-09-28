"""Export glyph outlines as SVG path data (font units, y up).

Skia stops rasterizing text beyond a size limit, so shapes used as masks at
extreme zoom are drawn from their outlines as vector paths instead.
Usage: python3 tools/glyph-paths.py <font.ttf> <chars> <out.json>
"""
import json
import sys
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen

font_path, chars, out = sys.argv[1], sys.argv[2], sys.argv[3]
font = TTFont(font_path)
gs = font.getGlyphSet()
cmap = font.getBestCmap()
data = {"unitsPerEm": font["head"].unitsPerEm, "glyphs": {}}
for ch in sorted(set(chars)):
    name = cmap.get(ord(ch))
    if not name:
        continue
    pen = SVGPathPen(gs)
    gs[name].draw(pen)
    data["glyphs"][ch] = {"d": pen.getCommands(), "adv": gs[name].width}
with open(out, "w") as fh:
    json.dump(data, fh)
print(out, len(data["glyphs"]), "glyphs")
