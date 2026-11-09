<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.events

> Needs the `"events"` permission in `mod.toml`.

Run the game's own event script (EMEVD) commands, and change its event scripts (applied from the next time that map's events load). The named functions below cover the most useful ones (entity = a map entity id, e.g. a character's entity_id, 10000 for the player); events.run runs any command by number. Map event scripts are lists of commands: a bank (category, e.g. 2003 events, 2004 characters, 2006 effects, 2007 messages, 2010 sound), an index and packed arguments. Kindle runs a command through the game's own handler, exactly as a script would. Use kindle.debug.trace_events(true) to see the commands the game runs and their arguments.

## Functions

- [`run`](#run): Run an event script command.
- [`script`](#script): The game's own event from a map's event file ("m40_00_00_00", "common", "common_func"): its commands with their arguments as 32-bit words and as bytes, their layers, and its parameters (the event's argument bytes source..
- [`view`](#view): The event as the game will run it: the game's, with every change mods and Kindle make to it now.
- [`edit`](#edit): Changes an event, all or nothing.
- [`find`](#find): Every command bank[index] in a map's event file as the game has it: {event, command (its number in the event, from 1), args (32-bit words), bytes} each.
- [`reset`](#reset): Drops this mod's changes to one map's events (from the next time that map's events load); returns how many there were.
- [`disable`](#disable): Makes an event do nothing (it ends at once).
- [`remove`](#remove): Removes an event's command number n.
- [`insert`](#insert): Inserts a command before number n (the count + 1 adds it at the end).
- [`replace`](#replace): Replaces an event's command number n.
- [`add`](#add): Adds a new event to a map.
- [`start`](#start): Starts an event when the map's events load, as the game's event 0 does: slot, then the event's arguments (format and values).
- [`ai_command`](#ai_command): Give an entity's AI a command (AI-specific).
- [`ai_replan`](#ai_replan): Make an entity's AI decide what to do again.
- [`apply_effect`](#apply_effect): Apply an SpEffect to an entity.
- [`area_name`](#area_name): Show an area-name title (place name text id).
- [`award_item_lot`](#award_item_lot): Give the player an item lot (ItemLotParam row), with the "item received" box.
- [`bonfire_recovery`](#bonfire_recovery): Restore the player as resting at a bonfire does.
- [`boss_defeat_banner`](#boss_defeat_banner): Handle a boss defeat and show a banner type (1 HEIR OF FIRE DESTROYED, 22 LORD OF CINDER FALLEN).
- [`camera_shake`](#camera_shake): Shake the camera (shake_id a camera vibration id) around an entity, fading between the distances.
- [`clear_ai_target`](#clear_ai_target): Make an entity forget its target.
- [`delete_map_sfx`](#delete_map_sfx): Stop a map SFX entity's effect.
- [`destroy_object`](#destroy_object): Break an object (slot 1 is what the game's scripts use).
- [`handle_boss_defeat`](#handle_boss_defeat): Mark a boss as defeated, as the game does when one dies (without a banner).
- [`show_hud`](#show_hud): Show (true) or hide (false) the HUD the way event scripts do: while it's hidden the pause menu can't be opened either, so always show it again.
- [`kill`](#kill): Kill an entity, giving the player its souls if give_souls.
- [`object_animation`](#object_animation): Play an object's animation (a lever pulled, a door opening).
- [`play_sound`](#play_sound): Play a sound on an entity: sound_type "sfx", "character", "menu", "environment", "voice", ...
- [`remove_effect`](#remove_effect): Remove an SpEffect from an entity.
- [`restore_object`](#restore_object): Put a broken object back whole.
- [`set_ai`](#set_ai): Turn an entity's AI on or off.
- [`set_ai_id`](#set_ai_id): Change an entity's AI (NpcThinkParam row).
- [`set_enabled`](#set_enabled): Enable or disable an entity (a disabled character disappears).
- [`set_hp_bar`](#set_hp_bar): Show or hide an entity's HP bar.
- [`set_immortal`](#set_immortal): An entity's HP can't drop below 1 while on.
- [`set_invincible`](#set_invincible): An entity takes no damage while on.
- [`set_map_part`](#set_map_part): Show or hide a map part entity.
- [`set_obj_act`](#set_obj_act): Turn an object action (ObjActParam row: a door, a lever, a chest) on or off on an object.
- [`set_object_active`](#set_object_active): Turn an object's action on or off.
- [`set_object_treasure`](#set_object_treasure): Let an object's treasure be picked up, or not.
- [`set_respawn_point`](#set_respawn_point): Set where the player respawns (a respawn point entity).
- [`set_team`](#set_team): Change an entity's team (a number or name: enemy, friendly_npc, ally, hostile_npc, ...).
- [`show_boss_bar`](#show_boss_bar): Show (on) or hide a boss health bar for entity: slot 0-2, name_id an NPC name text id.
- [`spawn_map_sfx`](#spawn_map_sfx): Start a map SFX entity's effect.
- [`spawn_sfx`](#spawn_sfx): Play a one-off visual effect (SFX id) on an entity: target_type "character", "object" or "area"; dummypoly -1 for the root.
- [`status_message`](#status_message): Show a status message (event text id) at the top of the screen.
- [`warp_player`](#warp_player): Warp the player to a map (area and block, e.g.

### run

```lua
kindle.events.run(bank: integer, index: integer, format: string?, ...: number) -> integer
```

Run an event script command. `format` has one letter per argument value: b/B i8/u8, h/H i16/u16, i/I i32/u32, f f32 (packed with each value aligned to its size, like the game's files). Runs on Kindle's next frame; an optional last argument on_done(done, err) gets whether the game's handler finished it. Example: kindle.events.run(2007, 2, "B", 1) runs DisplayBanner(1).

**Parameters**

- `bank` `integer`
- `index` `integer`
- `format` `string?`: argument format, e.g. "iB"
- `...` `number`: argument values, then optionally on_done

**Returns**

- `request` `integer`

### script

```lua
kindle.events.script(map: string, id: integer) -> EventScript?
```

The game's own event from a map's event file ("m40_00_00_00", "common", "common_func"): its commands with their arguments as 32-bit words and as bytes, their layers, and its parameters (the event's argument bytes source.. copied into command's bytes target..). nil if the map has no such event.

**Parameters**

- `map` `string`
- `id` `integer`

**Returns**

- [`EventScript?`](types.md#eventscript)

### view

```lua
kindle.events.view(map: string, id: integer) -> EventScript?
```

The event as the game will run it: the game's, with every change mods and Kindle make to it now. As events.script.

**Parameters**

- `map` `string`
- `id` `integer`

**Returns**

- [`EventScript?`](types.md#eventscript)

### edit

```lua
kindle.events.edit(map: string, id: integer, steps: table[], why: string?)
```

Changes an event, all or nothing. Each step names a command by its number in the event as it is before this edit (1 = the first; insert at the count + 1 adds at the end): {check = n, is = {bank, index, byte, ...}} (command n must be that: its first bytes), {remove = n}, {replace = n, bank =, index =, format =, values = {...}}, {insert = n, bank =, index =, format =, values = {...}} (before command n). Any step can say what its command must be (is). A new command can take the event's arguments (params = {{target =, source =, length =}}, in bytes) and command m's layer (layer_of = m; a replacement keeps its command's by default); a replacement can keep its command's argument entries (keep_params = true). If a check fails, nothing of the edit applies and the log says why, with the reason given. Example: events.edit("m38_00_00_00", 13805809, {{remove = 13, is = {1003, 12, 4, 0}}}, "the goblet for players")

**Parameters**

- `map` `string`
- `id` `integer`
- `steps` `table[]`
- `why` `string?`

### find

```lua
kindle.events.find(map: string, bank: integer, index: integer) -> {event: integer, command: integer, args: integer[], bytes: string}[]
```

Every command bank[index] in a map's event file as the game has it: {event, command (its number in the event, from 1), args (32-bit words), bytes} each. E.g. every boss health bar a map shows: events.find("m30_00_00_00", 2003, 11).

**Parameters**

- `map` `string`
- `bank` `integer`
- `index` `integer`

**Returns**

- `{event: integer, command: integer, args: integer[], bytes: string}[]`

### reset

```lua
kindle.events.reset(map: string) -> integer
```

Drops this mod's changes to one map's events (from the next time that map's events load); returns how many there were. For a mod whose changes depend on the character (a randomizer setting them anew for each).

**Parameters**

- `map` `string`

**Returns**

- `integer`

### disable

```lua
kindle.events.disable(map: string, id: integer)
```

Makes an event do nothing (it ends at once). Changes to event scripts apply from the next time that map's events load (loading into the area), in the order mods make them; command numbers (from 1) count the event's commands as they are by then.

**Parameters**

- `map` `string`: e.g. "m40_00_00_00"
- `id` `integer`

### remove

```lua
kindle.events.remove(map: string, id: integer, n: integer)
```

Removes an event's command number n.

**Parameters**

- `map` `string`
- `id` `integer`
- `n` `integer`

### insert

```lua
kindle.events.insert(map: string, id: integer, n: integer, bank: integer, index: integer, format: string?, ...: number)
```

Inserts a command before number n (the count + 1 adds it at the end). Arguments as for run.

**Parameters**

- `map` `string`
- `id` `integer`
- `n` `integer`
- `bank` `integer`
- `index` `integer`
- `format` `string?`
- `...` `number`

### replace

```lua
kindle.events.replace(map: string, id: integer, n: integer, bank: integer, index: integer, format: string?, ...: number)
```

Replaces an event's command number n.

**Parameters**

- `map` `string`
- `id` `integer`
- `n` `integer`
- `bank` `integer`
- `index` `integer`
- `format` `string?`
- `...` `number`

### add

```lua
kindle.events.add(map: string, id: integer, commands: {bank: integer, index: integer, format: string?, values: number[]?}[], rest: integer?)
```

Adds a new event to a map. rest: what happens when the map reloads (0 restart, the default; 1 end; 2 end and restart). Start it with events.start. Example: events.add("m40_00_00_00", 14009960, {{bank = 2003, index = 2, format = "iB", values = {14000998, 1}}})

**Parameters**

- `map` `string`
- `id` `integer`
- `commands` `{bank: integer, index: integer, format: string?, values: number[]?}[]`
- `rest` `integer?`

### start

```lua
kindle.events.start(map: string, id: integer, slot: integer?, format: string?, ...: number)
```

Starts an event when the map's events load, as the game's event 0 does: slot, then the event's arguments (format and values).

**Parameters**

- `map` `string`
- `id` `integer`
- `slot` `integer?`
- `format` `string?`
- `...` `number`

### ai_command

```lua
kindle.events.ai_command(entity: integer, command: integer, slot: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Give an entity's AI a command (AI-specific). (command 2004[17])

**Parameters**

- `entity` `integer`
- `command` `integer`
- `slot` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### ai_replan

```lua
kindle.events.ai_replan(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Make an entity's AI decide what to do again. (command 2004[20])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### apply_effect

```lua
kindle.events.apply_effect(entity: integer, speffect: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Apply an SpEffect to an entity. (command 2004[08])

**Parameters**

- `entity` `integer`
- `speffect` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### area_name

```lua
kindle.events.area_name(text_id: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show an area-name title (place name text id). (command 2007[11])

**Parameters**

- `text_id` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### award_item_lot

```lua
kindle.events.award_item_lot(lot: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Give the player an item lot (ItemLotParam row), with the "item received" box. (command 2003[04])

**Parameters**

- `lot` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### bonfire_recovery

```lua
kindle.events.bonfire_recovery(on_done: fun(done: boolean?, err: string?)?) -> integer
```

Restore the player as resting at a bonfire does. (command 2004[47])

**Parameters**

- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### boss_defeat_banner

```lua
kindle.events.boss_defeat_banner(entity: integer, banner: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Handle a boss defeat and show a banner type (1 HEIR OF FIRE DESTROYED, 22 LORD OF CINDER FALLEN). (command 2003[74])

**Parameters**

- `entity` `integer`
- `banner` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### camera_shake

```lua
kindle.events.camera_shake(shake_id: integer, target_type: "character"|"object"|"area"|integer, entity: integer, dummypoly: integer, fade_start: number, fade_end: number, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Shake the camera (shake_id a camera vibration id) around an entity, fading between the distances. (command 2008[02])

**Parameters**

- `shake_id` `integer`
- `target_type` `"character"` | `"object"` | `"area"` | `integer`
- `entity` `integer`
- `dummypoly` `integer`
- `fade_start` `number`
- `fade_end` `number`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### clear_ai_target

```lua
kindle.events.clear_ai_target(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Make an entity forget its target. (command 2004[16])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### delete_map_sfx

```lua
kindle.events.delete_map_sfx(entity: integer, root_only: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Stop a map SFX entity's effect. (command 2006[01])

**Parameters**

- `entity` `integer`
- `root_only` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### destroy_object

```lua
kindle.events.destroy_object(entity: integer, slot: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Break an object (slot 1 is what the game's scripts use). (command 2005[01])

**Parameters**

- `entity` `integer`
- `slot` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### handle_boss_defeat

```lua
kindle.events.handle_boss_defeat(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Mark a boss as defeated, as the game does when one dies (without a banner). (command 2003[12])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### show_hud

```lua
kindle.events.show_hud(shown: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show (true) or hide (false) the HUD the way event scripts do: while it's hidden the pause menu can't be opened either, so always show it again. To hide the HUD during play, use kindle.ui.set_hud("off") instead. (command 2003[71])

**Parameters**

- `shown` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### kill

```lua
kindle.events.kill(entity: integer, give_souls: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Kill an entity, giving the player its souls if give_souls. (command 2004[04])

**Parameters**

- `entity` `integer`
- `give_souls` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### object_animation

```lua
kindle.events.object_animation(entity: integer, animation: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Play an object's animation (a lever pulled, a door opening). (command 2005[07])

**Parameters**

- `entity` `integer`
- `animation` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### play_sound

```lua
kindle.events.play_sound(entity: integer, sound_type: "environment"|"character"|"menu"|"object"|"cutscene"|"sfx"|"bgm"|"voice"|integer, sound_id: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Play a sound on an entity: sound_type "sfx", "character", "menu", "environment", "voice", ... and its id. (command 2010[02])

**Parameters**

- `entity` `integer`
- `sound_type` `"environment"` | `"character"` | `"menu"` | `"object"` | `"cutscene"` | `"sfx"` | `"bgm"` | `"voice"` | `integer`
- `sound_id` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### remove_effect

```lua
kindle.events.remove_effect(entity: integer, speffect: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Remove an SpEffect from an entity. (command 2004[21])

**Parameters**

- `entity` `integer`
- `speffect` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### restore_object

```lua
kindle.events.restore_object(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Put a broken object back whole. (command 2005[02])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_ai

```lua
kindle.events.set_ai(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Turn an entity's AI on or off. (command 2004[01]) Undone when the mod unloads.

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_ai_id

```lua
kindle.events.set_ai_id(entity: integer, ai_id: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Change an entity's AI (NpcThinkParam row). (command 2004[19])

**Parameters**

- `entity` `integer`
- `ai_id` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_enabled

```lua
kindle.events.set_enabled(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Enable or disable an entity (a disabled character disappears). (command 2004[05]) Undone when the mod unloads.

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_hp_bar

```lua
kindle.events.set_hp_bar(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show or hide an entity's HP bar. (command 2004[30])

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_immortal

```lua
kindle.events.set_immortal(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

An entity's HP can't drop below 1 while on. (command 2004[12]) Undone when the mod unloads.

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_invincible

```lua
kindle.events.set_invincible(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

An entity takes no damage while on. (command 2004[15]) Undone when the mod unloads.

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_map_part

```lua
kindle.events.set_map_part(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show or hide a map part entity. (command 2012[01])

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_obj_act

```lua
kindle.events.set_obj_act(entity: integer, obj_act: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Turn an object action (ObjActParam row: a door, a lever, a chest) on or off on an object. (command 2005[06])

**Parameters**

- `entity` `integer`
- `obj_act` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_object_active

```lua
kindle.events.set_object_active(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Turn an object's action on or off. (command 2005[03])

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_object_treasure

```lua
kindle.events.set_object_treasure(entity: integer, on: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Let an object's treasure be picked up, or not. (command 2005[04])

**Parameters**

- `entity` `integer`
- `on` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_respawn_point

```lua
kindle.events.set_respawn_point(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Set where the player respawns (a respawn point entity). (command 2003[23])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### set_team

```lua
kindle.events.set_team(entity: integer, team: integer|"enemy"|"strong_enemy"|"friendly_npc"|"hostile_npc"|"ally"|"hostile_ally"|"friendly_enemy"|"human"|"indiscriminate"|"disabled", on_done: fun(done: boolean?, err: string?)?) -> integer
```

Change an entity's team (a number or name: enemy, friendly_npc, ally, hostile_npc, ...). (command 2004[02])

**Parameters**

- `entity` `integer`
- `team` `integer` | `"enemy"` | `"strong_enemy"` | `"friendly_npc"` | `"hostile_npc"` | `"ally"` | `"hostile_ally"` | `"friendly_enemy"` | `"human"` | `"indiscriminate"` | `"disabled"`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### show_boss_bar

```lua
kindle.events.show_boss_bar(on: boolean, entity: integer, slot: integer, name_id: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show (on) or hide a boss health bar for entity: slot 0-2, name_id an NPC name text id. (command 2003[11])

**Parameters**

- `on` `boolean`
- `entity` `integer`
- `slot` `integer`
- `name_id` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### spawn_map_sfx

```lua
kindle.events.spawn_map_sfx(entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Start a map SFX entity's effect. (command 2006[02])

**Parameters**

- `entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### spawn_sfx

```lua
kindle.events.spawn_sfx(target_type: "character"|"object"|"area"|integer, entity: integer, dummypoly: integer, sfx: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Play a one-off visual effect (SFX id) on an entity: target_type "character", "object" or "area"; dummypoly -1 for the root. (command 2006[03])

**Parameters**

- `target_type` `"character"` | `"object"` | `"area"` | `integer`
- `entity` `integer`
- `dummypoly` `integer`
- `sfx` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### status_message

```lua
kindle.events.status_message(text_id: integer, pad: boolean, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Show a status message (event text id) at the top of the screen. (command 2007[03])

**Parameters**

- `text_id` `integer`
- `pad` `boolean`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

### warp_player

```lua
kindle.events.warp_player(area: integer, block: integer, start_entity: integer, on_done: fun(done: boolean?, err: string?)?) -> integer
```

Warp the player to a map (area and block, e.g. 30, 0 for High Wall) at the entity start_entity. (command 2003[14])

**Parameters**

- `area` `integer`
- `block` `integer`
- `start_entity` `integer`
- `on_done` `fun(done: boolean?, err: string?)?`

**Returns**

- `request` `integer`

