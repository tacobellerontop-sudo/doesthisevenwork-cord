# Discord compatibility and support boundaries

DTEW Cord is an unofficial client and is not endorsed by Discord. Its implementation communicates with Discord services directly. Protocol behavior can change without notice, and technical compatibility does not imply permission under Discord's Terms of Service.

## Risks and limitations

- Using an unofficial client with a normal user account may violate Discord's rules and can lead to account restrictions or termination.
- Gateway and REST payloads evolve. Unknown fields, changed permission rules, new interaction types and undocumented behavior can break compatibility.
- Voice, video, screen sharing, rich presence, stickers, embeds and interactive components have additional protocol and media requirements beyond basic messaging.
- A successful connection is not evidence that every feature is compatible or safe to use.

## TUI-specific considerations

A terminal frontend should prioritize accessible text workflows and clearly report unsupported content. Images, animated media, video, rich embeds, polls, buttons, select menus, modals, voice and stream viewing may need alternate representations or explicit handoff behavior.

Keep protocol decoding separate from rendering. Unknown or newly introduced payload fields should not crash the terminal renderer. Avoid replaying ambiguous message-send operations after a connection failure; duplicate sends can be worse than reporting an uncertain result.

## Safer integration model

For bots, automation and third-party integrations, use Discord's documented bot and OAuth2 APIs with appropriately scoped permissions. Do not use this client as a way to bypass platform restrictions.
