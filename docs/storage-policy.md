# Storage and cache policy — integration summary

The client uses bounded in-memory caches and local SQLite persistence. These limits are part of the application's resilience model, not merely UI tuning.

## Components

- `session-cache` holds the active session's recent conversation state and bounded message history.
- `local-store` persists supported local data through SQLite and serializes storage work away from the rendering thread.
- `client-core` coordinates navigation, session generations and cache invalidation.

## TUI requirements

A new frontend should preserve the existing bounds and invalidation semantics. In particular:

- Do not create an unbounded per-channel in-memory message cache.
- Cancel stale history/search/profile work when navigation or session generation changes.
- Keep disk I/O and large payload decoding off the terminal draw loop.
- Clear or scope account-specific cached data on logout and account changes.
- Treat drafts, attachments and pending operations as session-scoped state with explicit cleanup behavior.
- Do not log message contents, credentials or private attachment data.

For exact limits and implementation details, inspect the current source in `crates/session-cache`, `crates/local-store` and `crates/client-core`; source code is authoritative when this summary and implementation differ.
