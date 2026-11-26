#!/usr/bin/env python3
"""Genera el paquete de stickers por defecto (D-056): un sello de lacre con
caras. Dibujos originales hechos aquí, con la misma licencia que el
proyecto. Escribe los SVG en svg/; generar.sh los pasa a PNG y los empaqueta.
"""
import math
import os

RED, DARK, LIGHT, CREAM = "#A8231F", "#7A1714", "#C9433A", "#FBF4EC"
CX, CY, R = 256, 262, 196


def blob(cx=CX, cy=CY, r=R, seed=0.0):
    """Contorno ondulado de una gota de lacre."""
    pts = []
    for i in range(120):
        t = 2 * math.pi * i / 120
        k = r * (1 + 0.035 * math.sin(7 * t + seed) + 0.02 * math.sin(3 * t + 2 * seed) + 0.012 * math.sin(13 * t))
        pts.append(f"{cx + k * math.cos(t):.1f},{cy + k * math.sin(t):.1f}")
    return "M" + " L".join(pts) + "Z"


def heart(cx, cy, s):
    """Corazón centrado en (cx, cy) de anchura ~2s."""
    return (f"M{cx},{cy + s * 0.9} C{cx - s * 1.4},{cy - s * 0.1} {cx - s * 0.7},{cy - s * 1.1} {cx},{cy - s * 0.45} "
            f"C{cx + s * 0.7},{cy - s * 1.1} {cx + s * 1.4},{cy - s * 0.1} {cx},{cy + s * 0.9}Z")


def body(shape=None, seed=0.0):
    d = shape or blob(seed=seed)
    return f"""
<defs><radialGradient id="g" cx="0.38" cy="0.32" r="0.75">
<stop offset="0" stop-color="{LIGHT}"/><stop offset="0.55" stop-color="{RED}"/><stop offset="1" stop-color="{DARK}"/>
</radialGradient><clipPath id="c"><path d="{d}"/></clipPath></defs>
<path d="{d}" fill="#fff" stroke="#fff" stroke-width="28" stroke-linejoin="round"/>
<path d="{d}" fill="url(#g)"/>
<ellipse cx="190" cy="150" rx="46" ry="22" transform="rotate(-28 190 150)" fill="{CREAM}" opacity="0.28" clip-path="url(#c)"/>
"""


def line(d, w=14, color=CREAM):
    return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"/>'


def fill(d, color=CREAM, extra=""):
    return f'<path d="{d}" fill="{color}" {extra}/>'


def eyes(rx=17, ry=25, y=232, dx=58):
    return "".join(f'<ellipse cx="{CX + s * dx}" cy="{y}" rx="{rx}" ry="{ry}" fill="{CREAM}"/>' for s in (-1, 1))


def cheeks():
    return "".join(f'<ellipse cx="{CX + s * 110}" cy="290" rx="24" ry="14" fill="#E8776B" opacity="0.55"/>' for s in (-1, 1))


def tear(x, y, s=1.0):
    return fill(f"M{x},{y} C{x - 16 * s},{y + 26 * s} {x - 14 * s},{y + 44 * s} {x},{y + 44 * s} "
                f"C{x + 14 * s},{y + 44 * s} {x + 16 * s},{y + 26 * s} {x},{y}Z", "#8FD3F4", 'stroke="#fff" stroke-width="5"')


STICKERS = {
    "feliz": lambda: body() + eyes() + cheeks() + line("M196,300 Q256,352 316,300"),
    "risa": lambda: body(seed=1) + line("M178,236 L208,220 L178,204", 13) + line("M334,236 L304,220 L334,204", 13)
    + fill("M182,282 Q256,284 330,282 Q322,372 256,372 Q190,372 182,282Z")
    + fill("M214,338 Q256,318 298,338 Q284,366 256,366 Q228,366 214,338Z", "#E8776B")
    + tear(150, 250, 0.9) + tear(362, 250, 0.9),
    "amor": lambda: body(seed=2) + fill(heart(198, 236, 30), "#FF5A6E", f'stroke="{CREAM}" stroke-width="6"')
    + fill(heart(314, 236, 30), "#FF5A6E", f'stroke="{CREAM}" stroke-width="6"') + cheeks()
    + line("M206,302 Q256,346 306,302"),
    "triste": lambda: body(seed=3) + eyes(15, 21, 240) + line("M170,196 L222,208", 11) + line("M342,196 L290,208", 11)
    + line("M206,332 Q256,296 306,332") + tear(318, 268),
    "enfado": lambda: body(seed=4) + eyes(16, 18, 244) + line("M166,196 L228,224", 14) + line("M346,196 L284,224", 14)
    + line("M210,326 Q256,300 302,326")
    + line("M396,92 q14,-14 0,-28 q-14,-14 0,-28", 9, "#9AA3AD") + line("M424,108 q14,-14 0,-28 q-14,-14 0,-28", 9, "#9AA3AD"),
    "sorpresa": lambda: body(seed=5) + "".join(
        f'<circle cx="{CX + s * 60}" cy="226" r="28" fill="{CREAM}"/><circle cx="{CX + s * 60}" cy="230" r="11" fill="{DARK}"/>'
        for s in (-1, 1)) + line("M170,176 Q198,160 226,172", 10) + line("M342,176 Q314,160 286,172", 10)
    + '<ellipse cx="256" cy="322" rx="26" ry="34" fill="#FBF4EC"/><ellipse cx="256" cy="326" rx="14" ry="20" fill="#7A1714"/>',
    "guino": lambda: body(seed=6) + f'<ellipse cx="198" cy="232" rx="17" ry="25" fill="{CREAM}"/>'
    + line("M290,236 Q316,214 342,236", 13) + cheeks() + line("M200,298 Q256,348 312,298")
    + fill("M262,322 Q292,318 296,346 Q286,372 268,360 Q258,340 262,322Z", "#FF7C8A"),
    "dormido": lambda: body(seed=7) + line("M172,240 Q198,256 224,240", 12) + line("M288,240 Q314,256 340,240", 12)
    + '<ellipse cx="256" cy="318" rx="14" ry="10" fill="#FBF4EC"/>'
    + line("M344,96 h34 l-34,40 h34", 9, "#5B6B8C") + line("M394,46 h26 l-26,30 h26", 8, "#5B6B8C")
    + line("M432,8 h18 l-18,22 h18", 7, "#5B6B8C"),
    "gafas": lambda: body(seed=8) + line("M150,214 H362", 10, "#1E1E24")
    + fill("M156,212 H244 Q244,268 202,268 Q156,268 156,212Z", "#1E1E24")
    + fill("M268,212 H356 Q356,268 310,268 Q268,268 268,212Z", "#1E1E24")
    + fill("M170,222 l22,0 l-26,26 Z", "#fff", 'opacity="0.5"') + fill("M282,222 l22,0 l-26,26 Z", "#fff", 'opacity="0.5"')
    + line("M220,316 Q270,334 310,304"),
    "pensando": lambda: body(seed=9) + "".join(
        f'<ellipse cx="{CX + s * 58}" cy="228" rx="17" ry="25" fill="{CREAM}"/><circle cx="{CX + s * 58 + 5}" cy="214" r="8" fill="{DARK}"/>'
        for s in (-1, 1)) + line("M168,180 Q196,170 222,182", 10) + line("M290,170 Q316,160 344,172", 10)
    + line("M220,322 L290,312", 13)
    + line("M384,64 q0,-34 34,-34 q34,0 34,30 q0,26 -34,34 v22", 14, "#5B6B8C")
    + '<circle cx="418" cy="160" r="10" fill="#5B6B8C"/>',
    "vale": lambda: body(seed=10) + f'<circle cx="256" cy="262" r="150" fill="none" stroke="{CREAM}" stroke-width="10" opacity="0.6"/>'
    + line("M180,266 L236,322 L338,200", 34),
    "no": lambda: body(seed=11) + f'<circle cx="256" cy="262" r="150" fill="none" stroke="{CREAM}" stroke-width="10" opacity="0.6"/>'
    + line("M196,202 L316,322", 34) + line("M316,202 L196,322", 34),
    "corazon": lambda: body(heart(256, 280, 205)) + line(heart(256, 270, 110), 12),
}


def main():
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "svg")
    os.makedirs(out, exist_ok=True)
    for i, (name, draw) in enumerate(STICKERS.items()):
        svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">{draw()}</svg>\n'
        with open(os.path.join(out, f"{i:02d}-{name}.svg"), "w") as f:
            f.write(svg)


if __name__ == "__main__":
    main()
