# Authentication notes

Serein's desktop application contains the existing login and credential lifecycle. Authentication is not a generic Discord OAuth2 bot flow; this project is an unofficial client that connects to Discord using a normal user account.

## Important account warning

Discord's rules restrict automating normal user accounts outside supported APIs. Using an unofficial client can result in account enforcement. This archive cannot make the login method risk-free, and the maintainers cannot guarantee account safety. Prefer an official bot account and supported bot APIs for automation or integrations.

## Code boundaries

- `apps/desktop` coordinates the current desktop login lifecycle.
- `platform` contains OS credential-store integration and the desktop authentication webview adapter.
- `discord-api` and `discord-gateway` consume authenticated session credentials for REST and Gateway traffic.
- `client-core` owns the application/session lifecycle; UI code should not read or persist credentials directly.

A TUI frontend should not copy credentials into logs, command history, environment dumps, crash reports or plaintext configuration. Reuse a supported credential-store abstraction where possible. If a terminal-only authentication flow is added, document its threat model and avoid asking users to paste account passwords or tokens into the terminal.

## Logout and shutdown

A frontend should cancel in-flight work, close Gateway connections, stop media workers and clear in-memory authenticated state on logout. Ensure credential deletion and session invalidation are ordered so an old worker cannot restore a credential after logout.
