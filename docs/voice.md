# Voice and media integration

Voice and media functionality is implemented primarily in `crates/discord-voice`, with platform/device integration in `crates/platform` and orchestration in the desktop application.

The existing desktop client includes voice transport, audio processing, camera/video and screen-sharing workflows. These capabilities are not automatically exposed as terminal-friendly controls.

## TUI adaptation

A terminal frontend should stage voice support separately from text messaging:

1. Implement call join/leave, mute and deafen state first.
2. Add input/output device selection through a platform adapter.
3. Expose per-user volume and speaking indicators where supported.
4. Treat camera, video and screen sharing as optional capabilities; a terminal may only show status or provide a handoff to the GUI.
5. Keep media processing and device callbacks off the terminal rendering loop.

Inspect `crates/discord-voice` and the desktop call orchestration before adding another frontend. Do not claim voice support merely because the underlying crate compiles; the frontend must expose and correctly manage the required lifecycle.
