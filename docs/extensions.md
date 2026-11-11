# Extensions

The extension implementation is split between the host-side crate at `crates/extensions` and the SDK/catalog workspace in `extensions/`.

## Relevant paths

- `crates/extensions` — host-side package parsing, validation, capability checks and integration with client state.
- `extensions/sdk` — SDK used to build compatible extensions.
- `extensions/plugins/` — bundled plugin source and packaged artifacts.
- `extensions/themes/` — declarative themes.
- `extensions/catalog.json` — package catalog consumed by the client.
- `docs/extension-sdk-*.md` — SDK overview, API reference, actions and troubleshooting.

## TUI integration

Do not assume graphical extension settings or UI actions can be reused unchanged in a terminal. Keep capability checks and package validation in the shared host layer, then expose a terminal-specific command/presentation adapter. Any extension execution should remain bounded, permission-gated and isolated from the terminal event loop.

Read `extensions/README.md` for catalog and package workflow details. The tutorial workspace is under `examples/extensions/`.
