# Ways to do it: one goal, every way Kindle offers

Most things a mod wants can be done more than one way, and the ways differ in what they reach, how long they
last and how much can go wrong. This page takes the common goals one at a time and lists the ways with a short
snippet and when to pick each. The examples named are in Kindle's `examples` folder. Every snippet passes `kindle
check` (as part of a scratch mod with the permissions it needs).

Dark Souls III unless marked. Kindle for Elden Ring is in development, with a smaller API;
[tour 6](mod-tours.md#tour-6-one-mod-for-both-games) covers a mod for both.

Contents: [Give the player an item](#give-the-player-an-item) · [Change what a pickup or drop gives](#change-what-a-pickup-or-drop-gives) ·
[Make enemies harder or easier](#make-enemies-harder-or-easier) · [Change the player](#change-the-player) ·
[Show something on screen](#show-something-on-screen) · [Make a menu or a button](#make-a-menu-or-a-button) ·
[Run code later, or again and again](#run-code-later-or-again-and-again) · [Keep state](#keep-state) ·
[React to what happens](#react-to-what-happens) · [Change movement and actions](#change-movement-and-actions) ·
[Change text](#change-text) · [Change the world](#change-the-world) · [Mods working together](#mods-working-together) ·
[Lua or Rust](#lua-or-rust)

## Give the player an item

| Way | Pick it when | Example |
|---|---|---|
| `items.give` | the mod decides when: a reward, a start kit | [new weapon](https://github.com/khaelemw/kindle-examples/blob/main/ex-new-weapon/README.md) |
| `events.award_item_lot` | you want the game's own reward box for a whole lot (several items, a lottery) | [event scripts](https://github.com/khaelemw/kindle-examples/blob/main/ex-event-scripts/README.md) |
| a shop row and `ui.shop` | the player chooses, and pays | [new item and shop](https://github.com/khaelemw/kindle-examples/blob/main/ex-new-item-shop/README.md) |
| an ItemLotParam row | it should lie in the world, picked up once | [pickup shuffle](https://github.com/khaelemw/kindle-examples/blob/main/ex-pickup-shuffle/README.md) |
| `ui.character_rows` | it's chosen when the character is made | [character start](https://github.com/khaelemw/kindle-examples/blob/main/ex-character-start/README.md) |

```lua
-- Once per character: the mark goes first, so an error after it can't give twice.
kindle.on("player_loaded", function()
  if kindle.store.get("gift_given") then return end
  kindle.store.set("gift_given", true)
  kindle.items.give("Ember", 3)               -- by name (in the game's language) or kindle.items.goods(500)
end)

kindle.events.award_item_lot(4000)            -- an ItemLotParam row, given as an event gives it
```

Without the done-once mark, `player_loaded` gives the item every time the character loads.

## Change what a pickup or drop gives

| Way | Reaches | Lasts | Example |
|---|---|---|---|
| write the ItemLotParam row | every roll of that lot: the pickup, or every enemy that drops it | while the mod runs | [pickup shuffle](https://github.com/khaelemw/kindle-examples/blob/main/ex-pickup-shuffle/README.md) |
| `params.copy` between lots | as above, many lots at once, fields kept | while the mod runs | pickup shuffle |
| the `item_lot` hook | one roll at a time: decide per drop | per roll | [random drops](https://github.com/khaelemw/kindle-examples/blob/main/ex-random-drops/README.md) |
| NpcParam's `ItemLotId1` | which lot an enemy type drops | while the mod runs | |

```lua
-- One lot: always an Ember, one of it.
local lot = kindle.params.get("ItemLotParam", 4000)
lot.ItemLotId1 = 500
lot.LotItemCategory01 = 0x40000000 -- goods
lot.LotItemNum1 = 1

-- Per roll: a tenth of the time, another lot instead.
kindle.on("item_lot", function(e)
  if e.lot == 4000 and math.random() < 0.1 then
    e.lot = 4010
  end
end)
```

The hook's catch: the lot that is rolled sets its own pickup flag. Sending a one-time pickup to another
one-time lot marks the other as taken. Redirect drops that come back (no flag), or point at a lot of your own
(`params.add_row`) that carries the original's `getItemFlagId`.

## Make enemies harder or easier

| Way | Reaches | Example |
|---|---|---|
| NpcParam `hp`, `getSoul` | every enemy of that row (enemy type) | [param tuning](https://github.com/khaelemw/kindle-examples/blob/main/ex-param-tuning/README.md) |
| the `damage` hook | every hit, decided per hit: who, from whom, what attack | [difficulty](https://github.com/khaelemw/kindle-examples/blob/main/ex-difficulty/README.md), [hard mode](https://github.com/khaelemw/kindle-examples/blob/main/ex-hard-mode/README.md) |
| `world.set_hp` on one character | the enemy in front of you, now | [practice tool](https://github.com/khaelemw/kindle-examples/blob/main/ex-practice-tool/README.md) |
| a different enemy in its place (`maps.edit`) | that spot in the map | [enemy swap](https://github.com/khaelemw/kindle-examples/blob/main/ex-enemy-swap/README.md) |

```lua
-- The High Wall's Hollow Soldiers (NpcParam row 110070) with double HP.
local hollow = kindle.params.get("NpcParam", 110070)
hollow.hp = hollow.hp * 2

-- Hits on enemies do 20% less; hits on the player 50% more.
kindle.on("damage", function(hit)
  if hit.target and hit.target.is_player then
    hit.attack_power = hit.attack_power * 1.5
  elseif hit.attacker and hit.attacker.is_player then
    hit.attack_power = hit.attack_power * 0.8
  end
end)
```

A param change is cheap and applies everywhere; the hook costs a little on every hit and can look at
anything. Whether an enemy already standing there takes a changed `hp` at once isn't checked; reloading the
area (rest at a bonfire) makes sure.

## Change the player

| Way | Pick it when |
|---|---|
| `player.set_stat(s)`, `set_soul_level` | levels and attributes, as the level-up screen sets them |
| `player.set_hp`, `set_fp`, `set_stamina`, `heal`, `set_estus` | the numbers right now |
| `player.apply_effect(id)` | an SpEffect: buffs, regeneration, any of the game's effects |
| SpEffectParam rows (`params`) | change what an effect does, for everyone it's on |
| `player.set_no_damage`, `set_infinite_stamina` | practice and debug switches |

```lua
kindle.player.set_stats({ vigor = 20, endurance = 15 })   -- the soul level follows unless keep_level
kindle.player.set_hp(kindle.player.max_hp())
kindle.player.apply_effect(100000)                        -- an SpEffectParam row id (an example id)
```

## Show something on screen

| Way | Looks like | Stays |
|---|---|---|
| `ui.toast(text, {color, seconds})` | a small note at the top right, up to 6 stacked | 4 seconds |
| `ui.status(text, progress)` | a line across the top of the screen (hidden while a menu is open), with a bar if `progress` is given | until `ui.status(nil)` |
| `ui.banner(text, {color, seconds})` | big centred text on a dark band | 3 seconds |
| `ui.place_banner(text)` | the game's own place-name banner | as the game's |
| `ui.game_banner(name)` | one of the game's banners exactly ("bonfire_lit", "you_died" ...) | as the game's |
| `ui.prompt(text)` | an action prompt at the bottom, like "Rest at bonfire" | until `ui.prompt(nil)` |
| `ui.message(text)` | the game's "OK" dialog | until dismissed |
| `ui.mod_tab(title, rows)` with info rows | lines in the mod's menu (Mods, then Mod menus) | until replaced |
| `ui.set_hud("off")` | hides the HUD | until the mod unloads |
| `ui.layer(elements)` | anything drawn: text, boxes, lines, bars, circles, on a 1920 x 1080 canvas or over a point in the world | until `layer:set` or `layer:remove` |

```lua
kindle.ui.toast("Saved", { color = "green" })
kindle.ui.status("Hollowing", 0.4)
kindle.ui.banner("A NEW CHALLENGER", { seconds = 4, color = "red" })
kindle.ui.place_banner("The Forgotten Road")
kindle.ui.game_banner("bonfire_lit")
local hud = kindle.ui.layer { { text = "Deaths: 0", x = 40, y = 1000, size = 32 } }
hud:set { { text = "Deaths: 1", x = 40, y = 1000, size = 32 } }
```

Toasts tell, status lines track, banners announce, prompts invite, the dialog interrupts, layers draw what
the others can't (a counter in a corner, a bar over an enemy: [HUD](https://github.com/khaelemw/kindle-examples/blob/main/ex-hud/README.md)). Elden Ring has
`message(text, kind)` with its own kinds, `banner`, `game_banner`, `toast`, `prompt` and `status`.

## Make a menu or a button

| Way | Where | Example |
|---|---|---|
| `ui.screen` | a full menu of the mod's own in the game's style: buttons, switches, choices, sliders, typed text, lines with values, headings, sub-screens | [mod screen](https://github.com/khaelemw/kindle-examples/blob/main/ex-mod-screen/README.md) |
| `ui.choose`, `ui.confirm` | a pick from a list, a yes or no (screens ready-made) | mod screen |
| `ui.mod_tab` | Esc, then Mods, then Mod menus: the mod's menu, up to 16 rows of buttons and info, with its settings | [run timer](https://github.com/khaelemw/kindle-examples/blob/main/ex-run-timer/README.md), [practice tool](https://github.com/khaelemw/kindle-examples/blob/main/ex-practice-tool/README.md) |
| `ui.add_menu_option` | a new line in bonfire and NPC menus; `screen = {...}` opens a screen of the mod's | [bonfire travel](https://github.com/khaelemw/kindle-examples/blob/main/ex-bonfire-travel/README.md) |
| `ui.menu` | a list in the game's own menu, from a menu option; nests | bonfire travel |
| `ui.open(screen)` | the game's level up, attune, storage, travel, reinforce, infuse, repair or estus screen | bonfire travel |
| `ui.title_entry` | a line on the title menu that opens your tab | |
| settings in `mod.toml` | the mod's settings in the Mods screen, with no code | [hard mode](https://github.com/khaelemw/kindle-examples/blob/main/ex-hard-mode/README.md) |

```lua
kindle.ui.add_menu_option {
  text = "Level up here",
  where = "bonfire",
  on_select = function() kindle.ui.open("level_up") end,
}

kindle.ui.mod_tab("My mod", {
  { label = "Deaths: 3", info = true },
  { label = "Reset", help = "Starts the count again.", on_press = function() kindle.store.set("deaths", 0) end },
})
```

A screen is the one to build a whole menu with: it's the mod's own, any length, any mix of rows, and it opens
from anywhere (a menu option, a tab's button, a hotkey through `key_pressed`).

```lua
kindle.ui.add_menu_option {
  text = "Options", where = "bonfire",
  screen = function()
    return { title = "Options", rows = {
      { text = "Hard mode", toggle = hard, on_change = function(on) hard = on end },
      { text = "Seed", input = seed, on_change = function(text) seed = text end },
    } }
  end,
}
```

For a value the player sets once, use a setting rather than a button. The Mods screen already draws it, saves
it and tells the mod with `setting_changed`.

## Run code later, or again and again

| Way | Runs |
|---|---|
| `kindle.after(seconds, fn)` | once, later |
| `kindle.every(seconds, fn)` | repeatedly; returns a handle for `kindle.cancel` |
| `kindle.on("frame", fn(dt))` | every frame, with the seconds since the last |
| `flags.on_set(id, fn)` / `on_clear` | when a flag turns on or off |
| an event or hook | when the game does the thing |

```lua
local timer = kindle.every(1, function()
  if kindle.player.loaded() and kindle.player.hp() < 100 then
    kindle.ui.toast("Low HP", { color = "red" })
  end
end)
kindle.after(30, function() kindle.cancel(timer) end)
```

A frame handler runs 60 times a second, so keep it short. If a check needn't happen every frame, `every`
with a second or half a second costs a sixtieth or a thirtieth as much. An event is better than polling
whenever one exists.

## Keep state

| Way | Survives | For |
|---|---|---|
| a `local` | until the mod reloads (saving a file reloads it) | anything short-lived |
| `kindle.keep(key, default)` | reloads, not quitting the game | state you don't want reset while you edit the mod |
| `kindle.store.get/set` | quitting; one per character (save slot) | the character's progress: counts, marks, a run |
| `kindle.store.get_global/set_global` | quitting; one for the mod | records across characters, a best time |
| settings | quitting; one for the mod; the player edits them | choices |
| event flags (`flags.set`) | in the save, as the game's own | state the game's own scripts should see |

```lua
local session = kindle.keep("session", { kills = 0 })    -- change it in place: session.kills = session.kills + 1

kindle.on("player_loaded", function()
  local runs = kindle.store.get("runs", 0) + 1
  kindle.store.set("runs", runs)
  local best = kindle.store.get_global("best_kills", 0)
  kindle.log.info("run %d, best ever %d", runs, best)
end)
```

The store needs a loaded character: read it in `player_loaded`, never at the top of `main.lua`.

## React to what happens

| Way | Pick it when |
|---|---|
| a game event (`player_died`, `character_killed`, `item_gained`, `level_up`, `bonfire_rested`, `boss_defeated`, `shop_buy`, `region_changed` ...) | you only need to know |
| a hook (`damage`, `animation`, `effect`, `item_used`, `item_lot`) | you want to change or stop it as it happens |
| `flags.on_set` | the game's scripts mark it with a flag (a boss's defeat, a door opened, an NPC's step) |
| `hks.on_act`, `hks.on_env`, `hks.on_call` | the behaviour script decides it (movement, actions, an edited script calling `kindle(...)`) |
| polling with `every` | nothing tells you: compare a value with the last |

```lua
kindle.on("item_gained", function(item, count, name)
  kindle.log.info("got %d x %s", count, name or "?")
end)

kindle.on("effect", function(e)
  if e.target.is_player and e.id == 4000 then   -- an SpEffectParam row id (an example id)
    return false                                 -- blocked
  end
end)

kindle.flags.on_set(kindle.flags.boss.iudex_gundyr, function()   -- boss defeat flags by name
  kindle.ui.toast("Iudex Gundyr is down")
end)
```

## Change movement and actions

| Way | Reaches | Example |
|---|---|---|
| `hks.on_act` returning false | blocks one action the behaviour script asks for (sprint, a variable write) | [hks movement](https://github.com/khaelemw/kindle-examples/blob/main/ex-hks-movement/README.md), [hard mode](https://github.com/khaelemw/kindle-examples/blob/main/ex-hard-mode/README.md) |
| `hks.on_env` returning a value | changes an answer the script gets (a fall's height, a state) | hks movement |
| the `animation` hook | plays a different animation, or none | |
| `input.block_actions(true)` | the player's buttons for actions (not menus or camera) | - |
| an edited behaviour script calling `kindle(name, ...)` | anything the script does, with the mod answering | `hks.on_call` in the API reference |

```lua
kindle.on("animation", function(e)
  if e.character.is_player and e.name == "W_Rolling" then
    return false                                -- no rolling
  end
end)
```

`kindle.debug.trace_hks(true)` logs the player's script calls, named where known. Turn it on, do the move,
and the log shows which `act` or `env` to catch.

## Change text

| Way | Pick it when |
|---|---|
| `items.set_name`, `set_info`, `set_description` | an item's text, by item |
| `text.set(category, id, value)` | any text by category (`place_name`, `npc_name`, `event_text`, `talk` ...) and id; adds the id if the game has none |
| a `files\` mod with the game's text files | a whole translation |

```lua
kindle.items.set_name("Ember", "Spark")
kindle.text.set("place_name", 300000, "The Forgotten Road")   -- a place name id (an example id)
```

All of them go back when the mod unloads. [Text pack](https://github.com/khaelemw/kindle-examples/blob/main/ex-text-pack/README.md)
does both.

## Change the world

| Way | Reaches | Example |
|---|---|---|
| `maps.edit` with `replace_enemy` | an enemy in the map file becomes another type, before the map loads | [enemy swap](https://github.com/khaelemw/kindle-examples/blob/main/ex-enemy-swap/README.md) |
| `world.spawn` | a new enemy now, near the player | [spawner](https://github.com/khaelemw/kindle-examples/blob/main/ex-spawner/README.md) |
| `events.add`, `events.start`, `events.edit` | the map's event scripts: add one, start one, change one | [event scripts](https://github.com/khaelemw/kindle-examples/blob/main/ex-event-scripts/README.md) |
| `events.run` and the named commands | one event command, now (a warp, an effect, a banner) | event scripts |
| a `files\` mod | any game file served in place of the game's | [swap lighting](https://github.com/khaelemw/kindle-examples/blob/main/ex-swap-lighting/README.md) |

A map edit is the deepest change: the enemy is part of the map from the start, with its place and route.
A spawn is the quickest but is gone when the area reloads. Event script changes reach what the game's own
scripts do: doors, bosses, rewards, warps.

## Mods working together

| Way | Pick it when | Example |
|---|---|---|
| `kindle.export` and `kindle.mods.get` | one mod offers functions another calls, with results | [shared library](https://github.com/khaelemw/kindle-examples/blob/main/ex-shared-lib/README.md) and [its user](https://github.com/khaelemw/kindle-examples/blob/main/ex-shared-lib-user/README.md) |
| `kindle.send` and `kindle.on_message` | a mod announces something; anyone may listen; no reply | |
| `depends` in `mod.toml` | the other mod must load first | shared library user |

```lua
-- In mod A:
kindle.export { double = function(n) return n * 2 end }

-- In mod B:
local a = kindle.mods.get("mod-a")
if a then kindle.log.info("%d", a.double(21)) end

kindle.send("boss_down", { name = "Vordt" })
kindle.on_message("boss_down", function(data, from) kindle.log.info("%s says %s fell", from, data.name) end)
```

## Lua or Rust

| Way | Pick it when | Where |
|---|---|---|
| Lua | almost always: reloads on save, the whole API, checked by `kindle check` | the API reference |
| Rust in Kindle itself | a new API every mod should have: a hook, a game function | `crates/` in Kindle's repository |

A Lua mod's errors stop the mod, not the game, and its param writes are range-checked. Code in Kindle runs in
the game's process with no sandbox, so a mistake there can crash the game. Use it for what the API can't
reach, then give Lua mods the new API.
