ZVUKOPAD — sounds on hotkeys, to your headphones and "to the mic"
=================================================================

Run: zvukopad.exe. Nothing needs to be installed.

Your sounds are in   Documents\Zvukopad\Sounds
Settings are in      %APPDATA%\Zvukopad
This program folder can be deleted or replaced with a new version — your data stays.

LANGUAGE
  The interface follows the Windows language (Russian or English, otherwise English).
  To change it: the globe button in the top-right corner, or Settings → Language.

HOW TO TALK WITH SOUNDS IN DISCORD / SKYPE
1. Install VB-Cable (free): https://vb-audio.com/Cable/ — and restart the computer.
2. Zvukopad → Settings → "To mic": choose CABLE Input.
3. In the same place enable "Mix my voice" and choose your microphone.
4. Discord → Settings → Voice & Video → Input Device: CABLE Output.
5. In Discord turn off noise suppression and automatic input sensitivity.
6. At the top of the Zvukopad window choose "To me and to mic".

HOTKEYS ("Zvukopad" scheme, change in Settings → Controls)
  Ctrl+Alt+S — stop all sounds           Ctrl+Alt+P — pause
  Ctrl+Alt+Space — repeat the last       Ctrl+Alt+X — random sound
  Ctrl+Alt+M — to me / to mic / both     Ctrl+Alt+Up/Down — volume
  Ctrl+Alt+O — overlay                   Ctrl+Alt+I — overlay for 3 seconds
  Ctrl+Alt+R — record                    Ctrl+Alt+E — effects
  Ctrl+Alt+B — save "what just happened" (if the buffer is on, "Record" tab)
  Ctrl+Alt+Q — queue play/stop           Ctrl+Alt+N — next in queue
  Ctrl+Alt+H — turn sound hotkeys off/on

SOUND NUMBER ON THE NUMPAD (like in Soundpad)
  Hold Alt, type the number with the numpad digits (NumLock on), release Alt.
  While you type, the overlay shows matching sounds.
  Alt+0 — repeat the last sound, Alt+period — cancel.
Hotkeys for individual sounds are assigned on the "Sounds" tab (mouse buttons work too).

RANDOM SOUND FROM A TAG
  Right-click a tag on the "Sounds" tab → "Assign a 'random from tag' key".

OFFLINE TEXT TO SPEECH (Piper)
  "Text to speech" tab → "Download engine" (22 MB) → download a voice (≈63 MB) → engine "Piper".
  Built-in list has Russian voices; English and other voices can be downloaded by link
  from https://huggingface.co/rhasspy/piper-voices (the .onnx file of a voice).
  Everything goes to %LOCALAPPDATA%\Zvukopad\tts\piper.

The plugins folder holds speech synthesis plugins. Don't delete it.

On first launch Windows may show "Windows protected your PC":
the program isn't signed with a paid certificate. Click "More info" → "Run anyway".