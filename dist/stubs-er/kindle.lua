---@meta
-- Kindle for Elden Ring: API type stubs for the Lua language server (autocomplete and docs in VS Code;
-- point Lua.workspace.library at this folder, apart from Dark Souls III's dist/stubs),
-- and what `kindle check` checks a mod with `games = ["er"]` against. They describe the API; the game
-- never loads them. Kindle's own API (kindle.on, timers, the store, settings, other mods) is the same as
-- in Dark Souls III; the game's modules are Elden Ring's.

---@class kindle  Events, timers and this mod's own info. Always available.
---@field version string Kindle runtime version
---@field api integer the Lua API version (mods set `api` in mod.toml to the one they were written for)
---@field mod {id: string, name: string, version: string} This mod's manifest info
kindle = {}

---@alias kindle.event "frame"|"player_loaded"|"player_unloaded"|"unload"|"setting_changed"|"player_died"|"map_changed"|"character_killed"|"character_spawned"|"item_gained"|"item_lost"|"effect_applied"|"effect_expired"|"level_up"

--- Subscribe to an event. `frame` handlers receive the seconds since the last frame.
--- `player_loaded` fires when a character is in the world: once the loading screen is over (and at
--- once on a (re)load if one already is). `unload` fires before the mod is reloaded or removed.
---
--- Game events (the game's state compared from frame to frame, only while a mod listens):
--- * `player_died()`
--- * `map_changed(new, old)` the map block the player is in, by its file name ("m60_42_36_00"):
---   a warp, or walking from one open-world tile into the next
--- * `character_killed(character)` a character's HP reached 0 (as world.characters lists it)
--- * `character_spawned(character)` a character appeared; not those already there when the player
---   arrives. Checked every 5 frames, as the inventory and effects are
--- * `item_gained(item, count, name)` / `item_lost(item, count, name)` inventory changes
--- * `effect_applied(id)` / `effect_expired(id)` SpEffects on the player
--- * `level_up(level, old_level, raised, runes)` the level rose: `raised` maps each attribute that
---   rose to its new value (`{ vigor = 21 }`), `runes` is how many went with it (the cost)
---@param event kindle.event
---@param fn function
---@return integer handle
---@overload fun(event: "frame", fn: fun(dt: number)): integer
---@overload fun(event: "player_loaded"|"player_unloaded"|"unload"|"player_died", fn: fun()): integer
---@overload fun(event: "setting_changed", fn: fun(name: string, value: boolean|number|string)): integer
---@overload fun(event: "map_changed", fn: fun(new: string, old: string)): integer
---@overload fun(event: "character_killed"|"character_spawned", fn: fun(character: kindle.character)): integer
---@overload fun(event: "item_gained"|"item_lost", fn: fun(item: kindle.item, count: integer, name: string?)): integer
---@overload fun(event: "effect_applied"|"effect_expired", fn: fun(id: integer)): integer
---@overload fun(event: "level_up", fn: fun(level: integer, old_level: integer, raised: table<string, integer>, runes: integer)): integer
function kindle.on(event, fn) end

--- A readable string for any value: tables show their contents (`depth` levels, default 3).
--- kindle.log uses it for tables too.
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

--- Frames since Kindle started.
---@return integer
function kindle.frame() end

--- Offer functions to other mods: kindle.export{ greet = function(name) ... end }. Calls from other
--- mods run in this mod; arguments and results are copied (plain data). Calling it again replaces them.
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

--- State that survives the mod being reloaded (saving its files reloads it), for this session: the
--- value kept under `key` when the mod last unloaded, else `default`. Change a kept table in place.
---@generic T
---@param key string
---@param default T
---@return T
function kindle.keep(key, default) end

---@class kindle.settings  The mod's settings: options declared in mod.toml ([settings.<name>] with
--- type "bool", "int", "float" or "choice", label, description, default, min/max or choices) that
--- players change in the overlay's Mods tab. Always available.
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
---@param id string
---@return table<string, function>?
function kindle.mods.get(id) end
--- Ids of the running mods.
---@return string[]
function kindle.mods.running() end


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

---@class kindle.log  Write to Kindle's log (the overlay's Log tab and kindle\logs\kindle.log). Always available.
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

---@alias kindle.storable nil|boolean|number|string|table A value kindle.store can keep: tables must be lists or have string keys.

---@class kindle.store  Values a mod keeps between sessions. Always available. `get`/`set` keep values
--- for the loaded character (its save slot); `get_global`/`set_global` for the mod itself.
kindle.store = {}
--- A value saved for the loaded character, or `default` if it was never set.
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

---@class kindle.flags  (permission: "flags") Read, set and watch the game's event flags (bosses killed,
--- graces found, doors opened, NPC quests). The game keeps them in blocks of 1000.
kindle.flags = {}
--- A flag's state (false for an id in none of the game's blocks).
---@param id integer
---@return boolean
function kindle.flags.get(id) end
--- Set a flag (true by default) through the game's own setter, as its event scripts do; true if it now
--- has that state. The change stays on this game (not queued for the game's sync of flags).
---@param id integer
---@param state? boolean
---@return boolean
function kindle.flags.set(id, state) end
--- The block holding `id`: its flags as 32 words (highest bit first), then its first flag; nil if none.
---@param id integer
---@return integer[]?, integer?
function kindle.flags.block(id) end
--- Call fn(id) when the flag turns on (checked once a frame).
---@param id integer
---@param fn fun(id: integer)
---@return integer handle
function kindle.flags.on_set(id, fn) end
--- Call fn(id) when the flag turns off.
---@param id integer
---@param fn fun(id: integer)
---@return integer handle
function kindle.flags.on_clear(id, fn) end

---@alias kindle.item integer An item: its param row id with its category (items.weapon(2000000)).
---@alias kindle.itemref kindle.item|string An item, or its name in the game's language ("Longsword", "Dagger +5").

---@class kindle.items  (permission: "items") Find items by id or name; give, take and count them; rename them.
kindle.items = {}
--- A weapon (EquipParamWeapon row), or one by name.
---@param id integer|string
---@return kindle.item
function kindle.items.weapon(id) end
--- An armor piece (EquipParamProtector row), or one by name.
---@param id integer|string
---@return kindle.item
function kindle.items.armor(id) end
--- A talisman (EquipParamAccessory row), or one by name. items.ring is the same (Dark Souls III's name).
---@param id integer|string
---@return kindle.item
function kindle.items.talisman(id) end
---@param id integer|string
---@return kindle.item
function kindle.items.ring(id) end
--- A goods item (EquipParamGoods row: consumables, materials, key items, spells), or one by name.
---@param id integer|string
---@return kindle.item
function kindle.items.goods(id) end
--- An Ash of War (EquipParamGem row), or one by name.
---@param id integer|string
---@return kindle.item
function kindle.items.ash(id) end
--- The item with this exact name (any case, any category). Errors if unknown or ambiguous.
---@param name string
---@return kindle.item
function kindle.items.id(name) end
--- The item's name in the game's language (nil if it has none).
---@param item kindle.itemref
---@return string?
function kindle.items.name(item) end
--- Names holding `query`: {name, category, id, item} each, exact names first (20 at most by default).
---@param query string
---@param limit? integer
---@return {name: string, category: string, id: integer, item: kindle.item}[]
function kindle.items.find(query, limit) end
--- Give the player an item (1 by default, at most 999), as the game's event scripts do. opts.popup
--- shows the game's own "item received" box (opts.on_done(done, err) when it ran). On a loaded character.
---@param item kindle.itemref
---@param quantity? integer
---@param opts? {popup: boolean?, on_done: fun(done: boolean, err: string?)?}
function kindle.items.give(item, quantity, opts) end
--- Take up to `quantity` (1 by default) of an item from the player.
---@param item kindle.itemref
---@param quantity? integer
function kindle.items.remove(item, quantity) end
--- How many of an item the player holds.
---@param item kindle.itemref
---@return integer
function kindle.items.count(item) end
--- Whether the player holds at least `quantity` (1 by default).
---@param item kindle.itemref
---@param quantity? integer
---@return boolean
function kindle.items.has(item, quantity) end
--- An item's category ("weapon", "armor", "talisman", "goods", "ash") and param row id.
---@param item kindle.itemref
---@return string category, integer id
function kindle.items.info(item) end
--- What the player holds, in the inventory's order.
---@return {item: kindle.item, kind: string, id: integer, quantity: integer}[]
function kindle.items.held() end
--- Rename an item wherever the game shows it (a row a mod added gets one). Undone when the mod
--- unloads. Needs "text" too.
---@param item kindle.itemref
---@param name string
function kindle.items.set_name(item, name) end
--- Replace an item's description (its long text). Needs "text" too.
---@param item kindle.itemref
---@param text string
function kindle.items.set_description(item, text) end
--- Replace an item's info (the short line under its name). Needs "text" too.
---@param item kindle.itemref
---@param text string
function kindle.items.set_info(item, text) end
--- The game's name, description and info back. Needs "text" too.
---@param item kindle.itemref
function kindle.items.reset_text(item) end

---@alias kindle.text_category "weapon_name"|"weapon_info"|"weapon_caption"|"weapon_effect"|"armor_name"|"armor_info"|"armor_caption"|"talisman_name"|"talisman_info"|"talisman_caption"|"goods_name"|"goods_info"|"goods_caption"|"goods_dialog"|"ash_name"|"ash_info"|"ash_caption"|"ash_effect"|"skill_name"|"skill_caption"|"npc_name"|"place_name"|"talk"|"blood_message"|"network_message"|"action_button"|"event_text"|"event_text_map"|"menu_text"|"line_help"|"key_guide"

---@class kindle.text  (permission: "text") Read, replace and add to the game's text, live.
---@field categories string[] the category names text.get takes
kindle.text = {}
--- What the game shows for (category, id), in its language (an FMG id works as the category). nil if none.
---@param category kindle.text_category|integer
---@param id integer
---@return string?
function kindle.text.get(category, id) end
--- Replace that text wherever the game shows it, or add it if the game has none; how many of the game's
--- tables changed. Undone when the mod unloads.
---@param category kindle.text_category|integer
---@param id integer
---@param value string
---@return integer
function kindle.text.set(category, id, value) end
--- The game's text back.
---@param category kindle.text_category|integer
---@param id integer
function kindle.text.reset(category, id) end
--- An item's "name", "info" or "description".
---@param item kindle.itemref
---@param field? "name"|"info"|"description"
---@return string?
function kindle.text.item(item, field) end

---@class kindle.param_row A live param row: read and assign fields by name (row.weight = 2.0).

---@class kindle.params  (permission: "params") Read and change the game's param tables (weapon stats,
--- effects, enemies, item lots ...) live. Field names are Paramdex's. Writes are undone when the mod unloads.
kindle.params = {}
--- A live row, or nil if the table has no such row.
---@param table string e.g. "EquipParamWeapon"
---@param id integer
---@return kindle.param_row?
function kindle.params.get(table, id) end
--- Add a row, a copy of `copy_from`, and return it. The id must be above every row but the table's end
--- rows (999999000 and up) and, in an item table, under 2^28. The row stays for the session.
---@param table string
---@param id integer
---@param copy_from integer
---@return kindle.param_row
function kindle.params.add_row(table, id, copy_from) end
--- Several rows with one rebuild of the table: {{id =, copy_from =}, ...}, ids ascending.
---@param table string
---@param rows {id: integer, copy_from: integer}[]
---@return kindle.param_row[]
function kindle.params.add_rows(table, rows) end
--- Copy rows over others in one go: {{from, to}, ...}, but the fields named in `keep`.
---@param table string
---@param pairs integer[][]
---@param keep? string[]
function kindle.params.copy(table, pairs, keep) end
--- Put back what this mod wrote: one row's fields, a table's, or all (no arguments). How many fields.
---@param table? string
---@param row? integer
---@return integer
function kindle.params.reset(table, row) end
--- The tables the game has loaded.
---@return string[]
function kindle.params.tables() end
--- A table's fields: {name, type, offset, count?, bit?, bits?} each.
---@param table string
---@return {name: string, type: string, offset: integer, count: integer?, bit: integer?, bits: integer?}[]
function kindle.params.fields(table) end
--- A table's row ids, in order.
---@param table string
---@return integer[]
function kindle.params.rows(table) end

---@alias kindle.attribute "vigor"|"mind"|"endurance"|"strength"|"dexterity"|"intelligence"|"faith"|"arcane"

---@class kindle.grace
---@field id integer its BonfireWarpParam row
---@field name string?
---@field entity integer its asset's entity id
---@field map string

---@class kindle.player  (permission: "player") The player: HP, FP, stamina, attributes, level, runes,
--- effects, position, warps.
kindle.player = {}
--- Whether a character is in the world.
---@return boolean
function kindle.player.loaded() end
---@return string
function kindle.player.name() end
--- The attributes: {vigor = ..., mind, endurance, strength, dexterity, intelligence, faith, arcane}.
---@return table<kindle.attribute, integer>
function kindle.player.stats() end
---@return integer
function kindle.player.hp() end
---@return integer
function kindle.player.max_hp() end
--- Set HP (1 to the maximum, which follows the attributes).
---@param v integer
function kindle.player.set_hp(v) end
---@return integer
function kindle.player.fp() end
---@return integer
function kindle.player.max_fp() end
---@param v integer
function kindle.player.set_fp(v) end
---@return integer
function kindle.player.stamina() end
---@return integer
function kindle.player.max_stamina() end
---@param v integer
function kindle.player.set_stamina(v) end
--- HP, FP and stamina to their maxima.
function kindle.player.heal() end
--- x, y (up), z in the physics' frame, shifted from the map's own coordinates: move relative to a read.
---@return number x, number y, number z
function kindle.player.position() end
--- Move the player to x, y (up), z, in the coordinates position gives; they fall if nothing is under them.
---@param x number
---@param y number
---@param z number
function kindle.player.set_position(x, y, z) end
---@return integer
function kindle.player.runes() end
--- Set the runes held (0 to 999,999,999).
---@param n integer
function kindle.player.set_runes(n) end
---@param n integer
function kindle.player.add_runes(n) end
--- Runes gained over the whole playthrough.
---@return integer
function kindle.player.total_runes() end
---@return integer
function kindle.player.level() end
--- Set the level (1 to 713), the attributes as they are.
---@param level integer
function kindle.player.set_level(level) end
--- Set an attribute (1 to 99); the level moves by as much unless keep_level.
---@param name kindle.attribute
---@param value integer
---@param keep_level? boolean
function kindle.player.set_stat(name, value, keep_level) end
--- Several at once: player.set_stats({vigor = 20, strength = 18}).
---@param t table<kindle.attribute, integer>
---@param keep_level? boolean
function kindle.player.set_stats(t, keep_level) end
--- Warp to a site of grace (its BonfireWarpParam row, or its name: "Church of Elleh") as the game's own
--- warps do. On Kindle's next frame; on_done(done, err).
---@param grace integer|string
---@param on_done? fun(done: boolean, err: string?)
function kindle.player.warp(grace, on_done) end
--- Every site of grace.
---@return kindle.grace[]
function kindle.player.graces() end
--- Apply an SpEffect (SpEffectParam row) to the player.
---@param id integer
function kindle.player.apply_effect(id) end
--- End an active SpEffect; how many instances ended.
---@param id integer
---@return integer
function kindle.player.remove_effect(id) end
--- The player's active SpEffects.
---@return integer[]
function kindle.player.effects() end
---@param id integer
---@return boolean
function kindle.player.has_effect(id) end
--- Dark Souls III's names, for mods written for both.
---@return integer
function kindle.player.souls() end
---@param n integer
function kindle.player.set_souls(n) end
---@param n integer
function kindle.player.add_souls(n) end
---@return integer
function kindle.player.total_souls() end
---@return integer
function kindle.player.soul_level() end
---@param level integer
function kindle.player.set_soul_level(level) end

---@class kindle.character
---@field handle integer
---@field entity integer? its map entity id (the game's commands need one)
---@field npc_id integer NpcParam row
---@field think_id integer NpcThinkParam row
---@field name string?
---@field model string e.g. "c4730"
---@field kind "player"|"npc"|"enemy"|"ally"|"helper"|"other"
---@field team integer
---@field hp integer
---@field max_hp integer
---@field alive boolean
---@field map integer its map block (m60_42_36_00 is 0x3C2A2400)
---@field is_player boolean
---@field x number
---@field y number
---@field z number
---@field distance number? metres from the player (nil across a legacy dungeon and the open world)

---@alias kindle.charref kindle.character|integer A character: its table, entity id (10000: the player) or handle.

---@class kindle.world  (permission: "world") Other characters: enemies, NPCs, players.
---@field PLAYER integer 10000, the player as event scripts name it
kindle.world = {}
--- Loaded characters, nearest first. opts: radius (metres), alive (default true), player (default
--- false), helpers (default false: the maps' invisible event characters), kind.
---@param opts? {radius: number?, alive: boolean?, player: boolean?, helpers: boolean?, kind: string?}
---@return kindle.character[]
function kindle.world.characters(opts) end
--- One character, as world.characters lists it, or nil if it isn't loaded.
---@param c kindle.charref
---@return kindle.character?
function kindle.world.character(c) end
---@param c kindle.charref
---@return boolean
function kindle.world.exists(c) end
---@param c kindle.charref
---@return number x, number y, number z
function kindle.world.position(c) end
--- Set its HP (0 to its maximum).
---@param c kindle.charref
---@param hp integer
function kindle.world.set_hp(c, hp) end
---@param c kindle.charref
function kindle.world.heal(c) end
--- Kill it as the game's scripts do (runes for the player if give_runes), on Kindle's next frame.
---@param c kindle.charref
---@param give_runes? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.kill(c, give_runes, on_done) end
---@param c kindle.charref
---@param id integer
function kindle.world.apply_effect(c, id) end
---@param c kindle.charref
---@param id integer
---@return integer
function kindle.world.remove_effect(c, id) end
---@param c kindle.charref
---@return integer[]
function kindle.world.effects(c) end
--- Its AI on or off (the game's command; entity ids only, as set_enabled, set_immortal, set_invincible, set_hp_bar).
---@param c kindle.charref
---@param on? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_ai(c, on, on_done) end
--- Shown and active, or hidden.
---@param c kindle.charref
---@param on? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_enabled(c, on, on_done) end
---@param c kindle.charref
---@param on? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_immortal(c, on, on_done) end
---@param c kindle.charref
---@param on? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_invincible(c, on, on_done) end
---@param c kindle.charref
---@param on? boolean
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_hp_bar(c, on, on_done) end
--- Its team: a number or a name ("enemy", "ally", "friendly_npc", "hostile_npc", "charmed" ...).
---@param c kindle.charref
---@param team integer|string
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.set_team(c, team, on_done) end
--- Forget its target and choose what to do again.
---@param c kindle.charref
---@param on_done? fun(done: boolean, err: string?)
function kindle.world.clear_aggro(c, on_done) end
--- Distance between two characters, or a character and a point {x, y, z} in its block's coordinates.
---@param a kindle.charref
---@param b kindle.charref|{x: number, y: number, z: number}
---@return number?
function kindle.world.distance(a, b) end

---@alias kindle.color "gold"|"white"|"red"|"blue"|"green"|string  string = "#rrggbb"

---@class kindle.ui  (permission: "ui") Messages: the game's own, and Kindle's drawn over the game.
---@field banners table<string, integer> the game's banners by name
kindle.ui = {}
--- Text as the game's own message: kind "blinking" (default), "full_screen", "status", "area", "subarea".
---@param text string
---@param kind? "blinking"|"full_screen"|"status"|"area"|"subarea"
---@param on_done? fun(done: boolean, err: string?)
function kindle.ui.message(text, kind, on_done) end
--- One of the game's banners: "god_slain", "great_enemy_felled", "grace_discovered", "you_died" ...
---@param kind string|integer
---@param on_done? fun(done: boolean, err: string?)
function kindle.ui.game_banner(kind, on_done) end
--- Big centred text on a dark band (Kindle's overlay). opts: seconds (3), color.
---@param message string
---@param opts? {seconds: number?, color: kindle.color?}
function kindle.ui.banner(message, opts) end
--- A small notification in the top-right corner. opts: seconds (4), color.
---@param message string
---@param opts? {seconds: number?, color: kindle.color?}
function kindle.ui.toast(message, opts) end
--- An action prompt at the bottom of the screen until cleared (nil). Each mod has its own; when
--- two have one, the one set last shows.
---@param text string?
function kindle.ui.prompt(text) end
--- A line across the screen until cleared (nil), with a progress bar (0 to 1) if given. Each mod
--- has its own line, stacked with the others' in the order they were first set; it goes when the
--- mod unloads or stops.
---@param text string?
---@param progress? number
function kindle.ui.status(text, progress) end

---@class kindle.event_command {bank: integer, index: integer, format: string?, values: number[]?}

---@class kindle.events  (permission: "events") Run the game's own event script (EMEVD) commands, and
--- change its event scripts. Maps are named by their event file ("m60_42_36_00", "common",
--- "common_func"); changes apply from the next time that map's events load (a warp or a load), in the
--- order mods make them. Command numbers start at 1.
---@field commands table<string, {[1]: integer, [2]: integer, [3]: string, [4]: string}> the named commands: bank, index, format, argument names
---@field PLAYER integer 10000
kindle.events = {}
--- Run a command: bank, index, the arguments' format (b/B i8/u8, h/H i16/u16, i/I i32/u32, f f32) and
--- values, then optionally on_done(done, err). On Kindle's next frame, through the game's own handler.
---@param bank integer
---@param index integer
---@param format? string
---@param ... any
function kindle.events.run(bank, index, format, ...) end
--- The game's own event: {id, rest, commands = {{bank, index, args, bytes, layer}}, parameters}, or nil.
---@param map string
---@param id integer
---@return table?
function kindle.events.script(map, id) end
--- The event as the game will run it, with every mod's changes.
---@param map string
---@param id integer
---@return table?
function kindle.events.view(map, id) end
--- Change an event, all or nothing: steps {check = n, is = {...}}, {remove = n},
--- {replace = n, bank =, index =, format =, values =}, {insert = n, ...}. `why` goes to the log.
---@param map string
---@param id integer
---@param steps table[]
---@param why? string
function kindle.events.edit(map, id, steps, why) end
--- Every command bank[index] in a map's event file: {{event, command, args, bytes}}.
---@param map string
---@param bank integer
---@param index integer
---@return table[]
function kindle.events.find(map, bank, index) end
--- Drop this mod's changes to a map's events; how many.
---@param map string
---@return integer
function kindle.events.reset(map) end
--- The event does nothing (it ends at once).
---@param map string
---@param id integer
function kindle.events.disable(map, id) end
--- Remove command n.
---@param map string
---@param id integer
---@param n integer
function kindle.events.remove(map, id, n) end
--- Insert a command before number n (the count + 1 adds it at the end).
---@param map string
---@param id integer
---@param n integer
---@param bank integer
---@param index integer
---@param format? string
---@param ... number
function kindle.events.insert(map, id, n, bank, index, format, ...) end
--- Replace command n.
---@param map string
---@param id integer
---@param n integer
---@param bank integer
---@param index integer
---@param format? string
---@param ... number
function kindle.events.replace(map, id, n, bank, index, format, ...) end
--- A new event; rest: 0 restart when the map reloads (default), 1 end, 2 end and restart.
---@param map string
---@param id integer
---@param commands kindle.event_command[]
---@param rest? integer
function kindle.events.add(map, id, commands, rest) end
--- Start an event when the map's events load (as its event 0 does): slot, then its arguments.
---@param map string
---@param id integer
---@param slot? integer
---@param format? string
---@param ... number
function kindle.events.start(map, id, slot, format, ...) end
--- Named commands (each takes its arguments, then optionally on_done):
---@param lot integer
function kindle.events.award_item_lot(lot) end
---@param area integer
---@param block integer
---@param region integer
---@param index integer
---@param start_entity integer
---@param place_name integer
function kindle.events.warp_player(area, block, region, index, start_entity, place_name) end
---@param on boolean
---@param entity integer
---@param slot integer
---@param name_id integer
function kindle.events.show_boss_bar(on, entity, slot, name_id) end
---@param entity integer
---@param animation integer
---@param loop boolean
---@param wait boolean
---@param skip_transition boolean
---@param comparison integer
---@param targets number
function kindle.events.force_animation(entity, animation, loop, wait, skip_transition, comparison, targets) end
---@param entity integer
function kindle.events.set_respawn_point(entity) end
---@param gesture integer
function kindle.events.award_gesture(gesture) end
---@param entity integer
---@param on boolean
function kindle.events.set_ai(entity, on) end
---@param entity integer
---@param team integer|string
function kindle.events.set_team(entity, team) end
---@param entity integer
---@param give_runes boolean
function kindle.events.kill(entity, give_runes) end
---@param entity integer
---@param on boolean
function kindle.events.set_enabled(entity, on) end
---@param entity integer
---@param speffect integer
function kindle.events.apply_effect(entity, speffect) end
---@param entity integer
---@param on boolean
function kindle.events.set_immortal(entity, on) end
---@param entity integer
---@param on boolean
function kindle.events.set_invincible(entity, on) end
---@param entity integer
function kindle.events.clear_ai_target(entity) end
---@param entity integer
---@param command integer
---@param slot integer
function kindle.events.ai_command(entity, command, slot) end
---@param entity integer
---@param ai_id integer
function kindle.events.set_ai_id(entity, ai_id) end
---@param entity integer
function kindle.events.ai_replan(entity) end
---@param entity integer
---@param speffect integer
function kindle.events.remove_effect(entity, speffect) end
---@param entity integer
---@param on boolean
function kindle.events.set_hp_bar(entity, on) end
function kindle.events.grace_recovery() end
---@param entity integer
---@param root_only boolean
function kindle.events.delete_map_sfx(entity, root_only) end
---@param entity integer
function kindle.events.spawn_map_sfx(entity) end
---@param target_type integer
---@param entity integer
---@param dummypoly integer
---@param sfx integer
function kindle.events.spawn_sfx(target_type, entity, dummypoly, sfx) end
---@param banner string|integer
function kindle.events.banner(banner) end
---@param text_id integer
---@param pad boolean
function kindle.events.status_message(text_id, pad) end
---@param text_id integer
function kindle.events.blinking_message(text_id) end
---@param text_id integer
function kindle.events.full_screen_message(text_id) end
---@param text_id integer
function kindle.events.subarea_name(text_id) end
---@param text_id integer
function kindle.events.area_name(text_id) end
---@param shake_id integer
---@param target_type integer
---@param entity integer
---@param dummypoly integer
---@param fade_start number
---@param fade_end number
function kindle.events.camera_shake(shake_id, target_type, entity, dummypoly, fade_start, fade_end) end
---@param entity integer
---@param sound_type integer|string
---@param sound_id integer
function kindle.events.play_sound(entity, sound_type, sound_id) end
---@param entity integer
---@param on boolean
function kindle.events.set_map_part(entity, on) end

---@class kindle.game  (permission: "game") The catalogue: the game's decisions, values, functions and
--- notifications Kindle names and reviews for mods. Elden Ring's has no entries yet.
kindle.game = {}
---@return table
function kindle.game.catalogue() end
---@param name string
---@param answer string
function kindle.game.decide(name, answer) end
---@param name string
---@param fn fun(...: integer): string?
---@return integer handle
function kindle.game.on_decide(name, fn) end
---@param name string
---@param handle? integer
---@return boolean|number
function kindle.game.get(name, handle) end
---@param name string
---@param value boolean|number
---@param handle? integer
function kindle.game.set(name, value, handle) end
---@param name string
---@return boolean|number|nil
function kindle.game.call(name, ...) end
---@param name string
---@param fn function
---@return integer handle
function kindle.game.on(name, fn) end
