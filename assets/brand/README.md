# DTEW Cord brand assets

DTEW Cord's own application mark: a speech bubble asking a question ("does this even work?"),
trailing a power cord that ends in a plug, on a mint-to-indigo plate. These files are original
DTEW Cord artwork, not Discord's, Serein's or any other vendor's branding, and no font is
embedded in them.

- `dtewcord.svg` — full-colour mark on its gradient plate; the master vector source.
- `dtewcord-flat.svg` — flat (non-gradient) variant, used for the 16 and 24 px `.ico` frames.
- `dtewcord-mark.svg` — solid white silhouette with the question mark knocked out, view box
  trimmed to the mark. `tools/generate-icons.py` rasterizes it into the shared icon atlas as
  `dtewcord-mark`, which the interface tints at draw time.
- `dtewcord-1024.png` — 1024×1024 sRGB RGBA preview render of the full-colour mark.
- `dtewcord-256.png` — 256×256 render used as the window icon (egui) and the in-app logo (Slint).
- `dtewcord-tray.png` — 72×72 black-on-transparent render of `dtewcord-mark.svg`.

`tools/generate-brand.py` renders every PNG here and the packaged Windows icon,
`packaging/windows/DTEW Cord.ico` (16, 24, 32, 48, 64, 128 and 256 px frames), from the SVGs.

Brand palette: `#3EE6C1` → `#1A9BD8` → `#1D3A9E` plate gradient, white bubble, cord and plug,
`#1B86CC` question mark. The interface accent (`design::DEFAULT_PRIMARY_RGB`, `#1a72e8`) sits in
the same blue family.
