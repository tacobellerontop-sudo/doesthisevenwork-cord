# Architecture and TUI integration notes

DTEW Cord is a Rust Discord client organized as a Cargo workspace. Its backend is separated from its egui desktop presentation layer, making the workspace a useful base for a second frontend. The existing desktop application remains in `apps/desktop` and `crates/ui`.

## Core layers

- **`model`** — shared identifiers, entities, patches and domain types. It should remain independent of terminal rendering.
- **`discord-protocol`** — Discord payload types and bounded wire decoding.
- **`discord-api`** — REST transport, request construction, response handling and shared request limits.
- **`discord-gateway`** — asynchronous Gateway connection, event decoding, heartbeats, identify/resume and reconnect behavior.
- **`client-core`** — session state, navigation, event application, message lifecycle and application commands. Network workers should communicate with this layer through typed events rather than mutating UI state directly.
- **`session-cache`** — bounded in-memory state for active conversations and recent history.
- **`local-store`** — SQLite-backed local persistence and worker-based storage operations.
- **`platform`** — operating-system integrations such as credential storage and native dialogs. Some APIs are desktop-oriented and need adapters for a TUI.
- **`discord-voice`** — voice transport and audio/media processing. Device selection and call controls need a terminal interaction design.
- **`extensions`** — theme palette types only, kept until DTEW Cord's own theming system replaces them. The extension (plugin) system inherited from Serein was removed.
- **`diagnostics`** — diagnostic and logging facilities.
- **`ui`** — egui presentation components. This is not required by a terminal frontend.
- **`apps/desktop`** — desktop composition root: startup, authentication lifecycle, worker wiring, native UI setup and application shutdown.

## Recommended TUI boundary

Create a new binary crate (for example `apps/tui`) and keep it independent of egui. Reuse the backend crates through their public APIs rather than copying their implementation.

A terminal application generally needs:

1. A Tokio runtime and a single owner for visible application state.
2. An input task that translates key events into typed application commands.
3. A bounded event channel receiving Gateway, REST, storage and voice results.
4. A reducer/update step that applies events to state.
5. A Ratatui renderer that reads state but does not perform network requests.
6. A cancellation and shutdown path that stops Gateway, REST, storage and voice workers cleanly.

Keep event queues bounded. Do not block the terminal draw loop on REST calls, file I/O, media decoding. Preserve the existing cache and payload limits when introducing new views.

## Desktop-only functionality to adapt

The current GUI includes egui panels, native dialogs, image/video surfaces, camera and screen capture controls, and profile previews. These are not automatically portable to a TUI. Provide terminal equivalents where practical, and make unsupported operations explicit rather than silently dropping them.

## Workspace notes

The root `Cargo.toml` uses workspace globs for `crates/*` and `tools/*`; retain those member directories and the root `Cargo.lock` when changing the workspace. Several crates also have their own tests and fixture data. Vendored source under `vendor/` is referenced by `[patch]` entries in the root manifest and is required for reproducible dependency resolution.
