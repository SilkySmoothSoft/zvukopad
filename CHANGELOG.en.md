# Version history

Newest first. The version number lives in one place — `Cargo.toml` (`[workspace.package] version`).

## 1.0.0 — 2026-10-21

First public release.

- Pattern editor: sounds on tracks on a grid, BPM, pitch and volume per sound, loop playback, export to WAV.
- Russian and English interface: follows the Windows language, globe switch in the top-right corner. Own translations via a file in the `lang` folder.
- Pause and seek each sound separately, waveform with a playhead in the sound card, progress bar in the list.
- "To me", "to mic" and voice volume right in the bottom bar — hover the level meter.
- The numpad number is applied after typing; a taken number asks "overwrite?".
- Rename by double-clicking the name (and F2); right-click on the name opens the menu.
- Copy and paste in text fields, right-click menu in the text-to-speech field.
- Hide folder paths (for streams and screenshots); the user name in the log is replaced with •••.
- Program icon — the mascot Ryzhik.
- Batch file for publishing the source to GitHub, build check on a clean Windows (GitHub Actions).

## 0.9.0

- "What just happened" buffer: the last seconds of sound kept in memory, saved to a file with a key.
- Sounds get quieter while you talk (ducking).
- Automatic push-to-talk key while a sound plays "to mic".
- Random sound from a tag with a key.
- Piper — fast offline neural voices, engine and voices downloaded with a button.

## 0.2.0

- Sound number on the numpad (like Soundpad) with overlay hints.
- Overlay: display modes, look presets, tiles, sound colors.
- Control schemes (including "Like Soundpad"), mouse buttons, volume keys, key pass-through.
- Repeat last, random sound, pause.
- Recording and playback timers, queue, tags, effects (reverb, echo, radio, robot and more).
- Log, delete to Recycle Bin, trimming by waveform, rename in the list.

## 0.1.x

- First working version: sound library, hotkeys, output "to me" and "to mic", recording (microphone and
  "what's playing"), Windows speech voices, plugins, overlay.
