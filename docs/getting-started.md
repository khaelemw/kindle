# Getting started

## What you need

- **Dark Souls III 1.15.2**, the current Steam version. Kindle checks the game version and
  refuses to run on any other.
- **VS Code** with the **Lua** extension (`sumneko.lua`), for autocomplete. Optional, but it
  makes writing mods much easier.

## Installing Kindle

1. Unzip `Kindle-DS3-<version>.zip` anywhere, open `Kindle Launcher.exe` in it, and press
   **Install**. It finds Dark Souls III through Steam (or **Browse…** to its folder) and puts
   `dinput8.dll` and the `kindle` folder into the game's `Game` folder. Install again from a newer
   zip to update: your settings and your own mods stay. Without the launcher (on Linux, say),
   copy `dinput8.dll` and `kindle` from the zip into `Game` yourself (in Steam: right-click Dark
   Souls III, Manage, Browse local files, then `Game`).
2. Start the game from Steam as usual. `dinput8.dll` loads Kindle as the game starts, and the
   title screen has **Mods**. On Linux (Proton), give the game the Steam launch option
   `WINEDLLOVERRIDES="dinput8=n,b" %command%` first.
3. Switch mods on in **Mods**, on the **Mods** tab. Bonfire Fate, which comes with Kindle as an
   example, is off until you do. More mods: [Kindle's README](https://github.com/khaelemw/kindle#mods).

While Kindle runs, the game stays off FromSoftware's servers (their terms don't allow modded play
online): if the game's **Launch Setting** (Network settings) is **Play Online**, Kindle changes it
to **Play Offline** and asks you to restart the game once, since the game reads it as it starts.
Kindle's co-op mod connects players without those servers.

To remove Kindle, delete `Game\dinput8.dll` and the `Game\kindle` folder (its `backups` folder
holds copies of your saves, if you want to keep them). Kindle's `dinput8.dll` takes the place of
any other mod's `dinput8.dll`: **Install** keeps one that was there as `dinput8.dll.before-kindle`.

## Kindle Launcher (optional)

`Kindle Launcher.exe`, in the `kindle` folder, starts the game with a choice of mods: Kindle
mods, DLL mods (Seamless Co-op, a crash fix) and file mods (folders of replacement game files,
through Mod Engine 2). **Install** sets it up: it finds the game, Mod Engine 2, Seamless Co-op
and a crash fix (if installed) and writes `launcher.toml` with a `default` profile (a launcher
without one shows **Find Dark Souls III**, which does the same, as does `kindle.exe launch --setup`). Then **Launch** (or
`kindle launch`) starts the game with Kindle.

Each profile has its own Kindle mods, DLL mods (Seamless Co-op, a crash fix) and file mods.
**Add mods** picks Kindle mods from the ones installed, **Add DLL** and **Add folder** add the
others, and **Remove** takes any of them out; untick a DLL or file mod to leave it out of the
next start. A start runs only the profile's Kindle mods. Switching a mod on or off in game
(Pause → Mods) adds it to or removes it from that profile once the game closes. Kindle mods'
settings can be changed in the launcher while the game is closed, or in game.

**Profiles.** `launcher.toml` can hold several setups, each with its own DLLs and file mods
(folders of replacement game files, like a randomizer's output). Turn a Mod Engine config you
already use into a profile with `kindle launch --import <name> <config.toml>`, list them with
`kindle launch --list`, and start one with `kindle launch <name>`. In the launcher they're
on the left, with New, Rename and Delete. Kindle is always loaded, after the profile's DLLs.

**Without Mod Engine 2 (preview).** Untick **Mod Engine 2** in the launcher (or
`kindle launch <name> --direct`, or `loader = "direct"` in a profile): Kindle starts the game
and loads the DLLs itself. Profiles with file mods still need Mod Engine 2.

**Mods** in the game's title menu and pause menu opens the Mods screen: the game's own settings
screen with Kindle's tabs. Kindle holds Kindle's own settings, Mods switches each mod on or off,
and Mod menus opens a mod's menu: its buttons (`kindle.ui.mod_tab`) and its settings. Changes
apply at once; Defaults restores a tab's defaults. **All mod settings** on the Kindle tab opens
Kindle's own Mods screen, with every mod and all its settings.

For mod authors: with **Developer overlay** on (Mods → Kindle), **F1** opens the developer
overlay (F1 again closes it). Its tabs show your mods, the log, the player, nearby characters,
event flags and params, and Find looks up any name (boss, bonfire, item, place, API function)
for its id and the Lua that uses it.

What's in the `kindle` folder:

```
kindle.dll       the runtime
Kindle Launcher.exe   starts the game; your mods, each with an on/off switch
kindle.exe       command-line tools (new mod, console, launch)
kindle.toml      settings (overlay key, console port, script time limit, ...)
launcher.toml    launch profiles (kindle launch)
stubs\           type stubs for your editor
logs\kindle.log  the log
mods\            your mods, one folder each
backups\         copies of your saves, made when the game starts with mods
data\            mods' saved data and Kindle's own records
```

## Your first mod

```
kindle\kindle.exe new my-mod
```

This makes `kindle\mods\my-mod` with three files. **`mod.toml`** says what the mod is and
what it may touch:

```toml
[mod]
id = "my-mod"
name = "My Mod"
version = "0.1.0"
author = "you"
description = "What it does."
api = 1
games = ["ds3"]

[permissions]
modules = ["flags", "items", "player"]
```

`api` is the version of Kindle's Lua API the mod is written for. It only goes up when a
change could break existing mods; a mod that needs a newer API than the installed Kindle
doesn't load and says so (`kindle.api` is the running Kindle's). `kindle check` reminds you if
it's missing.

`games` says which games the mod is for, by id: `ds3` is Dark Souls III (`er`, Elden Ring, is
for Kindle's Elden Ring runtime, not finished yet). Kindle doesn't load a mod in a game its list
leaves out, and its log says so. A mod without `games` is for Dark Souls III, as every mod
written before Kindle ran in a second game is.

Optional keys under `[mod]`: `depends = ["other-mod"]` (those mods load first and must be
running, or this one doesn't start) and `conflicts = ["other-mod"]` (this mod won't start
alongside them), and `enabled = false` to keep a mod installed but off.

Players can change a mod's **settings** in the overlay's Mods tab. Declare each one in
`mod.toml` and read it with `kindle.settings.get`:

```toml
[settings.chance]
type = "float"          # "bool", "int", "float" or "choice"
label = "Effect chance"
description = "How often the effect fires."
default = 0.5
min = 0.0
max = 1.0
```

```lua
local chance = kindle.settings.get("chance")
kindle.on("setting_changed", function(name, value)
  if name == "chance" then chance = value end
end)
```

Each API module beyond the basics needs to be listed under `modules`: `flags`, `items`,
`player`, `world`, `params`, `text`, `ui`, `events`, `hks` and `debug`. A mod can't use a module it
didn't ask for, so a mod's `mod.toml` tells players what it can do. The core functions
(`kindle.on`, timers, ...) and `kindle.log` are always available.

**`main.lua`** runs when the mod loads:

```lua
local log, player = kindle.log, kindle.player

kindle.on("player_loaded", function()
  log.info("Hello, %s!", player.name())
end)
```

**`.luarc.json`** points your editor at Kindle's type stubs (see below).

If the game is running, Kindle loads the new mod within a second. Save any `.lua` or
`mod.toml` file and the mod reloads: its `unload` handlers run, its changes to params and
text are reverted, and `main.lua` runs again. An error stops only that mod; the overlay's
Mods tab and the log show the file and line, and saving again restarts it.

A mod can split its code into files: `require("util")` loads `util.lua` from the mod's
folder (`require("lib.math")` loads `lib/math.lua`), once.

**Replacing game files.** Put a file in the mod's `files` folder at its path inside the
game's archives, and the game loads it instead of its own: `files\chr\c1100.chrbnd.dcx`,
`files\event\m30_00_00_00.emevd.dcx`, `files\msg\engus\item_dlc2.msgbnd.dcx`. No Mod
Engine needed. Kindle reads the list when the game starts (restart after adding or removing
files); if two mods replace the same file, the first by folder name wins and the log says
so. A mod can be only files, with an empty `main.lua`. Map files (`files\map\...`, such as
`mapstudio\m30_00_00_00.msb.dcx`) work too, at any size.

## Editor setup

Open the mod's folder in VS Code (File > Open Folder) with the Lua extension installed. You
get:

- completion and docs for the whole API as you type `kindle.`;
- event names in `kindle.on("`, and each event's handler arguments (`e.` in a `damage`
  handler lists `target`, `attacker`, `attack_power`, `attack_id`);
- item, animation and bonfire names: `kindle.items.give("` lists every item in your game;
- param tables and their fields: `kindle.params.get("EquipParamWeapon", id).` lists the
  weapon param's fields;
- warnings for typos and wrong argument types.

The names come from `kindle\stubs\kindle-game.lua`, which Kindle writes from your own game
(in its language) the first time you load into the world. The editor indexes it once; give
it a few seconds after opening the folder.

For a mod folder you made by hand, add a `.luarc.json` with the path to your stubs:

```json
{
  "runtime": { "version": "Lua 5.4" },
  "workspace": { "library": ["C:/Program Files (x86)/Steam/steamapps/common/DARK SOULS III/Game/kindle/stubs"] },
  "diagnostics": { "globals": ["kindle"] }
}
```

## The console

Try code in the running game before putting it in a mod. The console is off until you turn it
on: in `kindle\kindle.toml`, under `[runtime]`, set `console = true` and restart the game. Then:

```
kindle\kindle.exe console
```

Type Lua and press Enter; the result is printed (tables too). The console has every module,
with no permissions needed, and keeps your variables between lines. Code can span lines:
start a `function`, `if` or table and the console waits (`...`) until it's finished. Lines
starting with `:` are commands: `:mods` lists mods, `:reload [id]` reloads them, `:log [n]`
shows the last log lines, `:run <file>` runs a Lua file, `:help` lists the rest. Without the
interactive console: `kindle run "kindle.player.hp()"` for one line,
`kindle run -f test.lua` for a file.

The console only listens on your own machine (127.0.0.1, port 27850 by default). Any program on
the PC can use it while it's on, so turn it off again when you stop writing mods.

## Sharing a mod

```
kindle\kindle.exe check my-mod
kindle\kindle.exe pack my-mod
```

`check` reads `mod.toml` with the same rules Kindle uses, compiles every Lua file, and
checks each use of the API: misspelled functions (with a suggestion), modules used without
their permission, and permissions listed but never used. Names given as text are checked too:
events, settings (declared in `mod.toml`), required files and pictures (in the mod's folder),
and, once Kindle has written `kindle\stubs\kindle-game.lua` from your game, param tables and
the fields of a row kept in a local (`local row = kindle.params.get("EquipParamWeapon", id)`,
then `row.weight`), and the keys of an options table written into the call
(`kindle.world.spawn{ enemy = "wolf", ai_enabled = false }`). `pack` runs the same check and, if
it passes, writes `my-mod-<version>.zip` holding the mod's folder (without editor files),
ready to unzip into someone else's `kindle\mods`. Both take a mod's folder or its id.

## What mods can't do

Mods run in a sandbox: Lua's `table`, `string`, `math`, `utf8` and `coroutine` libraries,
plus Kindle's API. There's no network, OS or debug access, no file access but reading the
mod's own folder (`kindle.read_data`), and no loading of compiled bytecode. Each call into a mod (an event handler, a timer) has a time limit (200 ms by
default, `script_time_limit_ms` in `kindle.toml`), so an infinite loop is stopped instead of
freezing the game, and each mod can use up to 128 MB of memory.

To remember something between sessions, use `kindle.store`: values per character (save
slot) or for the mod itself, which Kindle saves under `kindle\data\<mod id>`.

Changes a mod makes to params and text are undone when it unloads. Changes to the game world
(items given, flags set, souls) are real and saved with the character, like anything else
that happens in game.
