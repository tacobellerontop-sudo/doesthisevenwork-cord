# Notification sounds

The sixteen cues in `tones/` are original to DTEW Cord. `generate.py` synthesizes each one from
sine partials with numpy and encodes it with ffmpeg's libmp3lame (48 kHz stereo, 128 kbit/s,
metadata stripped), so rerunning it rebuilds the same set. They are licensed MIT OR Apache-2.0,
like the rest of the project's own code.

| File | Used for |
| --- | --- |
| `message.mp3` | A message in another conversation |
| `current-channel.mp3` | A message in the open conversation |
| `incoming-ring.mp3` / `outgoing-ring.mp3` | Calls; they repeat every six and three seconds |
| `mute.mp3` / `unmute.mp3` | Microphone toggles |
| `deafen.mp3` / `undeafen.mp3` | Output toggles |
| `camera-on.mp3` | Camera on |
| `screen-share-on.mp3` / `screen-share-off.mp3` | Screen share start and end |
| `user-join.mp3` / `user-leave.mp3` | Someone joins or leaves your voice channel |
| `viewer-join.mp3` | Someone starts watching your stream |
| `ptt-start.mp3` / `ptt-stop.mp3` | Push-to-talk key down and up |

Each file stays under 128 KiB and six seconds, which `notification_sounds.rs` checks. Earlier
builds of the Serein base bundled Discord's own sound files; those were removed before the first
DTEW Cord release because no redistribution permission was documented.
