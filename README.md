# Battle Instinct

Battle Instinct is a MOD for *Sekiro: Shadow Dies Twice*. It gives players the ability to perform multiple combat arts using motion inputs.

The MOD supports both MNK and gamepads.

## Install

Click link below to download the MOD.

[![[DOWNLOAD]](https://img.shields.io/badge/DOWNLOAD-battle--instinct__x64.zip-blue)](https://github.com/dec32/sekiro-battle-instinct/releases/latest/download/battle-instinct_x64.zip)

To install the MOD, simply unzip the archive into the game directory (usually `C:\Program Files (x86)\Steam\steamapps\common\Sekiro`). You should have the following 2 files next to `sekiro.exe`:

1. `dinput8.dll`
2. `battle_instinct.cfg`

## Solve `dinput8.dll` Conflict

If you have MOD Engine or any other MOD that utilizes `dinput8.dll` installed, rename the **other** `dinput8.dll` files to `dinput8_{whatever_you_like}.dll`. For example you may have:

```
Sekiro/
├─ dinput8.dll             # that comes from the Battle Instinct MOD
├─ dinput8_debug.dll       # that comes form the Debug Menu MOD
├─ dinput8_fps_unlock.dll  # that comes from the FPS Unlock MOD
├─ dinput8_mod_engine.dll  # that comes from the MOD Engine
├─ sekiro.exe
├─ ...
```
The MOD will automatically chain load the renamed `.dll` files for you.


## Use

Press <kbd>Block</kbd> + <kbd>Attack</kbd> to perform the default combat art.

![](./docs/combat_art_0.webp)

**Hold** a motion input and pressing <kbd>Block</kbd> + <kbd>Attack</kbd> performs the combat art bound to that direction. This is similar to how you perform Nightjar Slash Reversal in the vanilla game.

![](./docs/combat_art_1.webp)

You can also **release** the input and press <kbd>Block</kbd> + <kbd>Attack</kbd> right after to perform the same combat art. This is similar to how you perform special moves in fighting games.

![](./docs/combat_art_2.webp)

A combat art can also be bound a sequence of motion inputs. When performing such combat arts, <kbd>Block</kbd> can be omitted.

![](./docs/combat_art_3.webp)


## Customize

You can customize your control scheme by editing `battle_instinct.cfg`. Here's a short example:

```
# This is a line of comment
5300  Ichimonji
5600  Floating Passage  ∅
5200  Nightjar Slash    ↑
7600  Shadowfall        ↑↑
```

The file is a plain text table formatted with space characters. The first column stores the UIDs of the combat arts and the last column specifies how you perform the combat arts. In the last column you can write:

1. Nothing, which means this combat art is ignored.
2. `∅`, which means this is the default combat art.
3. A sequence of `↑`/`→`/`↓`/`←`, which spells the corresponding motion inputs.

The columns in between store the names of the combat arts. They're only there for reference. Feel free to modify or delete them.

> [!NOTE] 
> Binding two **adjecent** directions (such as `↓→`) to a combat art is not recommended because this kind of inputs can be used for diagonal movements. 

