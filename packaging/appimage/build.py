#!/usr/bin/env python3
"""Builds dtewcord-v<version>-Linux-X64.AppImage (and its .zsync) from a staged dist/ folder.

Usage: build.py <dist> <version>

`cargo xtask package --format appimage` stages dist/ and runs this. appimagetool comes from
$APPIMAGETOOL or PATH; zsyncmake must be on PATH for the delta-update control file.

The image carries the client and its notices. GTK, WebKitGTK and GStreamer come from the host,
like the distribution packages: the login window and media need those system libraries.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

APP_ID = "io.github.tacobellerontop_sudo.dtewcord"
REPOSITORY = ("tacobellerontop-sudo", "doesthisevenwork-cord")
ROOT = Path(__file__).resolve().parents[2]

DESKTOP = f"""[Desktop Entry]
Type=Application
Name=DTEW Cord
GenericName=Chat client
Comment=Unofficial Discord client
Exec=dtewcord %U
Icon={APP_ID}
Terminal=false
Categories=Network;InstantMessaging;Chat;
StartupWMClass={APP_ID}
StartupNotify=true
"""

APPRUN = """#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/dtewcord" "$@"
"""


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    dist, version = Path(sys.argv[1]).resolve(), sys.argv[2]
    out = ROOT / "dist-appimage"
    appdir = out / "AppDir"
    shutil.rmtree(appdir, ignore_errors=True)
    (appdir / "usr/bin").mkdir(parents=True)
    shutil.copy2(dist / "dtewcord", appdir / "usr/bin/dtewcord")
    os.chmod(appdir / "usr/bin/dtewcord", 0o755)

    share = appdir / "usr/share"
    docs = share / "doc/dtewcord"
    docs.mkdir(parents=True)
    for item in dist.iterdir():
        if item.name == "dtewcord":
            continue
        target = docs / item.name
        (shutil.copytree if item.is_dir() else shutil.copy2)(item, target)

    (share / "applications").mkdir(parents=True)
    (share / "applications" / f"{APP_ID}.desktop").write_text(DESKTOP)
    (appdir / f"{APP_ID}.desktop").write_text(DESKTOP)
    for size, source in ((256, "dtewcord-256.png"), (1024, "dtewcord-1024.png")):
        icons = share / f"icons/hicolor/{size}x{size}/apps"
        icons.mkdir(parents=True)
        shutil.copy2(ROOT / "assets/brand" / source, icons / f"{APP_ID}.png")
    shutil.copy2(ROOT / "assets/brand/dtewcord-256.png", appdir / f"{APP_ID}.png")
    shutil.copy2(ROOT / "assets/brand/dtewcord-256.png", appdir / ".DirIcon")
    (appdir / "AppRun").write_text(APPRUN)
    os.chmod(appdir / "AppRun", 0o755)

    name = f"dtewcord-v{version}-Linux-X64.AppImage"
    tool = os.environ.get("APPIMAGETOOL") or shutil.which("appimagetool")
    if not tool:
        sys.exit("appimagetool not found; set APPIMAGETOOL or add it to PATH")
    update = "gh-releases-zsync|{}|{}|latest|dtewcord-*-Linux-X64.AppImage.zsync".format(*REPOSITORY)
    env = dict(os.environ, ARCH="x86_64")
    subprocess.run([tool, "--no-appstream", "-u", update, str(appdir), name], cwd=out, env=env, check=True)
    print(f"AppImage: {out / name}")


if __name__ == "__main__":
    main()
