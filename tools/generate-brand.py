#!/usr/bin/env python3
"""Rasterize the DTEW Cord brand artwork (development only).

Renders `assets/brand/dtewcord.svg` into the PNG previews, the window icon and the packaged
Windows `.ico`, and `dtewcord-mark.svg` into the 72x72 tray template. Uses the same pinned
`resvg` as `tools/generate-icons.py`, plus Pillow to pack the `.ico`:

    cargo install resvg --version 0.45.1 --root /tmp/resvg-tool
    python3 -m pip install pillow
    python3 tools/generate-brand.py --resvg /tmp/resvg-tool/bin/resvg

The icon atlas cell (`dtewcord-mark`) is rebuilt by `tools/generate-icons.py`.
"""

import argparse
import io
from pathlib import Path
import subprocess
import tempfile

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "assets" / "brand"
ICO = ROOT / "packaging" / "windows" / "DTEW Cord.ico"
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


RESVG = "resvg"


def render(svg, size):
    with tempfile.TemporaryDirectory() as scratch:
        source, target = Path(scratch) / "in.svg", Path(scratch) / "out.png"
        source.write_bytes(svg)
        subprocess.run([RESVG, "-w", str(size), "-h", str(size), str(source), str(target)], check=True)
        return Image.open(io.BytesIO(target.read_bytes())).convert("RGBA")


def main():
    global RESVG
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resvg", default="resvg", help="path to the resvg executable")
    RESVG = parser.parse_args().resvg
    master = (BRAND / "dtewcord.svg").read_bytes()
    flat = (BRAND / "dtewcord-flat.svg").read_bytes()
    render(master, 1024).save(BRAND / "dtewcord-1024.png", optimize=True)
    render(master, 256).save(BRAND / "dtewcord-256.png", optimize=True)

    # Each .ico frame is rendered at its own size rather than downscaled; the smallest frames use
    # the flat variant, whose single plate colour stays crisp at a few pixels.
    frames = [render(flat if size <= 24 else master, size) for size in ICO_SIZES]
    frames[-1].save(ICO, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])

    mark = (BRAND / "dtewcord-mark.svg").read_text(encoding="utf-8")
    tray = render(mark.replace("#FFFFFF", "#000000").encode(), 72)
    tray.save(BRAND / "dtewcord-tray.png", optimize=True)


if __name__ == "__main__":
    main()
