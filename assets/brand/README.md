# DTEW Cord brand assets

DTEW Cord's own application mark: a white monoline drawing of a hexagon framed by two arms,
with an interlaced 7 and loop, on a dark slate plate. These files are original
DTEW Cord artwork, not Discord's, Serein's or any other vendor's branding, and no font is
embedded in them.

- `dtewcord.svg` — the mark on its rounded plate; the master vector source.
- `dtewcord-flat.svg` — flat plate and a heavier stroke, used for the 16 and 24 px `.ico` frames.
- `dtewcord-mark.svg` — the white monoline mark alone, view box
  trimmed to the mark. `tools/generate-icons.py` rasterizes it into the shared icon atlas as
  `dtewcord-mark`, which the interface tints at draw time.
- `dtewcord-1024.png` — 1024×1024 sRGB RGBA preview render of the full-colour mark.
- `dtewcord-256.png` — 256×256 render used as the window icon (egui) and the in-app logo (Slint).
- `dtewcord-tray.png` — 72×72 black-on-transparent render of `dtewcord-mark.svg`.

`tools/generate-brand.py` renders every PNG here and the packaged Windows icon,
`packaging/windows/DTEW Cord.ico` (16, 24, 32, 48, 64, 128 and 256 px frames), from the SVGs.

Brand palette: `#27334A` → `#1A2233` plate gradient (`#1F2A3D` flat), white `#FFFFFF` strokes.