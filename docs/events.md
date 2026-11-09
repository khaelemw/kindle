# Events and hooks

A mod's `main.lua` runs once, when the mod loads. Everything after that happens in handlers:
functions the mod registers to be called when something happens in the game.

```lua
local handle = kindle.on("player_died", function()
  kindle.log.info("You died.")
end)

kindle.cancel(handle) -- stop listening
```

Every `kindle.on`, timer, flag watcher and menu option returns a handle for
`kindle.cancel`. All of them are removed when the mod unloads, so a mod never needs to clean
up its handlers itself.

## Kinds of event

### Lifecycle

| Event | When |
|---|---|
| `player_loaded` | a character entered the world: after loading a save, and again after load screens where the game rebuilds the player. If one already is when the mod loads (or reloads), right away. |
| `player_unloaded` | the character leaves the world (load screen, quitting to the menu). |
| `unload` | the mod is about to be reloaded or removed. Undo anything the game won't undo for you. |
| `setting_changed` | the player changed one of the mod's settings in the overlay: `fun(name, value)`. |
| `map_loading` | the game is about to read an area's file: `fun(map, file)`, file "map" (what stands where) or "events" (its event scripts). Changes made now (params, `kindle.maps`, `kindle.events`) are in place for this load; the loading screen waits for the handlers (10 seconds at most). |
| `frame` | every frame, with the seconds since the last one. |

Most game functions only work while a character is loaded; `kindle.player.loaded()` tells
you. A common shape:

```lua
kindle.on("player_loaded", function()
  -- set up per-load state here
end)
```

### Game events

Kindle notices these by checking the game's state once per frame, and only while some mod
listens for them. They report what happened; they can't change it.

| Event | Handler | When |
|---|---|---|
| `player_died` | `fun()` | the player died |
| `region_changed` | `fun(new, old)` | the player moved to another play region (ids) |
| `character_killed` | `fun(character)` | a non-player character's HP reached 0 |
| `character_spawned` | `fun(character)` | a non-player character appeared (created, or the area's enemies coming back when the player rests); not those already there when the player arrives |
| `item_gained` / `item_lost` | `fun(item, count, name)` | the player's inventory changed |
| `effect_applied` / `effect_expired` | `fun(id)` | an SpEffect started or ended on the player |
| `level_up` | `fun(level, old_level, raised, souls)` | the soul level rose (the game's level-up screen, or a script): `raised` maps each attribute that rose to its new value (`{ vigor = 13 }`), `souls` is how many souls went with it |
| `equipment_changed` | `fun(slot, item, old)` | what an equipment slot holds changed (a [`kindle.slot`](reference/types.md#kindleslot); `item` or `old` is nil for an empty slot). Checked every few frames |
| `spells_changed` | `fun(spells, old)` | the attuned spells changed: both lists of goods items, in slot order |
| `souls_changed` | `fun(souls, old)` | the souls held changed (a kill, a purchase, a level, dying) |
| `bonfire_lit` | `fun(bonfire)` | a bonfire was lit for the first time (a [`kindle.bonfire`](reference/types.md#kindlebonfire)). Checked twice a second |
| `menu_opened` / `menu_closed` | `fun(kind)` | one of the game's menus opened or closed: `"esc"` (the Esc menu) or `"menu"` (any other: a bonfire's, a talk's, a shop, level up...) |
| `ember_changed` | `fun(embered)` | the player was embered (`true`) or lost it (`false`) |
| `key_pressed` | `fun(key)` | the player pressed a key, a controller button or a mouse button: its name, as [`kindle.input.down`](reference/input.md) takes (`"f5"`, `"pad_y"`, `"mouse_middle"`). Not while a screen or window of Kindle's has the input. For a mod's hotkey |
| `bonfire_rested` | `fun(bonfire)` | the player sat down at a bonfire (`bonfire` is a [`kindle.bonfire`](reference/types.md#kindlebonfire), or nil) |
| `banner` | `fun(name, type)` | the game showed one of its banners, e.g. `"you_died"` (see [`kindle.banner`](reference/types.md#kindlebanner)) |
| `boss_bar` | `fun(shown, name, entity)` | the game's event scripts showed a boss's health bar (`shown` true: a boss fight began) or hid it (the boss died, or the fight reset when the player died): the name the bar shows (nil if none) and the boss's entity id. Bars a mod shows with `kindle.events.run` aren't reported |
| `shop_buy` | `fun(item, count, row)` | the player bought `count` of `item` in a shop (ShopLineupParam `row`); it reports the purchase, it can't undo it |
| `travel` | `fun(map, entity, how)` | the game began sending the player somewhere, before the warp and any loading screen: `how` `"bonfire"` (a bonfire's travel, the menu's or `kindle.bonfire.warp`; `entity` the bonfire's) or `"script"` (an event script's warp: a boss arena's way out, the Path of the Dragon; `entity` the player start it goes to); `map` the area, e.g. `"m40_00_00_00"`. It reports the travel, it can't stop it |
| `boss_defeated` | `fun(kind)` | a boss fell: `"heir_of_fire"` or `"lord_of_cinder"` (the game's own HEIR OF FIRE DESTROYED / LORD OF CINDER FALLEN banners) |

Event flags have their own watchers: `kindle.flags.on_set(id, fn)` and
`kindle.flags.on_clear(id, fn)`.

### Hooks

Hooks run while the game is doing the thing, so a handler can change the outcome. They get
one table, `e`, describing it:

| Event | `e` | The handler can |
|---|---|---|
| `damage` | `target`, `attacker`, `attack_power`, `attack_id` | assign `e.attack_power` to change the hit; return `false` to cancel it |
| `animation` | `character`, `name`, `id` | assign `e.name` or `e.id` to play another animation; return `false` to cancel it |
| `effect` | `target`, `source`, `id` | assign `e.id` to apply another SpEffect; return `false` to block it |
| `item_used` | `player`, `item`, `name` | return `false` to keep the item (its effect still happens) |
| `item_lot` | `lot` | assign `e.lot` to roll another item lot, return `false` for nothing: map pickups (an area's all at once as it loads, so also during loading screens), enemy drops (as the enemy dies), event and boss rewards. A cancelled pickup still shows but gives nothing. The game also gives the rows right after the lot (lot+1, lot+2 ... up to the first missing row; a boss reward and its soul are two such rows) in the same roll: the event fires once, for the first. Lots mods award (`events.award_item_lot`) pass through them too, just before they're given |

```lua
-- Halve all damage the player takes.
kindle.on("damage", function(e)
  if e.target and e.target.is_player then
    e.attack_power = e.attack_power / 2
  end
end)
```

Hooks fire for **every** character, so check who's involved (`is_player`, `kind`, `name`,
`entity_id`). Some fire very often: `animation` and `effect` many times a frame. Keep these
handlers short and return early when the event isn't one you care about.

The game runs hooks on its own worker threads. Kindle runs mods one at a time; if another
mod is busy when a hook fires, Kindle waits a few milliseconds and then lets the game carry
on without your handler. That's rare, but a mod shouldn't rely on seeing every single
animation or effect.

For anything slow, or anything that calls back into the game heavily, let the hook record
what happened and do the work later, from a timer:

```lua
kindle.on("animation", function(e)
  if e.character.is_player and e.name == "W_Rolling" then
    kindle.after(0, function() kindle.player.add_souls(10) end) -- runs on the next frame
  end
end)
```

### Behaviour scripts

The `hks` module hooks the characters' behaviour scripts (the HavokScript that decides what
a character does from moment to moment): `kindle.hks.on_env` changes the answers those
scripts get from the game, and `kindle.hks.on_act` watches or blocks what they do. Edited
scripts can call mods with `kindle(name, ...)`, handled by `kindle.hks.on_call`. See the
[hks reference](reference/hks.md).

## Characters in events

Characters arrive as tables ([`kindle.character`](reference/types.md#kindlecharacter)): a
snapshot of `name`, `hp`, position, `kind` and so on at that moment. To act on the
character later, pass the table (or its `handle`) to a `kindle.world` function, which looks
the character up again:

```lua
kindle.on("character_killed", function(c)
  kindle.log.info("%s died at %.0f m", c.name or c.model, c.distance or 0)
end)
```

## Timers

```lua
kindle.after(5, function() kindle.ui.toast("Five seconds later") end)
local h = kindle.every(60, function() kindle.ui.toast("A minute passed") end)
```

Times are real seconds, checked once per frame. `kindle.every` keeps running until you
cancel it or the mod unloads, so start repeating timers once (or cancel the old one first),
not every time `player_loaded` fires:

```lua
local timer
kindle.on("player_loaded", function()
  if timer then kindle.cancel(timer) end
  timer = kindle.every(60, tick)
end)
```

## Errors

An error in any handler stops the mod: Kindle logs it (with file and line), runs the mod's
`unload` handlers, reverts its param and text changes, and marks it failed in the overlay.
Save a file to start it again. For calls that may fail for reasons outside your control (an
item name that isn't in the player's language, a character that's gone), use `pcall`:

```lua
local ok, err = pcall(kindle.items.give, "Uchigatana")
if not ok then kindle.log.warn("couldn't give it: %s", err) end
```

Each handler also has a time limit (200 ms by default); one that runs longer is stopped as an
error, so a mistake can't freeze the game.
