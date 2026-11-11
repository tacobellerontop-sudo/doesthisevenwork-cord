# Threat model — concise integration guidance

DTEW Cord is an unofficial client that handles authenticated Discord sessions and private user content. This summary is intended for developers adding or adapting frontends; it is not a security audit or a guarantee of account safety.

## Assets to protect

- Authentication credentials and session identifiers.
- Private messages, attachments, server membership and profile data.
- Local drafts, caches and SQLite records.
- Extension permissions and data exposed to extension code.

## Core precautions

- Keep credentials in the operating-system credential store where supported; never print them or include them in diagnostics.
- Keep network origins fixed and validate responses before applying them to client state.
- Bound queues, response bodies, caches and attachment staging to limit resource exhaustion.
- Apply extension capabilities through explicit permission checks; do not treat extension packages as trusted merely because they are catalogued.
- Clear account-scoped state on logout and account changes.
- Avoid logging message bodies, tokens, private URLs or attachment contents.

## TUI-specific risks

Terminal scrollback, shell history, terminal recording, multiplexers and copy/paste buffers may retain private content outside the application's control. Avoid placing credentials in command-line arguments or interactive command history. Consider a privacy mode that suppresses message previews and sensitive notifications.

## Platform and account risk

Unofficial normal-account clients may violate Discord's Terms of Service and can be subject to account enforcement. This project cannot guarantee that an account will not be restricted. Use documented bot/OAuth2 APIs for supported automation.
