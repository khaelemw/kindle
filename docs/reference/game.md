<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.game

> Needs the `"game"` permission in `mod.toml`.

The catalogue: the game's decisions, values, functions and notifications Kindle names and reviews for mods. Entries are named, never addresses: see kindle.game.catalogue() for what this Kindle has.

## Functions

- [`catalogue`](#catalogue): Every entry: { hooks = { [name] = { doc, answers, answer } }, fields = { [name] = { doc, kind, character, writable } }, functions = { [name] = { doc, args } }, events = { [name] = { doc, args } } }.
- [`decide`](#decide): Answers a hook point (one of its answers; its first, the game's own, takes this mod's back).
- [`on_decide`](#on_decide): Answers each call of a hook point: fn gets the call's values and returns an answer, or nil for the switch's (kindle.game.decide).
- [`get`](#get): A field's value; a character's field takes the character's handle.
- [`set`](#set): Sets a field (checked against its kind and range).
- [`call`](#call): Calls a game function (its arguments checked against their kinds and ranges).
- [`on`](#on): A catalogue event's handler (as kindle.on); its values come as arguments.

### catalogue

```lua
kindle.game.catalogue() -> table
```

Every entry: { hooks = { [name] = { doc, answers, answer } }, fields = { [name] = { doc, kind, character, writable } }, functions = { [name] = { doc, args } }, events = { [name] = { doc, args } } }. This Kindle's: the hook points "bonfire.rest_requested" (bonfire, sitting: "allow" or "hold" the rest; held, the player sits on unrested), "bonfire.travel_requested" ("allow" or "refuse" the travel screen), "chr.departure_hit" ("count" or "ignore" a death floor's hit on another player's character, which would fade it out) and "load.bloodstain" ("host_only" or "any": whose bloodstain a load shows); where the game treats a phantom apart from the host: "events.chr_type_test", "events.own_world_test" (event, slot: "own_world" answers a joined player's own-world test as the host's game does), "events.multiplayer_state_test" (event, slot, state: "single_player" answers as a game alone), "npc.guest_look", "cutscene.skip_owner", "chr.ember_look" (an embered player's look, the HUD's fiery hexagon too, for any type), "load.estus_allotment", "bonfire.level_copy", "bonfire.session_list" ("keep": a joined player's list of the bonfires' fires is the host's after each of its loads, so its bonfires are lit as the host's), "talk.player_check", "talk.in_session", "talk.npc_ready", and a guest's loot: "loot.guest_loot", "loot.guest_item_ids", "loot.treasure_flag", "loot.item_messages", "loot.guest_awards", "enemy.host_hp_death"; where the game's multiplayer flow sends a player home: "flow.boss_clear_fade", "flow.boss_clear_home", "flow.boss_death_leaving", "flow.boss_leave_session", "flow.leave_limit", "world.reload_requested", "ui.network_notice" (row); and loading home: "load.phantom_copy" (asked), "load.player_type" (saved type), "chr_type.keep_home_type"; a hosted session's ends: "session.leave_requested", "session.close_requested"; "ui.hidden_name_tags" (no name tag over a character not drawn); "ui.banner" (type: "show" or "hide" one of the game's banners; the game shows "EMBER RESTORED" for another player's character embering too) (each with its answers in catalogue()); the fields "bonfire.sitting", "ui.menu_open", "ui.action_prompt", "ui.esc_menu_open", "ui.item_popup" (read only) and "chr.fall_time" (a character's, seconds in the air), "title.screen" (0 not at the title, 1 before its main menu, 2 the main menu); the functions "bonfire.rest" (entity: rest there as the game does), "bonfire.light_fire" (entity: light a bonfire's fire whose flag was set), "bloodstain.mark" and "bloodstain.leave" (the player's souls left where it was marked going down) and "chr.stand_on" (character, other: take the other's floor and the rooms it sees), "chr_type.get" and "chr_type.set" (the per-type table), "inventory.use_own_key_items", "flags.use_own_table" (digit: a session set uses the character's table), "flags.separate_block" (first: a session set's own copy of a block) and "world.send_home" (the player home as the game's flow sends it), "world.quit_to_title" (to the title's main menu as the pause menu's Quit Game goes), "title.back_to_press_start" (the main menu back to "PRESS ANY BUTTON", built anew after: its first entry chosen); the field "inventory.own_key_items"; the events "player.fall_death_blocked" (a fall death the no-death flag kept off), "flags.network_set" (first, last, on) and "flags.plain_set" (first, last, random: the random roll, each game's own; synced: the event keeps the game's network sync on, off for an event that runs for this game alone; event: its id), the game's event scripts setting flags on this game, "flags.changed" (flag, on, session, received: every flag the game changes, its own code's too, as a summon sign's; received, one another game of the session sent), "flow.boss_clear_stayed" (a boss's clear left this player where it is) and "session.left" (by_this_game: Kindle's session ended, by this game's leave or the link to the host closing), "lobby.closed" (the lobby's host closed it) and a lobby member's party save (kindle.net.send_party_save): "saves.party_save_ready" (has_character: put in place at the main menu; without a character the menu offers New Game), "saves.party_save_waiting" (it came while a character is loaded: quit to the title for it), "saves.party_save_left" (the lobby left while playing it) and "saves.own_saves_back" (the member's own saves at the main menu again).

**Returns**

- `table`

### decide

```lua
kindle.game.decide(name: string, answer: string)
```

Answers a hook point (one of its answers; its first, the game's own, takes this mod's back). Mods that answer a hook point give the same answer (a different one is an error); it goes back to the game's when they unload.

**Parameters**

- `name` `string`
- `answer` `string`

### on_decide

```lua
kindle.game.on_decide(name: string, fn: fun(...: integer): string?) -> integer
```

Answers each call of a hook point: fn gets the call's values and returns an answer, or nil for the switch's (kindle.game.decide). Only calls on the game's thread ask. Kindle hooks the game there when a mod first answers (an error if it can't on this game build).

**Parameters**

- `name` `string`
- `fn` `fun(...: integer): string?`

**Returns**

- `handle` `integer`

### get

```lua
kindle.game.get(name: string, handle: integer?) -> boolean|number
```

A field's value; a character's field takes the character's handle.

**Parameters**

- `name` `string`
- `handle` `integer?`

**Returns**

- `boolean` | `number`

### set

```lua
kindle.game.set(name: string, value: boolean|number, handle: integer?)
```

Sets a field (checked against its kind and range).

**Parameters**

- `name` `string`
- `value` `boolean` | `number`
- `handle` `integer?`

### call

```lua
kindle.game.call(name: string) -> boolean|number|nil
```

Calls a game function (its arguments checked against their kinds and ranges).

**Parameters**

- `name` `string`

**Returns**

- `boolean` | `number` | `nil`

### on

```lua
kindle.game.on(name: string, fn: function) -> integer
```

A catalogue event's handler (as kindle.on); its values come as arguments.

**Parameters**

- `name` `string`
- `fn` `function`

**Returns**

- `handle` `integer`

