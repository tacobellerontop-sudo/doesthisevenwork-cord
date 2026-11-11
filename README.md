# DTEW Cord — Discord client backend and desktop reference

DTEW Cord (short for *doesthisevenwork-cord*) is an unofficial Discord client written in Rust. It is a fork of [Serein](https://github.com/ViceVerse-cz/Serein); the Serein contributors' copyright notices are kept in `LICENSE-MIT`, `LICENSE-APACHE` and the extension packages. This trimmed source archive keeps the Rust workspace, Discord protocol/networking crates, client state and storage layers, extension SDK/catalog, native voice implementation, vendored dependency patches, assets, and the existing desktop GUI.

**This archive is not a finished Discord TUI.** It is a backend-rich starting point for building a separate terminal frontend (for example, with Ratatui) while retaining the current GUI as a working reference implementation.

> [!WARNING]
> DTEW Cord is not endorsed by Discord. Connecting a normal user account through an unofficial client may violate Discord's Terms of Service and can result in account enforcement. Review `docs/discord-compatibility.md`, `docs/authentication.md`, and `docs/threat-model.md` before use. No client can guarantee account safety.

## Workspace map

| Path | Purpose |
|---|---|
| `crates/model` | Shared domain types and identifiers; no UI or transport |
| `crates/discord-protocol` | Discord Gateway/REST payload models and wire decoding |
| `crates/discord-api` | REST transport and request handling |
| `crates/discord-gateway` | Gateway WebSocket lifecycle, events, heartbeats and reconnects |
| `crates/client-core` | Client/session state, navigation, event application and command lifecycle |
| `crates/session-cache` | Bounded in-memory session/timeline cache |
| `crates/local-store` | Local SQLite persistence |
| `crates/platform` | OS-specific credential and native platform integrations |
| `crates/discord-voice` | Voice transport, audio processing and media support |
| `crates/extensions` | Extension package parsing, permissions and runtime integration |
| `crates/diagnostics` | Diagnostics and logging support |
| `crates/ui` | Existing egui desktop presentation layer |
| `crates/test-support` | Test fixtures and synthetic support utilities |
| `apps/desktop` | Existing desktop executable and composition root |
| `apps/slint` | Slint client on the same login and backend crates (see `apps/slint/README.md`) |
| `extensions/` | Extension SDK, plugins, themes and catalog |
| `vendor/` | Patched dependencies referenced by the workspace manifest |
| `assets/` | Fonts, icons, sounds, emoji and license/provenance notices |
| `tools/` | Workspace development utilities and diagnostics |

## Suggested TUI integration strategy

Build the terminal frontend as a separate application crate rather than trying to make the egui widgets render in a terminal.

1. Start with `model`, `discord-protocol`, `discord-api`, and `discord-gateway` to understand the data and transport boundaries.
2. Reuse `client-core`, `session-cache`, and `local-store` where their public APIs fit. These are the most relevant layers for session state, history, navigation, and persistence.
3. Treat `crates/ui` as the desktop implementation, not as a dependency of the terminal renderer. A TUI should own its own event loop, rendering, keybindings, layout and terminal lifecycle.
4. Reuse `discord-voice`, `platform`, and `extensions` selectively. Voice devices, desktop credential dialogs, and graphical extension settings will need terminal-specific adapters or may be deferred.
5. Keep transport callbacks away from rendering. Bridge asynchronous Gateway/REST events into a bounded application event queue, then update state on the TUI task.

A possible future workspace layout is:

```text
apps/
  desktop/       # existing egui GUI
  tui/           # new Ratatui frontend (not included yet)
crates/
  model/
  discord-protocol/
  discord-api/
  discord-gateway/
  client-core/
  session-cache/
  local-store/
  ...
```

Do not assume every desktop capability is automatically available to a terminal frontend. Image/video presentation, file pickers, camera capture, rich profile cards, extension UI, and some voice controls require new terminal workflows.

## Build the existing desktop application

Install the Rust toolchain specified by `rust-toolchain.toml` and the native system dependencies required by your operating system. Then run:

```sh
cargo check --workspace
cargo run -p dtewcord
```

The workspace retains the original dependency lockfile and vendored patches. Removing dot-prefixed repository metadata means Git hosting workflows, ignore rules, and local Cargo configuration are not included in this archive. Cargo's project manifests and lockfile remain.

## Extensions

The extension SDK, bundled plugins, themes, catalog and authoring documentation remain under `extensions/`. Start with `extensions/README.md` and `docs/extension-sdk-overview.md`. Extensions are not a substitute for the core Discord transport or session state.

## License and third-party code

DTEW Cord's own code is dual-licensed under MIT or Apache-2.0; see `LICENSE-MIT` and `LICENSE-APACHE`. Vendored dependencies and bundled assets retain their upstream license and provenance files. See `THIRD_PARTY_NOTICES.md` and the relevant `assets/licenses/` directories.
