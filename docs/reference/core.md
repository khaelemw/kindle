<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle

Events, timers and this mod's own info. Always available.

## Fields

| Field | Type | Description |
|---|---|---|
| `version` | `string` | Kindle runtime version |
| `api` | `integer` | the Lua API version (mods set `api` in mod.toml to the one they were written for) |
| `mod` | `{id: string, name: string, version: string}` | This mod's manifest info |

## Functions

- [`on`](#on): Subscribe to an event.
- [`inspect`](#inspect): A readable string for any value: tables show their contents, lists as { 1, 2, 3 } and others as { key = value } with sorted keys, down to `depth` levels (default 3).
- [`after`](#after): Run `fn` once after `seconds`.
- [`every`](#every): Run `fn` every `seconds`.
- [`cancel`](#cancel): Cancel an event handler, timer or flag watcher.
- [`time`](#time): Seconds since Kindle started.
- [`clock`](#clock): Seconds since 1970-01-01 00:00 UTC, from the computer's clock (kindle.time is since Kindle started).
- [`date`](#date): The calendar date and time of `seconds` (a kindle.clock() value; default now), in the player's local time, or UTC with `utc` true.
- [`read_data`](#read_data): A file of the mod's own folder: its bytes as a Lua string (text for a text file).
- [`dlc_installed`](#dlc_installed): Whether a DLC is installed: 1 Ashes of Ariandel, 2 The Ringed City (nil before the game knows).
- [`frame`](#frame): Frames since Kindle started.
- [`export`](#export): Offer functions to other mods: kindle.export{ greet = function(name) ...
- [`send`](#send): Send a message to every other running mod, delivered on the next frame to their kindle.on_message(name, fn) handlers.
- [`on_message`](#on_message): Handle messages other mods send with kindle.send(name, data).
- [`keep`](#keep): State that survives the mod being reloaded (saving its files reloads it), for this session: the value kept under `key` when the mod last unloaded, else `default`.

### on_unload

`kindle.on_unload`: `fun()?`

Optional: assign a function here to run before the mod is reloaded/unloaded.

### on

```lua
kindle.on(event: kindle.event, fn: function) -> integer
```

Subscribe to an event. `frame` handlers receive the seconds since the last frame. `player_loaded` fires when a character is in the world: once the loading screen is over (and immediately on (re)load if one already is). `unload` fires before the mod is reloaded or removed.

Game events (polled each frame, only while something is subscribed):
- `player_died()`
- `region_changed(new, old)` play region ids
- `character_killed(character)` a non-player character's HP reached 0
- `character_spawned(character)` a non-player character appeared (one created, or the area's enemies coming back when the player rests); not the characters already there when the player arrives in an area. Checked every few frames.
- `item_gained(item, count, name)` / `item_lost(item, count, name)` inventory changes
- `effect_applied(id)` / `effect_expired(id)` SpEffects on the player
- `level_up(level, old_level, raised, souls)` the soul level rose (at the game's level-up screen, or by a script): `raised` maps each attribute that rose to its new value (`{ vigor = 13 }`), `souls` is how many souls went with it (the cost)
- `equipment_changed(slot, item, old)` what a slot holds changed (kindle.slot; item or old nil for an empty slot): equipping, unequipping, a weapon swapped by a script. Checked every few frames.
- `spells_changed(spells, old)` the attuned spells changed (lists of goods items, slot order)
- `souls_changed(souls, old)` the souls held changed (a kill, a purchase, a level, dying)
- `bonfire_lit(bonfire)` a bonfire was lit for the first time (a kindle.bonfire). Checked twice a second.
- `menu_opened(kind)` / `menu_closed(kind)` one of the game's menus opened or closed: kind "esc" (the Esc menu) or "menu" (any other: a bonfire's, a talk's, a shop, level up...)
- `ember_changed(embered)` the player was embered (true) or lost it (false)
- `key_pressed(key)` the player pressed a key, controller button or mouse button (kindle.key; not while a screen or window of Kindle's is open)
- `bonfire_rested(bonfire)` the player sat down at a bonfire; `bonfire` is the kindle.bonfire rested at (nil if unknown)
- `shop_buy(item, count, row)` the player bought `count` of `item` in a shop (ShopLineupParam `row`); the souls are already being taken: it reports, it can't undo
- `banner(name, type)` the game showed one of its banners (see kindle.banner)
- `boss_defeated(kind)` a boss fell: "heir_of_fire" (HEIR OF FIRE DESTROYED) or "lord_of_cinder" (LORD OF CINDER FALLEN), from the game's own banner
- `boss_bar(shown, name, entity)` the game's event scripts showed (a boss fight began) or hid (it ended: the boss died, or the player did and the fight reset) a boss's health bar: the name it shows (nil if it has none) and the boss's entity id

Hooked events (run as the game does it, so handlers can change the outcome):
- `damage(e)` every hit on any character. `e.target`, `e.attacker` (characters), `e.attack_power` (before the target's defence; assign to change the hit), `e.attack_id` (AtkParam row). Return false to cancel the hit.
- `animation(e)` every animation event any character starts: `e.character`, `e.name` (e.g. "W_Rolling"), `e.id`. Assign `e.name` or `e.id` to play something else instead; return false to cancel it. Fires often: keep handlers quick.
- `effect(e)` an SpEffect about to be applied to any character: `e.target`, `e.source` (characters), `e.id`. Assign `e.id` to apply a different effect; return false to block it. Fires often (many effects are re-applied every frame): keep handlers quick.
- `item_used(e)` a player used a goods item (as it takes effect): `e.player`, `e.item`, `e.name`. Return false to keep the item (it still takes effect).
- `item_lot(e)` an item lot about to be rolled: map pickups (all of an area's, as it loads, so also during loading screens), enemy drops (as the enemy dies), and event and boss rewards. `e.lot` (ItemLotParam row); assign it to roll another lot, return false for nothing (a cancelled pickup still shows but gives nothing). The rolled lot's own pickup flag is the one the game sets. The game also gives the rows right after the lot (lot+1, lot+2 ... up to the first missing row; a boss reward and its soul are two such rows), in the same roll: this fires once, for the first. Lots mods award (events.award_item_lot) pass through them too, just before they're given.

**Parameters**

- `event` [`kindle.event`](types.md#kindleevent)
- `fn` `function`

**Returns**

- `handle` `integer`

**Handlers by event**

| Event | Handler |
|---|---|
| `frame` | `fun(dt: number)` |
| `player_loaded, player_unloaded, unload, player_died` | `fun()` |
| `setting_changed` | `fun(name: string, value: boolean\|number\|string)` |
| `map_loading` | `fun(map: string, file: "map"\|"events")` |
| `region_changed` | `fun(new: integer, old: integer)` |
| `character_killed, character_spawned` | `fun(character: kindle.character)` |
| `item_gained, item_lost` | `fun(item: kindle.item, count: integer, name: string?)` |
| `effect_applied, effect_expired` | `fun(id: integer)` |
| `level_up` | `fun(level: integer, old_level: integer, raised: table<string, integer>, souls: integer)` |
| `equipment_changed` | `fun(slot: kindle.slot, item: kindle.item?, old: kindle.item?)` |
| `spells_changed` | `fun(spells: kindle.item[], old: kindle.item[])` |
| `souls_changed` | `fun(souls: integer, old: integer)` |
| `bonfire_lit` | `fun(bonfire: kindle.bonfire)` |
| `menu_opened, menu_closed` | `fun(kind: "esc"\|"menu")` |
| `ember_changed` | `fun(embered: boolean)` |
| `key_pressed` | `fun(key: kindle.key)` |
| `bonfire_rested` | `fun(bonfire: kindle.bonfire?)` |
| `banner` | `fun(name: kindle.banner?, type: integer)` |
| `boss_defeated` | `fun(kind: "heir_of_fire"\|"lord_of_cinder")` |
| `boss_bar` | `fun(shown: boolean, name: string?, entity: integer)` |
| `shop_buy` | `fun(item: kindle.item, count: integer, row: integer)` |
| `travel` | `fun(map: string, entity: integer, how: "bonfire"\|"script")` |
| `damage` | `fun(e: kindle.damage_event): boolean?` |
| `animation` | `fun(e: kindle.animation_event): boolean?` |
| `effect` | `fun(e: kindle.effect_event): boolean?` |
| `item_used` | `fun(e: kindle.item_used_event): boolean?` |
| `item_lot` | `fun(e: kindle.item_lot_event): boolean?` |

### inspect

```lua
kindle.inspect(value: any, depth: integer?) -> string
```

A readable string for any value: tables show their contents, lists as { 1, 2, 3 } and others as { key = value } with sorted keys, down to `depth` levels (default 3). print and kindle.log use it for tables too.

**Parameters**

- `value` `any`
- `depth` `integer?`

**Returns**

- `string`

### after

```lua
kindle.after(seconds: number, fn: fun()) -> integer
```

Run `fn` once after `seconds`.

**Parameters**

- `seconds` `number`
- `fn` `fun()`

**Returns**

- `handle` `integer`

### every

```lua
kindle.every(seconds: number, fn: fun()) -> integer
```

Run `fn` every `seconds`.

**Parameters**

- `seconds` `number`
- `fn` `fun()`

**Returns**

- `handle` `integer`

### cancel

```lua
kindle.cancel(handle: integer) -> boolean
```

Cancel an event handler, timer or flag watcher.

**Parameters**

- `handle` `integer`

**Returns**

- `removed` `boolean`

### time

```lua
kindle.time() -> number
```

Seconds since Kindle started.

**Returns**

- `number`

### clock

```lua
kindle.clock() -> number
```

Seconds since 1970-01-01 00:00 UTC, from the computer's clock (kindle.time is since Kindle started). For daily seeds, and dates and times that last across sessions.

**Returns**

- `number`

### date

```lua
kindle.date(seconds: number?, utc: boolean?) -> {year: integer, month: integer, day: integer, hour: integer, minute: integer, second: integer, weekday: integer, yearday: integer}
```

The calendar date and time of `seconds` (a kindle.clock() value; default now), in the player's local time, or UTC with `utc` true. `local d = kindle.date(); local seed = d.year * 10000 + d.month * 100 + d.day`

**Parameters**

- `seconds` `number?`
- `utc` `boolean?`

**Returns**

- `weekday` `{year: integer, month: integer, day: integer, hour: integer, minute: integer, second: integer, weekday: integer, yearday: integer}`: 1 Monday .. 7 Sunday

### read_data

```lua
kindle.read_data(path: string) -> string
```

A file of the mod's own folder: its bytes as a Lua string (text for a text file). `path` is relative to the folder, with / between folders; up to 16 MB. For data a mod ships: `local drops = kindle.json.decode(kindle.read_data("data/drops.json"))`

**Parameters**

- `path` `string`

**Returns**

- `string`

### dlc_installed

```lua
kindle.dlc_installed(dlc: integer) -> boolean?
```

Whether a DLC is installed: 1 Ashes of Ariandel, 2 The Ringed City (nil before the game knows).

**Parameters**

- `dlc` `integer`

**Returns**

- `boolean?`

### frame

```lua
kindle.frame() -> integer
```

Frames since Kindle started.

**Returns**

- `integer`

### export

```lua
kindle.export(functions: table<string, function>)
```

Offer functions to other mods: kindle.export{ greet = function(name) ... end }. Calls from other mods run in this mod; arguments and results are copied between the mods (plain data: nil, booleans, numbers, strings, tables). Calling it again replaces the exports.

**Parameters**

- `functions` `table<string, function>`

### send

```lua
kindle.send(name: string, data: kindle.storable?)
```

Send a message to every other running mod, delivered on the next frame to their kindle.on_message(name, fn) handlers. `data` is plain data (as for kindle.store).

**Parameters**

- `name` `string`
- `data` [`kindle.storable?`](types.md#kindlestorable)

### on_message

```lua
kindle.on_message(name: string, fn: fun(data: any, from: string)) -> integer
```

Handle messages other mods send with kindle.send(name, data).

**Parameters**

- `name` `string`
- `fn` `fun(data: any, from: string)`: from is the sending mod's id

**Returns**

- `handle` `integer`

### keep

```lua
kindle.keep(key: string, default: T) -> T
```

State that survives the mod being reloaded (saving its files reloads it), for this session: the value kept under `key` when the mod last unloaded, else `default`. Kindle keeps the value as the mod unloads, so change a kept table in place: `local state = kindle.keep("state", { kills = 0 })`, then `state.kills = state.kills + 1`. Plain data only (as for kindle.store). For values that last between sessions use kindle.store.

**Parameters**

- `key` `string`
- `default` `T`

**Returns**

- `T`

