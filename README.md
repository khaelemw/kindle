# Kindle

Lua scripting runtime for Dark Souls III (1.15.2). Mods are Lua files that run inside the
live game, each in its own sandbox, and reload the moment you save them.

```lua
local flags, items, log = kindle.flags, kindle.items, kindle.log

flags.on_set(flags.boss.iudex_gundyr, function()
  log.info("Here's an Ember.")
  items.give("Ember")                      -- with the game's "item received" box
end)
```

**Documentation:** [docs/](docs/README.md): getting started, events and hooks, a cookbook,
and the full [API reference](docs/reference/README.md). The same docs as one searchable page:
[docs/kindle-docs.html](docs/kindle-docs.html) (also installed as `Game\kindle\docs.html`).

Built on [mlua](https://github.com/mlua-rs/mlua). Param field layouts come from the DS3
paramdefs published with [fromsoftware-rs](https://github.com/vswarte/fromsoftware-rs) (MIT);
everything else that touches the game is Kindle's own. Licenses of what Kindle includes (Lua,
Rust crates, the paramdefs): [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Install

Each game has its own release: `Kindle-DS3-<version>.zip` is Kindle for Dark Souls III. Unzip it
anywhere, open `Kindle Launcher.exe` in it and press **Install**: it finds the game through Steam
(or **Browse…** to it) and copies `dinput8.dll` and `kindle\` into the game's `Game` folder (or copy
them there yourself). Then start the game from Steam: `Game\dinput8.dll` loads Kindle as the game
starts (on Linux, with the Steam launch option `WINEDLLOVERRIDES="dinput8=n,b" %command%`). The title screen's **Mods** switches mods on; Bonfire Fate,
which comes with Kindle as an example, is off until you do. More in
[getting started](docs/getting-started.md).

## Mods

Each mod is its own repo with a release zip that unzips into `Game\kindle\mods`:

| Mod | What it does |
|---|---|
| [Bonfire Fate](mods/bonfire-fate/README.md) | every few minutes, a random cause paired with a random effect (comes with Kindle) |
| [Randomizer](https://github.com/khaelemw/kindle-randomizer) | items in new places (the game can always be finished) and enemies of other types |
| [Chaos](https://github.com/khaelemw/kindle-chaos) | every so often something random happens for a while |
| [Practice Tool](https://github.com/khaelemw/kindle-practice-tool) | save a spot with its enemies and go back to it |
| [Run Timer](https://github.com/khaelemw/kindle-run-timer) | a speedrun clock with splits for bosses and bonfires |
| [Photo Mode](https://github.com/khaelemw/kindle-photo-mode) | freeze the scene and fly the camera around it |
| [Pickup Shuffle](https://github.com/khaelemw/kindle-pickup-shuffle) | the world's pickups trade places, from a seed |
| [Co-op (beta)](https://github.com/khaelemw/kindle-coop) | play the whole game together |

Example mods for making your own, each a different kind of change: [kindle-examples](https://github.com/khaelemw/kindle-examples).

From source:

```powershell
.\install.ps1 -GameDir "<DS3 install>\Game"   # builds and copies into <Game>\kindle
```

`-GameDir` can also come from the `KINDLE_GAME_DIR` environment variable; the default is
the standard Steam location. It installs `kindle\` only: copy `target\release\dinput8.dll` into
`Game` to start the game from Steam with Kindle, or start it from the launcher.

The launcher: start the game from `kindle\Kindle Launcher.exe`. **Install** set it up (or,
the first time, **Find Dark Souls III**): it finds the game, Mod Engine 2, Seamless Co-op and a crash
fix. In the launcher you make profiles, pick each one's Kindle mods from the installed ones,
add DLL and file mods, change Kindle mods' settings, and **Launch**. The same from the command line:

```powershell
<Game>\kindle\kindle.exe launch --setup                    # finds the game, Mod Engine 2, Seamless
<Game>\kindle\kindle.exe launch --import rando config.toml  # optional: a profile from a Mod Engine config
<Game>\kindle\kindle.exe launch [profile]                   # start the game
```

Profiles in `kindle\launcher.toml` list extra DLLs and file mods; Kindle is always loaded last.
The game starts through Mod Engine 2, or without it with `--direct` / `loader = "direct"`
(preview; profiles with file mods still need Mod Engine 2).

Layout in the game folder:

```
dinput8.dll            loads Kindle when the game starts from Steam
kindle\kindle.dll      runtime
kindle\Kindle Launcher.exe   start the game; see and switch mods on or off
kindle\kindle.exe      CLI (launch, console, new mod)
kindle\kindle.toml     settings
kindle\launcher.toml   launch profiles (kindle launch --setup writes it)
kindle\stubs\          type stubs for editor autocomplete (kindle-game.lua is generated in game)
kindle\logs\kindle.log
kindle\mods\<id>\      mod.toml + main.lua
kindle\THIRD_PARTY_NOTICES.md   licenses of what Kindle includes
```

## Writing a mod

```
kindle new my-mod        # creates kindle\mods\my-mod with mod.toml, main.lua, .luarc.json
kindle new my-mod --template menu   # or item, event, hud: a working starter of that kind
kindle check my-mod      # mod.toml, Lua syntax, API names and permissions
kindle pack my-mod       # check, then zip it for sharing (my-mod-<version>.zip)
```

### Editor setup (autocomplete)

Use VS Code with the **Lua** extension (`sumneko.lua`) and open the mod's folder. The
`.luarc.json` that `kindle new` writes points the editor at `kindle\stubs`, which gives
completion, docs and type checks for the whole API: event names and each event's handler
arguments, item names (`items.give("`), animation and bonfire names, param tables and their
fields (`params.get("EquipParamWeapon", id).`), and the HKS ids. The names come from
`kindle-game.lua`, which Kindle writes from your own game (language and version) the first
time you load into the world. For a mod made by hand, copy a `.luarc.json`:

```json
{
  "runtime": { "version": "Lua 5.4" },
  "workspace": { "library": ["C:/path/to/DARK SOULS III/Game/kindle/stubs"] },
  "diagnostics": { "globals": ["kindle"] }
}
```

`mod.toml`:

```toml
[mod]
id = "my-mod"
name = "My Mod"
version = "0.1.0"

[permissions]
modules = ["flags", "items", "player", "params"]   # only what you need
```

Save any `.lua` or `mod.toml` file while the game runs and the mod reloads (its unload
handlers run and its param edits are reverted first). A script error stops only that mod
and is logged with file and line; saving again restarts it.

## API (v0.1)

| | |
|---|---|
| `kindle.on(event, fn)` | `frame` (dt), `player_loaded`, `player_unloaded`, `unload`; game events `player_died`, `region_changed(new, old)`, `character_killed(c)`, `item_gained/item_lost(item, n, name)`, `effect_applied/effect_expired(id)`; hooked `damage(e)` for every hit: change `e.attack_power` or return false to cancel; hooked `animation(e)`: replace (`e.name`) or cancel any character's animation; hooked `effect(e)`: swap (`e.id`) or block any SpEffect on any character; hooked `item_used(e)`: return false to keep the item; hooked `item_lot(e)`: roll another item lot (`e.lot`) or return false for nothing |
| `kindle.after(s, fn)` / `kindle.every(s, fn)` / `kindle.cancel(h)` | timers |
| `kindle.log.info/warn/error/debug(fmt, ...)`, `print` | log to `kindle.log` |
| `kindle.time()`, `kindle.frame()`, `kindle.mod`, `kindle.version` | runtime info |
| `require("name")` | loads `name.lua` from the mod's folder |
| **flags** | `get(id)`, `set(id, state)`, `on_set(id, fn)`, `on_clear(id, fn)` |
| **items** | `give("Ember", n, {popup})`, `remove`, `count`, `has`, `name`, `find("ring")`, `id(name)`, `info`, `goods/weapon/armor/ring(id or name)`. Names come from the game's own text, in its language; weapons take `"Name +N"` |
| **player** | `loaded`, `name`, `stats`, `hp/fp/stamina`, `max_*`, `set_*`, `heal`, `set_stat(name, n)`, `set_stats`, `set_soul_level`, `equipment()`, `equip(slot, item)`, `unequip`, `spells`, `ng_cycle`, `deaths`, `play_time`, `status()`, `poise()`, `animation()`, `set_animation_speed`, `set_no_death`, `set_no_damage`, `set_infinite_stamina/fp/items`, `warp(bonfire)`, `last_bonfire`, `souls`, `set_souls`, `add_souls`, `soul_level`, `total_souls`, `embered`, `set_embered`, `estus`, `set_estus`, `refill_estus`, `position`, `camera()` (position and look direction), `teleport`, `facing`, `set_facing`, `invulnerable`, `set_invulnerable`, `set_gravity`, `play_animation(name or id)`, `animations()`, `apply_effect(id)`, `remove_effect(id)`, `effects()`, `has_effect(id)` |
| **world** | `characters({radius, alive, player, helpers, kind})` nearest first with name/model/kind/hp/position/entity id, `character(handle or entity)`, `spawn{enemy = "black_knight", ...}` (experimental: any of 151 enemies by name, or model / npc_param / think_param; enemies from other areas get their own AI, loaded on first use, `on_spawn(c)` when they appear; `ai_enabled = false` for a still one; removed when the area unloads), `load_ai(area)`, `set_hp`, `kill`, `heal`, `teleport`, `facing`, `set_facing`, `invulnerable`, `set_invulnerable`, `set_gravity`, `set_visible`, `play_animation`, `animations`, `status`, `poise`, `animation`, `set_animation_speed`, `set_no_death`, `set_no_damage`, `set_team`, `bonfires()`, `distance(a, b)`, `apply_effect`, `remove_effect`, `effects` |
| **events** | the game's event script (EMEVD) commands: named ones like `award_item_lot`, `spawn_sfx`, `play_sound`, `warp_player`, `show_boss_bar`, `set_ai`, and `run(bank, index, format, ...)` for any command, through the game's own handler; `debug.trace_events(true)` shows the commands the game runs |
| **hks** | the behaviour scripts (HavokScript): `on_env(id, fn)` changes the answers they get from the game, `on_act(id, fn)` watches or blocks their actions (e.g. act 148 sets behaviour variables); `run_act`/`ask_env` make a character's script act or answer, `eval` runs HKS source in it; edited scripts can call mods with `kindle(name, ...)` (`on_call`); `debug.trace_hks(true)` shows the player's calls |
| **settings** | always available: options declared under `[settings.<name>]` in mod.toml (bool, int, float, choice) appear in the overlay's Mods tab; `settings.get(name)`, `kindle.on("setting_changed", fn)` |
| **mods** | always available: `kindle.export{...}` offers functions to other mods, `mods.get(id)` calls theirs, `kindle.send(name, data)` / `kindle.on_message(name, fn)` pass messages; `depends` / `conflicts` in mod.toml |
| **store** | always available: `get(key, default)` / `set(key, value)` per character (save slot), `get_global` / `set_global` for the mod's settings; saved under `kindle\data\<mod id>` |
| **params** | `get(table, row)` → live row (`row.field`, `row.field = v`), `add_row(table, id, copy_from)` adds brand-new rows (new items with their own ids), `copy(table, {{from, to}, ...}, keep)` copies rows over others in one go, `reset` puts back what the mod wrote, `tables`, `fields`, `rows` |
| **maps** | the game's map files: `parts(map)` (enemies, objects, collisions as the file has them), `edit(map, changes)` makes a map's enemy one of another type from any map (keeping its place and walk route), applied when the area next loads; `preload`, `reset` |
| **input** | what the player presses: `held`, `using_pad`, `press`; `block_actions` / `block_interact` keep the player's actions from the game |
| **net** | the game's online session over Kindle's own connection: `host_session`, `join(code)`, `leave`, `code`, `state`, `members`, `pings`; a lobby before the world (`open_lobby`, `join_lobby`, `lobby_send`); `send` / `on` pass this mod's messages to its copies on the other games |
| **game** | the catalogue: the game's decisions, values, functions and notifications by name (`catalogue()`): `decide` / `on_decide` answer a hook point, `get` / `set` a field, `call` a function, `on` an event |
| **text** | `get(category, id)`, `set(category, id, text)` (adds the entry if the id has none), `reset`, `item(item, field)`; plus `items.set_name/set_description/set_info/reset_text`. Live, reverted on unload |
| **ui** | `banner(text, {seconds, color})`, `toast(text, {seconds, color})`, drawn by the overlay even when it's closed; `message(text)` in the game's own dialog box; `game_banner(name)` the game's own banners (e.g. `"bonfire_lit"`); `set_hud("off"|"on"|"auto")` the HUD setting; `add_menu_option{text, on_select, where}` adds options to the game's own NPC and bonfire menus; from there `menu{entries}` opens native sub-menus (bonfires and NPCs that sell things) and `shop(first, last)` the game's shop |
| **debug** | read-only memory inspection: `singletons`, `singleton(name)`, `read(addr, type)`, `bytes`, `wstr`, `module` (for reverse engineering; safe on bad addresses) |

Full reference: [docs/reference](docs/reference/README.md), generated from the annotations in
[`dist/stubs/kindle.lua`](dist/stubs/kindle.lua) by `cargo run -p kindle-docs`.

## Replacing game files

A mod's `files` folder replaces game files by their path in the archives, e.g.
`files\chr\c1100.chrbnd.dcx` or `files\event\m30_00_00_00.emevd.dcx`, without Mod Engine
(read when the game starts; the first mod by folder name wins a conflict).

## Overlay

Pick **Mods** in the game's title menu or pause menu (the skull, sixth in the top row) for the
Mods screen: Kindle's own settings, each mod on or off, and a tab for each mod's settings.
**All mod settings** (Kindle tab) opens Kindle's own Mods screen with every setting.
`game_menus = false` in `kindle.toml` → `[overlay]` removes those entries.

With **Developer overlay** on (Mods → Kindle, the launcher, or `developer = true` in `[overlay]`),
**F1** (`toggle_key`) opens the developer overlay:

| Tab | |
|---|---|
| Console | Lua inside the game, same as `kindle console` |
| Mods | status, errors, permissions, time spent in Lua (ms/s), Reload / Disable / Enable buttons (remembered across sessions), and each mod's settings |
| Player | HP/FP/stamina, stats, inventory |
| Nearby | living characters within 40 m: HP, distance, entity id; Kill / Heal / apply an effect |
| Find | search every name Kindle knows (bosses, bonfires, items, places, NPC names, param tables, events, API functions) for its id and the Lua that uses it; click to copy |
| Flags | watch flags live, set/clear |
| Params | browse any table/row, edit numeric fields (until restart) |
| Log | live log, errors highlighted |

While it's open the game ignores keyboard, mouse and controller input. Drag it by its title bar.

## Console

```
kindle console                         # interactive Lua inside the running game
kindle run "flags.get(flags.boss.iudex_gundyr)"   # one-shot
kindle run ":mods"                     # :mods :reload [id] :log [n] :status :help
```

The console is off until `console = true` in `kindle.toml` (`[runtime]`); it listens on 127.0.0.1 only
(`console_port`).

## Faster starts

- The Bandai Namco and FromSoftware logos are skipped, so the title comes about 5 s sooner
  (`skip_logos` in `kindle.toml`).
- Kindle remembers where it found the game's code in `kindle\cache\patterns.json` (for this
  game build), so later starts skip the searches (about a second, some of it while the game
  shows its first frames). Deleting the file is harmless.

## Safety

- **Safe mode:** hold Shift while the game starts (until the title screen), or set
  `safe_mode = true` in `kindle.toml`, and Kindle loads no mods that session.
- **Save backups:** each time the game starts with mods, Kindle copies your saves to
  `kindle\backups\<date time>` if they changed since the last copy (the newest 10 are kept;
  `save_backups` in `kindle.toml`), with a `mods.txt` of the mods installed then. The Mods tab
  lists every mod that has run with the loaded character.
- The overlay's Mods tab shows how much time each mod spends in Lua (ms per second).
- Mods get `table string math utf8 coroutine` only — no files, OS or debug library, and
  text chunks only (no bytecode).
- Each script call has a time limit (`script_time_limit_ms`); infinite loops are aborted.
- Param tables are layout-checked against game memory; a mismatching table is unavailable.
- Hooks into the game are installed only when needed. Most swap a function pointer the game
  calls through (damage, menus, behaviour scripts); a few patch the start of a game function
  (animation, effect and item-use events, and adding `kindle()` to behaviour scripts).
  Arxan doesn't restore either kind.
- A panic inside Kindle stops Kindle, not the game.

## License

Kindle is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
What it includes from others is under their own licenses: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
