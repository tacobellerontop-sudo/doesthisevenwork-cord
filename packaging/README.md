# Packaging

DTEW Cord is Windows only.

Release packages are built by `.github/workflows/release.yml` when a `v*` tag or a `release/v*` branch is pushed. The
tag must match the workspace version in `Cargo.toml`.

| Path | Used for |
|---|---|
| `windows/dtewcord.rc`, `windows/DTEW Cord.ico` | The executable's icon and version resource |
| `windows/install-notifications.ps1` | Start Menu shortcut with the app id, needed for Windows notifications |
| `release-notes/<tag>.md` | GitHub release notes; the in-app updater shows its `###` sections and `-` items |

The in-app updater downloads `dtewcord-<tag>-Windows-X64.zip` from this repository's releases and
checks it against `SHA256SUMS.txt`. `cargo xtask package` builds the same zip contents into `dist/`.
