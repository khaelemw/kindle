<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.player

> Needs the `"player"` permission in `mod.toml`.

The player: HP/FP/stamina, stats, effects, equipment, position, souls, animations.

## Functions

- [`loaded`](#loaded): Whether a character is in the world (the loading screen is over).
- [`name`](#name): The character's name.
- [`stats`](#stats): The character's nine attributes.
- [`hp`](#hp): Current HP.
- [`max_hp`](#max_hp): Maximum HP.
- [`set_hp`](#set_hp): Set HP.
- [`fp`](#fp): Current FP.
- [`max_fp`](#max_fp): Maximum FP.
- [`set_fp`](#set_fp): Set FP.
- [`stamina`](#stamina): Current stamina.
- [`max_stamina`](#max_stamina): Maximum stamina.
- [`set_stamina`](#set_stamina): Set stamina.
- [`heal`](#heal): Restore HP and FP to max.
- [`apply_effect`](#apply_effect): Apply an SpEffect (SpEffectParam row id) to the player: buffs, debuffs, states.
- [`remove_effect`](#remove_effect): End an active SpEffect (the game removes it on its next update).
- [`effects`](#effects): The player's active SpEffects.
- [`has_effect`](#has_effect): Whether an SpEffect is currently active on the player.
- [`position`](#position): The player's position (y is up).
- [`camera`](#camera): The game camera: its position (`x`, `y`, `z`), where it looks (`dx`, `dy`, `dz`, unit length), `yaw` (radians, like `world.facing`: a character with this facing looks the way the camera does) and `fov` (radians).
- [`lock_target`](#lock_target): The character the player is locked on to, as world.characters gives it, or nil when the player isn't locked on.
- [`set_lock_target`](#set_lock_target): Locks the player on to a character, as the lock-on button and a target switch do (the camera turns to it), or releases the lock (nil).
- [`set_camera`](#set_camera): Hold the game's camera: at x, y, z, looking along dx, dy, dz (any length) or at the point `look_at`, with `fov` (radians; default the game's).
- [`teleport`](#teleport): Move the player.
- [`facing`](#facing): Facing angle in radians.
- [`set_facing`](#set_facing): Turn the player to face an angle (radians).
- [`invulnerable`](#invulnerable): Whether the player is invulnerable.
- [`set_invulnerable`](#set_invulnerable): Make the player invulnerable (or not).
- [`set_gravity`](#set_gravity): false disables gravity (the player floats).
- [`play_animation`](#play_animation): Play an animation event by name (case-insensitive, e.g.
- [`animations`](#animations): Every animation event the player's behaviour accepts.
- [`souls`](#souls): Souls carried.
- [`set_souls`](#set_souls): Clamped to 0..999,999,999.
- [`add_souls`](#add_souls): Add souls; a negative number removes them (down to 0).
- [`soul_level`](#soul_level): Soul level.
- [`total_souls`](#total_souls): Souls gained over the whole playthrough.
- [`embered`](#embered): Whether the player is embered.
- [`set_embered`](#set_embered): true goes through the game's own transition (SpEffect 3290); false clears the state (experimental: the HUD may not update until the next load).
- [`estus`](#estus): Estus / Ashen Estus charges and allotments.
- [`set_estus`](#set_estus): Set charges; nil leaves one unchanged.
- [`refill_estus`](#refill_estus): Refill both estus flasks to their allotment.
- [`set_stat`](#set_stat): Set an attribute (1-99).
- [`set_stats`](#set_stats): Set several attributes: player.set_stats({vigor = 20, strength = 18})
- [`set_soul_level`](#set_soul_level): Set the soul level number.
- [`equipment`](#equipment): Equipped items by slot (item ids; empty slots are absent).
- [`equip`](#equip): Equip an item from the inventory, through the game's own equip path.
- [`unequip`](#unequip): Empty a slot (weapons fall back to Fists, armour to the bare piece).
- [`spells`](#spells): Attuned spells (item ids), in slot order.
- [`ng_cycle`](#ng_cycle): 0 on the first playthrough, 1 in NG+, ...
- [`deaths`](#deaths): How many times the character has died.
- [`play_time`](#play_time): Play time in seconds.
- [`status`](#status): Status build-up; the effect triggers when value reaches max.
- [`poise`](#poise): Current and max poise (most characters only have poise during certain attacks).
- [`animation`](#animation): Name of the current animation state, e.g.
- [`animation_speed`](#animation_speed): Animation speed multiplier (1 is normal).
- [`set_animation_speed`](#set_animation_speed): 1 is normal speed; 0-10.
- [`set_no_death`](#set_no_death): true: HP can't drop below 1.
- [`set_no_damage`](#set_no_damage): true: hits do no damage.
- [`set_infinite_stamina`](#set_infinite_stamina): true: stamina doesn't drain.
- [`set_infinite_fp`](#set_infinite_fp): true: FP doesn't drain.
- [`set_infinite_items`](#set_infinite_items): true: using consumables doesn't use them up.
- [`warp`](#warp): Warp to a bonfire like the bonfire menu does.
- [`last_bonfire`](#last_bonfire): Entity id of the bonfire last rested at.

### loaded

```lua
kindle.player.loaded() -> boolean
```

Whether a character is in the world (the loading screen is over). Most player functions need one.

**Returns**

- `boolean`

### name

```lua
kindle.player.name() -> string
```

The character's name.

**Returns**

- `string`

### stats

```lua
kindle.player.stats() -> {vigor: integer, attunement: integer, endurance: integer, vitality: integer, strength: integer, dexterity: integer, intelligence: integer, faith: integer, luck: integer}
```

The character's nine attributes.

**Returns**

- `{vigor: integer, attunement: integer, endurance: integer, vitality: integer, strength: integer, dexterity: integer, intelligence: integer, faith: integer, luck: integer}`

### hp

```lua
kindle.player.hp() -> integer
```

Current HP.

**Returns**

- `integer`

### max_hp

```lua
kindle.player.max_hp() -> integer
```

Maximum HP.

**Returns**

- `integer`

### set_hp

```lua
kindle.player.set_hp(value: integer)
```

Set HP.

**Parameters**

- `value` `integer`: clamped to 0..max_hp

### fp

```lua
kindle.player.fp() -> integer
```

Current FP.

**Returns**

- `integer`

### max_fp

```lua
kindle.player.max_fp() -> integer
```

Maximum FP.

**Returns**

- `integer`

### set_fp

```lua
kindle.player.set_fp(value: integer)
```

Set FP.

**Parameters**

- `value` `integer`

### stamina

```lua
kindle.player.stamina() -> integer
```

Current stamina.

**Returns**

- `integer`

### max_stamina

```lua
kindle.player.max_stamina() -> integer
```

Maximum stamina.

**Returns**

- `integer`

### set_stamina

```lua
kindle.player.set_stamina(value: integer)
```

Set stamina.

**Parameters**

- `value` `integer`

### heal

```lua
kindle.player.heal()
```

Restore HP and FP to max.

### apply_effect

```lua
kindle.player.apply_effect(id: integer)
```

Apply an SpEffect (SpEffectParam row id) to the player: buffs, debuffs, states. Errors if the id doesn't exist. Tip: an item's effect is often its EquipParamGoods refId_default (when refCategory is 2).

**Parameters**

- `id` `integer`

### remove_effect

```lua
kindle.player.remove_effect(id: integer) -> integer
```

End an active SpEffect (the game removes it on its next update).

**Parameters**

- `id` `integer`

**Returns**

- `ended` `integer`: number of instances ended

### effects

```lua
kindle.player.effects() -> {id: integer, time_left: number, duration: number}[]
```

The player's active SpEffects. duration -1 = permanent.

**Returns**

- `{id: integer, time_left: number, duration: number}[]`

### has_effect

```lua
kindle.player.has_effect(id: integer) -> boolean
```

Whether an SpEffect is currently active on the player.

**Parameters**

- `id` `integer`

**Returns**

- `boolean`

### position

```lua
kindle.player.position() -> number, number, number
```

The player's position (y is up).

**Returns**

- `x` `number`
- `y` `number`
- `z` `number`

### camera

```lua
kindle.player.camera() -> {x: number, y: number, z: number, dx: number, dy: number, dz: number, yaw: number, fov: number}
```

The game camera: its position (`x`, `y`, `z`), where it looks (`dx`, `dy`, `dz`, unit length), `yaw` (radians, like `world.facing`: a character with this facing looks the way the camera does) and `fov` (radians). Out of view, 3 m behind the camera: `local c = kindle.player.camera(); local _, y = kindle.player.position()` `kindle.world.spawn{npc_param = 110000, x = c.x - c.dx * 3, y = y, z = c.z - c.dz * 3}`

**Returns**

- `{x: number, y: number, z: number, dx: number, dy: number, dz: number, yaw: number, fov: number}`

### lock_target

```lua
kindle.player.lock_target() -> kindle.character?
```

The character the player is locked on to, as world.characters gives it, or nil when the player isn't locked on. Example, a mod's own target marker: each frame, `local t = kindle.player.lock_target()` and draw at t.x, t.y, t.z.

**Returns**

- [`kindle.character?`](types.md#kindlecharacter)

### set_lock_target

```lua
kindle.player.set_lock_target(target: kindle.character?) -> boolean
```

Locks the player on to a character, as the lock-on button and a target switch do (the camera turns to it), or releases the lock (nil). Only a character the player could lock on to now (in range and in view) can be chosen: false otherwise, and the lock stays as it was. Example, lock on to the nearest character: `kindle.player.set_lock_target(kindle.world.characters{radius = 20}[1])`.

**Parameters**

- `target` [`kindle.character?`](types.md#kindlecharacter)

**Returns**

- `boolean`

### set_camera

```lua
kindle.player.set_camera(cam: {x: number, y: number, z: number, dx?: number, dy?: number, dz?: number, look_at?: {x: number, y: number, z: number}, fov?: number}?) -> boolean
```

Hold the game's camera: at x, y, z, looking along dx, dy, dz (any length) or at the point `look_at`, with `fov` (radians; default the game's). It stays there, every frame, until the mod calls it again (a camera that moves) or with nil, which gives the game its camera back (as the mod unloading does). The character still moves with the player's input (kindle.input.block_actions stops that). One mod holds the camera at a time. Returns whether it held it (nil: whether this mod had it). `local c = kindle.player.camera()` `kindle.player.set_camera{x = c.x, y = c.y + 10, z = c.z, look_at = {kindle.player.position()}}`

**Parameters**

- `cam` `{x: number, y: number, z: number, dx?: number, dy?: number, dz?: number, look_at?: {x: number, y: number, z: number}, fov?: number}?`

**Returns**

- `boolean`

### teleport

```lua
kindle.player.teleport(x: number, y: number, z: number)
```

Move the player.

**Parameters**

- `x` `number`
- `y` `number`: up
- `z` `number`

### facing

```lua
kindle.player.facing() -> number?
```

Facing angle in radians.

**Returns**

- `number?`

### set_facing

```lua
kindle.player.set_facing(radians: number)
```

Turn the player to face an angle (radians).

**Parameters**

- `radians` `number`

### invulnerable

```lua
kindle.player.invulnerable() -> boolean?
```

Whether the player is invulnerable.

**Returns**

- `boolean?`

### set_invulnerable

```lua
kindle.player.set_invulnerable(on: boolean?)
```

Make the player invulnerable (or not).

**Parameters**

- `on` `boolean?`: default true

### set_gravity

```lua
kindle.player.set_gravity(on: boolean)
```

false disables gravity (the player floats).

**Parameters**

- `on` `boolean`

### play_animation

```lua
kindle.player.play_animation(anim: kindle.animation_name|string|integer)
```

Play an animation event by name (case-insensitive, e.g. "W_ItemDrink") or id.

**Parameters**

- `anim` [`kindle.animation_name`](types.md#kindleanimation_name) | `string` | `integer`

### animations

```lua
kindle.player.animations() -> table<integer, string>
```

Every animation event the player's behaviour accepts.

**Returns**

- `table<integer, string>`

### souls

```lua
kindle.player.souls() -> integer
```

Souls carried.

**Returns**

- `integer`

### set_souls

```lua
kindle.player.set_souls(souls: integer)
```

Clamped to 0..999,999,999.

**Parameters**

- `souls` `integer`

### add_souls

```lua
kindle.player.add_souls(n: integer)
```

Add souls; a negative number removes them (down to 0).

**Parameters**

- `n` `integer`

### soul_level

```lua
kindle.player.soul_level() -> integer
```

Soul level.

**Returns**

- `integer`

### total_souls

```lua
kindle.player.total_souls() -> integer
```

Souls gained over the whole playthrough.

**Returns**

- `integer`

### embered

```lua
kindle.player.embered() -> boolean
```

Whether the player is embered.

**Returns**

- `boolean`

### set_embered

```lua
kindle.player.set_embered(on: boolean)
```

true goes through the game's own transition (SpEffect 3290); false clears the state (experimental: the HUD may not update until the next load).

**Parameters**

- `on` `boolean`

### estus

```lua
kindle.player.estus() -> {hp: integer, hp_max: integer, fp: integer, fp_max: integer}
```

Estus / Ashen Estus charges and allotments.

**Returns**

- `{hp: integer, hp_max: integer, fp: integer, fp_max: integer}`

### set_estus

```lua
kindle.player.set_estus(hp: integer?, fp: integer?)
```

Set charges; nil leaves one unchanged. Clamped to the allotment.

**Parameters**

- `hp` `integer?`
- `fp` `integer?`

### refill_estus

```lua
kindle.player.refill_estus()
```

Refill both estus flasks to their allotment.

### set_stat

```lua
kindle.player.set_stat(name: kindle.attribute, value: integer, keep_level: boolean?)
```

Set an attribute (1-99). Derived stats (max HP/FP/stamina, equip load, ...) update by themselves. Soul level moves by the same amount unless keep_level.

**Parameters**

- `name` [`kindle.attribute`](types.md#kindleattribute)
- `value` `integer`
- `keep_level` `boolean?`

### set_stats

```lua
kindle.player.set_stats(stats: table<kindle.attribute, integer>, keep_level: boolean?)
```

Set several attributes: player.set_stats({vigor = 20, strength = 18})

**Parameters**

- `stats` `table<kindle.attribute, integer>`
- `keep_level` `boolean?`

### set_soul_level

```lua
kindle.player.set_soul_level(level: integer)
```

Set the soul level number. Attributes are unchanged (see set_stat).

**Parameters**

- `level` `integer`: 1-802

### equipment

```lua
kindle.player.equipment() -> table<kindle.slot, integer>
```

Equipped items by slot (item ids; empty slots are absent).

**Returns**

- `table<kindle.slot, integer>`

### equip

```lua
kindle.player.equip(slot: kindle.slot, item: integer|string)
```

Equip an item from the inventory, through the game's own equip path. The item must be in the inventory (items.give it first) and fit the slot.

**Parameters**

- `slot` [`kindle.slot`](types.md#kindleslot)
- `item` `integer` | `string`

### unequip

```lua
kindle.player.unequip(slot: kindle.slot)
```

Empty a slot (weapons fall back to Fists, armour to the bare piece).

**Parameters**

- `slot` [`kindle.slot`](types.md#kindleslot)

### spells

```lua
kindle.player.spells() -> integer[]
```

Attuned spells (item ids), in slot order.

**Returns**

- `integer[]`

### ng_cycle

```lua
kindle.player.ng_cycle() -> integer
```

0 on the first playthrough, 1 in NG+, ...

**Returns**

- `integer`

### deaths

```lua
kindle.player.deaths() -> integer
```

How many times the character has died.

**Returns**

- `integer`

### play_time

```lua
kindle.player.play_time() -> number
```

Play time in seconds.

**Returns**

- `number`

### status

```lua
kindle.player.status() -> kindle.status
```

Status build-up; the effect triggers when value reaches max.

**Returns**

- [`kindle.status`](types.md#kindlestatus)

### poise

```lua
kindle.player.poise() -> number, number
```

Current and max poise (most characters only have poise during certain attacks).

**Returns**

- `current` `number`
- `max` `number`

### animation

```lua
kindle.player.animation() -> string?
```

Name of the current animation state, e.g. "Idle".

**Returns**

- `string?`

### animation_speed

```lua
kindle.player.animation_speed() -> number?
```

Animation speed multiplier (1 is normal).

**Returns**

- `number?`

### set_animation_speed

```lua
kindle.player.set_animation_speed(speed: number)
```

1 is normal speed; 0-10.

**Parameters**

- `speed` `number`

### set_no_death

```lua
kindle.player.set_no_death(on: boolean?)
```

true: HP can't drop below 1.

**Parameters**

- `on` `boolean?`: default true

### set_no_damage

```lua
kindle.player.set_no_damage(on: boolean?)
```

true: hits do no damage.

**Parameters**

- `on` `boolean?`: default true

### set_infinite_stamina

```lua
kindle.player.set_infinite_stamina(on: boolean?)
```

true: stamina doesn't drain.

**Parameters**

- `on` `boolean?`: default true

### set_infinite_fp

```lua
kindle.player.set_infinite_fp(on: boolean?)
```

true: FP doesn't drain.

**Parameters**

- `on` `boolean?`: default true

### set_infinite_items

```lua
kindle.player.set_infinite_items(on: boolean?)
```

true: using consumables doesn't use them up.

**Parameters**

- `on` `boolean?`: default true

### warp

```lua
kindle.player.warp(bonfire: kindle.bonfire|kindle.bonfire_name|string|integer, force: boolean?)
```

Warp to a bonfire like the bonfire menu does. Only lit, warpable bonfires unless force.

**Parameters**

- `bonfire` [`kindle.bonfire`](types.md#kindlebonfire) | [`kindle.bonfire_name`](types.md#kindlebonfire_name) | `string` | `integer`: a bonfire table, its name, entity id or row id
- `force` `boolean?`

### last_bonfire

```lua
kindle.player.last_bonfire() -> integer?
```

Entity id of the bonfire last rested at.

**Returns**

- `integer?`

