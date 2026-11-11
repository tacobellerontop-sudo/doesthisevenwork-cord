# DTEW Cord extensions

This directory contains the extension SDK, bundled plugin source and packages, declarative themes, and the extension catalog. Host-side validation and integration live in `crates/extensions`.

| Path | Contents |
|---|---|
| `sdk/` | Rust SDK used by extensions |
| `plugins/<id>/` | Plugin source, manifests and packaged modules |
| `plugins/packages/` | Packaged plugin artifacts |
| `themes/` | Declarative theme packages |
| `catalog.json` | Generated package catalog |
| `licenses/` | Third-party extension license texts |
| `previews/` | Catalog preview images and provenance |
| `catalog.py`, `pack.py` | Catalog validation and packaging tools |

Start with `docs/extension-sdk-overview.md` and `examples/extensions/README.md` for authoring guidance. `docs/extension-sdk-reference.md` documents the SDK surface.

## Frontend integration

The SDK and host validation are shared capabilities; graphical settings panels and desktop-only actions are not automatically suitable for a terminal. A TUI should expose extension controls through commands and terminal-native views while preserving the host's capability checks, consent flow and package validation.

The catalog is generated. Preserve package identities, manifests, hashes and license notices when modifying packages. Consult the individual plugin README files before rebuilding or republishing an artifact.
