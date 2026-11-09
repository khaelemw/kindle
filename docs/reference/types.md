<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# Types

Data that functions take and return, and the names of things the API accepts.

## Tables

### kindle.screen_spec

| Field | Type | Description |
|---|---|---|
| `title` | `string` |  |
| `subtitle` | `string?` | a line under the title |
| `closable` | `boolean?` | false: Back doesn't close it (the mod does, with screen:close()) |
| `on_close` | `fun()?` | the player backed out of it |
| `rows` | [`kindle.screen_row[]?`](types.md#kindlescreen_row) | its rows (or tabs) |
| `tabs` | [`kindle.screen_tab[]?`](types.md#kindlescreen_tab) | tabs of rows instead of one list (up to 8; LB/RB, Q/E or a click turns them) |
| `tab` | `integer?` | the tab it opens on (1-based); an update giving another turns to it |
| `on_tab` | `fun(index: integer, title: string)?` | the player turned to another tab |

### kindle.screen_tab

| Field | Type | Description |
|---|---|---|
| `title` | `string` |  |
| `rows` | [`kindle.screen_row[]`](types.md#kindlescreen_row) |  |

### kindle.screen_row

| Field | Type | Description |
|---|---|---|
| `text` | `string?` | the row's label (every row but a heading) |
| `header` | `string?` | a section's heading (not selectable) |
| `help` | `string?` | shown at the bottom while the row is selected |
| `disabled` | `boolean?` | greyed; can't be picked or changed |
| `details` | `{title?: string, text?: string, image?: string\|integer}?` | shown in a pane right of the list while the row is selected (as the game's item menus show a picture and description): image a PNG in the mod's folder or the game's icon id; text wraps, "\n" breaks it |
| `icon` | `string` \| `integer?` | a PNG file in the mod's folder ("art/flask.png"), or the game's icon of that id (an item's, from items.info), drawn left of the text at its height |
| `info` | `boolean?` | a line of text, not selectable: greyed, or with its value on the right |
| `on_select` | `fun()?` | a button: runs when picked |
| `screen` | `kindle.screen_spec\|fun():?` | kindle.screen_spec? a button that opens a sub-screen |
| `toggle` | `boolean?` | a switch, on or off |
| `choices` | `string[]?` | one of a list (value: 1-based index or the text) |
| `slider` | `{min: number, max: number, step?: number}?` | a number in a range |
| `input` | `string?` | a line of text the player types (Enter to type, Enter to keep it, Esc to leave it as it was; a keyboard's) |
| `max` | `integer?` | an input's most characters (default 64, up to 256) |
| `key` | `string?` | a key the player picks by pressing it (Enter, then the key, controller button or mouse button; Esc leaves it): its name as kindle.input.down and key_pressed use ("f6", "pad_y", "mouse_middle"), "" for none |
| `value` | `any?` | a choice's or slider's value, or the text a button or info line shows on its right |
| `on_change` | `fun(value: any, text: string?)?` | a switch, choice or slider changed (a choice: index, text), an input's text was kept, or a key row's key was picked (its name) |

### kindle.screen

A screen kindle.ui.screen opened: screen:update(spec) changes it in place (the selected row stays), screen:close() closes it and any opened from it (on_close doesn't run), screen:is_open().

| Field | Type | Description |
|---|---|---|
| `id` | `integer` |  |
| `update` | `fun(self: kindle.screen, spec: kindle.screen_spec)` |  |
| `close` | `fun(self: kindle.screen): boolean` |  |
| `is_open` | `fun(self: kindle.screen): boolean` |  |

### kindle.layer_element

One thing a layer draws (kindle.ui.layer): x and y on the 1920 x 1080 canvas (scaled to the window), or an offset from `at`, and one of text, rect, line, bar, circle or image.

| Field | Type | Description |
|---|---|---|
| `x` | `number?` | default 0 |
| `y` | `number?` | default 0 |
| `at` | `{x: number, y: number, z: number}\|number[]\|kindle.character?` | a point in the world: the element is drawn where the camera sees it (x, y an offset from there), hidden behind the camera (not behind walls: it shows through them) |
| `range` | `number?` | with `at`: hidden further than this from the camera (metres) |
| `color` | `string` \| `number[]?` | "#rrggbb", "#rrggbbaa" or {r, g, b, a} (0-1); default the game's menu text colour (an image: white, as it is; another colour tints it) |
| `text` | `string?` | text, top-left at x, y (see align) |
| `size` | `number?` | text height on the canvas (default 28) |
| `font` | `"serif"` \| `"sans"?` | default "serif" (the game-like one) |
| `align` | `"left"` \| `"center"` \| `"right"?` | where x is on the text (default "left") |
| `shadow` | `boolean?` | a dark shadow under the text (default true) |
| `rect` | `{w: number, h: number}\|number[]?` | a box from x, y |
| `filled` | `boolean?` | a rect or circle filled (default true) or only outlined |
| `rounding` | `number?` | a rect's corner radius |
| `thickness` | `number?` | an outline's or a line's width (default 2) |
| `line` | `{dx: number, dy: number}\|number[]?` | a line from x, y to x + dx, y + dy |
| `bar` | `{w: number, h: number, value: number}\|number[]?` | a bar filled `value` (0-1) of its width with `color`, over `back` |
| `back` | `string` \| `number[]?` | a bar's empty part (default dark, translucent) |
| `circle` | `{r: number}\|number[]?` | a circle around x, y |
| `image` | `string` \| `integer?` | a PNG file in the mod's folder ("art/icon.png"), top-left at x, y, read once and again if the file changes; or the game's icon of that id (an item's, from items.info: 512 x 512) |
| `w` | `number?` | an image's width on the canvas (default its width in pixels) |
| `h` | `number?` | an image's height on the canvas (default its height in pixels) |

### kindle.layer

A layer kindle.ui.layer added: layer:set(elements) replaces what it draws, layer:show(on) shows or hides it, layer:remove() takes it away.

| Field | Type | Description |
|---|---|---|
| `id` | `integer` |  |
| `set` | `fun(self: kindle.layer, elements: kindle.layer_element[])` |  |
| `show` | `fun(self: kindle.layer, on: boolean?)` |  |
| `remove` | `fun(self: kindle.layer): boolean` |  |

### kindle.damage_event

A hit about to land (kindle.on("damage")). Return false from the handler to cancel it.

| Field | Type | Description |
|---|---|---|
| `target` | [`kindle.character?`](types.md#kindlecharacter) | who is hit |
| `attacker` | [`kindle.character?`](types.md#kindlecharacter) | who hit them (nil for traps, falls, etc.) |
| `attack_power` | `number` | attack power before the target's defence; assign to change the hit |
| `attack_id` | `integer` | AtkParam row of the attack |

### kindle.animation_event

An animation event a character is starting (kindle.on("animation")). Assign name or id to play something else; return false to cancel it.

| Field | Type | Description |
|---|---|---|
| `character` | [`kindle.character`](types.md#kindlecharacter) |  |
| `name` | [`kindle.animation_name`](types.md#kindleanimation_name) \| `string` | e.g. "W_Rolling" |
| `id` | `integer` |  |

### kindle.effect_event

An SpEffect about to be applied (kindle.on("effect")). Assign id to apply another effect; return false to block it.

| Field | Type | Description |
|---|---|---|
| `target` | [`kindle.character`](types.md#kindlecharacter) |  |
| `source` | [`kindle.character?`](types.md#kindlecharacter) | who applied it |
| `id` | `integer` | SpEffectParam row |

### kindle.item_used_event

A goods item a player used, as it takes effect (kindle.on("item_used")). Return false to keep the item (it still takes effect).

| Field | Type | Description |
|---|---|---|
| `player` | [`kindle.character`](types.md#kindlecharacter) |  |
| `item` | [`kindle.item`](types.md#kindleitem) |  |
| `name` | `string?` | the item's name |

### kindle.item_lot_event

An item lot about to be rolled (kindle.on("item_lot")): a map pickup (as its area loads), an enemy drop (as the enemy dies), or an event or boss reward. Assign lot to roll another; return false for nothing.

| Field | Type | Description |
|---|---|---|
| `lot` | `integer` | ItemLotParam row |

### kindle.menu

A game list menu being built.

| Field | Type | Description |
|---|---|---|
| `entries` | `{slot: integer, text_id: integer, text: string?}[]` |  |
| `bonfire` | `boolean` |  |

### kindle.character

| Field | Type | Description |
|---|---|---|
| `handle` | `integer` | stable while the character is loaded |
| `entity_id` | `integer?` | the map's entity id (used by event scripts and randomizer logs) |
| `npc_id` | `integer` | NpcParam row id |
| `name` | `string?` | display name for named NPCs/bosses (regular enemies have none) |
| `model` | `string` | model id, e.g. "c1000_00" |
| `kind` | `"player"` \| `"enemy"` \| `"npc"` \| `"helper"` \| `"other"` | from the live team |
| `team` | `integer` | raw live team value (1 player, 6 enemy, 26 friendly NPC, 8 event helper) |
| `hp` | `integer` |  |
| `max_hp` | `integer` |  |
| `alive` | `boolean` |  |
| `is_player` | `boolean` | this game's player |
| `other_player` | `boolean` | another player's, in a multiplayer session |
| `x` | `number` |  |
| `y` | `number` |  |
| `z` | `number` |  |
| `distance` | `number` | metres from the player |
| `address` | `integer` | memory address (for the debug module) |

### kindle.object

A placed object of a map (a door, lever, chest, ladder, breakable, ...).

| Field | Type | Description |
|---|---|---|
| `handle` | `integer` | stable while the object is loaded |
| `entity_id` | `integer?` | the map's entity id (event scripts use it; kindle.maps.parts lists the part with its name and model) |
| `map` | `string` | the map it belongs to, e.g. "m40_00_00_00" |
| `x` | `number` |  |
| `y` | `number` |  |
| `z` | `number` |  |
| `destroyed` | `boolean?` | broken (a pot, a breakable wall), as the game's event scripts read it |
| `distance` | `number?` | from the player |

### kindle.bonfire

| Field | Type | Description |
|---|---|---|
| `id` | `integer` | BonfireWarpParam row |
| `entity_id` | `integer` |  |
| `name` | `string?` |  |
| `lit` | `boolean` |  |
| `flag` | `integer` | the event flag set once it's lit |
| `warpable` | `boolean` |  |
| `dlc` | `integer` | 0 base game, 1 Ashes of Ariandel, 2 The Ringed City |
| `last_rested` | `boolean` |  |

### kindle.hks_call

| Field | Type | Description |
|---|---|---|
| `character` | [`kindle.character?`](types.md#kindlecharacter) | the character whose behaviour script made the call |
| `id` | `integer` \| `string` | env/act id (a number, or the Japanese name some calls use) |
| `args` | `any[]` | the call's other arguments (numbers, booleans, strings) |
| `result` | `integer?` | env only: the game's answer |

### EventScript

| Field | Type | Description |
|---|---|---|
| `id` | `integer` |  |
| `rest` | `integer` |  |
| `commands` | `{bank: integer, index: integer, args: integer[], bytes: integer[], layer: integer}[]` |  |
| `parameters` | `{command: integer, target: integer, source: integer, length: integer}[]` |  |

### kindle.hks_script_call

| Field | Type | Description |
|---|---|---|
| `character` | [`kindle.character?`](types.md#kindlecharacter) | the character whose script called |
| `name` | `string` | the first argument to kindle(...) |
| `args` | `any[]` | the other arguments (numbers, booleans, strings) |

### kindle.param_row

A live param row: read/assign fields by name, e.g. row.weight = 0. Each table's row type (in kindle-game.lua) lists its fields.

### kindle.map_part

One part of a map file, as kindle.maps.parts gives it.

| Field | Type | Description |
|---|---|---|
| `name` | `string` |  |
| `type` | `integer` | the part's type number |
| `kind` | `"map_piece"` \| `"object"` \| `"enemy"` \| `"player"` \| `"collision"` \| `"dummy_object"` \| `"dummy_enemy"` \| `"connect_collision"` \| `nil` |  |
| `model` | `string` | its model's name, e.g. "c1100" |
| `position` | `number[]` | {x, y, z} |
| `rotation` | `number[]` | {x, y, z}, as the file has it |
| `entity_id` | `integer?` |  |
| `entity_groups` | `integer[]?` |  |
| `think_param` | `integer?` | an enemy's AI (NpcThinkParam row) |
| `npc_param` | `integer?` | an enemy's NpcParam row |
| `talk` | `integer?` | an enemy's talk script |

## Aliases

### kindle.event

```lua
"frame" | "player_loaded" | "player_unloaded" | "unload" | "setting_changed" | "map_loading" | "player_died" | "region_changed" | "character_killed" | "character_spawned" | "item_gained" | "item_lost" | "effect_applied" | "effect_expired" | "level_up" | "equipment_changed" | "spells_changed" | "souls_changed" | "bonfire_lit" | "menu_opened" | "menu_closed" | "ember_changed" | "key_pressed" | "damage" | "animation" | "effect" | "item_used" | "item_lot" | "bonfire_rested" | "banner" | "boss_defeated" | "boss_bar" | "shop_buy" | "travel"
```

### kindle.enemy

Enemies `kindle.world.spawn{enemy = ...}` knows by name.

```lua
"abyss_watchers" | "aldrich_devourer_of_gods" | "ancient_wyvern" | "ancient_wyvern_boss" | "ancient_wyvern_mob" | "angel" | "angel_pilgrim" | "anor_londo_writhing_flesh" | "archdeacon_of_the_deep" | "archives_gargoyle" | "ariandel_greater_crab" | "basilisk" | "black_knight" | "bonewheel_skeleton" | "boreal_outrider_knight" | "bridge_darkeater_midir" | "cage_spider" | "carthus_curved_sword_skeleton" | "carthus_sandworm" | "carthus_shotel_skeleton" | "catacombs_writhing_flesh" | "cathedral_deacon" | "cathedral_evangelist" | "cathedral_hollow_soldier" | "cathedral_knight" | "cathedral_large_hollow_soldier" | "cemetery_hollow" | "consumed_king_oceiros" | "corpse_grub" | "corvian" | "corvian_knight" | "corvian_settler" | "crystal_lizard" | "crystal_sage_in_archives" | "crystal_sage_in_farron" | "dancer_of_the_boreal_valley" | "darkeater_midir" | "darkwraith" | "deep_accursed" | "demon" | "demon_cleric" | "demon_cleric_helper" | "demon_prince" | "demon_prince_helper_1" | "demon_prince_helper_2" | "demonic_statue" | "dragonslayer_armour" | "drakeblood_knight" | "dreg_heap_hollow_slave" | "dreg_heap_lothric_knight" | "dummy" | "elder_ghru" | "farron_follower" | "father_ariandel" | "fire_witch" | "ghru_grunt" | "giant_fly" | "giant_slave" | "grand_archives_hollow_soldier" | "grand_archives_scholar" | "grave_warden" | "greater_crab" | "greatwolf" | "gundyr" | "harald_legion_knight" | "havel_knight" | "high_lord_wolnir" | "high_wall_lothric_knight" | "hollow_cleric" | "hollow_manservant" | "hollow_slave" | "hollow_soldier" | "hound_rat" | "humanity_sprite" | "infested_corpse" | "invisible" | "irirthyll_deacon" | "irirthyll_dungeon_peasant_hollow" | "irirthyll_tall_deacon" | "irirthyll_wide_deacon" | "irithyll_giant_slave" | "irithyll_starved_hound" | "irithyllian_slave" | "jailer" | "judicator" | "judicator_helper" | "king_of_the_storm" | "large_hollow_soldier" | "large_hound_rat" | "large_serpent_man" | "large_starved_hound" | "lesser_crab" | "locust_preacher" | "lords_of_cinder" | "lothric_castle_hollow_soldier" | "lothric_castle_large_hollow_soldier" | "lothric_castle_lothric_knight" | "lothric_priest" | "lothric_thief" | "lycanthrope" | "lycanthrope_hunter" | "maggot_belly_starved_hound" | "millwood_knight" | "mimic_chest" | "monstrosity_of_sin" | "murkman" | "murkman_summoner" | "nameless_king" | "old_demon_king" | "peasant_hollow" | "pilgrim_butterfly" | "poisonhorn_bug" | "pontiff_knight" | "pontiff_sulyvahn" | "profaned_capital_gargoyle" | "pus_of_man" | "rapier_champion" | "ravenous_crystal_lizard" | "reanimated_corpse" | "red_eyed_lothric_knight" | "ringed_city_skeleton" | "ringed_knight" | "road_of_sacrifices_hollow_soldier" | "road_of_sacrifices_sorcerer" | "rock_lizard" | "rotted_greatwood" | "rotten_slug" | "serpent_man" | "serpent_man_summoner" | "sewer_centipede" | "silver_knight" | "sister_friede" | "skeleton" | "slave_knight_gael_1" | "slave_knight_gael_2" | "small_locust_preacher" | "smouldering_ghru_grunt" | "smouldering_writhing_flesh" | "starved_hound" | "stray_demon" | "sulyvahns_beast" | "tall_deacon_of_the_deep" | "tree_woman" | "twin_princes" | "twin_princes_helper" | "vordt_of_the_boreal_valley" | "wide_deacon_of_the_deep" | "winged_knight" | "wolf" | "wretch" | "yhorm_the_giant"
```

### kindle.storable

A value kindle.store can keep: tables must be lists or have string keys (nested up to 32 levels).

```lua
nil | boolean | number | string | table
```

### kindle.item

An encoded item id (category + param id). Build with items.goods() etc.

```lua
integer
```

### kindle.itemref

An item, or its name in the game's language ("Ember", "Shortsword +5").

```lua
kindle.item | kindle.item_name | string
```

### kindle.text_category

```lua
"goods_name" | "weapon_name" | "armor_name" | "ring_name" | "spell_name" | "npc_name" | "place_name" | "goods_info" | "weapon_info" | "armor_info" | "ring_info" | "goods_caption" | "weapon_caption" | "armor_caption" | "ring_caption" | "spell_info" | "spell_caption" | "talk" | "event_text" | integer
```

### kindle.screen

```lua
"level_up" | "attune" | "storage" | "travel" | "reinforce" | "infuse" | "repair" | "estus"
```

### kindle.color

string = "#rrggbb"

```lua
"gold" | "white" | "red" | "blue" | "green" | string
```

### kindle.banner

The game's banners: 1 heir_of_fire_destroyed, 2 you_died (also 6), 4 souls_retrieved, 5 host_of_embers_destroyed, 7 dark_spirit_destroyed, 12 begin_match, 13 bonfire_lit, 14 draw, 15 you_win, 16 you_lose, 17 ember_restored (also 18), 19 duty_fulfilled (also 20), 21 hollowing_reversed, 22 lord_of_cinder_fallen.

```lua
"heir_of_fire_destroyed" | "you_died" | "souls_retrieved" | "host_of_embers_destroyed" | "dark_spirit_destroyed" | "begin_match" | "bonfire_lit" | "draw" | "you_win" | "you_lose" | "ember_restored" | "duty_fulfilled" | "hollowing_reversed" | "lord_of_cinder_fallen"
```

### kindle.attribute

```lua
"vigor" | "attunement" | "endurance" | "vitality" | "strength" | "dexterity" | "intelligence" | "faith" | "luck"
```

### kindle.key

```lua
"esc" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "0" | "minus" | "equals" | "backspace" | "tab" | "q" | "w" | "e" | "r" | "t" | "y" | "u" | "i" | "o" | "p" | "lbracket" | "rbracket" | "enter" | "ctrl" | "a" | "s" | "d" | "f" | "g" | "h" | "j" | "k" | "l" | "semicolon" | "apostrophe" | "grave" | "shift" | "backslash" | "z" | "x" | "c" | "v" | "b" | "n" | "m" | "comma" | "period" | "slash" | "rshift" | "alt" | "space" | "capslock" | "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10" | "f11" | "f12" | "numlock" | "scrolllock" | "num0" | "num1" | "num2" | "num3" | "num4" | "num5" | "num6" | "num7" | "num8" | "num9" | "num_multiply" | "num_minus" | "num_plus" | "num_period" | "num_enter" | "num_divide" | "rctrl" | "ralt" | "home" | "pageup" | "end" | "pagedown" | "insert" | "delete" | "up" | "left" | "right" | "down" | "pad_up" | "pad_down" | "pad_left" | "pad_right" | "pad_start" | "pad_back" | "pad_ls" | "pad_rs" | "pad_lb" | "pad_rb" | "pad_a" | "pad_b" | "pad_x" | "pad_y" | "mouse_left" | "mouse_right" | "mouse_middle" | "mouse_4" | "mouse_5"
```

### kindle.slot

```lua
"left1" | "left2" | "left3" | "right1" | "right2" | "right3" | "arrow1" | "arrow2" | "bolt1" | "bolt2" | "head" | "body" | "arms" | "legs" | "ring1" | "ring2" | "ring3" | "ring4" | "covenant" | "quick1" | "quick2" | "quick3" | "quick4" | "quick5" | "quick6" | "quick7" | "quick8" | "quick9" | "quick10" | "tool1" | "tool2" | "tool3" | "tool4" | "tool5"
```

### kindle.status

```lua
{poison: {value: integer, max: integer}, toxic: {value: integer, max: integer}, bleed: {value: integer, max: integer}, curse: {value: integer, max: integer}, frost: {value: integer, max: integer}}
```

### kindle.charref

A character table, handle or entity id.

```lua
kindle.character | integer
```

## Names from your game

Kindle writes these to `kindle\stubs\kindle-game.lua` from your own game the first time you load into the world, so your editor can complete them (in your game's language).

### kindle.goods_name

A goods item's name, e.g. "Ember".

### kindle.weapon_name

A weapon's name (shields, catalysts and ammunition included), e.g. "Uchigatana". Add " +N" for an upgrade level.

### kindle.armor_name

An armor piece's name.

### kindle.ring_name

A ring's name.

### kindle.item_name

Any item's name.

### kindle.animation_name

An animation event the player's behaviour accepts, e.g. "W_Rolling".

### kindle.bonfire_name

A bonfire's name, e.g. "Firelink Shrine".

### kindle.param_table

A param table's name, e.g. "EquipParamWeapon". Each table also has a row type of the same name listing its fields.

