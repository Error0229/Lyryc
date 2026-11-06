"""Lyryc's icon: the カシ seal, for the app and the tray alike.

Regenerate: run with a Python that has fonttools, Pillow and resvg-py, on a
machine with Noto Sans JP installed; then `npx tauri icon app-1024.png` for
the app set, and copy tray-N.png to ../tray/N.png. The SVGs here are already
outlined and need no fonts.

カシ (kashi, "lyrics") in Noto Sans JP Black (OFL), paper-coloured on a solid
teal seal: two simple katakana stay legible at 16 px. Sizes are rendered at
their real pixel size so they can be judged as they'll actually appear.
"""
import io, os, sys
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.transformPen import TransformPen
from PIL import Image, ImageDraw
import resvg_py

OUT = os.path.dirname(os.path.abspath(__file__))
FONTS = r"C:\Windows\Fonts"
INK = "#3F6F75"     # 錆浅葱 (sabi-asagi) teal: the app's brand colour
PAPER = "#F5EDE0"   # warm paper


def load(name, wght):
    f = TTFont(os.path.join(FONTS, name))
    return instantiateVariableFont(f, {"wght": wght}) if "fvar" in f else f


SANS = load("NotoSansJP-VF.ttf", 900)


def glyph(font, ch, cx, cy, height, fill):
    """`ch` outlined as an SVG path, its ink box centred on (cx, cy), `height` tall."""
    gs = font.getGlyphSet()
    name = font.getBestCmap()[ord(ch)]
    bp = BoundsPen(gs)
    gs[name].draw(bp)
    x0, y0, x1, y1 = bp.bounds
    s = height / (y1 - y0)
    # font units are y-up: flip, scale, then centre the ink box
    tx = cx - (x0 + x1) / 2 * s
    ty = cy + (y0 + y1) / 2 * s
    pen = SVGPathPen(gs)
    gs[name].draw(TransformPen(pen, (s, 0, 0, -s, tx, ty)))
    return f'<path fill="{fill}" d="{pen.getCommands()}"/>'


def tray_svg(text, font):
    """Solid seal, paper-coloured characters: the hanko in white-script (白文) style."""
    n = len(text)
    h = 700 if n == 1 else 440
    xs = [512] if n == 1 else [290, 734]
    parts = [f'<rect x="40" y="40" width="944" height="944" rx="150" fill="{INK}"/>']
    parts += [glyph(font, ch, x, 512, h, PAPER) for ch, x in zip(text, xs)]
    return svg(parts)


def svg(parts):
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
            + "".join(parts) + "</svg>")


def render(markup, size):
    # supersample, then downscale: resvg at 4x + Lanczos is crisper at 16-32 px
    data = resvg_py.svg_to_bytes(svg_string=markup, width=size * 4, height=size * 4, skip_system_fonts=True)
    img = Image.open(io.BytesIO(bytes(data))).convert("RGBA")
    return img.resize((size, size), Image.LANCZOS)


def save(markup, stem, sizes):
    with open(os.path.join(OUT, stem + ".svg"), "w", encoding="utf-8") as f:
        f.write(markup)
    return {s: render(markup, s) for s in sizes}


def main():
    app = save(tray_svg("カシ", SANS), "app", [1024, 256, 128, 64, 48, 32, 16])
    app[1024].save(os.path.join(OUT, "app-1024.png"))
    # カシ: two simple katakana stay legible at 16 px, where 歌詞's 25 strokes don't.
    trays = {"tray": save(tray_svg("カシ", SANS), "tray", [48, 40, 32, 24, 20, 16])}
    for stem, imgs in trays.items():
        for s, im in imgs.items():
            im.save(os.path.join(OUT, f"{stem}-{s}.png"))
    sheet(app, trays)


def sheet(app, trays):
    """App icon large + small on light/dark; each tray option at 32/24/16 on both taskbars, plus 4x zoom of 16."""
    W, H = 1500, 980
    im = Image.new("RGB", (W, H), "#ECE9E4")
    d = ImageDraw.Draw(im)
    big = app[1024].resize((420, 420), Image.LANCZOS)
    im.paste(big, (40, 40), big)
    for i, bg in enumerate(["#202020", "#F3F3F3"]):
        y = 500 + i * 110
        d.rounded_rectangle((40, y, 460, y + 90), 12, fill=bg)
        x = 60
        for s in (64, 48, 32, 16):
            ic = app[s]
            im.paste(ic, (x, y + 45 - s // 2), ic)
            x += s + 30
    for r, (stem, imgs) in enumerate(trays.items()):
        y0 = 40 + r * 300
        d.text((520, y0), stem, fill="#333")
        for c, bg in enumerate(["#202020", "#F3F3F3"]):
            x0 = 520 + c * 470
            d.rounded_rectangle((x0, y0 + 24, x0 + 450, y0 + 260), 12, fill=bg)
            x = x0 + 24
            for s in (32, 24, 16):
                ic = imgs[s]
                im.paste(ic, (x, y0 + 80 - s // 2), ic)
                x += 60
            z = imgs[16].resize((64, 64), Image.NEAREST)  # what 16 px really holds
            im.paste(z, (x0 + 24, y0 + 150), z)
            d.text((x0 + 100, y0 + 175), "16px, 4x zoom", fill="#888" if c else "#aaa")
    im.save(os.path.join(OUT, "sheet.png"))


if __name__ == "__main__":
    main()
