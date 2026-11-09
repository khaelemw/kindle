# When a mod goes wrong: the messages and what they mean

A mod can fail at three points: **`kindle check`** on your PC, **loading** when the game starts, or **running**
when one of its handlers hits an error. This page lists the messages each one gives, with the mistake that
causes it and the fix.

The `kindle check` messages were captured on 2026-11-03 from twenty deliberately broken mods
(`kindle check <folder>`). The runtime messages are quoted from Kindle's source, with the parts that
vary in angle brackets.

Contents: [1. What happens to a mod with an error](#1-what-happens-to-a-mod-with-an-error) ·
[2. kindle check: mod.toml](#2-kindle-check-modtoml) · [3. kindle check: the Lua](#3-kindle-check-the-lua) ·
[4. What kindle check doesn't catch](#4-what-kindle-check-doesnt-catch) ·
[5. Errors while the mod runs](#5-errors-while-the-mod-runs) · [6. A routine for a broken mod](#6-a-routine-for-a-broken-mod)

## 1. What happens to a mod with an error

- **kindle check** prints `error:` lines (the mod won't work) and `warning:` lines (it will, but something
  is off), then a total. A clean mod prints `<id>: ok (<n> Lua file(s))`.
- **Loading.** A mod whose `mod.toml` doesn't parse, or whose `main.lua` errors as it runs for the first
  time, isn't started. The error goes to `logs\kindle.log` in the kindle folder.
- **Running.** An error in any handler (an event, a timer, a button) **stops the whole mod**. Kindle writes
  the error to `kindle.log`, shows a toast for 10 seconds (`<mod name> stopped: <first line of the error>
  (F1 > Log)`), unloads the mod and undoes what it changed in the game (param writes, text, hooks). The
  Mods screen shows it as failed. Save the fixed file and Kindle reloads the mod.
- **Too slow.** A call that runs longer than `script_time_limit_ms` (200 ms of CPU time by default, in
  `kindle.toml`) is stopped with `script exceeded its time limit (see script_time_limit_ms)`, and the mod
  stops as above. This guards the game against an endless loop.

Because one error stops everything, a handler that might fail on bad data (a row that may not exist, a
character that may have gone) is better checked than trusted. See [section 5](#5-errors-while-the-mod-runs).

## 2. kindle check: mod.toml

| Message | Mistake | Fix |
|---|---|---|
| `mod.toml: TOML parse error at line 1, column 1` ... `missing field \`id\`` | no `id` under `[mod]` | `id = "my-mod"`: lowercase, the folder's name by convention |
| `mod.toml: TOML parse error at line 2, column 6` ... `string values must be quoted, expected literal string` | `id = broken` | quote strings: `id = "broken"` |
| `mod.toml: unknown module "param" in permissions (available: flags, items, player, world, params, maps, text, ui, hks, events, debug, game, net, input)` | a misspelt permission | use a name from the list (`params`) |
| `kindle.params needs the "params" permission: add it to modules in mod.toml` | the Lua uses a module the manifest doesn't list | add it: `modules = ["params"]` under `[permissions]` |
| `warning: permission "world" is listed but the mod never uses kindle.world` | a permission left over | remove it; a mod should ask for what it uses |
| `warning: mod.toml: "ds" in games isn't a game this Kindle runs in (ds3: Dark Souls III, er: Elden Ring) (did you mean "ds3"?)` | a misspelt game id | `games = ["ds3"]`, `["er"]` or both. Kindle won't load the mod at all with only an unknown id |
| `mod.toml: this mod needs Kindle API 9, but this Kindle has API 1: update Kindle` | `api` higher than this Kindle's | `api = 1` (the current one) |
| `TOML parse error at line 7, column 2` ... `settings.name: unknown type "string" (bool, int, float or choice)` | a setting type that doesn't exist | one of the four. For text, use a `choice` with the options listed |
| `TOML parse error at line 7, column 2` ... `settings.mode: default: "c" isn't one of a, b` | a choice's default not in its list | make the default one of the `choices` |
| `main.lua is missing` | a mod with Lua but no `main.lua`, or a misnamed one (`Main.lua`, `main.lua.txt`) | rename it. A mod of files only (a `files\` folder and no Lua) needs no `main.lua` |

## 3. kindle check: the Lua

| Message | Mistake | Fix |
|---|---|---|
| `main.lua:1: kindle.player.positon doesn't exist (did you mean player.position?)` | a misspelt function or field | take the suggestion; the editor's autocomplete (the API stubs) stops this before it happens |
| `main.lua:1: unknown event "boss_killed"` | an event that doesn't exist | the event list is in the API reference under `kindle.on`. This one is `boss_defeated` |
| `main.lua:1: unknown event "bonfire_rested"` (an Elden Ring mod) | a Dark Souls III event in an Elden Ring mod | Elden Ring's events are fewer: no `bonfire_rested`, `damage`, `item_lot` and so on. Its list is in the ER guide |
| `main.lua:1: kindle.ui.mod_tab doesn't exist` (an Elden Ring mod) | a Dark Souls III function in an Elden Ring mod | Elden Ring's UI is `message`, `banner`, `game_banner`, `toast`, `prompt`, `status` |
| `main.lua:1: kindle.player.runes doesn't exist` (a Dark Souls III mod) | an Elden Ring function in a Dark Souls III mod | `player.souls()` / `add_souls(n)`. In a mod for both games, see [the kill streak example](https://github.com/khaelemw/kindle-examples/blob/main/ex-kill-streak/README.md) |
| `main.lua:4: unexpected symbol near ')'` | a Lua syntax error: here an `if` without its `end`, so the `end)` closes the `if` and the `)` is left over | count the `end`s: `function`, `if`, `for`, `while` and `do` each need one. The line is where Lua gave up, often after the real mistake |

A mod for both games (`games = ["ds3", "er"]`) is checked against Dark Souls III's names. To check it
against Elden Ring's too, check a copy with `games = ["er"]`.

## 4. What kindle check doesn't catch

These passed `kindle check` and fail (or misbehave) in the game. The messages are from Kindle's source.

| Mistake | What happens | Fix |
|---|---|---|
| `kindle.settings.get("speed")` with no `[settings.speed]` (kindle check finds it when the name is written out) | stops the mod: `no setting "speed": this mod declares no settings ([settings.<name>] in mod.toml)`, or `no setting "speed" (settings: <the mod's>)` | declare it in `mod.toml`, or fix the name |
| `require("util")` with no `util.lua` (kindle check finds it when the name is written out) | stops the mod: `require "util": util.lua: <the system's "file not found">` | the file next to `main.lua`; `require("lib.util")` is `lib\util.lua` |
| `row.atkPhysics = 200` (the field is `atkBasePhysics`) | stops the mod: `EquipParamWeapon has no field "atkPhysics" (see params.fields("EquipParamWeapon"))` | `kindle.params.fields(table)` in the console, lists the names |
| a param value out of its paramdef's limits | stops the mod: `<table>: <value> is outside <field>'s range <lo>..=<hi> (its paramdef)` | stay in range; a value the field already has is always allowed |
| `kindle.store.get(...)` before a character is loaded (in `main.lua`'s top level, or at the title) | stops the mod: `no character is loaded (store.get_global and store.set_global keep values that belong to no character)` | read the store in `player_loaded`, or use `get_global` for values that belong to no character |
| an int setting's `default` outside its `min`..`max` | the mod gets the default as written until the player changes the setting; the Mods screen then keeps it in range | keep the default in range |
| a misspelt option: `ui.toast("hi", { colour = "red" })` | nothing: the option is ignored and the toast is gold | the API reference's spelling (`color`) |
| a global variable (`count = 0` without `local`) | works: each mod has a Lua state of its own, so a global can't clash with another mod's | `local` anyway: a global misspelt in one place is a new variable, not an error |
| a handler that does too much each frame | slows the game, or stops the mod at the time limit | `kindle.every(seconds, fn)` for things that needn't happen every frame |

## 5. Errors while the mod runs

**Check what may be missing.** `params.get` returns nil for a row that doesn't exist, and `world.character`
returns nil for a character that isn't loaded. Indexing nil (`row.weight`) is an error that stops the mod:
`attempt to index a nil value (local 'row')`.

```lua
local row = kindle.params.get("EquipParamWeapon", id)
if not row then
  kindle.log.warn("no weapon row %d: skipped", id)
  return
end
row.weight = 0
```

**Catch what you can't check.** `pcall(fn, ...)` runs `fn` and returns `false, message` on an error instead
of stopping the mod. The [text pack example](https://github.com/khaelemw/kindle-examples/blob/main/ex-text-pack/README.md) uses it for
items named in English: a game in another language has no "Homeward Bone", and the rename is an error there.

```lua
local ok, err = pcall(kindle.items.set_name, "Homeward Bone", "Bone of Return")
if not ok then
  kindle.log.warn("couldn't rename the Homeward Bone: %s", err)
end
```

**Read the log.** The error line in `logs\kindle.log` starts with the mod's id and what was running (the
event or the file), then the message, which for an error in your Lua begins with the file and line
(`main.lua:12: attempt to index a nil value`). The traceback is left out, to keep it to one line. In the
game, with **Developer overlay** on (Mods, the Kindle tab), F1 opens the developer overlay, whose Log tab
shows the same lines.

**Memory.** A mod's Lua can use up to 128 MB. A table that grows every frame and is never emptied reaches it
eventually, and the mod stops with Lua's "not enough memory".

**The console.** `kindle console` (or `kindle run "<lua>"`) runs Lua in the game while it's running: try the
failing call there with the same arguments and see what it returns. Turn it on first: `console = true` under
`[runtime]` in `kindle\kindle.toml`, then restart the game. The console is its own mod, so an error
there doesn't stop yours.

## 6. A routine for a broken mod

1. `kindle check <folder>`. Fix every `error:` and read every `warning:`.
2. Start the game. If the mod isn't in the Mods screen, look in `kindle.log` for its id: a `mod.toml` that
   didn't parse, or `not loaded: it's for <games>, not <this game>`.
3. If it shows as failed, the toast and the log say why. Fix and save; Kindle reloads it.
4. If it runs but does nothing, add `kindle.log.info` lines at each step (the handler fired, the value read,
   the row changed) and watch the log. A handler that never fires is often an event name that exists but
   means something else: `player_loaded` fires once per character load, not on every respawn.
5. Try the one call that matters in `kindle console`, with the values from the log.
