# Packaging

Releases are Windows only. The Linux and macOS files below are kept for building packages by hand.

Release packages are built by `.github/workflows/release.yml` when a `v*` tag or a `release/v*` branch is pushed. The
tag must match the workspace version in `Cargo.toml`.

| Path | Used for |
|---|---|
| `appimage/build.py` | Linux AppImage and its `.zsync` (`cargo run -p xtask -- package --format appimage`) |
| `macos/Info.plist`, `macos/compile-icon.sh` | The `DTEW Cord.app` bundle and its icon; `build.rs` also embeds the plist |
| `windows/dtewcord.rc`, `windows/DTEW Cord.ico` | The executable's icon and version resource |
| `windows/install-notifications.ps1` | Start Menu shortcut with the app id, needed for Windows notifications |
| `release-notes/<tag>.md` | GitHub release notes; the in-app updater shows its `###` sections and `-` items |

The in-app updater downloads `dtewcord-<tag>-Windows-X64.zip` from this repository's releases and
checks it against `SHA256SUMS.txt`. On Linux and macOS it only reports that a newer version exists.

`cargo xtask package --format deb|rpm|arch` needs `linux/package.py`, which isn't in this tree.
