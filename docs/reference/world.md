<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.world

> Needs the `"world"` permission in `mod.toml`.

Other characters: enemies, NPCs, players.

## Functions

- [`characters`](#characters): Loaded characters, nearest first.
- [`objects`](#objects): The maps' placed objects that are loaded, nearest first.
- [`destroy_object`](#destroy_object): Break an object as a hit does: its pieces fall and any treasure in it drops (the game's command 2005[01]; the game keeps it broken, as it keeps a pot the player broke).
- [`restore_object`](#restore_object): Put a broken object back whole (the game's command 2005[02]).
- [`set_object_active`](#set_object_active): Turn an object's action on or off: a door that can't be opened, a lever that can't be pulled (the game's command 2005[03]).
- [`players`](#players): The players' characters, alive or not: this game's first (is_player), then in a session the others' (other_player).
- [`character`](#character): One character by handle, entity id or table; nil if not loaded.
- [`spawn`](#spawn): Create a character (experimental): any enemy, from any area.
- [`load_ai`](#load_ai): Load another area's AI (experimental), so enemies from there act when spawned here: `area` is the map name: m30_00_00_00 High Wall, m30_01_00_00 Lothric Castle, m31_00_00_00 Undead Settlement, m32_00_00_00 Archdragon Peak, m33_00_00_00 Road of Sacrifices and Farron Keep, m34_01_00_00 Grand Archives, m35_00_00_00 Cathedral of the Deep, m37_00_00_00 Irithyll, m38_00_00_00 Catacombs and Smouldering Lake, m39_00_00_00 Irithyll Dungeon, m40_00_00_00 Cemetery of Ash and Firelink, m41_00_00_00 Kiln, m45_00_00_00 Ariandel, m50_00_00_00 Dreg Heap, m51_00_00_00 Ringed City.
- [`set_hp`](#set_hp): Set a character's HP.
- [`kill`](#kill): Kill a character.
- [`heal`](#heal): Restore a character's HP to its maximum.
- [`apply_effect`](#apply_effect): Apply an SpEffect to a character.
- [`remove_effect`](#remove_effect): End an active SpEffect on a character; returns how many instances ended.
- [`effects`](#effects): A character's active SpEffects (duration -1 = permanent).
- [`teleport`](#teleport): Move a character.
- [`facing`](#facing): A character's facing angle in radians.
- [`set_facing`](#set_facing): Turn a character to face an angle (radians).
- [`invulnerable`](#invulnerable): Whether a character is invulnerable.
- [`set_invulnerable`](#set_invulnerable): Make a character invulnerable (or not).
- [`set_gravity`](#set_gravity): false disables a character's gravity (it floats).
- [`set_visible`](#set_visible): Show or hide a character's model.
- [`play_animation`](#play_animation): Play an animation event on a character by name or id.
- [`animations`](#animations): Every animation event a character's behaviour accepts.
- [`distance`](#distance): Distance in metres between two characters, or a character and a point table {x, y, z}.
- [`status`](#status): A character's status build-up; the effect triggers when value reaches max.
- [`poise`](#poise): A character's current and max poise.
- [`animation`](#animation): Name of a character's current animation state.
- [`animation_speed`](#animation_speed): A character's animation speed multiplier (1 is normal).
- [`set_animation_speed`](#set_animation_speed): Set a character's animation speed.
- [`set_no_death`](#set_no_death): true: the character's HP can't drop below 1.
- [`set_no_damage`](#set_no_damage): true: hits do no damage to the character.
- [`set_enabled`](#set_enabled): Turn a character on or off: a disabled character disappears and does nothing until enabled again.
- [`set_ai`](#set_ai): Turn a character's AI on or off: with it off, it stands still and won't attack.
- [`clear_aggro`](#clear_aggro): Make a character forget its target (drop aggro) and choose what to do again.
- [`set_team`](#set_team): Change a character's team, e.g.
- [`bonfires`](#bonfires): Every bonfire in the game.

### characters

```lua
kindle.world.characters(opts: {radius?: number, alive?: boolean, player?: boolean, helpers?: boolean, kind?: string}?) -> kindle.character[]
```

Loaded characters, nearest first.

**Parameters**

- `opts` `{radius?: number, alive?: boolean, player?: boolean, helpers?: boolean, kind?: string}?`: defaults: any distance, living only, no players (this game's and, in a session, the others'), no helpers

**Returns**

- [`kindle.character[]`](types.md#kindlecharacter)

### objects

```lua
kindle.world.objects(opts: {radius?: number, map?: string, entity_id?: integer}?) -> kindle.object[]
```

The maps' placed objects that are loaded, nearest first. Join with kindle.maps.parts(o.map) by entity_id for an object's part name and model. Example, the objects within 5 m: `for _, o in ipairs(kindle.world.objects{ radius = 5 }) do kindle.log.info("%s %s", o.map, tostring(o.entity_id)) end`

**Parameters**

- `opts` `{radius?: number, map?: string, entity_id?: integer}?`: defaults: every loaded object

**Returns**

- [`kindle.object[]`](types.md#kindleobject)

### destroy_object

```lua
kindle.world.destroy_object(o: kindle.object|integer)
```

Break an object as a hit does: its pieces fall and any treasure in it drops (the game's command 2005[01]; the game keeps it broken, as it keeps a pot the player broke). Example, break every pot within 3 m: `for _, o in ipairs(kindle.world.objects{radius = 3}) do if o.destroyed == false and o.entity_id then kindle.world.destroy_object(o) end end`

**Parameters**

- `o` [`kindle.object`](types.md#kindleobject) | `integer`: an object from world.objects, or its entity id

### restore_object

```lua
kindle.world.restore_object(o: kindle.object|integer)
```

Put a broken object back whole (the game's command 2005[02]).

**Parameters**

- `o` [`kindle.object`](types.md#kindleobject) | `integer`

### set_object_active

```lua
kindle.world.set_object_active(o: kindle.object|integer, on: boolean?)
```

Turn an object's action on or off: a door that can't be opened, a lever that can't be pulled (the game's command 2005[03]).

**Parameters**

- `o` [`kindle.object`](types.md#kindleobject) | `integer`
- `on` `boolean?`: default true

### players

```lua
kindle.world.players() -> kindle.character[]
```

The players' characters, alive or not: this game's first (is_player), then in a session the others' (other_player). Quick enough for every frame (world.characters reads every character of the area).

**Returns**

- [`kindle.character[]`](types.md#kindlecharacter)

### character

```lua
kindle.world.character(c: kindle.charref) -> kindle.character?
```

One character by handle, entity id or table; nil if not loaded.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- [`kindle.character?`](types.md#kindlecharacter)

### spawn

```lua
kindle.world.spawn(opts: {enemy?: kindle.enemy|string, model?: integer|string, npc_param?: integer, think_param?: integer, area?: string, ai_enabled?: boolean, distance?: number, x?: number, y?: number, z?: number, angle?: number, on_spawn?: fun(c: kindle.character)}) -> kindle.character?
```

Create a character (experimental): any enemy, from any area. Name it with `enemy` ("black_knight", or "Black Knight"; the editor completes the names, and the overlay's Find tab lists them), or give `model` (1300 or "c1300") / `npc_param` / `think_param` (its AI) / `area` (its home map) yourself; explicit ids override the named enemy's. It appears `distance` metres (3) in front of the player, facing them, unless `x`, `y`, `z` / `angle` are given.

With `ai_enabled` (the default) an enemy from another area gets its own AI: the first time, its area's AI loads, so the enemy appears a moment later: the call returns nil and `on_spawn(c)` runs once it exists (it also runs when the enemy is created at once). With `ai_enabled = false` it's created at once with its AI off and stands still.

Spawned characters fight, die and drop souls, are removed when the area unloads, and have no entity id (event-script functions like set_enabled can't target them). `kindle.world.spawn{enemy = "black_knight", on_spawn = function(c) kindle.log.info("%s is here", c.model) end}`

**Parameters**

- `opts` `{enemy?: kindle.enemy|string, model?: integer|string, npc_param?: integer, think_param?: integer, area?: string, ai_enabled?: boolean, distance?: number, x?: number, y?: number, z?: number, angle?: number, on_spawn?: fun(c: kindle.character)}`

**Returns**

- [`kindle.character?`](types.md#kindlecharacter)

### load_ai

```lua
kindle.world.load_ai(area: string) -> boolean
```

Load another area's AI (experimental), so enemies from there act when spawned here: `area` is the map name: m30_00_00_00 High Wall, m30_01_00_00 Lothric Castle, m31_00_00_00 Undead Settlement, m32_00_00_00 Archdragon Peak, m33_00_00_00 Road of Sacrifices and Farron Keep, m34_01_00_00 Grand Archives, m35_00_00_00 Cathedral of the Deep, m37_00_00_00 Irithyll, m38_00_00_00 Catacombs and Smouldering Lake, m39_00_00_00 Irithyll Dungeon, m40_00_00_00 Cemetery of Ash and Firelink, m41_00_00_00 Kiln, m45_00_00_00 Ariandel, m50_00_00_00 Dreg Heap, m51_00_00_00 Ringed City. The first call starts loading and returns false; call again (a timer) until it returns true, then spawn. Call it again after the player changes area: the game rebuilds its AI then.

```lua
local t; t = kindle.every(0.2, function()
  if kindle.world.load_ai("m33_00_00_00") then
    kindle.cancel(t)
    kindle.world.spawn{model = 1300, npc_param = 130610, think_param = 130010}  -- Black Knight
  end
end)
```

**Parameters**

- `area` `string`

**Returns**

- `ready` `boolean`

### set_hp

```lua
kindle.world.set_hp(c: kindle.charref, hp: integer)
```

Set a character's HP.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `hp` `integer`: clamped to 0..max_hp

### kill

```lua
kindle.world.kill(c: kindle.charref)
```

Kill a character.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

### heal

```lua
kindle.world.heal(c: kindle.charref)
```

Restore a character's HP to its maximum.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

### apply_effect

```lua
kindle.world.apply_effect(c: kindle.charref, id: integer)
```

Apply an SpEffect to a character.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `id` `integer`: SpEffectParam row id

### remove_effect

```lua
kindle.world.remove_effect(c: kindle.charref, id: integer) -> integer
```

End an active SpEffect on a character; returns how many instances ended.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `id` `integer`

**Returns**

- `ended` `integer`

### effects

```lua
kindle.world.effects(c: kindle.charref) -> {id: integer, time_left: number, duration: number}[]
```

A character's active SpEffects (duration -1 = permanent).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `{id: integer, time_left: number, duration: number}[]`

### teleport

```lua
kindle.world.teleport(c: kindle.charref, x: number, y: number, z: number)
```

Move a character.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `x` `number`
- `y` `number`: up
- `z` `number`

### facing

```lua
kindle.world.facing(c: kindle.charref) -> number?
```

A character's facing angle in radians.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `radians` `number?`

### set_facing

```lua
kindle.world.set_facing(c: kindle.charref, radians: number)
```

Turn a character to face an angle (radians).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `radians` `number`

### invulnerable

```lua
kindle.world.invulnerable(c: kindle.charref) -> boolean?
```

Whether a character is invulnerable.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `boolean?`

### set_invulnerable

```lua
kindle.world.set_invulnerable(c: kindle.charref, on: boolean?)
```

Make a character invulnerable (or not).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### set_gravity

```lua
kindle.world.set_gravity(c: kindle.charref, on: boolean)
```

false disables a character's gravity (it floats).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean`

### set_visible

```lua
kindle.world.set_visible(c: kindle.charref, on: boolean?)
```

Show or hide a character's model.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### play_animation

```lua
kindle.world.play_animation(c: kindle.charref, anim: kindle.animation_name|string|integer)
```

Play an animation event on a character by name or id.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `anim` [`kindle.animation_name`](types.md#kindleanimation_name) | `string` | `integer`

### animations

```lua
kindle.world.animations(c: kindle.charref) -> table<integer, string>
```

Every animation event a character's behaviour accepts.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `table<integer, string>`

### distance

```lua
kindle.world.distance(a: kindle.charref|{x: number, y: number, z: number}, b: kindle.charref|{x: number, y: number, z: number}) -> number?
```

Distance in metres between two characters, or a character and a point table {x, y, z}.

**Parameters**

- `a` `kindle.charref|{x: number, y: number, z: number}`
- `b` `kindle.charref|{x: number, y: number, z: number}`

**Returns**

- `number?`

### status

```lua
kindle.world.status(c: kindle.charref) -> kindle.status
```

A character's status build-up; the effect triggers when value reaches max.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- [`kindle.status`](types.md#kindlestatus)

### poise

```lua
kindle.world.poise(c: kindle.charref) -> number, number
```

A character's current and max poise.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `current` `number`
- `max` `number`

### animation

```lua
kindle.world.animation(c: kindle.charref) -> string?
```

Name of a character's current animation state.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `string?`

### animation_speed

```lua
kindle.world.animation_speed(c: kindle.charref) -> number?
```

A character's animation speed multiplier (1 is normal).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

**Returns**

- `number?`

### set_animation_speed

```lua
kindle.world.set_animation_speed(c: kindle.charref, speed: number)
```

Set a character's animation speed.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `speed` `number`: 1 is normal; 0-10

### set_no_death

```lua
kindle.world.set_no_death(c: kindle.charref, on: boolean?)
```

true: the character's HP can't drop below 1.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### set_no_damage

```lua
kindle.world.set_no_damage(c: kindle.charref, on: boolean?)
```

true: hits do no damage to the character.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### set_enabled

```lua
kindle.world.set_enabled(c: kindle.charref, on: boolean?)
```

Turn a character on or off: a disabled character disappears and does nothing until enabled again. Needs the character's entity_id (map characters have one). Undone when the mod unloads.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### set_ai

```lua
kindle.world.set_ai(c: kindle.charref, on: boolean?)
```

Turn a character's AI on or off: with it off, it stands still and won't attack. It stays off until the mod turns it on or unloads: Kindle stops the game's own scripts from waking it (ambushes, enemies that get back up) and re-applies it while the character is loaded. Undone when the mod unloads.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `on` `boolean?`: default true

### clear_aggro

```lua
kindle.world.clear_aggro(c: kindle.charref)
```

Make a character forget its target (drop aggro) and choose what to do again.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)

### set_team

```lua
kindle.world.set_team(c: kindle.charref, team: "enemy"|"npc"|"player"|"helper"|integer)
```

Change a character's team, e.g. make an enemy friendly.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref)
- `team` `"enemy"` | `"npc"` | `"player"` | `"helper"` | `integer`

### bonfires

```lua
kindle.world.bonfires() -> kindle.bonfire[]
```

Every bonfire in the game.

**Returns**

- [`kindle.bonfire[]`](types.md#kindlebonfire)

