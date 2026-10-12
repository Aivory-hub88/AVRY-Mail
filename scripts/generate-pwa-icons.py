#!/usr/bin/env python3
"""Generate the PWA / home-screen icons in web/public from the logo mark.

    python3 scripts/generate-pwa-icons.py

Needs Pillow and macOS `sips` (it rasterises the SVG). The outputs are
committed, so this only has to be re-run when the logo changes.

The mark is a near-square envelope, so a "maskable" icon (which Android crops
to a circle, squircle, ...) must keep it inside the central 80% circle: its
corners sit at ~0.707 * (mark width) from the centre, hence the 0.56 scale.
"""
import subprocess, sys, tempfile
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SVG = ROOT / "web/public/Favicon_Aivory-Mail.svg"
OUT = ROOT / "web/public"
BACKGROUND = (24, 24, 27, 255)  # #18181b, the app's dark surface

# file name -> (pixel size, share of the tile the mark may fill)
ICONS = {
    "icon-192.png": (192, 0.72),
    "icon-512.png": (512, 0.72),
    "icon-maskable-512.png": (512, 0.56),
    "apple-touch-icon.png": (180, 0.74),
}


def render_mark(size: int) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "mark.png"
        subprocess.run(["sips", "-s", "format", "png", "-z", str(size), str(size), str(SVG), "--out", str(png)],
                       check=True, capture_output=True)
        return Image.open(png).convert("RGBA")


def main() -> int:
    mark = render_mark(1024)
    box = mark.getchannel("A").point(lambda v: 255 if v > 8 else 0).getbbox()
    mark = mark.crop(box)  # trim transparent margins so the scale is exact
    for name, (size, share) in ICONS.items():
        target = max(1, round(size * share))
        scale = target / max(mark.size)
        glyph = mark.resize((max(1, round(mark.width * scale)), max(1, round(mark.height * scale))), Image.LANCZOS)
        tile = Image.new("RGBA", (size, size), BACKGROUND)
        tile.alpha_composite(glyph, ((size - glyph.width) // 2, (size - glyph.height) // 2))
        tile.convert("RGB").save(OUT / name, optimize=True)
        print(f"wrote web/public/{name} ({size}x{size}, mark {share:.0%})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
