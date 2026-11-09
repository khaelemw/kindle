<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.flags

> Needs the `"flags"` permission in `mod.toml`.

Read, set and watch the game's event flags (bosses killed, doors opened, NPC progress).

## Functions

- [`get`](#get): Read an event flag.
- [`set`](#set): Set an event flag (default true).
- [`set_in_use`](#set_in_use): The set of flags in use: "own", or "session" (a player in a co-op session).
- [`block`](#block): A set's block of 1000 flags from `first` (a multiple of 1000) as 32 words, the flags' bits from the highest bit of the first word; nil where the set has no such block now.
- [`set_block`](#set_block): Writes a set's block of 1000 flags from `first` (32 words, as flags.block gives them).
- [`on_set`](#on_set): Call `fn(id)` when the flag turns on.
- [`on_clear`](#on_clear): Call `fn(id)` when the flag turns off.

### boss

`kindle.flags.boss`: `kindle.boss_flags`

Boss defeat flags by name (set when the boss dies), e.g. `kindle.flags.on_set(kindle.flags.boss.iudex_gundyr, fn)`. Taken from the game's own event scripts.

| Field | Type | Description |
|---|---|---|
| `iudex_gundyr` | `integer` | 14000800: Iudex Gundyr |
| `vordt_of_the_boreal_valley` | `integer` | 13000800: Vordt of the Boreal Valley |
| `curse_rotted_greatwood` | `integer` | 13100800: Curse-rotted Greatwood |
| `crystal_sage` | `integer` | 13300850: Crystal Sage |
| `abyss_watchers` | `integer` | 13300800: Abyss Watchers |
| `deacons_of_the_deep` | `integer` | 13500800: Deacons of the Deep |
| `high_lord_wolnir` | `integer` | 13800800: High Lord Wolnir |
| `old_demon_king` | `integer` | 13800830: Old Demon King |
| `pontiff_sulyvahn` | `integer` | 13700850: Pontiff Sulyvahn |
| `aldrich_devourer_of_gods` | `integer` | 13700800: Aldrich, Devourer of Gods |
| `yhorm_the_giant` | `integer` | 13900800: Yhorm the Giant |
| `dancer_of_the_boreal_valley` | `integer` | 13000890: Dancer of the Boreal Valley |
| `oceiros_the_consumed_king` | `integer` | 13000830: Oceiros, the Consumed King |
| `dragonslayer_armour` | `integer` | 13010800: Dragonslayer Armour |
| `twin_princes` | `integer` | 13410830: Lorian, Elder Prince and Lothric, Younger Prince |
| `champion_gundyr` | `integer` | 14000830: Champion Gundyr |
| `ancient_wyvern` | `integer` | 13200800: Ancient Wyvern |
| `nameless_king` | `integer` | 13200850: Nameless King |
| `soul_of_cinder` | `integer` | 14100800: Soul of Cinder |
| `sister_friede` | `integer` | 14500800: Sister Friede (Ashes of Ariandel) |
| `champions_gravetender` | `integer` | 14500860: Champion's Gravetender (Ashes of Ariandel) |
| `demon_prince` | `integer` | 15000800: Demon Prince (The Ringed City) |
| `halflight` | `integer` | 15100800: Halflight, Spear of the Church (The Ringed City) |
| `darkeater_midir` | `integer` | 15100850: Darkeater Midir (The Ringed City) |
| `slave_knight_gael` | `integer` | 15110800: Slave Knight Gael (The Ringed City) |

### bonfire

`kindle.flags.bonfire`: `kindle.bonfire_flags`

Each bonfire's lit flag by name, e.g. `kindle.flags.bonfire.firelink_shrine`. Read from the game's bonfire list; the names are in the game stubs Kindle writes (kindle-game.lua).

| Field | Type | Description |
|---|---|---|

### get

```lua
kindle.flags.get(id: integer, set: "own"|"session"?) -> boolean
```

Read an event flag. set: "own" (the character's own flags) or "session" (the copy a player's game uses in a co-op session, the host's world's); nil for the set in use. Another set than the one in use is an error when it has no table for that flag now.

**Parameters**

- `id` `integer`
- `set` `"own"` | `"session"?`

**Returns**

- `boolean`

### set

```lua
kindle.flags.set(id: integer, state: boolean?, set: "own"|"session"?) -> boolean
```

Set an event flag (default true). Returns whether the flag exists. set as for get.

**Parameters**

- `id` `integer`
- `state` `boolean?`
- `set` `"own"` | `"session"?`

**Returns**

- `boolean`

### set_in_use

```lua
kindle.flags.set_in_use() -> "own"|"session"
```

The set of flags in use: "own", or "session" (a player in a co-op session).

**Returns**

- `"own"` | `"session"`

### block

```lua
kindle.flags.block(first: integer, set: "own"|"session") -> integer[]?
```

A set's block of 1000 flags from `first` (a multiple of 1000) as 32 words, the flags' bits from the highest bit of the first word; nil where the set has no such block now. Its values only.

**Parameters**

- `first` `integer`
- `set` `"own"` | `"session"`

**Returns**

- `integer[]?`

### set_block

```lua
kindle.flags.set_block(first: integer, words: integer[], set: "own"|"session")
```

Writes a set's block of 1000 flags from `first` (32 words, as flags.block gives them). On the game's thread (in a handler or the frame), as the game's own flag writes.

**Parameters**

- `first` `integer`
- `words` `integer[]`
- `set` `"own"` | `"session"`

### on_set

```lua
kindle.flags.on_set(id: integer, fn: fun(id: integer)) -> integer
```

Call `fn(id)` when the flag turns on. Edge-triggered, only while a character is loaded.

**Parameters**

- `id` `integer`
- `fn` `fun(id: integer)`

**Returns**

- `handle` `integer`

### on_clear

```lua
kindle.flags.on_clear(id: integer, fn: fun(id: integer)) -> integer
```

Call `fn(id)` when the flag turns off.

**Parameters**

- `id` `integer`
- `fn` `fun(id: integer)`

**Returns**

- `handle` `integer`

