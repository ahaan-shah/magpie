#!/usr/bin/env python3
"""Regenerates the Magpie icon.

packaging/linux/magpie.svg is the icon everywhere on Linux. It's a vector, so
it's sharp at any size and any display scale: the tile is a true superellipse
(n = 5) written as smooth cubic Béziers. magpie-16.svg and magpie-24.svg are
hinted variants for tiny sizes. Only two rasters are made, both with resvg (or
rsvg-convert): icon-1024.png for the macOS .icns, and the window icon. The
Windows magpie.ico holds every size rendered separately (needs ImageMagick).

    python3 scripts/make-icons.py
"""

import math
import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "packaging" / "linux"

C, N = 512.0, 5.0  # centre, superellipse exponent
BIRD = "M622.81 419.19Q622.81 429.84 615.36 437.3Q607.9 444.76 597.24 444.76Q586.59 444.76 579.13 437.3Q571.67 429.84 571.67 419.19Q571.67 408.53 579.13 401.07Q586.59 393.61 597.24 393.61Q607.9 393.61 615.36 401.07Q622.81 408.53 622.81 419.19ZM759.2 444.76Q759.2 444.76 759.2 444.76Q759.2 444.76 759.2 444.76Q759.2 449.02 757.07 452.75Q754.94 456.48 751.74 459.14L708.06 487.91V530Q708.06 575.82 690.47 616.31Q672.89 656.8 643.06 686.63Q613.22 716.47 572.73 734.05Q532.24 751.63 486.43 751.63H298.9Q284.51 751.63 274.66 741.77Q264.8 731.92 264.8 717.53Q264.8 711.67 266.93 706.08Q269.06 700.48 272.26 696.22L452.33 480.45V438.37Q452.33 411.19 462.45 387.75Q472.58 364.31 489.62 346.73Q506.67 329.15 530.11 319.03Q553.02 308.37 579.66 308.37H580.19Q580.19 308.37 580.19 308.37Q580.19 308.37 580.19 308.37Q622.81 308.37 656.11 333.41Q689.41 358.45 701.66 396.81L702.19 397.34L751.74 430.37Q754.94 433.04 757.07 436.77Q759.2 440.5 759.2 444.76Q759.2 444.76 759.2 444.76Q759.2 444.76 759.2 444.76ZM711.25 444.76 678.22 422.38Q675.56 420.78 673.69 418.12Q671.83 415.46 670.76 412.79Q662.77 381.89 637.73 362.18Q612.69 342.47 580.19 342.47Q580.19 342.47 580.19 342.47Q580.19 342.47 580.19 342.47H579.66Q541.3 342.47 513.86 370.71Q486.43 398.94 486.43 438.37V486.31Q486.43 486.31 486.43 486.31Q486.43 486.31 486.43 486.31Q486.43 489.51 485.36 492.17Q484.3 494.84 482.7 497.5L298.9 717.53H356.43L507.21 536.39Q509.87 533.2 513.33 531.6Q516.79 530 520.52 530Q527.98 530 532.78 534.79Q537.57 539.59 537.57 547.05Q537.57 550.24 536.51 552.91Q535.44 555.57 533.31 558.24L533.84 557.7L400.65 717.53H486.43Q525.32 717.53 559.42 702.61Q593.51 688.23 619.08 662.66Q644.66 637.08 659.04 602.99Q673.96 568.89 673.96 530V478.86Q673.96 478.86 673.96 478.86Q673.96 478.86 673.96 478.86Q673.96 474.59 676.09 470.86Q678.22 467.13 681.42 464.47Z"


def squircle(R, samples=96):
    pts = []
    for i in range(samples):
        a = 2 * math.pi * i / samples
        c, s = math.cos(a), math.sin(a)
        x = math.copysign(abs(c) ** (2 / N), c)
        y = math.copysign(abs(s) ** (2 / N), s)
        pts.append((C + R * x, C + R * y))
    # Closed Catmull-Rom spline -> cubic Béziers.
    f = lambda v: f"{v:.2f}".rstrip("0").rstrip(".")
    d = [f"M{f(pts[0][0])} {f(pts[0][1])}"]
    n = len(pts)
    for i in range(n):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[(i + 1) % n], pts[(i + 2) % n]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d.append(f"C{f(c1[0])} {f(c1[1])} {f(c2[0])} {f(c2[1])} {f(p2[0])} {f(p2[1])}")
    return "".join(d) + "Z"


def svg(small=0):
    """small=0 is the master; 1 and 2 are hinted variants for 24px and 16px:
    less padding, no shadow and a heavier bird so it stays legible.

    Deliberately plain SVG: paths, gradients, opacity and transforms only.
    No filters, clip paths or <use> references, which Qt's SVG renderer (and
    some icon loaders) ignore or draw wrong. The soft shadows are stacks of
    faint copies instead of blurs, so every renderer draws the same icon."""
    r = [412.0, 470.0, 492.0][small]
    tile = squircle(r)
    zoom = [1.0, 1.12, 1.2][small]
    out = []
    if small == 0:
        # Drop shadow: 14 faint, progressively larger and lower copies.
        for k in range(14, 0, -1):
            grow = k * 2.0
            out.append(
                f'  <path transform="translate(0 {6 + k * 0.9:.1f})" d="{squircle(r + grow, 48)}" '
                f'fill="#3A1405" fill-opacity="0.024"/>'
            )
    out.append(f'  <path d="{tile}" fill="url(#fill)"/>')
    out.append(f'  <path d="{tile}" fill="url(#glow)"/>')
    # Rim light: a stroke on an inset outline, so it sits fully inside the tile.
    out.append(f'  <path d="{squircle(r - 4)}" fill="none" stroke="url(#edge)" stroke-width="8"/>')
    bird_tf = (
        f' transform="translate({512 - 512 * zoom - 18 * small:.1f} {512 - 512 * zoom:.1f}) scale({zoom})"'
        if small
        else ""
    )
    weight = ["", ' stroke="#FFFFFF" stroke-width="14" stroke-linejoin="round"',
              ' stroke="#FFFFFF" stroke-width="24" stroke-linejoin="round"'][small]
    out.append(f"  <g{bird_tf}>")
    if small == 0:
        for k in range(1, 9):
            out.append(
                f'    <path transform="translate(0 {k * 1.4:.1f})" d="{BIRD}" fill="#5A1E05" fill-opacity="0.045"/>'
            )
    out.append(f'    <path d="{BIRD}" fill="#FFFFFF"{weight}/>')
    out.append("  </g>")
    body = "\n".join(out)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="1024" height="1024" viewBox="0 0 1024 1024">
  <title>Magpie</title>
  <defs>
    <linearGradient id="fill" x1="164" y1="100" x2="860" y2="924" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#F49563"/>
      <stop offset="0.55" stop-color="#D2602A"/>
      <stop offset="1" stop-color="#A9461A"/>
    </linearGradient>
    <radialGradient id="glow" cx="350" cy="180" r="700" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.30"/>
      <stop offset="0.65" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="edge" x1="0" y1="100" x2="0" y2="924" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.45"/>
      <stop offset="0.3" stop-color="#FFFFFF" stop-opacity="0"/>
      <stop offset="0.75" stop-color="#5A1E05" stop-opacity="0"/>
      <stop offset="1" stop-color="#5A1E05" stop-opacity="0.30"/>
    </linearGradient>
  </defs>
{body}
</svg>
"""


def render(src, dst, size):
    if shutil.which("resvg"):
        subprocess.run(["resvg", "-w", str(size), "-h", str(size), src, dst], check=True)
    else:
        subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), src, "-o", dst], check=True)


def main():
    """Writes the SVGs the desktop uses, plus the few PNGs that must be raster:
    the macOS .icns source (1024) and the window icon embedded in the binary."""
    src = OUT / "magpie.svg"
    src.write_text(svg())
    for small, size in ((1, 24), (2, 16)):
        (OUT / f"magpie-{size}.svg").write_text(svg(small))
    for old in OUT.glob("icon-*.png"):
        old.unlink()
    render(str(src), str(OUT / "icon-1024.png"), 1024)
    render(str(src), str(ROOT / "crates" / "app" / "assets" / "icon-512.png"), 512)
    # Windows .ico: each size rendered on its own (hinted variants at 16/24)
    # so Explorer, the taskbar and Start never scale a bitmap.
    tmp = pathlib.Path(tempfile.mkdtemp())
    layers = []
    for size in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256):
        small = 2 if size <= 20 else 1 if size <= 32 else 0
        svg_path = tmp / f"m{size}.svg"
        svg_path.write_text(svg(small))
        png = tmp / f"m{size}.png"
        render(str(svg_path), str(png), size)
        layers.append(str(png))
    ico = ROOT / "crates" / "app" / "assets" / "magpie.ico"
    subprocess.run(["magick", *layers, str(ico)], check=True)
    shutil.rmtree(tmp)
    print(f"wrote {src.relative_to(ROOT)}, magpie-24.svg, magpie-16.svg, magpie.ico and the raster icons")


if __name__ == "__main__":
    main()
