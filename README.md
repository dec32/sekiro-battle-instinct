# Battle Instinct

Battle Instinct is a MOD for *Sekiro: Shadow Dies Twice*. It gives players the ability to perform multiple combat arts using directional inputs.

The MOD supports both keyboard & mouse and game controllers.

## Install

Click link below to download the MOD.

[![[DOWNLOAD]](https://img.shields.io/badge/DOWNLOAD-battle--instinct__x64.zip-blue)](https://github.com/dec32/sekiro-battle-instinct/releases/latest/download/battle-instinct_x64.zip)

To install the MOD, simply unzip the archive into the game directory (usually `C:\Program Files (x86)\Steam\steamapps\common\Sekiro`). You should have the following 2 files next to `sekiro.exe`:

1. `dinput8.dll`
2. `battle_instinct.cfg`

## Compatibility Issue with `dinput8.dll`

If the game is already modded with MOD Engine or any other MOD that utilizes `dinput8.dll`. You can rename the **other** `.dll` files into `dinput8_{whatever_you_like}.dll`. The MOD will automatically chain load the renamed `.dll` files for you. For example you may have:

```
Sekiro/
├─ dinput8.dll             # that comes from the Battle Instinct MOD
├─ dinput8_debug.dll       # that comes form the Debug Menu MOD
├─ dinput8_fps_unlock.dll  # that comse from the FPS Unlock MOD
├─ dinput8_mod_engine.dll  # that comes from the MOD Engine
├─ sekiro.exe
├─ ...
```