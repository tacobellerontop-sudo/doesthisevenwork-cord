# Serein (Slint client)

A second Serein frontend written with [Slint](https://slint.dev). It uses the same sign-in, credential store, REST and Gateway crates as the egui desktop app in `apps/desktop`, so both clients behave the same way towards Discord. Only the presentation layer is new.

> [!WARNING]
> Like the desktop app, this client signs in a normal Discord user account through Discord's own login page. Serein isn't endorsed by Discord, and using an unofficial client may break Discord's Terms of Service and lead to account enforcement.

## Run it

```sh
# Offline preview with synthetic data; never contacts Discord.
cargo run -p serein-slint --features demo -- --demo

# Real sign-in through Discord's login page; the session is saved in the system keychain.
cargo run -p serein-slint
```

Set `SEREIN_THEME=light` to start in the light theme. The button next to the account name switches themes at runtime.

## What milestone 1 covers

- Sign-in through `platform::LoginView`. The login is restored from, saved to, and removed from the system keychain through the same `platform` functions the desktop app uses.
- A server rail in the user's folder order, with unread pills.
- A channel sidebar: categories that fold, text before voice within each category, and a Direct Messages list ordered by recent activity.
- A live timeline. Messages are grouped by author, with day dividers, reply previews and mention highlights. Discord formatting is rendered: bold, italics, strikethrough, inline and block code, quotes, headings and links. User, role and channel mentions and `<t:…>` timestamps show as names and dates.
- Sending, with pending, failed and unknown-outcome states, plus typing indicators from other people.
- Loading older history when you scroll to the top, and a reconnecting banner when the Gateway drops.

The egui app remains the complete client: voice and video, forums, threads, reactions, attachments, profiles, settings, extensions and so on aren't ported yet.

## Layout

| File | Purpose |
|---|---|
| `ui/app.slint` | All views, the theme and input handling |
| `src/main.rs` | Wires `client-core` state, the session and sign-in to the window |
| `src/session.rs` | One Discord session: `discord-api` for REST, `discord-gateway` for live events |
| `src/view.rs` | Pure mapping from `client_core::State` to the rows the views render (unit tested) |
| `src/markdown.rs` | Discord text → the CommonMark subset Slint's `StyledText` renders, with HTML escaped |
| `src/clock.rs` | Local-time labels for headers and day dividers |

## Licensing

Slint is available under GPLv3, the Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0, or a commercial license. Serein's own code stays MIT OR Apache-2.0. A distributed build of this client has to meet the terms of whichever Slint license you choose. For example, the royalty-free license asks for Slint attribution in the app or on its website. Slint's crates aren't on the `deny.toml` license allow-list yet, so `cargo deny check licenses` reports them until that choice is recorded there.
