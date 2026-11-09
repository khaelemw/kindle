---@meta
-- Kindle API type stubs for the Lua language server (autocomplete + docs in VS Code).
-- These describe the API; they are not loaded by the game. Names the API accepts (items,
-- animations, bonfires, param tables and fields) are in kindle-game.lua next to this file,
-- which Kindle generates from your game once you load in.

---@class kindle  Events, timers and this mod's own info. Always available.
---@field version string Kindle runtime version
---@field api integer the Lua API version (mods set `api` in mod.toml to the one they were written for)
---@field mod {id: string, name: string, version: string} This mod's manifest info
kindle = {}

---@alias kindle.event "frame"|"player_loaded"|"player_unloaded"|"unload"|"setting_changed"|"map_loading"|"player_died"|"region_changed"|"character_killed"|"character_spawned"|"item_gained"|"item_lost"|"effect_applied"|"effect_expired"|"level_up"|"equipment_changed"|"spells_changed"|"souls_changed"|"bonfire_lit"|"menu_opened"|"menu_closed"|"ember_changed"|"key_pressed"|"damage"|"animation"|"effect"|"item_used"|"item_lot"|"bonfire_rested"|"banner"|"boss_defeated"|"boss_bar"|"shop_buy"|"travel"
--- Enemies `kindle.world.spawn{enemy = ...}` knows by name.
---@alias kindle.enemy "abyss_watchers"|"aldrich_devourer_of_gods"|"ancient_wyvern"|"ancient_wyvern_boss"|"ancient_wyvern_mob"|"angel"|"angel_pilgrim"|"anor_londo_writhing_flesh"|"archdeacon_of_the_deep"|"archives_gargoyle"|"ariandel_greater_crab"|"basilisk"|"black_knight"|"bonewheel_skeleton"|"boreal_outrider_knight"|"bridge_darkeater_midir"|"cage_spider"|"carthus_curved_sword_skeleton"|"carthus_sandworm"|"carthus_shotel_skeleton"|"catacombs_writhing_flesh"|"cathedral_deacon"|"cathedral_evangelist"|"cathedral_hollow_soldier"|"cathedral_knight"|"cathedral_large_hollow_soldier"|"cemetery_hollow"|"consumed_king_oceiros"|"corpse_grub"|"corvian"|"corvian_knight"|"corvian_settler"|"crystal_lizard"|"crystal_sage_in_archives"|"crystal_sage_in_farron"|"dancer_of_the_boreal_valley"|"darkeater_midir"|"darkwraith"|"deep_accursed"|"demon"|"demon_cleric"|"demon_cleric_helper"|"demon_prince"|"demon_prince_helper_1"|"demon_prince_helper_2"|"demonic_statue"|"dragonslayer_armour"|"drakeblood_knight"|"dreg_heap_hollow_slave"|"dreg_heap_lothric_knight"|"dummy"|"elder_ghru"|"farron_follower"|"father_ariandel"|"fire_witch"|"ghru_grunt"|"giant_fly"|"giant_slave"|"grand_archives_hollow_soldier"|"grand_archives_scholar"|"grave_warden"|"greater_crab"|"greatwolf"|"gundyr"|"harald_legion_knight"|"havel_knight"|"high_lord_wolnir"|"high_wall_lothric_knight"|"hollow_cleric"|"hollow_manservant"|"hollow_slave"|"hollow_soldier"|"hound_rat"|"humanity_sprite"|"infested_corpse"|"invisible"|"irirthyll_deacon"|"irirthyll_dungeon_peasant_hollow"|"irirthyll_tall_deacon"|"irirthyll_wide_deacon"|"irithyll_giant_slave"|"irithyll_starved_hound"|"irithyllian_slave"|"jailer"|"judicator"|"judicator_helper"|"king_of_the_storm"|"large_hollow_soldier"|"large_hound_rat"|"large_serpent_man"|"large_starved_hound"|"lesser_crab"|"locust_preacher"|"lords_of_cinder"|"lothric_castle_hollow_soldier"|"lothric_castle_large_hollow_soldier"|"lothric_castle_lothric_knight"|"lothric_priest"|"lothric_thief"|"lycanthrope"|"lycanthrope_hunter"|"maggot_belly_starved_hound"|"millwood_knight"|"mimic_chest"|"monstrosity_of_sin"|"murkman"|"murkman_summoner"|"nameless_king"|"old_demon_king"|"peasant_hollow"|"pilgrim_butterfly"|"poisonhorn_bug"|"pontiff_knight"|"pontiff_sulyvahn"|"profaned_capital_gargoyle"|"pus_of_man"|"rapier_champion"|"ravenous_crystal_lizard"|"reanimated_corpse"|"red_eyed_lothric_knight"|"ringed_city_skeleton"|"ringed_knight"|"road_of_sacrifices_hollow_soldier"|"road_of_sacrifices_sorcerer"|"rock_lizard"|"rotted_greatwood"|"rotten_slug"|"serpent_man"|"serpent_man_summoner"|"sewer_centipede"|"silver_knight"|"sister_friede"|"skeleton"|"slave_knight_gael_1"|"slave_knight_gael_2"|"small_locust_preacher"|"smouldering_ghru_grunt"|"smouldering_writhing_flesh"|"starved_hound"|"stray_demon"|"sulyvahns_beast"|"tall_deacon_of_the_deep"|"tree_woman"|"twin_princes"|"twin_princes_helper"|"vordt_of_the_boreal_valley"|"wide_deacon_of_the_deep"|"winged_knight"|"wolf"|"wretch"|"yhorm_the_giant"

--- Subscribe to an event. `frame` handlers receive the seconds since the last frame.
--- `player_loaded` fires when a character is in the world: once the loading screen is
--- over (and immediately on (re)load if one already is). `unload` fires before the mod is reloaded or removed.
---
--- Game events (polled each frame, only while something is subscribed):
--- * `player_died()`
--- * `region_changed(new, old)` play region ids
--- * `character_killed(character)` a non-player character's HP reached 0
--- * `character_spawned(character)` a non-player character appeared (one created, or the
---   area's enemies coming back when the player rests); not the characters already there
---   when the player arrives in an area. Checked every few frames.
--- * `item_gained(item, count, name)` / `item_lost(item, count, name)` inventory changes
--- * `effect_applied(id)` / `effect_expired(id)` SpEffects on the player
--- * `level_up(level, old_level, raised, souls)` the soul level rose (at the game's level-up
---   screen, or by a script): `raised` maps each attribute that rose to its new value
---   (`{ vigor = 13 }`), `souls` is how many souls went with it (the cost)
--- * `equipment_changed(slot, item, old)` what a slot holds changed (kindle.slot; item or old nil
---   for an empty slot): equipping, unequipping, a weapon swapped by a script. Checked every few frames.
--- * `spells_changed(spells, old)` the attuned spells changed (lists of goods items, slot order)
--- * `souls_changed(souls, old)` the souls held changed (a kill, a purchase, a level, dying)
--- * `bonfire_lit(bonfire)` a bonfire was lit for the first time (a kindle.bonfire). Checked
---   twice a second.
--- * `menu_opened(kind)` / `menu_closed(kind)` one of the game's menus opened or closed: kind
---   "esc" (the Esc menu) or "menu" (any other: a bonfire's, a talk's, a shop, level up...)
--- * `ember_changed(embered)` the player was embered (true) or lost it (false)
--- * `key_pressed(key)` the player pressed a key, controller button or mouse button (kindle.key; not
---   while a screen or window of Kindle's is open)
--- * `bonfire_rested(bonfire)` the player sat down at a bonfire; `bonfire` is the
---   kindle.bonfire rested at (nil if unknown)
--- * `shop_buy(item, count, row)` the player bought `count` of `item` in a shop
---   (ShopLineupParam `row`); the souls are already being taken: it reports, it can't undo
--- * `banner(name, type)` the game showed one of its banners (see kindle.banner)
--- * `boss_defeated(kind)` a boss fell: "heir_of_fire" (HEIR OF FIRE DESTROYED) or
---   "lord_of_cinder" (LORD OF CINDER FALLEN), from the game's own banner
--- * `boss_bar(shown, name, entity)` the game's event scripts showed (a boss fight began) or hid
---   (it ended: the boss died, or the player did and the fight reset) a boss's health bar: the
---   name it shows (nil if it has none) and the boss's entity id
---
--- Hooked events (run as the game does it, so handlers can change the outcome):
--- * `damage(e)` every hit on any character. `e.target`, `e.attacker` (characters),
---   `e.attack_power` (before the target's defence; assign to change the hit),
---   `e.attack_id` (AtkParam row). Return false to cancel the hit.
--- * `animation(e)` every animation event any character starts: `e.character`, `e.name`
---   (e.g. "W_Rolling"), `e.id`. Assign `e.name` or `e.id` to play something else instead;
---   return false to cancel it. Fires often: keep handlers quick.
--- * `effect(e)` an SpEffect about to be applied to any character: `e.target`, `e.source`
---   (characters), `e.id`. Assign `e.id` to apply a different effect; return false to block
---   it. Fires often (many effects are re-applied every frame): keep handlers quick.
--- * `item_used(e)` a player used a goods item (as it takes effect): `e.player`, `e.item`,
---   `e.name`. Return false to keep the item (it still takes effect).
--- * `item_lot(e)` an item lot about to be rolled: map pickups (all of an area's, as it loads,
---   so also during loading screens), enemy drops (as the enemy dies), and event and boss
---   rewards. `e.lot` (ItemLotParam row); assign it to roll another lot, return false for
---   nothing (a cancelled pickup still shows but gives nothing). The rolled lot's own pickup
---   flag is the one the game sets. The game also gives the rows right after the lot (lot+1,
---   lot+2 ... up to the first missing row; a boss reward and its soul are two such rows), in
---   the same roll: this fires once, for the first. Lots mods award (events.award_item_lot)
---   pass through them too, just before they're given.
---@param event kindle.event
---@param fn function
---@return integer handle
---@overload fun(event: "frame", fn: fun(dt: number)): integer
---@overload fun(event: "player_loaded"|"player_unloaded"|"unload"|"player_died", fn: fun()): integer
---@overload fun(event: "setting_changed", fn: fun(name: string, value: boolean|number|string)): integer
---@overload fun(event: "map_loading", fn: fun(map: string, file: "map"|"events")): integer
---@overload fun(event: "region_changed", fn: fun(new: integer, old: integer)): integer
---@overload fun(event: "character_killed"|"character_spawned", fn: fun(character: kindle.character)): integer
---@overload fun(event: "item_gained"|"item_lost", fn: fun(item: kindle.item, count: integer, name: string?)): integer
---@overload fun(event: "effect_applied"|"effect_expired", fn: fun(id: integer)): integer
---@overload fun(event: "level_up", fn: fun(level: integer, old_level: integer, raised: table<string, integer>, souls: integer)): integer
---@overload fun(event: "equipment_changed", fn: fun(slot: kindle.slot, item: kindle.item?, old: kindle.item?)): integer
---@overload fun(event: "spells_changed", fn: fun(spells: kindle.item[], old: kindle.item[])): integer
---@overload fun(event: "souls_changed", fn: fun(souls: integer, old: integer)): integer
---@overload fun(event: "bonfire_lit", fn: fun(bonfire: kindle.bonfire)): integer
---@overload fun(event: "menu_opened"|"menu_closed", fn: fun(kind: "esc"|"menu")): integer
---@overload fun(event: "ember_changed", fn: fun(embered: boolean)): integer
---@overload fun(event: "key_pressed", fn: fun(key: kindle.key)): integer
---@overload fun(event: "bonfire_rested", fn: fun(bonfire: kindle.bonfire?)): integer
---@overload fun(event: "banner", fn: fun(name: kindle.banner?, type: integer)): integer
---@overload fun(event: "boss_defeated", fn: fun(kind: "heir_of_fire"|"lord_of_cinder")): integer
---@overload fun(event: "boss_bar", fn: fun(shown: boolean, name: string?, entity: integer)): integer
---@overload fun(event: "shop_buy", fn: fun(item: kindle.item, count: integer, row: integer)): integer
---@overload fun(event: "travel", fn: fun(map: string, entity: integer, how: "bonfire"|"script")): integer
---@overload fun(event: "damage", fn: fun(e: kindle.damage_event): boolean?): integer
---@overload fun(event: "animation", fn: fun(e: kindle.animation_event): boolean?): integer
---@overload fun(event: "effect", fn: fun(e: kindle.effect_event): boolean?): integer
---@overload fun(event: "item_used", fn: fun(e: kindle.item_used_event): boolean?): integer
---@overload fun(event: "item_lot", fn: fun(e: kindle.item_lot_event): boolean?): integer
function kindle.on(event, fn) end

---@class kindle.screen_spec
---@field title string
---@field subtitle? string a line under the title
---@field closable? boolean false: Back doesn't close it (the mod does, with screen:close())
---@field on_close? fun() the player backed out of it
---@field rows? kindle.screen_row[] its rows (or tabs)
---@field tabs? kindle.screen_tab[] tabs of rows instead of one list (up to 8; LB/RB, Q/E or a click turns them)
---@field tab? integer the tab it opens on (1-based); an update giving another turns to it
---@field on_tab? fun(index: integer, title: string) the player turned to another tab

---@class kindle.screen_tab
---@field title string
---@field rows kindle.screen_row[]

---@class kindle.screen_row
---@field text? string the row's label (every row but a heading)
---@field header? string a section's heading (not selectable)
---@field help? string shown at the bottom while the row is selected
---@field disabled? boolean greyed; can't be picked or changed
---@field details? {title?: string, text?: string, image?: string|integer} shown in a pane right of the list while the row is selected (as the game's item menus show a picture and description): image a PNG in the mod's folder or the game's icon id; text wraps, "\n" breaks it
---@field icon? string|integer a PNG file in the mod's folder ("art/flask.png"), or the game's icon of that id (an item's, from items.info), drawn left of the text at its height
---@field info? boolean a line of text, not selectable: greyed, or with its value on the right
---@field on_select? fun() a button: runs when picked
---@field screen? kindle.screen_spec|fun(): kindle.screen_spec? a button that opens a sub-screen
---@field toggle? boolean a switch, on or off
---@field choices? string[] one of a list (value: 1-based index or the text)
---@field slider? {min: number, max: number, step?: number} a number in a range
---@field input? string a line of text the player types (Enter to type, Enter to keep it, Esc to leave it as it was; a keyboard's)
---@field max? integer an input's most characters (default 64, up to 256)
---@field key? string a key the player picks by pressing it (Enter, then the key, controller button or mouse button; Esc leaves it): its name as kindle.input.down and key_pressed use ("f6", "pad_y", "mouse_middle"), "" for none
---@field value? any a choice's or slider's value, or the text a button or info line shows on its right
---@field on_change? fun(value: any, text: string?) a switch, choice or slider changed (a choice: index, text), an input's text was kept, or a key row's key was picked (its name)

--- A screen kindle.ui.screen opened: screen:update(spec) changes it in place (the selected row
--- stays), screen:close() closes it and any opened from it (on_close doesn't run), screen:is_open().
---@class kindle.screen
---@field id integer
---@field update fun(self: kindle.screen, spec: kindle.screen_spec)
---@field close fun(self: kindle.screen): boolean
---@field is_open fun(self: kindle.screen): boolean

--- One thing a layer draws (kindle.ui.layer): x and y on the 1920 x 1080 canvas (scaled to the
--- window), or an offset from `at`, and one of text, rect, line, bar, circle or image.
---@class kindle.layer_element
---@field x? number default 0
---@field y? number default 0
---@field at? {x: number, y: number, z: number}|number[]|kindle.character a point in the world: the element is drawn where the camera sees it (x, y an offset from there), hidden behind the camera (not behind walls: it shows through them)
---@field range? number with `at`: hidden further than this from the camera (metres)
---@field color? string|number[] "#rrggbb", "#rrggbbaa" or {r, g, b, a} (0-1); default the game's menu text colour (an image: white, as it is; another colour tints it)
---@field text? string text, top-left at x, y (see align)
---@field size? number text height on the canvas (default 28)
---@field font? "serif"|"sans" default "serif" (the game-like one)
---@field align? "left"|"center"|"right" where x is on the text (default "left")
---@field shadow? boolean a dark shadow under the text (default true)
---@field rect? {w: number, h: number}|number[] a box from x, y
---@field filled? boolean a rect or circle filled (default true) or only outlined
---@field rounding? number a rect's corner radius
---@field thickness? number an outline's or a line's width (default 2)
---@field line? {dx: number, dy: number}|number[] a line from x, y to x + dx, y + dy
---@field bar? {w: number, h: number, value: number}|number[] a bar filled `value` (0-1) of its width with `color`, over `back`
---@field back? string|number[] a bar's empty part (default dark, translucent)
---@field circle? {r: number}|number[] a circle around x, y
---@field image? string|integer a PNG file in the mod's folder ("art/icon.png"), top-left at x, y, read once and again if the file changes; or the game's icon of that id (an item's, from items.info: 512 x 512)
---@field w? number an image's width on the canvas (default its width in pixels)
---@field h? number an image's height on the canvas (default its height in pixels)

--- A layer kindle.ui.layer added: layer:set(elements) replaces what it draws, layer:show(on)
--- shows or hides it, layer:remove() takes it away.
---@class kindle.layer
---@field id integer
---@field set fun(self: kindle.layer, elements: kindle.layer_element[])
---@field show fun(self: kindle.layer, on: boolean?)
---@field remove fun(self: kindle.layer): boolean

--- A hit about to land (kindle.on("damage")). Return false from the handler to cancel it.
---@class kindle.damage_event
---@field target kindle.character? who is hit
---@field attacker kindle.character? who hit them (nil for traps, falls, etc.)
---@field attack_power number attack power before the target's defence; assign to change the hit
---@field attack_id integer AtkParam row of the attack

--- An animation event a character is starting (kindle.on("animation")). Assign name or id to
--- play something else; return false to cancel it.
---@class kindle.animation_event
---@field character kindle.character
---@field name kindle.animation_name|string e.g. "W_Rolling"
---@field id integer

--- An SpEffect about to be applied (kindle.on("effect")). Assign id to apply another effect;
--- return false to block it.
---@class kindle.effect_event
---@field target kindle.character
---@field source kindle.character? who applied it
---@field id integer SpEffectParam row

--- A goods item a player used, as it takes effect (kindle.on("item_used")). Return false to
--- keep the item (it still takes effect).
---@class kindle.item_used_event
---@field player kindle.character
---@field item kindle.item
---@field name string? the item's name

--- An item lot about to be rolled (kindle.on("item_lot")): a map pickup (as its area loads),
--- an enemy drop (as the enemy dies), or an event or boss reward. Assign lot to roll another;
--- return false for nothing.
---@class kindle.item_lot_event
---@field lot integer ItemLotParam row

--- A readable string for any value: tables show their contents, lists as { 1, 2, 3 } and
--- others as { key = value } with sorted keys, down to `depth` levels (default 3). print and
--- kindle.log use it for tables too.
---@param value any
---@param depth? integer
---@return string
function kindle.inspect(value, depth) end

--- Run `fn` once after `seconds`.
---@param seconds number
---@param fn fun()
---@return integer handle
function kindle.after(seconds, fn) end

--- Run `fn` every `seconds`.
---@param seconds number
---@param fn fun()
---@return integer handle
function kindle.every(seconds, fn) end

--- Cancel an event handler, timer or flag watcher.
---@param handle integer
---@return boolean removed
function kindle.cancel(handle) end

--- Seconds since Kindle started.
---@return number
function kindle.time() end

--- Seconds since 1970-01-01 00:00 UTC, from the computer's clock (kindle.time is since Kindle
--- started). For daily seeds, and dates and times that last across sessions.
---@return number
function kindle.clock() end

--- The calendar date and time of `seconds` (a kindle.clock() value; default now), in the
--- player's local time, or UTC with `utc` true.
--- `local d = kindle.date(); local seed = d.year * 10000 + d.month * 100 + d.day`
---@param seconds? number
---@param utc? boolean
---@return {year: integer, month: integer, day: integer, hour: integer, minute: integer, second: integer, weekday: integer, yearday: integer} weekday 1 Monday .. 7 Sunday
function kindle.date(seconds, utc) end

--- A file of the mod's own folder: its bytes as a Lua string (text for a text file). `path` is
--- relative to the folder, with / between folders; up to 16 MB. For data a mod ships:
--- `local drops = kindle.json.decode(kindle.read_data("data/drops.json"))`
---@param path string
---@return string
function kindle.read_data(path) end

--- Whether a DLC is installed: 1 Ashes of Ariandel, 2 The Ringed City (nil before the game knows).
---@param dlc integer
---@return boolean?
function kindle.dlc_installed(dlc) end

--- Frames since Kindle started.
---@return integer
function kindle.frame() end

--- Offer functions to other mods: kindle.export{ greet = function(name) ... end }. Calls
--- from other mods run in this mod; arguments and results are copied between the mods
--- (plain data: nil, booleans, numbers, strings, tables). Calling it again replaces the
--- exports.
---@param functions table<string, function>
function kindle.export(functions) end

--- Send a message to every other running mod, delivered on the next frame to their
--- kindle.on_message(name, fn) handlers. `data` is plain data (as for kindle.store).
---@param name string
---@param data? kindle.storable
function kindle.send(name, data) end

--- Handle messages other mods send with kindle.send(name, data).
---@param name string
---@param fn fun(data: any, from: string) from is the sending mod's id
---@return integer handle
function kindle.on_message(name, fn) end

--- State that survives the mod being reloaded (saving its files reloads it), for this
--- session: the value kept under `key` when the mod last unloaded, else `default`. Kindle
--- keeps the value as the mod unloads, so change a kept table in place:
--- `local state = kindle.keep("state", { kills = 0 })`, then `state.kills = state.kills + 1`.
--- Plain data only (as for kindle.store). For values that last between sessions use
--- kindle.store.
---@generic T
---@param key string
---@param default T
---@return T
function kindle.keep(key, default) end

---@class kindle.settings  The mod's settings: options declared in mod.toml that players change
--- in the overlay's Mods tab. Always available. Declare each as a table:
---
--- ```toml
--- [settings.chance]
--- type = "float"            # "bool", "int", "float" or "choice"
--- label = "Effect chance"   # shown in the overlay (default: the name)
--- description = "How often fate strikes."   # tooltip
--- default = 0.5
--- min = 0.0                 # int/float: range (sliders when both are set)
--- max = 1.0
--- # choice: choices = ["easy", "normal", "hard"]
--- ```
---
--- Values are saved for the mod (all characters). kindle.on("setting_changed",
--- fn(name, value)) runs when the player changes one.
kindle.settings = {}
--- A setting's current value: the player's choice, or the default.
---@param name string
---@return boolean|number|string
function kindle.settings.get(name) end
--- Change a setting from the mod (checked against its declaration; ranges clamp).
---@param name string
---@param value boolean|number|string
function kindle.settings.set(name, value) end
--- The declared settings' names, in mod.toml order.
---@return string[]
function kindle.settings.names() end

---@class kindle.mods  Other mods: their exports and who's running. Always available.
kindle.mods = {}
--- Another mod's exported functions (see kindle.export), or nil if that mod isn't running.
--- List the mod under `depends` in mod.toml so it's loaded first.
--- Example: local fate = kindle.mods.get("bonfire-fate"); if fate then fate.reroll() end
---@param id string
---@return table<string, function>?
function kindle.mods.get(id) end
--- Ids of the running mods.
---@return string[]
function kindle.mods.running() end

--- Optional: assign a function here to run before the mod is reloaded/unloaded.
---@type fun()?
kindle.on_unload = nil

---@class kindle.json  JSON text: for data a mod ships (kindle.read_data) or keeps as text.
kindle.json = {}
--- A value as JSON text (`pretty`: indented on several lines). A table whose keys are 1..n is an
--- array, any other an object (number keys become text); an empty table is `[]`.
---@param value any
---@param pretty? boolean
---@return string
function kindle.json.encode(value, pretty) end
--- JSON text as a value: objects and arrays as tables, null as nil. An error if it isn't JSON.
---@param text string
---@return any
function kindle.json.decode(text) end

---@class kindle.log  Write to Kindle's log (the overlay's Log tab and logs\kindle.log). Always available.
kindle.log = {}
--- Log with string.format-style arguments, e.g. log.info("hp=%d", hp).
---@param fmt any
---@param ... any
function kindle.log.debug(fmt, ...) end
--- Log at info level (arguments as for log.debug).
---@param fmt any
---@param ... any
function kindle.log.info(fmt, ...) end
--- Log a warning (arguments as for log.debug).
---@param fmt any
---@param ... any
function kindle.log.warn(fmt, ...) end
--- Log an error (arguments as for log.debug). The mod keeps running.
---@param fmt any
---@param ... any
function kindle.log.error(fmt, ...) end

---@alias kindle.storable nil|boolean|number|string|table A value kindle.store can keep: tables must be lists or have string keys (nested up to 32 levels).

---@class kindle.store  Values a mod keeps between sessions. Always available.
--- `get`/`set` keep values for the loaded character (saved per save slot; a new character
--- in the slot starts empty); `get_global`/`set_global` keep values for the mod itself, the
--- same for every character (settings). Kindle writes them to disk (kindle\data\<mod id>)
--- about once a second and when the mod unloads or the character leaves the world.
kindle.store = {}
--- A value saved for the loaded character, or `default` if it was never set. Errors if no
--- character is loaded.
---@param key string
---@param default? any
---@return any
function kindle.store.get(key, default) end
--- Save a value for the loaded character (nil removes it).
---@param key string
---@param value kindle.storable
function kindle.store.set(key, value) end
--- The keys saved for the loaded character.
---@return string[]
function kindle.store.keys() end
--- A value saved for the mod itself, or `default` if it was never set.
---@param key string
---@param default? any
---@return any
function kindle.store.get_global(key, default) end
--- Save a value for the mod itself (nil removes it).
---@param key string
---@param value kindle.storable
function kindle.store.set_global(key, value) end
--- The keys saved for the mod itself.
---@return string[]
function kindle.store.global_keys() end

---@class kindle.flags  (permission: "flags") Read, set and watch the game's event flags (bosses killed, doors opened, NPC progress).
kindle.flags = {}
--- Read an event flag. set: "own" (the character's own flags) or "session" (the copy a player's
--- game uses in a co-op session, the host's world's); nil for the set in use. Another set than the
--- one in use is an error when it has no table for that flag now.
---@param id integer
---@param set? "own"|"session"
---@return boolean
function kindle.flags.get(id, set) end
--- Set an event flag (default true). Returns whether the flag exists. set as for get.
---@param id integer
---@param state? boolean
---@param set? "own"|"session"
---@return boolean
function kindle.flags.set(id, state, set) end
--- The set of flags in use: "own", or "session" (a player in a co-op session).
---@return "own"|"session"
function kindle.flags.set_in_use() end
--- A set's block of 1000 flags from `first` (a multiple of 1000) as 32 words, the flags' bits from
--- the highest bit of the first word; nil where the set has no such block now. Its values only.
---@param first integer
---@param set "own"|"session"
---@return integer[]?
function kindle.flags.block(first, set) end
--- Writes a set's block of 1000 flags from `first` (32 words, as flags.block gives them). On the
--- game's thread (in a handler or the frame), as the game's own flag writes.
---@param first integer
---@param words integer[]
---@param set "own"|"session"
function kindle.flags.set_block(first, words, set) end
--- Call `fn(id)` when the flag turns on. Edge-triggered, only while a character is loaded.
---@param id integer
---@param fn fun(id: integer)
---@return integer handle
function kindle.flags.on_set(id, fn) end
--- Call `fn(id)` when the flag turns off.
---@param id integer
---@param fn fun(id: integer)
---@return integer handle
function kindle.flags.on_clear(id, fn) end
--- Boss defeat flags by name (set when the boss dies), e.g.
--- `kindle.flags.on_set(kindle.flags.boss.iudex_gundyr, fn)`. Taken from the game's own event
--- scripts.
---@type kindle.boss_flags
kindle.flags.boss = {}
--- Each bonfire's lit flag by name, e.g. `kindle.flags.bonfire.firelink_shrine`. Read from the
--- game's bonfire list; the names are in the game stubs Kindle writes (kindle-game.lua).
---@type kindle.bonfire_flags
kindle.flags.bonfire = {}

---@class kindle.boss_flags
---@field iudex_gundyr integer 14000800: Iudex Gundyr
---@field vordt_of_the_boreal_valley integer 13000800: Vordt of the Boreal Valley
---@field curse_rotted_greatwood integer 13100800: Curse-rotted Greatwood
---@field crystal_sage integer 13300850: Crystal Sage
---@field abyss_watchers integer 13300800: Abyss Watchers
---@field deacons_of_the_deep integer 13500800: Deacons of the Deep
---@field high_lord_wolnir integer 13800800: High Lord Wolnir
---@field old_demon_king integer 13800830: Old Demon King
---@field pontiff_sulyvahn integer 13700850: Pontiff Sulyvahn
---@field aldrich_devourer_of_gods integer 13700800: Aldrich, Devourer of Gods
---@field yhorm_the_giant integer 13900800: Yhorm the Giant
---@field dancer_of_the_boreal_valley integer 13000890: Dancer of the Boreal Valley
---@field oceiros_the_consumed_king integer 13000830: Oceiros, the Consumed King
---@field dragonslayer_armour integer 13010800: Dragonslayer Armour
---@field twin_princes integer 13410830: Lorian, Elder Prince and Lothric, Younger Prince
---@field champion_gundyr integer 14000830: Champion Gundyr
---@field ancient_wyvern integer 13200800: Ancient Wyvern
---@field nameless_king integer 13200850: Nameless King
---@field soul_of_cinder integer 14100800: Soul of Cinder
---@field sister_friede integer 14500800: Sister Friede (Ashes of Ariandel)
---@field champions_gravetender integer 14500860: Champion's Gravetender (Ashes of Ariandel)
---@field demon_prince integer 15000800: Demon Prince (The Ringed City)
---@field halflight integer 15100800: Halflight, Spear of the Church (The Ringed City)
---@field darkeater_midir integer 15100850: Darkeater Midir (The Ringed City)
---@field slave_knight_gael integer 15110800: Slave Knight Gael (The Ringed City)

--- Bonfire lit flags; Kindle adds a field per bonfire in kindle-game.lua.
---@class kindle.bonfire_flags

---@alias kindle.item integer An encoded item id (category + param id). Build with items.goods() etc.
---@alias kindle.itemref kindle.item|kindle.item_name|string An item, or its name in the game's language ("Ember", "Shortsword +5").

---@class kindle.items  (permission: "items") Find items by id or name; give, take and count them; rename them.
kindle.items = {}
--- A weapon by EquipParamWeapon row id or by name.
---@param id_or_name integer|kindle.weapon_name|string
---@return kindle.item
function kindle.items.weapon(id_or_name) end
--- Armor by EquipParamProtector row id or by name.
---@param id_or_name integer|kindle.armor_name|string
---@return kindle.item
function kindle.items.armor(id_or_name) end
--- A ring by EquipParamAccessory row id or by name.
---@param id_or_name integer|kindle.ring_name|string
---@return kindle.item
function kindle.items.ring(id_or_name) end
--- Goods (consumables, key items, spells) by EquipParamGoods row id or by name.
---@param id_or_name integer|kindle.goods_name|string
---@return kindle.item
function kindle.items.goods(id_or_name) end
--- The item with this exact name in any category. Errors if unknown or ambiguous
--- (then use items.weapon/armor/ring/goods(name) to pick the category).
---@param name kindle.item_name|string
---@return kindle.item
function kindle.items.id(name) end
--- The item's name in the game's language, or nil.
---@param item kindle.itemref
---@return string?
function kindle.items.name(item) end
--- Search item names (case-insensitive substring), exact matches first.
---@param query string
---@param limit? integer default 25
---@return {name: string, category: "weapon"|"armor"|"ring"|"goods", id: integer, item: kindle.item}[]
function kindle.items.find(query, limit) end
--- Give an item. Shows the game's "item received" box unless opts.popup = false.
---@param item kindle.itemref
---@param quantity? integer default 1
---@param opts? {popup: boolean}
function kindle.items.give(item, quantity, opts) end
--- Take an item from the player's inventory.
---@param item kindle.itemref
---@param quantity? integer default 1
function kindle.items.remove(item, quantity) end
--- How many the player carries.
---@param item kindle.itemref
---@return integer
function kindle.items.count(item) end
--- Whether the player carries at least `quantity` of an item.
---@param item kindle.itemref
---@param quantity? integer default 1
---@return boolean
function kindle.items.has(item, quantity) end
--- An item's category, param row id, name and icon (the game's icon id: a layer's image or a
--- screen row's icon shows it).
---@param item kindle.itemref
---@return "weapon"|"armor"|"ring"|"goods" category
---@return integer param_id
---@return string? name
---@return integer? icon
function kindle.items.info(item) end
--- Rename an item everywhere the game shows it. Needs the "text" permission too.
--- Reverted when the mod unloads; the new name also works in items.give("New Name").
---@param item kindle.itemref
---@param name string
function kindle.items.set_name(item, name) end
--- Replace an item's long description (needs "text").
---@param item kindle.itemref
---@param text string
function kindle.items.set_description(item, text) end
--- Replace an item's short effect line (needs "text").
---@param item kindle.itemref
---@param text string
function kindle.items.set_info(item, text) end
--- Restore an item's original name, description and info (needs "text").
---@param item kindle.itemref
function kindle.items.reset_text(item) end

---@alias kindle.text_category "goods_name"|"weapon_name"|"armor_name"|"ring_name"|"spell_name"|"npc_name"|"place_name"|"goods_info"|"weapon_info"|"armor_info"|"ring_info"|"goods_caption"|"weapon_caption"|"armor_caption"|"ring_caption"|"spell_info"|"spell_caption"|"talk"|"event_text"|integer

---@class kindle.text  (permission: "text") Read and replace any of the game's text, live.
---@field categories string[] named categories (DLC tables are searched automatically)
kindle.text = {}
--- The text the game shows for (category, id), or nil.
---@param category kindle.text_category name, or a raw FMG id
---@param id integer
---@return string?
function kindle.text.get(category, id) end
--- Replace that text everywhere the game shows it. Gives the id an entry if the game has none (a new bonfire's name, say).
--- Reverted when the mod unloads.
---@param category kindle.text_category
---@param id integer
---@param value string
---@return integer changed number of tables changed
function kindle.text.set(category, id, value) end
--- Restore the original text.
---@param category kindle.text_category
---@param id integer
function kindle.text.reset(category, id) end
--- An item's "name" (default), "info" or "description" text as currently shown.
---@param item kindle.itemref
---@param field? "name"|"info"|"description"
---@return string?
function kindle.text.item(item, field) end

---@class kindle.ui  (permission: "ui") On-screen messages, drawn even when the overlay is closed.
kindle.ui = {}
--- Show text in the game's own "OK" message dialog (up to ~1000 characters). Does nothing
--- while some game menus are open.
---@param text string
function kindle.ui.message(text) end

---@class kindle.menu  A game list menu being built.
---@field entries {slot: integer, text_id: integer, text: string?}[]
---@field bonfire boolean

--- Add an option to the game's own list menus (NPC and bonfire menus). Picking it closes the
--- menu (as if backed out) and runs on_select, or opens `screen` (a kindle.ui.screen spec, or a
--- function returning one, built when it's picked).
---@param opts {text: string, on_select?: fun(), screen?: kindle.screen_spec|fun(): kindle.screen_spec?, where?: "any"|"bonfire"|"npc"|fun(menu: kindle.menu): boolean}
---@return integer handle  kindle.cancel(handle) removes it
function kindle.ui.add_menu_option(opts) end

--- From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen: a shop,
--- reinforcing, levelling up ...): show a list in the game's own menu. An entry's on_select can open another menu or a shop; one that opens
--- nothing (or backing out) returns to the bonfire's or NPC's own menu. Up to 64 entries.
--- Picking an entry closes and re-opens the list (the game's menus always close on a pick).
---@param entries {text: string, on_select?: fun()}[]
function kindle.ui.menu(entries) end

--- From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open
--- the game's shop over ShopLineupParam rows first..last (inclusive), e.g. the Shrine Handmaid's
--- 110000-119899. Closing it returns to the menu it was opened from.
---@param first integer
---@param last integer
function kindle.ui.shop(first, last) end

---@alias kindle.screen "level_up"|"attune"|"storage"|"travel"|"reinforce"|"infuse"|"repair"|"estus"

--- From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open one
--- of the game's own screens: level up, attune spells, the storage box, travel (bonfire warp), reinforce
--- weapon, infuse weapon, repair equipment or estus allotment. Closing it returns to the menu
--- it was opened from.
--- `kindle.ui.add_menu_option{text = "Level Up", on_select = function() kindle.ui.open("level_up") end}`
---@param screen kindle.screen
function kindle.ui.open(screen) end
--- Change a menu option's text (the handle add_menu_option returned). Menus built after this
--- show the new text: set it from the option's `where` function to show state, e.g. a price
--- only until something is bought.
---@param handle integer
---@param text string
function kindle.ui.set_menu_option_text(handle, text) end
---@alias kindle.color "gold"|"white"|"red"|"blue"|"green"|string  string = "#rrggbb"
--- Big centred text on a dark band, like the game's own banners.
---@param message string
---@param opts? {seconds?: number, color?: kindle.color}  defaults: 3 seconds, gold
function kindle.ui.banner(message, opts) end
--- The game's own banner across the screen, as a place's name shows on arriving, with this text
--- (the game's font: no colon).
---@param text string
function kindle.ui.place_banner(text) end
--- Hides one of the game's banners (a name as ui.game_banner takes, or its type) for some seconds:
--- a mod playing the bonfire lighting's animation hides the "BONFIRE LIT" it shows.
---@param kind string|integer
---@param seconds number
function kindle.ui.hide_game_banner(kind, seconds) end
--- An action prompt at the bottom of the screen, as the game's own ("Rest at bonfire"), until
--- cleared (nil). Each mod has its own; when two have one, the one set last shows.
---@param text string?
function kindle.ui.prompt(text) end
--- A line across the top of the screen that stays until cleared (nil), for a state the player has
--- to see the whole time (hidden while a menu is open or the HUD is off), with a progress bar under it if `progress`
--- (0 to 1) is given. Each mod has its own line: another mod's status shows above or below it (in
--- the order they were first set), and
--- setting it again changes this mod's line only. It goes when the mod unloads or stops.
---@param text string?
---@param progress? number
function kindle.ui.status(text, progress) end
--- A screen of this mod's own: full-screen, in the game's menu style (as the Mods screen), worked
--- with the controller, the keyboard or the mouse; the game's input is the screen's while it's
--- open. Rows are buttons (on_select; `value` shown on the right; `screen` opens a sub-screen that
--- Back returns from), switches (`toggle`), choices (`choices` + `value`, 1-based or the text),
--- sliders (`slider = {min, max, step}` + `value`), lines of text (`info = true`: greyed, or with
--- a `value` shown on the right, as a stat; not selectable), text the player types (`input`, with a
--- keyboard) and headings
--- (`header`). A row's `help` shows at the bottom while it's selected; `disabled` greys it and
--- stops it changing. A switch's, choice's or slider's on_change gets the new value (a choice its
--- index and text), and the row keeps it. Up to 200 rows (the list scrolls). Screens opened from
--- a screen stack: Back closes the top one (unless `closable = false`), and on_close runs.
--- `kindle.ui.screen{title = "Hard Mode", rows = {{text = "Enemy damage", slider = {min = 1, max = 3, step = 0.5}, value = 1.5, on_change = function(v) damage = v end}}}`
--- Tabs instead of rows put the rows on pages the player turns with LB/RB, Q/E or a click, as
--- the game's own menus: `kindle.ui.screen{title = "Options", tabs = {{title = "Enemies", rows =
--- {...}}, {title = "Player", rows = {...}}}}` (200 rows across them).
---@param spec kindle.screen_spec
---@return kindle.screen
function kindle.ui.screen(spec) end
--- A layer of this mod's drawn over the game: text, boxes, lines, bars, circles and pictures (PNG
--- files in the mod's folder, up to 4096 pixels a side, and the game's own icons) on a 1920 x
--- 1080 canvas scaled to the window (a wider window keeps it centred), or pinned to a point in
--- the world with `at` (a marker over a bonfire, a bar over an enemy) and drawn where the camera
--- sees it. A layer stays until it's changed: set it when its numbers change, or every frame for
--- something that moves. Layers are drawn under Kindle's menus and screens, in the order they
--- were added, and hidden on loading screens; they go when the mod unloads. Up to 500 elements.
--- `local hud = kindle.ui.layer{{text = "Souls: 0", x = 1880, y = 40, align = "right"}}`
--- `hud:set{{bar = {w = 300, h = 10, value = 0.5}, x = 810, y = 1000, color = "#c08030"}}`
--- `kindle.ui.layer{{image = "art/crest.png", x = 40, y = 40, w = 96, h = 96}}`
--- `local _, _, _, icon = kindle.items.info("Ember"); kindle.ui.layer{{image = icon, x = 40, y = 40, w = 64, h = 64}}`
---@param elements? kindle.layer_element[]
---@return kindle.layer
function kindle.ui.layer(elements) end
--- A question with two answers, on a screen of this mod's (kindle.ui.screen): the answer picked
--- runs its function and closes it; backing out is no.
--- `kindle.ui.confirm{title = "Reset?", text = "Your run's numbers go back to 0.", on_yes = reset}`
---@param opts {title?: string, text?: string, yes?: string, no?: string, on_yes?: fun(), on_no?: fun()}
---@return kindle.screen
function kindle.ui.confirm(opts) end
--- One of a list, on a screen of this mod's (kindle.ui.screen). An option is a text, or {text,
--- help, value, disabled} (value shown on the right). Picking one closes it and runs
--- on_pick(index, option); backing out runs on_cancel.
--- `kindle.ui.choose{title = "Difficulty", options = {"Easy", "Normal", "Hard"}, on_pick = function(i, name) level = i end}`
---@param opts {title?: string, text?: string, options: (string|{text: string, help?: string, value?: any, disabled?: boolean})[], on_pick?: fun(index: integer, option: any), on_cancel?: fun()}
---@return kindle.screen
function kindle.ui.choose(opts) end
--- This mod's menu of buttons, opened from the game's Mods screen (Esc -> Mods, and at the
--- title): its Mod menus tab lists each mod with a menu or settings, and picking one opens that
--- mod's menu over it, these buttons and the mod's settings (rows past what the screen shows go
--- on a More row). Each button: a label (48
--- characters), its help line, and on_press, run at Kindle's next frame. A paste button's on_press
--- gets the clipboard's text, read as the player pressed it (a mod reads the clipboard no other
--- way). A row with info = true is a line of information, shown greyed (no on_press). 1 to 16
--- rows; replaces this mod's earlier tab (the same rows again change nothing; this mod's menu,
--- open in play, closes and opens again with the new rows, at the page it was on); gone when the
--- mod unloads.
---@param title string 1 to 24 characters
---@param buttons {label: string, help?: string, paste?: boolean, info?: boolean, on_press?: fun(pasted: string?)}[]
function kindle.ui.mod_tab(title, buttons) end
--- An entry of this mod's on the title's main menu, after Mods, opening this mod's menu
--- (ui.mod_tab and its settings), or, given `open`, a screen of the mod's own over the menu (a
--- kindle.ui.screen spec, or a function run when it's picked, which can return one). Shown the
--- next time the main menu is built; two mods' entries at most. nil removes it; gone when the
--- mod unloads.
---@param label string? 1 to 24 characters
---@param help? string at most 120 characters
---@param open? kindle.screen_spec|fun(): kindle.screen_spec?
function kindle.ui.title_entry(label, help, open) end
--- This mod's rows on the character creation screen, under the game's own (two places shared by
--- all mods). A page row opens the System screen with the page's tabs of switches (1 to 4 tabs,
--- each 1 to 24 switches); a text row opens the game's text box, and a toast shows what was
--- typed (`empty` when nothing). Read what the player chose with ui.character_choices when the
--- mod sees the new character. Replaces this mod's rows; gone when the mod unloads.
--- Example: ui.character_rows{{label = "Hard mode", help = "...", page = {{title = "Hard
--- mode", icon = "skulls", switches = {{key = "on", label = "On", help = "...", default = false}}}}}}
---@param rows {label: string, help?: string, page?: {title: string, icon?: "candelabra"|"bell"|"skulls"|"equipment"|"shrine", switches: {key: string, label: string, help?: string, default?: boolean}[]}[], text?: {key: string, empty?: string}}[]
function kindle.ui.character_rows(rows) end
--- What this mod's creation screen rows hold now, for the character being made: {key = value},
--- a switch true or false, a text row its text ("" when nothing was typed). Defaults where the
--- player changed nothing.
---@return table<string, boolean|string>
function kindle.ui.character_choices() end
--- This mod's creation screen rows back to their defaults (once it has taken the choices for a
--- character, so the next one starts afresh).
function kindle.ui.reset_character_choices() end
--- Puts text on the clipboard (1024 characters at most), as for a code to send someone; false if it
--- couldn't.
---@param text string
---@return boolean
function kindle.ui.copy(text) end
--- A small notification in the top-right corner (up to 6 stack).
---@param message string
---@param opts? {seconds?: number, color?: kindle.color}  defaults: 4 seconds, gold
function kindle.ui.toast(message, opts) end
--- Change the HUD setting (Settings > HUD), as the settings menu does: menus keep working.
--- The player's own setting comes back when the mod unloads.
---@param mode "off"|"on"|"auto" auto shows the HUD when something changes
function kindle.ui.set_hud(mode) end
--- The current HUD setting.
---@return "off"|"on"|"auto"
function kindle.ui.hud() end
--- One of the game's own banners, drawn by the game exactly like its own, by name or type.
--- A mod's own banners don't fire the banner / boss_defeated events.
---@param banner kindle.banner|integer a name, or a type 1-22 (3 and 8-11 show nothing)
function kindle.ui.game_banner(banner) end

--- The game's banners: 1 heir_of_fire_destroyed, 2 you_died (also 6), 4 souls_retrieved,
--- 5 host_of_embers_destroyed, 7 dark_spirit_destroyed, 12 begin_match, 13 bonfire_lit,
--- 14 draw, 15 you_win, 16 you_lose, 17 ember_restored (also 18), 19 duty_fulfilled (also
--- 20), 21 hollowing_reversed, 22 lord_of_cinder_fallen.
---@alias kindle.banner "heir_of_fire_destroyed"|"you_died"|"souls_retrieved"|"host_of_embers_destroyed"|"dark_spirit_destroyed"|"begin_match"|"bonfire_lit"|"draw"|"you_win"|"you_lose"|"ember_restored"|"duty_fulfilled"|"hollowing_reversed"|"lord_of_cinder_fallen"

---@class kindle.player  (permission: "player") The player: HP/FP/stamina, stats, effects, equipment, position, souls, animations.
kindle.player = {}
--- Whether a character is in the world (the loading screen is over). Most player
--- functions need one.
---@return boolean
function kindle.player.loaded() end
--- The character's name.
---@return string
function kindle.player.name() end
--- The character's nine attributes.
---@return {vigor: integer, attunement: integer, endurance: integer, vitality: integer, strength: integer, dexterity: integer, intelligence: integer, faith: integer, luck: integer}
function kindle.player.stats() end
--- Current HP.
---@return integer
function kindle.player.hp() end
--- Maximum HP.
---@return integer
function kindle.player.max_hp() end
--- Set HP.
---@param value integer clamped to 0..max_hp
function kindle.player.set_hp(value) end
--- Current FP.
---@return integer
function kindle.player.fp() end
--- Maximum FP.
---@return integer
function kindle.player.max_fp() end
--- Set FP.
---@param value integer
function kindle.player.set_fp(value) end
--- Current stamina.
---@return integer
function kindle.player.stamina() end
--- Maximum stamina.
---@return integer
function kindle.player.max_stamina() end
--- Set stamina.
---@param value integer
function kindle.player.set_stamina(value) end
--- Restore HP and FP to max.
function kindle.player.heal() end
--- Apply an SpEffect (SpEffectParam row id) to the player: buffs, debuffs, states.
--- Errors if the id doesn't exist. Tip: an item's effect is often its EquipParamGoods
--- refId_default (when refCategory is 2).
---@param id integer
function kindle.player.apply_effect(id) end
--- End an active SpEffect (the game removes it on its next update).
---@param id integer
---@return integer ended number of instances ended
function kindle.player.remove_effect(id) end
--- The player's active SpEffects. duration -1 = permanent.
---@return {id: integer, time_left: number, duration: number}[]
function kindle.player.effects() end
--- Whether an SpEffect is currently active on the player.
---@param id integer
---@return boolean
function kindle.player.has_effect(id) end
--- The player's position (y is up).
---@return number x, number y, number z
function kindle.player.position() end
--- The game camera: its position (`x`, `y`, `z`), where it looks (`dx`, `dy`, `dz`, unit
--- length), `yaw` (radians, like `world.facing`: a character with this facing looks the way
--- the camera does) and `fov` (radians). Out of view, 3 m behind the camera:
--- `local c = kindle.player.camera(); local _, y = kindle.player.position()`
--- `kindle.world.spawn{npc_param = 110000, x = c.x - c.dx * 3, y = y, z = c.z - c.dz * 3}`
---@return {x: number, y: number, z: number, dx: number, dy: number, dz: number, yaw: number, fov: number}
function kindle.player.camera() end
--- The character the player is locked on to, as world.characters gives it, or nil when the
--- player isn't locked on. Example, a mod's own target marker: each frame, `local t =
--- kindle.player.lock_target()` and draw at t.x, t.y, t.z.
---@return kindle.character?
function kindle.player.lock_target() end
--- Locks the player on to a character, as the lock-on button and a target switch do (the camera
--- turns to it), or releases the lock (nil). Only a character the player could lock on to now
--- (in range and in view) can be chosen: false otherwise, and the lock stays as it was. Example,
--- lock on to the nearest character: `kindle.player.set_lock_target(kindle.world.characters{radius = 20}[1])`.
---@param target kindle.character?
---@return boolean
function kindle.player.set_lock_target(target) end
--- Hold the game's camera: at x, y, z, looking along dx, dy, dz (any length) or at the point
--- `look_at`, with `fov` (radians; default the game's). It stays there, every frame, until the
--- mod calls it again (a camera that moves) or with nil, which gives the game its camera back
--- (as the mod unloading does). The character still moves with the player's input
--- (kindle.input.block_actions stops that). One mod holds the camera at a time. Returns whether
--- it held it (nil: whether this mod had it).
--- `local c = kindle.player.camera()`
--- `kindle.player.set_camera{x = c.x, y = c.y + 10, z = c.z, look_at = {kindle.player.position()}}`
---@param cam? {x: number, y: number, z: number, dx?: number, dy?: number, dz?: number, look_at?: {x: number, y: number, z: number}, fov?: number}
---@return boolean
function kindle.player.set_camera(cam) end
--- Move the player.
---@param x number
---@param y number up
---@param z number
function kindle.player.teleport(x, y, z) end
--- Facing angle in radians.
---@return number?
function kindle.player.facing() end
--- Turn the player to face an angle (radians).
---@param radians number
function kindle.player.set_facing(radians) end
--- Whether the player is invulnerable.
---@return boolean?
function kindle.player.invulnerable() end
--- Make the player invulnerable (or not).
---@param on? boolean default true
function kindle.player.set_invulnerable(on) end
--- false disables gravity (the player floats).
---@param on boolean
function kindle.player.set_gravity(on) end
--- Play an animation event by name (case-insensitive, e.g. "W_ItemDrink") or id.
---@param anim kindle.animation_name|string|integer
function kindle.player.play_animation(anim) end
--- Every animation event the player's behaviour accepts.
---@return table<integer, string>
function kindle.player.animations() end
--- Souls carried.
---@return integer
function kindle.player.souls() end
--- Clamped to 0..999,999,999.
---@param souls integer
function kindle.player.set_souls(souls) end
--- Add souls; a negative number removes them (down to 0).
---@param n integer
function kindle.player.add_souls(n) end
--- Soul level.
---@return integer
function kindle.player.soul_level() end
--- Souls gained over the whole playthrough.
---@return integer
function kindle.player.total_souls() end
--- Whether the player is embered.
---@return boolean
function kindle.player.embered() end
--- true goes through the game's own transition (SpEffect 3290); false clears the state
--- (experimental: the HUD may not update until the next load).
---@param on boolean
function kindle.player.set_embered(on) end
--- Estus / Ashen Estus charges and allotments.
---@return {hp: integer, hp_max: integer, fp: integer, fp_max: integer}
function kindle.player.estus() end
--- Set charges; nil leaves one unchanged. Clamped to the allotment.
---@param hp integer?
---@param fp integer?
function kindle.player.set_estus(hp, fp) end
--- Refill both estus flasks to their allotment.
function kindle.player.refill_estus() end

---@alias kindle.attribute "vigor"|"attunement"|"endurance"|"vitality"|"strength"|"dexterity"|"intelligence"|"faith"|"luck"
--- Set an attribute (1-99). Derived stats (max HP/FP/stamina, equip load, ...) update by
--- themselves. Soul level moves by the same amount unless keep_level.
---@param name kindle.attribute
---@param value integer
---@param keep_level? boolean
function kindle.player.set_stat(name, value, keep_level) end
--- Set several attributes: player.set_stats({vigor = 20, strength = 18})
---@param stats table<kindle.attribute, integer>
---@param keep_level? boolean
function kindle.player.set_stats(stats, keep_level) end
--- Set the soul level number. Attributes are unchanged (see set_stat).
---@param level integer 1-802
function kindle.player.set_soul_level(level) end

---@alias kindle.key "esc"|"1"|"2"|"3"|"4"|"5"|"6"|"7"|"8"|"9"|"0"|"minus"|"equals"|"backspace"|"tab"|"q"|"w"|"e"|"r"|"t"|"y"|"u"|"i"|"o"|"p"|"lbracket"|"rbracket"|"enter"|"ctrl"|"a"|"s"|"d"|"f"|"g"|"h"|"j"|"k"|"l"|"semicolon"|"apostrophe"|"grave"|"shift"|"backslash"|"z"|"x"|"c"|"v"|"b"|"n"|"m"|"comma"|"period"|"slash"|"rshift"|"alt"|"space"|"capslock"|"f1"|"f2"|"f3"|"f4"|"f5"|"f6"|"f7"|"f8"|"f9"|"f10"|"f11"|"f12"|"numlock"|"scrolllock"|"num0"|"num1"|"num2"|"num3"|"num4"|"num5"|"num6"|"num7"|"num8"|"num9"|"num_multiply"|"num_minus"|"num_plus"|"num_period"|"num_enter"|"num_divide"|"rctrl"|"ralt"|"home"|"pageup"|"end"|"pagedown"|"insert"|"delete"|"up"|"left"|"right"|"down"|"pad_up"|"pad_down"|"pad_left"|"pad_right"|"pad_start"|"pad_back"|"pad_ls"|"pad_rs"|"pad_lb"|"pad_rb"|"pad_a"|"pad_b"|"pad_x"|"pad_y"|"mouse_left"|"mouse_right"|"mouse_middle"|"mouse_4"|"mouse_5"
---@alias kindle.slot "left1"|"left2"|"left3"|"right1"|"right2"|"right3"|"arrow1"|"arrow2"|"bolt1"|"bolt2"|"head"|"body"|"arms"|"legs"|"ring1"|"ring2"|"ring3"|"ring4"|"covenant"|"quick1"|"quick2"|"quick3"|"quick4"|"quick5"|"quick6"|"quick7"|"quick8"|"quick9"|"quick10"|"tool1"|"tool2"|"tool3"|"tool4"|"tool5"
--- Equipped items by slot (item ids; empty slots are absent).
---@return table<kindle.slot, integer>
function kindle.player.equipment() end
--- Equip an item from the inventory, through the game's own equip path. The item must be
--- in the inventory (items.give it first) and fit the slot.
---@param slot kindle.slot
---@param item integer|string
function kindle.player.equip(slot, item) end
--- Empty a slot (weapons fall back to Fists, armour to the bare piece).
---@param slot kindle.slot
function kindle.player.unequip(slot) end
--- Attuned spells (item ids), in slot order.
---@return integer[]
function kindle.player.spells() end
--- 0 on the first playthrough, 1 in NG+, ...
---@return integer
function kindle.player.ng_cycle() end
--- How many times the character has died.
---@return integer
function kindle.player.deaths() end
--- Play time in seconds.
---@return number
function kindle.player.play_time() end
---@alias kindle.status {poison: {value: integer, max: integer}, toxic: {value: integer, max: integer}, bleed: {value: integer, max: integer}, curse: {value: integer, max: integer}, frost: {value: integer, max: integer}}
--- Status build-up; the effect triggers when value reaches max.
---@return kindle.status
function kindle.player.status() end
--- Current and max poise (most characters only have poise during certain attacks).
---@return number current, number max
function kindle.player.poise() end
--- Name of the current animation state, e.g. "Idle".
---@return string?
function kindle.player.animation() end
--- Animation speed multiplier (1 is normal).
---@return number?
function kindle.player.animation_speed() end
--- 1 is normal speed; 0-10.
---@param speed number
function kindle.player.set_animation_speed(speed) end
--- true: HP can't drop below 1.
---@param on? boolean default true
function kindle.player.set_no_death(on) end
--- true: hits do no damage.
---@param on? boolean default true
function kindle.player.set_no_damage(on) end
--- true: stamina doesn't drain.
---@param on? boolean default true
function kindle.player.set_infinite_stamina(on) end
--- true: FP doesn't drain.
---@param on? boolean default true
function kindle.player.set_infinite_fp(on) end
--- true: using consumables doesn't use them up.
---@param on? boolean default true
function kindle.player.set_infinite_items(on) end
--- Warp to a bonfire like the bonfire menu does. Only lit, warpable bonfires unless force.
---@param bonfire kindle.bonfire|kindle.bonfire_name|string|integer a bonfire table, its name, entity id or row id
---@param force? boolean
function kindle.player.warp(bonfire, force) end
--- Entity id of the bonfire last rested at.
---@return integer?
function kindle.player.last_bonfire() end

---@class kindle.character
---@field handle integer stable while the character is loaded
---@field entity_id integer? the map's entity id (used by event scripts and randomizer logs)
---@field npc_id integer NpcParam row id
---@field name string? display name for named NPCs/bosses (regular enemies have none)
---@field model string model id, e.g. "c1000_00"
---@field kind "player"|"enemy"|"npc"|"helper"|"other" from the live team
---@field team integer raw live team value (1 player, 6 enemy, 26 friendly NPC, 8 event helper)
---@field hp integer
---@field max_hp integer
---@field alive boolean
---@field is_player boolean this game's player
---@field other_player boolean another player's, in a multiplayer session
---@field x number
---@field y number
---@field z number
---@field distance number metres from the player
---@field address integer memory address (for the debug module)

---@alias kindle.charref kindle.character|integer A character table, handle or entity id.

---@class kindle.world  (permission: "world") Other characters: enemies, NPCs, players.
kindle.world = {}
--- Loaded characters, nearest first.
---@param opts? {radius?: number, alive?: boolean, player?: boolean, helpers?: boolean, kind?: string} defaults: any distance, living only, no players (this game's and, in a session, the others'), no helpers
---@return kindle.character[]
function kindle.world.characters(opts) end
---@class kindle.object A placed object of a map (a door, lever, chest, ladder, breakable, ...).
---@field handle integer stable while the object is loaded
---@field entity_id integer? the map's entity id (event scripts use it; kindle.maps.parts lists the part with its name and model)
---@field map string the map it belongs to, e.g. "m40_00_00_00"
---@field x number
---@field y number
---@field z number
---@field destroyed boolean? broken (a pot, a breakable wall), as the game's event scripts read it
---@field distance number? from the player
--- The maps' placed objects that are loaded, nearest first. Join with kindle.maps.parts(o.map) by
--- entity_id for an object's part name and model. Example, the objects within 5 m:
--- `for _, o in ipairs(kindle.world.objects{ radius = 5 }) do kindle.log.info("%s %s", o.map, tostring(o.entity_id)) end`
---@param opts? {radius?: number, map?: string, entity_id?: integer} defaults: every loaded object
---@return kindle.object[]
function kindle.world.objects(opts) end
--- Break an object as a hit does: its pieces fall and any treasure in it drops (the game's
--- command 2005[01]; the game keeps it broken, as it keeps a pot the player broke). Example,
--- break every pot within 3 m: `for _, o in ipairs(kindle.world.objects{radius = 3}) do if
--- o.destroyed == false and o.entity_id then kindle.world.destroy_object(o) end end`
---@param o kindle.object|integer an object from world.objects, or its entity id
function kindle.world.destroy_object(o) end
--- Put a broken object back whole (the game's command 2005[02]).
---@param o kindle.object|integer
function kindle.world.restore_object(o) end
--- Turn an object's action on or off: a door that can't be opened, a lever that can't be pulled
--- (the game's command 2005[03]).
---@param o kindle.object|integer
---@param on? boolean default true
function kindle.world.set_object_active(o, on) end
--- The players' characters, alive or not: this game's first (is_player), then in a session the
--- others' (other_player). Quick enough for every frame (world.characters reads every character
--- of the area).
---@return kindle.character[]
function kindle.world.players() end
--- One character by handle, entity id or table; nil if not loaded.
---@param c kindle.charref
---@return kindle.character?
function kindle.world.character(c) end
--- Create a character (experimental): any enemy, from any area. Name it with `enemy`
--- ("black_knight", or "Black Knight"; the editor completes the names, and the overlay's Find
--- tab lists them), or give `model` (1300 or "c1300") / `npc_param` / `think_param` (its AI)
--- / `area` (its home map) yourself; explicit ids override the named enemy's. It appears
--- `distance` metres (3) in front of the player, facing them, unless `x`, `y`, `z` / `angle`
--- are given.
---
--- With `ai_enabled` (the default) an enemy from another area gets its own AI: the first
--- time, its area's AI loads, so the enemy appears a moment later: the call returns nil and
--- `on_spawn(c)` runs once it exists (it also runs when the enemy is created at once). With
--- `ai_enabled = false` it's created at once with its AI off and stands still.
---
--- Spawned characters fight, die and drop souls, are removed when the area unloads, and have
--- no entity id (event-script functions like set_enabled can't target them).
--- `kindle.world.spawn{enemy = "black_knight", on_spawn = function(c) kindle.log.info("%s is here", c.model) end}`
---@param opts {enemy?: kindle.enemy|string, model?: integer|string, npc_param?: integer, think_param?: integer, area?: string, ai_enabled?: boolean, distance?: number, x?: number, y?: number, z?: number, angle?: number, on_spawn?: fun(c: kindle.character)}
---@return kindle.character?
function kindle.world.spawn(opts) end
--- Load another area's AI (experimental), so enemies from there act when spawned here:
--- `area` is the map name: m30_00_00_00 High Wall, m30_01_00_00 Lothric Castle, m31_00_00_00
--- Undead Settlement, m32_00_00_00 Archdragon Peak, m33_00_00_00 Road of Sacrifices and Farron
--- Keep, m34_01_00_00 Grand Archives, m35_00_00_00 Cathedral of the Deep, m37_00_00_00
--- Irithyll, m38_00_00_00 Catacombs and Smouldering Lake, m39_00_00_00 Irithyll Dungeon,
--- m40_00_00_00 Cemetery of Ash and Firelink, m41_00_00_00 Kiln, m45_00_00_00 Ariandel,
--- m50_00_00_00 Dreg Heap, m51_00_00_00 Ringed City. The first call starts loading and
--- returns false; call again (a timer) until it returns true, then spawn. Call it again after
--- the player changes area: the game rebuilds its AI then.
--- ```lua
--- local t; t = kindle.every(0.2, function()
---   if kindle.world.load_ai("m33_00_00_00") then
---     kindle.cancel(t)
---     kindle.world.spawn{model = 1300, npc_param = 130610, think_param = 130010}  -- Black Knight
---   end
--- end)
--- ```
---@param area string
---@return boolean ready
function kindle.world.load_ai(area) end
--- Set a character's HP.
---@param c kindle.charref
---@param hp integer clamped to 0..max_hp
function kindle.world.set_hp(c, hp) end
--- Kill a character.
---@param c kindle.charref
function kindle.world.kill(c) end
--- Restore a character's HP to its maximum.
---@param c kindle.charref
function kindle.world.heal(c) end
--- Apply an SpEffect to a character.
---@param c kindle.charref
---@param id integer SpEffectParam row id
function kindle.world.apply_effect(c, id) end
--- End an active SpEffect on a character; returns how many instances ended.
---@param c kindle.charref
---@param id integer
---@return integer ended
function kindle.world.remove_effect(c, id) end
--- A character's active SpEffects (duration -1 = permanent).
---@param c kindle.charref
---@return {id: integer, time_left: number, duration: number}[]
function kindle.world.effects(c) end
--- Move a character.
---@param c kindle.charref
---@param x number
---@param y number up
---@param z number
function kindle.world.teleport(c, x, y, z) end
--- A character's facing angle in radians.
---@param c kindle.charref
---@return number? radians
function kindle.world.facing(c) end
--- Turn a character to face an angle (radians).
---@param c kindle.charref
---@param radians number
function kindle.world.set_facing(c, radians) end
--- Whether a character is invulnerable.
---@param c kindle.charref
---@return boolean?
function kindle.world.invulnerable(c) end
--- Make a character invulnerable (or not).
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_invulnerable(c, on) end
--- false disables a character's gravity (it floats).
---@param c kindle.charref
---@param on boolean
function kindle.world.set_gravity(c, on) end
--- Show or hide a character's model.
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_visible(c, on) end
--- Play an animation event on a character by name or id.
---@param c kindle.charref
---@param anim kindle.animation_name|string|integer
function kindle.world.play_animation(c, anim) end
--- Every animation event a character's behaviour accepts.
---@param c kindle.charref
---@return table<integer, string>
function kindle.world.animations(c) end
--- Distance in metres between two characters, or a character and a point table {x, y, z}.
---@param a kindle.charref|{x: number, y: number, z: number}
---@param b kindle.charref|{x: number, y: number, z: number}
---@return number?
function kindle.world.distance(a, b) end
--- A character's status build-up; the effect triggers when value reaches max.
---@param c kindle.charref
---@return kindle.status
function kindle.world.status(c) end
--- A character's current and max poise.
---@param c kindle.charref
---@return number current, number max
function kindle.world.poise(c) end
--- Name of a character's current animation state.
---@param c kindle.charref
---@return string?
function kindle.world.animation(c) end
--- A character's animation speed multiplier (1 is normal).
---@param c kindle.charref
---@return number?
function kindle.world.animation_speed(c) end
--- Set a character's animation speed.
---@param c kindle.charref
---@param speed number 1 is normal; 0-10
function kindle.world.set_animation_speed(c, speed) end
--- true: the character's HP can't drop below 1.
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_no_death(c, on) end
--- true: hits do no damage to the character.
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_no_damage(c, on) end
--- Turn a character on or off: a disabled character disappears and does nothing until
--- enabled again. Needs the character's entity_id (map characters have one).
--- Undone when the mod unloads.
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_enabled(c, on) end
--- Turn a character's AI on or off: with it off, it stands still and won't attack. It stays
--- off until the mod turns it on or unloads: Kindle stops the game's own scripts from waking
--- it (ambushes, enemies that get back up) and re-applies it while the character is loaded.
--- Undone when the mod unloads.
---@param c kindle.charref
---@param on? boolean default true
function kindle.world.set_ai(c, on) end
--- Make a character forget its target (drop aggro) and choose what to do again.
---@param c kindle.charref
function kindle.world.clear_aggro(c) end
--- Change a character's team, e.g. make an enemy friendly.
---@param c kindle.charref
---@param team "enemy"|"npc"|"player"|"helper"|integer
function kindle.world.set_team(c, team) end

---@class kindle.bonfire
---@field id integer BonfireWarpParam row
---@field entity_id integer
---@field name string?
---@field lit boolean
---@field flag integer the event flag set once it's lit
---@field warpable boolean
---@field dlc integer 0 base game, 1 Ashes of Ariandel, 2 The Ringed City
---@field last_rested boolean

--- Every bonfire in the game.
---@return kindle.bonfire[]
function kindle.world.bonfires() end

---@class kindle.hks_call
---@field character kindle.character? the character whose behaviour script made the call
---@field id integer|string env/act id (a number, or the Japanese name some calls use)
---@field args any[] the call's other arguments (numbers, booleans, strings)
---@field result integer? env only: the game's answer

---@class kindle.events  (permission: "events") Run the game's own event script (EMEVD) commands,
--- and change its event scripts (applied from the next time that map's events load).
--- The named functions below cover the most useful ones (entity = a map entity id, e.g. a
--- character's entity_id, 10000 for the player); events.run runs any command by number.
--- Map event scripts are lists of commands: a bank (category, e.g. 2003 events, 2004
--- characters, 2006 effects, 2007 messages, 2010 sound), an index and packed arguments.
--- Kindle runs a command through the game's own handler, exactly as a script would. Use
--- kindle.debug.trace_events(true) to see the commands the game runs and their arguments.
kindle.events = {}
--- Run an event script command. `format` has one letter per argument value: b/B i8/u8,
--- h/H i16/u16, i/I i32/u32, f f32 (packed with each value aligned to its size, like the
--- game's files). Runs on Kindle's next frame; an optional last argument on_done(done, err)
--- gets whether the game's handler finished it.
--- Example: kindle.events.run(2007, 2, "B", 1) runs DisplayBanner(1).
---@param bank integer
---@param index integer
---@param format string? argument format, e.g. "iB"
---@param ... number argument values, then optionally on_done
---@return integer request
function kindle.events.run(bank, index, format, ...) end
---@class EventScript
---@field id integer
---@field rest integer
---@field commands {bank: integer, index: integer, args: integer[], bytes: integer[], layer: integer}[]
---@field parameters {command: integer, target: integer, source: integer, length: integer}[]

--- The game's own event from a map's event file ("m40_00_00_00", "common", "common_func"):
--- its commands with their arguments as 32-bit words and as bytes, their layers, and its
--- parameters (the event's argument bytes source.. copied into command's bytes target..).
--- nil if the map has no such event.
---@param map string
---@param id integer
---@return EventScript?
function kindle.events.script(map, id) end
--- The event as the game will run it: the game's, with every change mods and Kindle make to it
--- now. As events.script.
---@param map string
---@param id integer
---@return EventScript?
function kindle.events.view(map, id) end
--- Changes an event, all or nothing. Each step names a command by its number in the event as it
--- is before this edit (1 = the first; insert at the count + 1 adds at the end):
--- {check = n, is = {bank, index, byte, ...}} (command n must be that: its first bytes),
--- {remove = n}, {replace = n, bank =, index =, format =, values = {...}},
--- {insert = n, bank =, index =, format =, values = {...}} (before command n).
--- Any step can say what its command must be (is). A new command can take the event's arguments
--- (params = {{target =, source =, length =}}, in bytes) and command m's layer (layer_of = m; a
--- replacement keeps its command's by default); a replacement can keep its command's argument
--- entries (keep_params = true). If a check fails, nothing of the edit applies and the log says
--- why, with the reason given.
--- Example: events.edit("m38_00_00_00", 13805809, {{remove = 13, is = {1003, 12, 4, 0}}}, "the goblet for players")
---@param map string
---@param id integer
---@param steps table[]
---@param why string?
function kindle.events.edit(map, id, steps, why) end
--- Every command bank[index] in a map's event file as the game has it: {event, command (its
--- number in the event, from 1), args (32-bit words), bytes} each. E.g. every boss health bar a
--- map shows: events.find("m30_00_00_00", 2003, 11).
---@param map string
---@param bank integer
---@param index integer
---@return {event: integer, command: integer, args: integer[], bytes: string}[]
function kindle.events.find(map, bank, index) end
--- Drops this mod's changes to one map's events (from the next time that map's events load);
--- returns how many there were. For a mod whose changes depend on the character (a randomizer
--- setting them anew for each).
---@param map string
---@return integer
function kindle.events.reset(map) end
--- Makes an event do nothing (it ends at once). Changes to event scripts apply from the next
--- time that map's events load (loading into the area), in the order mods make them; command
--- numbers (from 1) count the event's commands as they are by then.
---@param map string e.g. "m40_00_00_00"
---@param id integer
function kindle.events.disable(map, id) end
--- Removes an event's command number n.
---@param map string
---@param id integer
---@param n integer
function kindle.events.remove(map, id, n) end
--- Inserts a command before number n (the count + 1 adds it at the end). Arguments as for run.
---@param map string
---@param id integer
---@param n integer
---@param bank integer
---@param index integer
---@param format string?
---@param ... number
function kindle.events.insert(map, id, n, bank, index, format, ...) end
--- Replaces an event's command number n.
---@param map string
---@param id integer
---@param n integer
---@param bank integer
---@param index integer
---@param format string?
---@param ... number
function kindle.events.replace(map, id, n, bank, index, format, ...) end
--- Adds a new event to a map. rest: what happens when the map reloads (0 restart, the default;
--- 1 end; 2 end and restart). Start it with events.start.
--- Example: events.add("m40_00_00_00", 14009960, {{bank = 2003, index = 2, format = "iB", values = {14000998, 1}}})
---@param map string
---@param id integer
---@param commands {bank: integer, index: integer, format: string?, values: number[]?}[]
---@param rest integer?
function kindle.events.add(map, id, commands, rest) end
--- Starts an event when the map's events load, as the game's event 0 does: slot, then the
--- event's arguments (format and values).
---@param map string
---@param id integer
---@param slot integer?
---@param format string?
---@param ... number
function kindle.events.start(map, id, slot, format, ...) end
--- Give an entity's AI a command (AI-specific). (command 2004[17])
---@param entity integer
---@param command integer
---@param slot integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.ai_command(entity, command, slot, on_done) end
--- Make an entity's AI decide what to do again. (command 2004[20])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.ai_replan(entity, on_done) end
--- Apply an SpEffect to an entity. (command 2004[08])
---@param entity integer
---@param speffect integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.apply_effect(entity, speffect, on_done) end
--- Show an area-name title (place name text id). (command 2007[11])
---@param text_id integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.area_name(text_id, on_done) end
--- Give the player an item lot (ItemLotParam row), with the "item received" box. (command 2003[04])
---@param lot integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.award_item_lot(lot, on_done) end
--- Restore the player as resting at a bonfire does. (command 2004[47])
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.bonfire_recovery(on_done) end
--- Handle a boss defeat and show a banner type (1 HEIR OF FIRE DESTROYED, 22 LORD OF CINDER FALLEN). (command 2003[74])
---@param entity integer
---@param banner integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.boss_defeat_banner(entity, banner, on_done) end
--- Shake the camera (shake_id a camera vibration id) around an entity, fading between the distances. (command 2008[02])
---@param shake_id integer
---@param target_type "character"|"object"|"area"|integer
---@param entity integer
---@param dummypoly integer
---@param fade_start number
---@param fade_end number
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.camera_shake(shake_id, target_type, entity, dummypoly, fade_start, fade_end, on_done) end
--- Make an entity forget its target. (command 2004[16])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.clear_ai_target(entity, on_done) end
--- Stop a map SFX entity's effect. (command 2006[01])
---@param entity integer
---@param root_only boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.delete_map_sfx(entity, root_only, on_done) end
--- Break an object (slot 1 is what the game's scripts use). (command 2005[01])
---@param entity integer
---@param slot integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.destroy_object(entity, slot, on_done) end
--- Mark a boss as defeated, as the game does when one dies (without a banner). (command 2003[12])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.handle_boss_defeat(entity, on_done) end
--- Show (true) or hide (false) the HUD the way event scripts do: while it's hidden the pause
--- menu can't be opened either, so always show it again. To hide the HUD during play, use
--- kindle.ui.set_hud("off") instead. (command 2003[71])
---@param shown boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.show_hud(shown, on_done) end
--- Kill an entity, giving the player its souls if give_souls. (command 2004[04])
---@param entity integer
---@param give_souls boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.kill(entity, give_souls, on_done) end
--- Play an object's animation (a lever pulled, a door opening). (command 2005[07])
---@param entity integer
---@param animation integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.object_animation(entity, animation, on_done) end
--- Play a sound on an entity: sound_type "sfx", "character", "menu", "environment", "voice", ... and its id. (command 2010[02])
---@param entity integer
---@param sound_type "environment"|"character"|"menu"|"object"|"cutscene"|"sfx"|"bgm"|"voice"|integer
---@param sound_id integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.play_sound(entity, sound_type, sound_id, on_done) end
--- Remove an SpEffect from an entity. (command 2004[21])
---@param entity integer
---@param speffect integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.remove_effect(entity, speffect, on_done) end
--- Put a broken object back whole. (command 2005[02])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.restore_object(entity, on_done) end
--- Turn an entity's AI on or off. (command 2004[01])
--- Undone when the mod unloads.
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_ai(entity, on, on_done) end
--- Change an entity's AI (NpcThinkParam row). (command 2004[19])
---@param entity integer
---@param ai_id integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_ai_id(entity, ai_id, on_done) end
--- Enable or disable an entity (a disabled character disappears). (command 2004[05])
--- Undone when the mod unloads.
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_enabled(entity, on, on_done) end
--- Show or hide an entity's HP bar. (command 2004[30])
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_hp_bar(entity, on, on_done) end
--- An entity's HP can't drop below 1 while on. (command 2004[12])
--- Undone when the mod unloads.
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_immortal(entity, on, on_done) end
--- An entity takes no damage while on. (command 2004[15])
--- Undone when the mod unloads.
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_invincible(entity, on, on_done) end
--- Show or hide a map part entity. (command 2012[01])
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_map_part(entity, on, on_done) end
--- Turn an object action (ObjActParam row: a door, a lever, a chest) on or off on an object. (command 2005[06])
---@param entity integer
---@param obj_act integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_obj_act(entity, obj_act, on, on_done) end
--- Turn an object's action on or off. (command 2005[03])
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_object_active(entity, on, on_done) end
--- Let an object's treasure be picked up, or not. (command 2005[04])
---@param entity integer
---@param on boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_object_treasure(entity, on, on_done) end
--- Set where the player respawns (a respawn point entity). (command 2003[23])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_respawn_point(entity, on_done) end
--- Change an entity's team (a number or name: enemy, friendly_npc, ally, hostile_npc, ...). (command 2004[02])
---@param entity integer
---@param team integer|"enemy"|"strong_enemy"|"friendly_npc"|"hostile_npc"|"ally"|"hostile_ally"|"friendly_enemy"|"human"|"indiscriminate"|"disabled"
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.set_team(entity, team, on_done) end
--- Show (on) or hide a boss health bar for entity: slot 0-2, name_id an NPC name text id. (command 2003[11])
---@param on boolean
---@param entity integer
---@param slot integer
---@param name_id integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.show_boss_bar(on, entity, slot, name_id, on_done) end
--- Start a map SFX entity's effect. (command 2006[02])
---@param entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.spawn_map_sfx(entity, on_done) end
--- Play a one-off visual effect (SFX id) on an entity: target_type "character", "object" or "area"; dummypoly -1 for the root. (command 2006[03])
---@param target_type "character"|"object"|"area"|integer
---@param entity integer
---@param dummypoly integer
---@param sfx integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.spawn_sfx(target_type, entity, dummypoly, sfx, on_done) end
--- Show a status message (event text id) at the top of the screen. (command 2007[03])
---@param text_id integer
---@param pad boolean
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.status_message(text_id, pad, on_done) end
--- Warp the player to a map (area and block, e.g. 30, 0 for High Wall) at the entity start_entity. (command 2003[14])
---@param area integer
---@param block integer
---@param start_entity integer
---@param on_done? fun(done: boolean?, err: string?)
---@return integer request
function kindle.events.warp_player(area, block, start_entity, on_done) end

---@class kindle.hks  (permission: "hks") The character behaviour scripts (HavokScript, c0000.hks
--- etc.). They ask the game questions with env(id, ...) and act with act(id, ...); mods can
--- change the answers and block actions without editing the scripts. Use
--- kindle.debug.trace_hks(true) to see the player's calls (known ids are named). Example:
--- blocking act SET_VARIABLE for "MoveSpeedLevelReal" stops the player sprinting.
kindle.hks = {}
--- Change the answer to env(id, ...): return a number/boolean (nil keeps the game's answer).
--- Called very often, from the game's script threads: keep handlers quick.
---@param id integer|string
---@param fn fun(e: kindle.hks_call): (number|boolean|nil)
---@return integer handle
function kindle.hks.on_env(id, fn) end
--- Known env ids by name: numbers, or the Japanese names some calls use. Unconfirmed
--- ones are well-founded guesses.
---@class kindle.hks_env_ids
---@field HP integer 1000: current HP
---@field STAMINA integer 1001: current stamina
---@field FP integer 2016: current FP
---@field HAS_SPEFFECT integer 1116: env(1116, id): 1 if the character has SpEffect id
---@field WEAPON_MOTION_CATEGORY integer 225: env(225, hand): the hand's weapon motion category (wepmotionCategory; 42 = fists; hand 0 left, 1 right)
---@field GUARD_MOTION_CATEGORY integer 278: env(278, hand): the hand's guard motion category (GUARD_STYLE_*: 0 default, 1 small, 2 large, 3 torch)
---@field WEAPON_ART_CATEGORY integer 345: env(345, hand): the hand's weapon art category (spAtkcategory)
---@field HAND_STYLE integer 207: how weapons are held (HAND_*: 1 right one-handed, 2 two-handing left, 3 two-handing right)
---@field EQUIP_WEIGHT integer 257: equip load class (WEIGHT_*: 0 light, 5 normal, 10 heavy, 15 overweight, 16 super light) (unconfirmed)
---@field ACTION_REQUEST integer 1106: env(1106, action): 1 when that action's input is requested (ACTION_ARM_*: 0 R1, 1 R2, 2 L1, 3 L2, 7 use item, 12 backstep, 13 roll, 14 jump) (unconfirmed)
---@field ACTION_HELD integer 1107: env(1107, action): 1 while that action's input is held (unconfirmed)
---@field ACTION_HOLD_TIME integer 1108: env(1108, action): how long that action's input has been held (unconfirmed)
---@field ANY_ACTION_REQUEST integer 1109: 1 when any action input is requested (IsActionRequest) (unconfirmed)
---@field HAS_CONDITION integer 305: env(305, type): 1 if the character is in that condition (CONDITION_TYPE_*: 117 stone, 136 crystal, 154 no estus, 268 hollowing death, 286/287 gravity) (unconfirmed)
---@field ITEM_ANIM_TYPE integer 231: animation type of the item being used (ITEM_*: 10 drink, 19 drink FP, 21 drink sake) (unconfirmed)
---@field WEAPON_ART_INFO integer 326: env(326, hand): weapon art info for the hand (GetSwordArtsInfo) (unconfirmed)
---@field MOVE_CANCEL_TIMING integer 2000: 1 when the current animation can be cancelled into movement (unconfirmed)
---@field GUARD_CANCEL_TIMING integer 243: 1 when the current animation can be cancelled into guarding (unconfirmed)
---@field FALL_START integer 200: asked when a fall may start (ExecFallStart) (unconfirmed)
---@field FALLING_FOR integer 315: env(315, seconds): 1 once the character has been falling that long (the player's script kills it at 12)
---@field FRAME_TIME integer 333: frame time (GetDeltaTime; observed 18-25) (unconfirmed)
---@field LADDER_COMMAND integer 105: env(105, n): ladder event command (-1 none) (unconfirmed)
---@field IS_FEMALE string "女性か": 1 if the character is female (unconfirmed)
---@field IS_DUAL_BLADE string "双剣か": env(name, hand): 1 if the hand holds a paired weapon (unconfirmed)
---@field WEAPON_ART_CATEGORY_BY_NAME string "装備武器特殊カテゴリ番号取得": env(name, hand): the hand's weapon art category (spAtkcategory)
---@field WEAPON_ART_CANCEL_TYPE string "剣戟キャンセルタイプを取得": env(name, hand): weapon art cancel type (unconfirmed)
---@field CAN_CANCEL_WEAPON_ART string "剣戟キャンセル可能か": 1 when the weapon art can be cancelled (unconfirmed)
---@field IS_EQUIP_MENU_OPEN string "装備メニュー開いているか": 1 while the equipment menu is open (unconfirmed)
---@field HAS_EVENT_ANIM_REQUEST string "イベントアニメ再生要求があるか": 1 when an event animation is requested (unconfirmed)
---@field IDLE_ANIM_CATEGORY string "待機アニメカテゴリ取得": idle animation category (unconfirmed)
---@field IS_ANIM_END string "アニメ終了か": env(name, layer): 1 when the animation has ended (unconfirmed)
---@field WEAPON_STORAGE_TYPE string "武器格納場所タイプ取得": env(name, hand): where the weapon is stowed (back, waist, shoulder) (unconfirmed)
---@field UPPER_ARM_OUTER_ANGLE string "上腕制御外側角度": upper arm control: outward angle (unconfirmed)
---@field UPPER_ARM_VERTICAL_ANGLE string "上腕制御上下角度": upper arm control: vertical angle (unconfirmed)
---@type kindle.hks_env_ids
kindle.hks.env = {}
--- Known act ids by name: numbers, or Japanese names.
---@class kindle.hks_act_ids
---@field SET_VARIABLE integer 148: act(148, name, value): set a behaviour graph variable
---@field APPLY_SPEFFECT integer 2002: act(2002, id): apply SpEffect id
---@field ADD_STAMINA integer 1001: act(1001, amount): add (negative: spend) stamina (unconfirmed)
---@field SET_HAND_STYLE integer 104: act(104, style): change how weapons are held (HAND_*) (unconfirmed)
---@field SET_THROW_ATTACK_INVALID integer 102: act(102, n): disable throw attacks (SetThrowAtkInvalid) (unconfirmed)
---@field SET_WEAPON_ART_CANCEL_TYPE integer 2011: act(2011, type): weapon art cancel type (unconfirmed)
---@field SET_AI_ACTION_STATE integer 9103: act(9103, state): AI action state (unconfirmed)
---@field RESET_REQUEST integer 9101: clear pending action requests (ResetRequest) (unconfirmed)
---@field LOCKON_TURN_LIMIT string "ロックオン時システム旋回不可角度": act(name, a, b): angles the system won't turn to while locked on (unconfirmed)
---@field LOCKON_RELEASE_ANGLE_LOCK string "ロックオン中角度固定解除": release the angle lock while locked on (unconfirmed)
---@field AUTO_TARGET_SET string "自動捕捉対象設定": set the auto-aim target (unconfirmed)
---@field AUTO_TARGET_CLEAR string "自動捕捉対象クリア": clear the auto-aim target (unconfirmed)
---@field AI_NOTIFY_ATTACK_TYPE string "AI通知攻撃タイプ": act(name, type): attack type told to AI (unconfirmed)
---@field SET_4WAY_MOVE_THRESHOLD string "4方向移動閾値設定": act(name, a, b): four-way movement thresholds (unconfirmed)
---@field DISABLE_AVARICE_DRAIN string "貪欲者の印スリップダメージ無効": pause the Symbol of Avarice HP drain (done at bonfires) (unconfirmed)
---@field SET_IN_ITEM_ANIM string "アイテムアニメ中か設定": mark the character as in an item animation (unconfirmed)
---@field DISABLE_ITEM_SWITCH string "道具ID切り替え無効": disable switching the selected item (unconfirmed)
---@field SET_WEAPON_PARAM_HAND string "武器パラメータ参照先": act(name, hand): which hand's weapon params attacks use (unconfirmed)
---@type kindle.hks_act_ids
kindle.hks.act = {}
--- Have a character's script perform act(id, ...): it runs at that script's next call (within
--- a frame). An optional last argument on_done(result, err) gets the result (nil, with a
--- reason in err, if the character's script didn't run within ~5 seconds).
--- Example: kindle.hks.run_act(nil, kindle.hks.act.SET_VARIABLE, "MoveSpeedLevelReal", 2)
---@param c kindle.charref|nil nil = the player
---@param id integer|string
---@param ... any arguments, then optionally on_done
---@return integer request
function kindle.hks.run_act(c, id, ...) end
--- Ask a character's script env(id, ...); the last argument is fn(result) for the answer
--- (nil if the character's script didn't run within ~5 seconds).
---@param c kindle.charref|nil nil = the player
---@param id integer|string
---@param ... any arguments, then fn(result)
---@return integer request
function kindle.hks.ask_env(c, id, ...) end
--- Run HKS source in a character's script state, with access to its globals and functions
--- (the script's own Lua, not Kindle's). The first value it returns (number, boolean, string
--- or nil) goes to fn(result, err); err is set if it didn't compile or raised an error.
--- Before the first chunk runs, Kindle removes the io, os, package and debug libraries (and
--- require, dofile, loadfile) from that script state: the game's scripts don't use them.
--- Source text only: a precompiled chunk is refused, as loadstring and load refuse one inside it.
--- Example: kindle.hks.eval(nil, "return env(1000)", function(hp) print(hp) end)
---@param c kindle.charref|nil nil = the player
---@param source string
---@param fn fun(result: number|boolean|string|nil, err: string?)
---@return integer request
function kindle.hks.eval(c, source, fn) end
--- Read a behaviour graph variable (the values the animation graph runs on, e.g.
--- "MoveSpeedLevelReal", "MoveType"); fn(value, err), err set for an unknown name.
---@param c kindle.charref|nil nil = the player
---@param name string
---@param fn fun(value: number|boolean|nil, err: string?)
---@return integer request
function kindle.hks.get_variable(c, name, fn) end
--- Set a behaviour graph variable. The script sets many of them every frame, so a value can
--- be overwritten right away; to hold one, block the script's own writes with
--- kindle.hks.on_act(kindle.hks.act.SET_VARIABLE, ...).
---@param c kindle.charref|nil nil = the player
---@param name string
---@param value number|boolean
---@param on_done fun(result: nil, err: string?)?
---@return integer request
function kindle.hks.set_variable(c, name, value, on_done) end
--- Read a global of the character's script, e.g. one of its constants: fn(value, err).
--- Tables and functions read as nil.
---@param c kindle.charref|nil nil = the player
---@param name string
---@param fn fun(value: number|boolean|string|nil, err: string?)
---@return integer request
function kindle.hks.get_global(c, name, fn) end
--- Set a global of the character's script (nil removes it).
---@param c kindle.charref|nil nil = the player
---@param name string
---@param value number|boolean|string|nil
---@param on_done fun(result: nil, err: string?)?
---@return integer request
function kindle.hks.set_global(c, name, value, on_done) end

---@class kindle.hks_script_call
---@field character kindle.character? the character whose script called
---@field name string the first argument to kindle(...)
---@field args any[] the other arguments (numbers, booleans, strings)

--- Handle kindle(name, ...) calls from edited behaviour scripts. Kindle adds a global
--- `kindle` function to every script; a script calls e.g. `if kindle then kindle("dodged",
--- 1) end`. Every handler for that name runs; the first number/boolean/string one returns
--- is what the script gets back. Runs on the game's script threads: keep it quick.
---@param name string
---@param fn fun(e: kindle.hks_script_call): (number|boolean|string|nil)
---@return integer handle
function kindle.hks.on_call(name, fn) end
--- Watch or block act(id, ...): return false to block the action.
---@param id integer|string
---@param fn fun(e: kindle.hks_call): (boolean|nil)
---@return integer handle
function kindle.hks.on_act(id, fn) end

---@class kindle.param_row A live param row: read/assign fields by name, e.g. row.weight = 0.
--- Each table's row type (in kindle-game.lua) lists its fields.

---@class kindle.params  (permission: "params") Read and change the game's param tables (weapon stats, effects, ...) live.
kindle.params = {}
--- A live row, or nil if the row doesn't exist. Writes are reverted when the mod unloads.
--- The row is typed by its table (e.g. EquipParamWeapon), so its fields autocomplete.
--- In the item tables the row can also be the item's name: params.get("EquipParamGoods", "Ember").
---@generic T: kindle.param_table
---@param table `T` e.g. "EquipParamGoods"
---@param row integer|kindle.item_name|string row id, or an item's name in an item table
---@return T?
function kindle.params.get(table, row) end
--- Every param table's name.
---@return kindle.param_table[]
function kindle.params.tables() end
--- Add a brand-new row to a table, starting as a copy of row copy_from, and return it (a
--- live row, like params.get). The id must be above every existing id in the table,
--- including rows other mods added (mods load in folder-name order): pick ids well apart. For
--- item tables (EquipParamGoods, EquipParamWeapon, EquipParamProtector, EquipParamAccessory)
--- the new item also gets its template's name, effect line and description, until the mod
--- sets its own (items.set_name etc.). Rows stay for the rest of the session, even after the
--- mod unloads (an item the player holds must keep its row); adding the same id again
--- returns that row. A save holding a new item needs the mod when it's loaded.
--- copy_from can be an item's name in the item tables. Example, a new goods item based on
--- Ember: `local MY_ITEM = 4000000` (your own id), then params.add_row("EquipParamGoods", MY_ITEM, "Ember")
---@generic T: kindle.param_table
---@param table `T`
---@param id integer above every existing id
---@param copy_from integer|kindle.item_name|string the row to copy, or an item's name in an item table
---@return T
function kindle.params.add_row(table, id, copy_from) end
--- Add several rows to a table with one rebuild of it: add_row rebuilds the table for each row
--- (a second each, and every place the game holds the table moves), so a mod adding many rows
--- adds them together. `rows` are {id = ID, copy_from = ROW}, ids ascending and above every
--- existing id; the rules of add_row hold. Returns the live rows, in order. Edits made to a
--- table's rows stay through later additions to it. Example: params.add_rows("AtkParam_Npc",
--- {{id = 12000000, copy_from = 2110010}, {id = 12000001, copy_from = 2110020}})
---@generic T: kindle.param_table
---@param table `T`
---@param rows {id: integer, copy_from: integer|kindle.item_name|string}[]
---@return T[]
function kindle.params.add_rows(table, rows) end
--- Copies rows over others in one go: each `to` gets what its `from` holds before any of them is
--- copied (so rows can trade contents), except the fields named in `keep`. Reverted when the mod
--- unloads, as any write. Example, two item lots trading contents but keeping their pickup
--- flags: params.copy("ItemLotParam", {{4000, 4010}, {4010, 4000}}, {"getItemFlagId"})
---@param table kindle.param_table
---@param pairs integer[][] {{from, to}, ...}
---@param keep string[]? field names left as they are
function kindle.params.copy(table, pairs, keep) end
--- Puts back what this mod wrote to the game's params: one row's fields (table and row), a
--- table's, or everything (no arguments). Returns how many fields.
---@param table kindle.param_table?
---@param row integer?
---@return integer
function kindle.params.reset(table, row) end

---@class kindle.map_part One part of a map file, as kindle.maps.parts gives it.
---@field name string
---@field type integer the part's type number
---@field kind "map_piece"|"object"|"enemy"|"player"|"collision"|"dummy_object"|"dummy_enemy"|"connect_collision"|nil
---@field model string its model's name, e.g. "c1100"
---@field position number[] {x, y, z}
---@field rotation number[] {x, y, z}, as the file has it
---@field entity_id integer?
---@field entity_groups integer[]?
---@field think_param integer? an enemy's AI (NpcThinkParam row)
---@field npc_param integer? an enemy's NpcParam row
---@field talk integer? an enemy's talk script

---@class kindle.maps  (permission: "maps") The game's map files (what stands where: enemies, objects,
--- collisions) as the game has them, and changes to them, applied when the game reads the map's
--- file (the next time that area loads; in "map_loading" for the load starting).
kindle.maps = {}
--- A map's parts as the game's file has them, in the file's order (part 1 first), or nil if the
--- game has no such map (a DLC's that isn't installed). Read from the game's archives the first
--- time (up to a second; maps.preload reads ahead).
---@param map string e.g. "m30_00_00_00"
---@return kindle.map_part[]?
function kindle.maps.parts(map) end
--- Reads maps' parts ahead on a thread of Kindle's (the game goes on), so maps.parts gives them
--- at once later.
---@param maps string[]
function kindle.maps.preload(maps) end
--- Sets this mod's changes to a map's file (replacing its earlier ones for that map). A change:
--- {replace_enemy = n, from = {map =, part = m}, keep_ids = false}: part n (an enemy) becomes an
--- enemy of part m's type (of any map): it keeps its place, the ground it stands on and its walk
--- route; its entity id and groups go (its spot's event scripts no longer act on it) unless
--- keep_ids. Kindle loads the AI of the maps the new enemies come from. A change
--- {replace_enemy = n, model = "c7200", npc_param = A, think_param = B, ai = "name", keep_ids = false}
--- makes part n an enemy of a model no map has (a mod's own, its files served by the mod) with
--- the NpcParam row and AI row given (each only if given), its goals in script/<ai>.luabnd (loaded
--- as long as the map is); the model's name has as many characters as the part's own. A changed map
--- file is kept in kindle\cache for the next session.
---@param map string
---@param changes ({replace_enemy: integer, from: {map: string, part: integer}, keep_ids: boolean?}|{replace_enemy: integer, model: string, npc_param: integer?, think_param: integer?, ai: string?, keep_ids: boolean?})[]
function kindle.maps.edit(map, changes) end
--- Drops this mod's changes to a map (to every map without one); how many there were.
---@param map string?
---@return integer
function kindle.maps.reset(map) end
--- Watch which game code touches [addr, addr+len) (within one 4 KB page): debug.watch_hits()
--- lists each instruction (offset in the exe), the address it touched and return offsets.
--- Stops by itself after max (16) distinct instructions.
---@param addr integer
---@param len? integer default 1
---@param max? integer default 16
function kindle.debug.watch(addr, len, max) end
--- Stop the current watch.
function kindle.debug.watch_stop() end
--- Record every instruction the next call of the game function at `rva` runs (an offset in
--- DarkSoulsIII.exe; the function's first instruction), the game's protected code included,
--- up to `max` (2,000,000). The trace is saved to logs/trace_<n>.bin: the game module's base
--- (u64), then for each instruction its address (u64) and its 16 bytes. For finding what a
--- protected function really does: most of it is ordinary code the trace names.
---@param rva integer
---@param max? integer
function kindle.debug.trace_next(rva, max) end
--- Hold a key or controller button down for the game for a moment, as if the player pressed
--- it (menus, title screen, testing). Keys: "enter", "esc", "e", "q", "f", "space", "up",
--- "down", "left", "right", letters, digits, "f1"-"f5", or a DirectInput key code.
--- Controller: "pad_a", "pad_b", "pad_x", "pad_y", "pad_start", "pad_back", "pad_up",
--- "pad_down", "pad_left", "pad_right", "pad_lb", "pad_rb", "pad_ls", "pad_rs" (reported on
--- controller 1, connected if none is, and for a minute after the last press so the game keeps
--- reading it; with no controller Kindle tells the game one arrived and the first press starts
--- 0.6 s late, once the game reads it). Controller presses work at the title screen even when
--- the game ignores the keyboard there. Needs the overlay enabled (its input hooks).
---@param key string|integer
---@param seconds? number default 0.1
function kindle.debug.press(key, seconds) end
--- How often the game has read input since Kindle started: {keyboard reads, failed
--- keyboard reads, mouse reads, controller reads, controller reads with none connected}.
---@return integer[]
function kindle.debug.input_stats() end
--- Milliseconds per second each mod spent in its Lua over the last second, in total and by
--- handler (named by event: "frame", "timer", "damage", "message", ...): { [mod id] = { total
--- = ms, [handler] = ms } }. The overlay's Mods tab shows the same when you hover a mod's
--- ms/s.
---@return table<string, table<string, number>>
function kindle.debug.profile() end
--- How often the game ran each of Kindle's hooks since it was hooked, and on which threads
--- (raw access only).
---@return {area: string, name: string, calls: integer, off_game_thread: integer, threads: integer, per_second: number?}[]
function kindle.debug.census() end
--- What the watch saw.
---@return {rip: integer, address: integer, returns: integer[]}[]
function kindle.debug.watch_hits() end
--- Save a game file from the game's archives, decrypted (as the game reads it), e.g.
--- debug.export("/event/m40_00_00_00.emevd.dcx", "m40.emevd.dcx"): dest is a file name, written
--- in kindle/exports/<mod id>/ (with raw access, any path). Returns which archive held it and its
--- size.
---@param path string
---@param dest string
---@return string
function kindle.debug.export(path, dest) end
--- A table's fields: name, type, byte offset, and array or bitfield size.
---@param table kindle.param_table
---@return {name: string, type: string, offset: integer, count?: integer, bit?: integer, bits?: integer}[]
function kindle.params.fields(table) end
--- A table's row ids, in table order.
---@param table kindle.param_table
---@return integer[]
function kindle.params.rows(table) end

---@class kindle.debug  (permission: "debug") Tools for finding things. The memory and code functions
--- (singletons, singleton, class_name, module, read, bytes, wstr, watch, watch_stop, watch_hits,
--- trace_next, census) exist only with raw access: for the mods the player names in kindle.toml
--- [developer] (raw = true, raw_mods), and the console.
kindle.debug = {}
--- name -> static address of every game singleton: the ones Kindle uses, plus every static
--- in the game's data that points to an object the game's type information names (found by
--- scanning, so it takes a moment).
---@return table<string, integer>
function kindle.debug.singletons() end
--- The class of the object at an address, from the game's type information (e.g.
--- "WorldChrMan"), or nil.
---@param addr integer
---@return string?
function kindle.debug.class_name(addr) end
--- Instance address of a named singleton (e.g. "MsgRepository"), or nil.
---@param name string
---@return integer?
function kindle.debug.singleton(name) end
--- Base address and size of DarkSoulsIII.exe.
---@return integer base, integer size
function kindle.debug.module() end
--- Read a value; nil if the address is unreadable (never crashes).
---@param addr integer
---@param type? "u8"|"i8"|"u16"|"i16"|"u32"|"i32"|"u64"|"ptr"|"f32"|"f64" default "u64"
---@return number?
function kindle.debug.read(addr, type) end
--- Hex dump of up to 4096 bytes.
---@param addr integer
---@param len integer
---@return string?
function kindle.debug.bytes(addr, len) end
--- Read a null-terminated UTF-16 string.
---@param addr integer
---@param max? integer default 256
---@return string?
function kindle.debug.wstr(addr, max) end
--- Format a number as hexadecimal, e.g. for addresses.
---@param n integer
---@return string
function kindle.debug.hex(n) end
--- Log the player's behaviour script calls (env/act, with known ids named): each env
--- question and its answer once, act calls when their arguments change.
---@param on? boolean default true
function kindle.debug.trace_hks(on) end
--- Log each distinct event script command the game runs, e.g. "event command 2007[02](1)"
--- (bank 2007, index 2, arguments as 32-bit words). For finding commands for events.run.
---@param on? boolean default true
function kindle.debug.trace_events(on) end
--- Log the talk script (NPC and bonfire menu) commands and queries as they run.
---@param on? boolean default true
function kindle.debug.trace_talk(on) end
--- Count the game's "who is this player" checks (session guest, multiplayer, world owner, host,
--- online, the per-character-type table's columns) by the game code calling them and their
--- answers, from now; debug.predicates() lists them, as do the co-op watchdog's reports.
--- Returns how many of the checks are hooked.
---@param on? boolean default true
---@return integer
function kindle.debug.trace_predicates(on) end
--- The counts since debug.trace_predicates(true), most called first: the check, the caller (an
--- offset in DarkSoulsIII.exe), the character type asked about (the type table's), how many
--- calls and how many answered true.
---@return {predicate: string, caller: integer, type: integer?, calls: integer, yes: integer}[]
function kindle.debug.predicates() end

---@class kindle.input  (permission: "input") What the player presses, and keeping it from the game.
kindle.input = {}
--- Keeps the player's actions from the game (on, or nil), or gives them back (false): its character
--- doesn't act, and the game doesn't see the D-pad or the arrow keys (they switch weapons, items and
--- spells). The camera, the menus and popups still work. One mod blocks at a time; its unload gives
--- them back.
---@param on? boolean
function kindle.input.block_actions(on) end
--- While this mod blocks the player's actions: keeps the interact button and key (A, E) from the
--- game too (on, or nil), or not (false). With them kept, the character doesn't take things, read
--- messages or rest; the menus still take them.
---@param on? boolean
function kindle.input.block_interact(on) end
--- Whether the player holds "left" (the D-pad, left stick, left arrow or A), "right" (as left) or
--- "action" (A on the controller, or E), the game seeing it or not.
---@param name "left"|"right"|"action"
---@return boolean
function kindle.input.held(name) end
--- Whether the player holds a key, a controller button or a mouse button, the game seeing it or
--- not (kindle.input.press's presses don't count). For a hotkey, the key_pressed event says when
--- one goes down.
--- `if kindle.input.down("shift") then ... end`
---@param name kindle.key
---@return boolean
function kindle.input.down(name) end
--- Where the mouse cursor is, on the 1920 x 1080 canvas kindle.ui.layer draws on (so a layer's rect
--- can be hit-tested against it), or nil while it is outside the game's window. While the player
--- moves the camera with the mouse the game keeps the cursor where it put it; in menus and screens it
--- follows the mouse.
--- `local at = kindle.input.mouse() if at and at.x < 200 then ... end`
---@return {x: number, y: number}?
function kindle.input.mouse() end
--- Whether the player has used the controller more lately than the keyboard and mouse (to name the
--- right buttons in a text).
---@return boolean
function kindle.input.using_pad() end
--- Holds a key ("esc", "up", "1"... or a DirectInput key code) or a controller button ("pad_a",
--- "pad_down"...) down for the game for `seconds` (0.1).
---@param key string|integer
---@param seconds? number
function kindle.input.press(key, seconds) end

---@class kindle.net  (permission: "net") The game's online session over Kindle's own connection
--- (no Steam or FromSoftware server: a host shares a code, players join with it): hosting,
--- joining, leaving, a lobby players gather in before a session (from the title screen on), and
--- messages to this mod on the session's or lobby's other games (a mod reaches only itself there).
kindle.net = {}
--- Send a value (boolean, number, string or table, as kindle.store keeps them; 16 KB at most as
--- JSON) to "host", "all" (every other game) or a member's id (kindle.net.members()). A player's
--- goes through the host. Without a session, over the lobby (net.lobby); an error without either.
---@param to string
---@param channel string 1 to 64 bytes
---@param value any
function kindle.net.send(to, channel, value) end
--- Call `fn(value, from)` for this mod's messages on `channel` (on Kindle's frame).
---@param channel string
---@param fn fun(value: any, from: string)
---@return integer handle
function kindle.net.on(channel, fn) end
--- A host's join data: `value` (as net.send takes) goes to each player as it joins, before its
--- game loads this world, and to the players in now; nil removes it. For what a player's game
--- needs in place as it arrives, e.g. a randomizer's seed for the maps it loads.
---@param channel string 1 to 64 bytes
---@param value any
function kindle.net.set_welcome(channel, value) end
--- A player in a host's world: the host's join data on `channel` (this mod's), or nil. There
--- from the start of the join, before the host's world loads.
---@param channel string
---@return any
function kindle.net.welcome(channel) end
--- What players joining this host's world need: a list of DLCs; a player without one is refused
--- and told why. {} for none.
---@param dlcs ("ashes_of_ariandel"|"ringed_city")[]
function kindle.net.require_dlc(dlcs) end
--- This game in the session: "host", "player", or nil without one.
---@return "host"|"player"|nil
function kindle.net.role() end
--- This game's id in the session ("0x..."), or nil.
---@return string?
function kindle.net.me() end
--- The host's id ("0x..."; this game's when it hosts), or nil.
---@return string?
function kindle.net.host() end
--- Every member's id ("0x..."), or nil without a session.
---@return string[]?
function kindle.net.members() end
--- Opens a lobby this game hosts: players gather in it before a session, from the title screen on
--- (it has the co-op port; a session hosted later shares it). Its code, for the others to join with.
---@return string
function kindle.net.open_lobby() end
--- Joins a lobby with its code (from the title screen or anywhere): Kindle connects off the game's
--- thread, then fn(result, why): "joined", "refused" (why: the host's answer) or "failed".
---@param code string
---@param fn fun(result: "joined"|"refused"|"failed", why: string?)
function kindle.net.join_lobby(code, fn) end
--- Leaves the lobby; a host closes it (its members' links end). A member playing its party save
--- has its own saves again at the main menu.
function kindle.net.leave_lobby() end
--- The lobby this game is in, or nil: its role ("host" or "member"), code (the host's), me and
--- host (ids, as members() names them), and members: {id, name (the character's, or "Player"
--- and an id's end at the title), place ("title", "loading" or "world")}. The host's also has
--- world: the world its party plays in (its character's, or at the title the one its Continue
--- will load: the last it played; nil if Kindle has seen none). While there's no session,
--- net.send and net.on carry the mod's messages between the lobby's games.
---@return {role: "host"|"member", code: string?, me: string, host: string, world: string?, members: {id: string, name: string, place: "title"|"loading"|"world"}[]}?
function kindle.net.lobby() end
--- Sends a value to this mod on the lobby's other games ("host", "all" or a member's id), as
--- net.send does, over the lobby even while there's a session (members not in it get it too).
--- Received with net.on.
---@param to string
---@param channel string
---@param value any
function kindle.net.lobby_send(to, channel, value) end
--- The lobby's host: sends a member its party save for this game's world (its character there,
--- kept by this game, or none yet), and keeps what the member's game writes back to it. The
--- member's game plays it from its main menu (the events saves.party_save_ready and the others in
--- kindle.game.catalogue()). The world: the host's character's, or at the title the one its
--- Continue will load (net.lobby().world), so a member's is in place before the host loads. The
--- world it was for.
---@param member string
---@return string world
function kindle.net.send_party_save(member) end
--- A member's character in this game's world ("host" or a member's id; this game's own too): its
--- handle (for kindle.world) and the character's name, or nil while it isn't there.
---@param id string
---@return {handle: integer, name: string}?
function kindle.net.player(id) end
--- Host a session (from the world, with a character loaded and no session). Its code is
--- kindle.net.code() once it's up.
function kindle.net.host_session() end
--- Join the session with this code (from the world, with no session), without holding up the game:
--- Kindle connects to the host on its own thread, then the game joins it (summoned to the host's
--- place), and `fn(result, why)` follows on Kindle's frame: result "joined", "gone" (nothing
--- answers at the host's address), "refused" (the host turned this player away for now: why, as it
--- says, such as "the host is on a loading screen" or a DLC this game doesn't have) or "failed"
--- (why). One join at a time; an error if one can't start.
---@param code string
---@param fn fun(result: "joined"|"gone"|"refused"|"failed", why: string?)
function kindle.net.join(code, fn) end
--- Leave the session: Kindle's connection (the others see this game leave) and the game's (a
--- player's game goes home; a host's session ends). The catalogue's event session.left follows.
--- for_now: the player means to come back (following the host to its new place): Kindle's party
--- keeps joining it to the host's world.
---@param for_now? boolean
function kindle.net.leave(for_now) end
--- The other players' names and pings in milliseconds, as the HUD shows them (a player's ping to
--- another player is through the host). Empty without a session.
---@return {name: string, ms: integer}[]
function kindle.net.pings() end
--- The game's session: "none", "creating", "create failed", "hosting", "joining", "join failed",
--- "joined" or "leaving".
---@return string
function kindle.net.state() end
--- This session's code: a host's, or the one this player joined with; nil without one. And whether
--- it names an address on the host's own network only (for the internet, kindle.toml's [coop]
--- public_address).
---@return string? code
---@return boolean is_local
function kindle.net.code() end

---@class kindle.game  (permission: "game") The catalogue: the game's decisions, values, functions and
--- notifications Kindle names and reviews for mods. Entries are named, never addresses: see
--- kindle.game.catalogue() for what this Kindle has.
kindle.game = {}
--- Every entry: { hooks = { [name] = { doc, answers, answer } }, fields = { [name] = { doc, kind,
--- character, writable } }, functions = { [name] = { doc, args } }, events = { [name] = { doc,
--- args } } }. This Kindle's: the hook points "bonfire.rest_requested" (bonfire, sitting:
--- "allow" or "hold" the rest; held, the player sits on unrested), "bonfire.travel_requested"
--- ("allow" or "refuse" the travel screen), "chr.departure_hit" ("count" or "ignore" a death
--- floor's hit on another player's character, which would fade it out) and "load.bloodstain"
--- ("host_only" or "any": whose bloodstain a load shows); where the game treats a phantom apart
--- from the host: "events.chr_type_test", "events.own_world_test" (event, slot: "own_world" answers a joined player's own-world test as the host's game does), "events.multiplayer_state_test" (event, slot, state: "single_player" answers as a game alone), "npc.guest_look", "cutscene.skip_owner",
--- "chr.ember_look" (an embered player's look, the HUD's fiery hexagon too, for any type),
--- "load.estus_allotment", "bonfire.level_copy", "bonfire.session_list" ("keep": a joined
--- player's list of the bonfires' fires is the host's after each of its loads, so its bonfires are lit as the host's),
--- "talk.player_check", "talk.in_session",
--- "talk.npc_ready", and a guest's loot: "loot.guest_loot", "loot.guest_item_ids",
--- "loot.treasure_flag", "loot.item_messages", "loot.guest_awards", "enemy.host_hp_death"; where
--- the game's multiplayer flow sends a player home: "flow.boss_clear_fade", "flow.boss_clear_home",
--- "flow.boss_death_leaving", "flow.boss_leave_session", "flow.leave_limit",
--- "world.reload_requested", "ui.network_notice" (row); and loading home: "load.phantom_copy"
--- (asked), "load.player_type" (saved type), "chr_type.keep_home_type"; a hosted session's ends:
--- "session.leave_requested", "session.close_requested"; "ui.hidden_name_tags" (no name tag over
--- a character not drawn); "ui.banner" (type: "show" or "hide" one of the game's banners; the game
--- shows "EMBER RESTORED" for another player's character embering too) (each with its answers in
--- catalogue());
--- the fields "bonfire.sitting",
--- "ui.menu_open", "ui.action_prompt", "ui.esc_menu_open", "ui.item_popup" (read only) and
--- "chr.fall_time" (a character's, seconds in the air), "title.screen" (0 not at the title, 1
--- before its main menu, 2 the main menu); the functions "bonfire.rest" (entity: rest
--- there as the game does), "bonfire.light_fire" (entity: light a bonfire's fire whose flag was
--- set), "bloodstain.mark" and "bloodstain.leave" (the player's souls left where it was marked
--- going down) and "chr.stand_on" (character, other: take the other's floor and the rooms it
--- sees), "chr_type.get" and "chr_type.set" (the per-type table), "inventory.use_own_key_items", "flags.use_own_table" (digit: a session set uses the
--- character's table), "flags.separate_block" (first: a session set's own copy of a block) and
--- "world.send_home" (the player home as the game's flow sends it), "world.quit_to_title" (to the
--- title's main menu as the pause menu's Quit Game goes), "title.back_to_press_start" (the main menu
--- back to "PRESS ANY BUTTON", built anew after: its first entry chosen);
--- the field "inventory.own_key_items"; the events "player.fall_death_blocked" (a fall death the
--- no-death flag kept off), "flags.network_set" (first, last, on) and "flags.plain_set" (first,
--- last, random: the random roll, each game's own; synced: the event keeps the game's network
--- sync on, off for an event that runs for this game alone; event: its id), the game's event scripts setting flags on this
--- game, "flags.changed" (flag, on, session,
--- received: every flag the game changes, its own code's too, as a summon sign's; received, one
--- another game of the session sent), "flow.boss_clear_stayed" (a
--- boss's clear left this player where it is) and "session.left" (by_this_game: Kindle's session
--- ended, by this game's leave or the link to the host closing), "lobby.closed" (the lobby's host
--- closed it) and a lobby member's party save (kindle.net.send_party_save):
--- "saves.party_save_ready" (has_character: put in place at the main menu; without a character the
--- menu offers New Game), "saves.party_save_waiting" (it came while a character is loaded: quit
--- to the title for it), "saves.party_save_left" (the lobby left while playing it) and
--- "saves.own_saves_back" (the member's own saves at the main menu again).
---@return table
function kindle.game.catalogue() end
--- Answers a hook point (one of its answers; its first, the game's own, takes this mod's back).
--- Mods that answer a hook point give the same answer (a different one is an error); it goes
--- back to the game's when they unload.
---@param name string
---@param answer string
function kindle.game.decide(name, answer) end
--- Answers each call of a hook point: fn gets the call's values and returns an answer, or nil
--- for the switch's (kindle.game.decide). Only calls on the game's thread ask. Kindle hooks the
--- game there when a mod first answers (an error if it can't on this game build).
---@param name string
---@param fn fun(...: integer): string?
---@return integer handle
function kindle.game.on_decide(name, fn) end
--- A field's value; a character's field takes the character's handle.
---@param name string
---@param handle? integer
---@return boolean|number
function kindle.game.get(name, handle) end
--- Sets a field (checked against its kind and range).
---@param name string
---@param value boolean|number
---@param handle? integer
function kindle.game.set(name, value, handle) end
--- Calls a game function (its arguments checked against their kinds and ranges).
---@param name string
---@return boolean|number|nil
function kindle.game.call(name, ...) end
--- A catalogue event's handler (as kindle.on); its values come as arguments.
---@param name string
---@param fn function
---@return integer handle
function kindle.game.on(name, fn) end
