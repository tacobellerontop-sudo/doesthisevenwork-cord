#!/usr/bin/env python3
"""Synthesizes DTEW Cord's notification and call tones into assets/sounds/tones/.

Every cue is generated here from sine partials, so the files are original to this project
(MIT OR Apache-2.0, like the code). Needs numpy and ffmpeg with libmp3lame.
Run: python3 assets/sounds/generate.py
"""
import subprocess
import wave
from pathlib import Path

import numpy as np

RATE = 48000
OUT = Path(__file__).resolve().parent / "tones"


def note(freq, length, decay=6.0, partials=((1, 1.0), (2, 0.35), (3, 0.12))):
    t = np.arange(int(RATE * length)) / RATE
    tone = sum(level * np.sin(2 * np.pi * freq * ratio * t) for ratio, level in partials)
    attack = np.minimum(t / 0.006, 1.0)
    release = np.minimum((length - t) / 0.02, 1.0)
    return tone * attack * release * np.exp(-decay * t)


def cue(length, notes, gain=0.32, pan=0.0):
    """notes: (start seconds, frequency, note length, decay)."""
    mono = np.zeros(int(RATE * length))
    for start, freq, size, decay in notes:
        part = note(freq, size, decay)
        first = int(RATE * start)
        mono[first:first + len(part)] += part[: len(mono) - first]
    mono *= gain / max(1e-9, np.max(np.abs(mono)))
    left, right = mono * (1 - max(pan, 0)), mono * (1 + min(pan, 0))
    return np.stack([left, right], axis=1)


def ring(length, period, pattern, gain):
    notes = []
    start = 0.0
    while start + 0.6 < length:
        notes += [(start + offset, freq, size, 5.0) for offset, freq, size in pattern]
        start += period
    return cue(length, notes, gain)


C5, D5, E5, F5, G5, A5, B5 = 523.25, 587.33, 659.25, 698.46, 783.99, 880.0, 987.77
C6, E6, G6 = 1046.5, 1318.5, 1567.98
G4, A4, E4, C4 = 392.0, 440.0, 329.63, 261.63

CUES = {
    "message": cue(0.38, [(0, E6, 0.38, 14)]),
    "current-channel": cue(0.70, [(0, C6, 0.3, 12), (0.12, G6, 0.58, 9)], 0.22),
    "incoming-ring": ring(5.3, 1.3, [(0, E5, 0.5), (0.18, G5, 0.5), (0.36, C6, 0.6)], 0.34),
    "mute": cue(0.45, [(0, A5, 0.2, 14), (0.12, E5, 0.33, 12)]),
    "unmute": cue(0.45, [(0, E5, 0.2, 14), (0.12, A5, 0.33, 12)]),
    "deafen": cue(0.75, [(0, C6, 0.25, 10), (0.15, G5, 0.25, 10), (0.3, C5, 0.45, 8)]),
    "undeafen": cue(0.80, [(0, C5, 0.25, 10), (0.15, G5, 0.25, 10), (0.3, C6, 0.5, 8)]),
    "camera-on": cue(1.0, [(0, D5, 0.4, 7), (0.2, A5, 0.8, 5)]),
    "screen-share-on": cue(1.75, [(0, C5, 0.6, 5), (0.25, E5, 0.6, 5), (0.5, G5, 1.25, 3.5)]),
    "screen-share-off": cue(1.65, [(0, G5, 0.6, 5), (0.25, E5, 0.6, 5), (0.5, C5, 1.15, 3.5)]),
    "outgoing-ring": ring(2.45, 2.45, [(0, A4, 0.8), (0.4, C5, 1.6)], 0.28),
    "user-join": cue(1.1, [(0, G5, 0.4, 8), (0.18, C6, 0.92, 5)]),
    "user-leave": cue(1.0, [(0, C6, 0.4, 8), (0.18, G5, 0.82, 5)]),
    "ptt-start": cue(0.12, [(0, B5, 0.12, 25)], 0.22),
    "ptt-stop": cue(0.15, [(0, F5, 0.15, 22)], 0.22),
    "viewer-join": cue(1.65, [(0, E5, 0.5, 6), (0.2, A5, 0.5, 6), (0.4, E6, 1.25, 4)], 0.26),
}


def main():
    OUT.mkdir(exist_ok=True)
    for name, data in CUES.items():
        pcm = (np.clip(data, -1, 1) * 32767).astype("<i2")
        temporary = OUT / f"{name}.wav"
        with wave.open(str(temporary), "wb") as out:
            out.setnchannels(2)
            out.setsampwidth(2)
            out.setframerate(RATE)
            out.writeframes(pcm.tobytes())
        subprocess.run(
            ["ffmpeg", "-loglevel", "error", "-y", "-i", str(temporary), "-codec:a", "libmp3lame",
             "-b:a", "128k", "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:a", "+bitexact",
             str(OUT / f"{name}.mp3")],
            check=True,
        )
        temporary.unlink()


if __name__ == "__main__":
    main()
