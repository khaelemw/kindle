-- Elden Ring's part of the `kindle` API: its modules, built from the private `native` table.
-- Kindle's prelude (kindle-core's prelude.lua) runs it once per Lua state, after the sandbox and
-- Kindle's own API, with the `kindle` table so far and Kindle's argument check. Only the modules
-- listed in `allowed` are exposed. So far: flags, items, params, player, events, world, ui and text.

local native, allowed, info, kindle, check = ...
local fmt, type, setmetatable, ipairs, pairs, error = string.format, type, setmetatable, ipairs, pairs, error
local tremove = table.remove

-- The game's event script commands by name: {bank, index, the arguments' format, their names}, each
-- from Elden Ring's instruction table (format letters as events.run's).
local EVENT_COMMANDS = {
  award_item_lot = { 2003, 4, "i", "lot" },
  warp_player = { 2003, 14, "BBBBIi", "area, block, region, index, start_entity, place_name" },
  show_boss_bar = { 2003, 11, "bIhi", "on, entity, slot, name_id" },
  force_animation = { 2003, 18, "IiBBBbf", "entity, animation, loop, wait, skip_transition, comparison, targets" },
  set_respawn_point = { 2003, 23, "I", "entity" },
  award_gesture = { 2003, 71, "i", "gesture" },
  set_ai = { 2004, 1, "IB", "entity, on" },
  set_team = { 2004, 2, "IB", "entity, team" },
  kill = { 2004, 4, "IB", "entity, give_runes" },
  set_enabled = { 2004, 5, "IB", "entity, on" },
  apply_effect = { 2004, 8, "Ii", "entity, speffect" },
  set_immortal = { 2004, 12, "IB", "entity, on" },
  set_invincible = { 2004, 15, "IB", "entity, on" },
  clear_ai_target = { 2004, 16, "I", "entity" },
  ai_command = { 2004, 17, "IiB", "entity, command, slot" },
  set_ai_id = { 2004, 19, "Ii", "entity, ai_id" },
  ai_replan = { 2004, 20, "I", "entity" },
  remove_effect = { 2004, 21, "Ii", "entity, speffect" },
  set_hp_bar = { 2004, 30, "IB", "entity, on" },
  grace_recovery = { 2004, 47, "", "" },
  delete_map_sfx = { 2006, 1, "IB", "entity, root_only" },
  spawn_map_sfx = { 2006, 2, "I", "entity" },
  spawn_sfx = { 2006, 3, "iIii", "target_type, entity, dummypoly, sfx" },
  banner = { 2007, 2, "B", "banner" },
  status_message = { 2007, 3, "iB", "text_id, pad" },
  blinking_message = { 2007, 4, "i", "text_id" },
  full_screen_message = { 2007, 9, "i", "text_id" },
  subarea_name = { 2007, 13, "i", "text_id" },
  area_name = { 2007, 14, "i", "text_id" },
  camera_shake = { 2008, 2, "iiIiff", "shake_id, target_type, entity, dummypoly, fade_start, fade_end" },
  play_sound = { 2010, 2, "Iii", "entity, sound_type, sound_id" },
  set_map_part = { 2012, 1, "IB", "entity, on" },
}
-- Names a named command's arguments take for their numbers.
local EVENT_NAMES = {
  banner = { you_died = 2, host_vanquished = 5, grace_discovered = 13, legend_felled = 15, demigod_felled = 16,
    great_enemy_felled = 17, enemy_felled = 18, duty_fulfilled = 20, map_found = 22, great_rune_restored = 26,
    god_slain = 27, duelist_vanquished = 28, recusant_vanquished = 29, invader_vanquished = 30, commence = 31,
    stalemate = 32, victory = 33, defeat = 34 },
  team = { human = 1, enemy = 6, strong_enemy = 7, ally = 8, hostile_ally = 9, charmed = 15, host = 19, coop = 20,
    friendly_npc = 26, hostile_npc = 27, coop_npc = 28, indiscriminate = 29 },
  sound_type = { environment = 0, motion = 1, menu = 2, asset = 3, cutscene = 4, sfx = 5, bgm = 6, voice = 7 },
}
-- The player, as event scripts name it.
local PLAYER = 10000

-- Runs a named command: its arguments as its names say (true/false as 1/0, a name for a number where
-- EVENT_NAMES has one), on_done(done, err) as for events.run.
local function run_named(name, args, on_done)
  local c = EVENT_COMMANDS[name]
  local names = {}
  for n in c[4]:gmatch("[%w_]+") do names[#names + 1] = n end
  local values = {}
  for i, n in ipairs(names) do
    local v = args[i]
    if type(v) == "boolean" then v = v and 1 or 0 end
    if type(v) == "string" and EVENT_NAMES[n] then
      local known = EVENT_NAMES[n][v]
      check(known ~= nil, fmt("events.%s: unknown %s %q", name, n, v))
      v = known
    end
    check(type(v) == "number", fmt("events.%s: %s must be a number (arguments: %s)", name, n, c[4]))
    values[i] = v
  end
  return native.events_run(c[1], c[2], c[3], values, on_done)
end

-- ── flags ───────────────────────────────────────────────────────────────────
if allowed.flags then
  local flags = {}
  --- A flag's state (false for an id in none of the game's blocks of flags, as the game reads it).
  function flags.get(id) return native.flag_get(id) end
  --- Sets a flag (true by default) through the game's own setter, as its event scripts do; true if it
  --- now has that state (false for an id in none of the game's blocks). The change stays on this
  --- game: it isn't queued for the game's own sync of flags.
  function flags.set(id, state)
    if state == nil then state = true end
    return native.flag_set(id, state)
  end
  --- The game's block of flags holding `id` (it keeps them in blocks of 1000): its flags as 32
  --- words, the highest bit first (the block's first flag is bit 31 of the first word), then its
  --- first flag; nil when the game has no such block.
  function flags.block(id) return native.flag_block(id) end
  --- Call fn(id) when the flag turns on (edge-triggered: checked once a frame).
  function flags.on_set(id, fn) return native.flag_watch(id, true, fn) end
  --- Call fn(id) when the flag turns off.
  function flags.on_clear(id, fn) return native.flag_watch(id, false, fn) end
  kindle.flags = flags
end

-- ── items ───────────────────────────────────────────────────────────────────
if allowed.items then
  local items = {}
  -- An item is its param row's id with its category: EquipParamWeapon's rows are weapons,
  -- EquipParamProtector's armor, EquipParamAccessory's talismans, EquipParamGoods' goods and
  -- EquipParamGem's ashes of war. Or its name in the game's language, as the game shows it (a
  -- weapon's with its upgrade level: "Dagger +5").
  local function resolve(item)
    if type(item) == "string" then return native.item_by_name(item, nil) end
    return item
  end
  local function maker(kind)
    return function(x)
      if type(x) == "string" then return native.item_by_name(x, kind) end
      return native.item_make(kind, x)
    end
  end
  items.weapon, items.armor, items.talisman = maker("weapon"), maker("armor"), maker("talisman")
  items.goods, items.ash = maker("goods"), maker("ash")
  -- Dark Souls III's name for talismans, for mods written for both.
  items.ring = items.talisman
  --- The item with this exact name (any case, any category). Errors if unknown or ambiguous.
  function items.id(name) return native.item_by_name(name, nil) end
  --- The item's name in the game's language (nil if it has none).
  function items.name(item) return native.text_item_get(resolve(item), "name") end
  --- Names holding `query`: a list of {name, category, id, item}, exact names first (20 at most by default).
  function items.find(query, limit) return native.item_find(query, limit) end
  --- Give the player an item (1 by default, at most 999), as the game's event scripts do: there is
  --- no "item received" box unless opts.popup: then the game's own box shows it, as a map's item does
  --- (on Kindle's next frame; opts.on_done(done, err) as for events.run). What the inventory can't
  --- hold goes to the game's storage. On a loaded character.
  function items.give(item, quantity, opts)
    if opts and opts.popup then return native.item_give_popup(resolve(item), quantity or 1, opts.on_done) end
    native.item_give(resolve(item), quantity or 1)
  end
  --- Take up to `quantity` (1 by default) of an item from the player.
  function items.remove(item, quantity) native.item_remove(resolve(item), quantity or 1) end
  --- How many of an item the player holds (a weapon or armor piece: 1 if they have one).
  function items.count(item) return native.item_count(resolve(item)) end
  function items.has(item, quantity) return native.item_count(resolve(item)) >= (quantity or 1) end
  --- An item's category ("weapon", "armor", "talisman", "goods", "ash") and param row id.
  function items.info(item) return native.item_info(resolve(item)) end
  --- What the player holds, in the inventory's order: {{item, kind, id, quantity}, ...}.
  function items.held() return native.item_held() end
  if allowed.text then
    --- Rename an item wherever the game shows its name (a row a mod added gets one). Undone when the
    --- mod unloads.
    function items.set_name(item, name) native.text_item_set(resolve(item), "name", tostring(name)) end
    --- Replace an item's description (its long text).
    function items.set_description(item, text) native.text_item_set(resolve(item), "description", tostring(text)) end
    --- Replace an item's info (its short text under the name).
    function items.set_info(item, text) native.text_item_set(resolve(item), "info", tostring(text)) end
    --- The game's name, description and info back.
    function items.reset_text(item)
      local it = resolve(item)
      for _, f in ipairs({ "name", "description", "info" }) do native.text_item_reset(it, f) end
    end
  end
  kindle.items = items
end

-- ── params ──────────────────────────────────────────────────────────────────
if allowed.params then
  local params = {}
  local function row_proxy(tbl, id)
    return setmetatable({}, {
      __index = function(_, field) return native.param_read(tbl, id, field) end,
      __newindex = function(_, field, value) native.param_write(tbl, id, field, value) end,
      __tostring = function() return fmt("%s[%d]", tbl, id) end,
    })
  end
  -- (Errors are raised at level 4: the mod's call to params.get/add_row.)
  local function row_id(id, what)
    if math.type(id) ~= "integer" then error(what .. ": row must be an integer id", 4) end
    return id
  end
  --- A live row: read and assign fields by name. Edits are reverted when the mod unloads.
  function params.get(tbl, id)
    id = row_id(id, "params.get")
    if native.param_has(tbl, id) then return row_proxy(tbl, id) end
    return nil
  end
  --- Add a new row to a table, a copy of row copy_from, and return it (a live row, like get).
  --- The id must be above every row of the table but its end rows (the dummy 999999999 and the
  --- like that Elden Ring's item and effect tables end in) and below those; in an item table
  --- (EquipParamWeapon, Protector, Accessory, Goods, Gem) under 2^28. The row stays for the session.
  function params.add_row(tbl, id, copy_from)
    check(type(tbl) == "string", "params.add_row: table must be a string")
    check(math.type(id) == "integer", "params.add_row: id must be an integer")
    copy_from = row_id(copy_from, "params.add_row")
    native.param_add_row(tbl, id, copy_from)
    return row_proxy(tbl, id)
  end
  --- Add several rows to a table with one rebuild of it (add_row rebuilds the table for each):
  --- `rows` is {{id = ID, copy_from = ROW}, ...}, ids ascending, as add_row's. Returns the live
  --- rows, in order.
  function params.add_rows(tbl, rows)
    check(type(tbl) == "string", "params.add_rows: table must be a string")
    check(type(rows) == "table", "params.add_rows: rows must be a list of {id = ..., copy_from = ...}")
    local ids, copies = {}, {}
    for i, r in ipairs(rows) do
      check(type(r) == "table" and math.type(r.id) == "integer", "params.add_rows: each row needs an integer id")
      ids[i] = r.id
      copies[i] = row_id(r.copy_from, "params.add_rows")
    end
    native.param_add_rows(tbl, ids, copies)
    local out = {}
    for i, r in ipairs(rows) do out[i] = row_proxy(tbl, r.id) end
    return out
  end
  --- Copies rows over others in one go: `pairs` is {{from, to}, ...}, each `to` getting what its
  --- `from` holds before any of them is copied, except the fields named in `keep`. Reverted when
  --- the mod unloads, as any write.
  function params.copy(tbl, pairs, keep)
    check(type(tbl) == "string", "params.copy: table must be a string")
    check(type(pairs) == "table", "params.copy: pairs must be a list of {from, to}")
    for _, pair in ipairs(pairs) do
      check(type(pair) == "table" and math.type(pair[1]) == "integer" and math.type(pair[2]) == "integer", "params.copy: each pair is {from, to}, row ids")
    end
    check(keep == nil or type(keep) == "table", "params.copy: keep must be a list of field names")
    native.param_copy(tbl, pairs, keep)
  end
  --- Put back what this mod wrote to the game's params: one row's fields (table and row), a
  --- table's, or all (no arguments). Returns how many fields.
  function params.reset(tbl, row)
    check(tbl == nil or type(tbl) == "string", "params.reset: table must be a string")
    check(row == nil or math.type(row) == "integer", "params.reset: row must be an integer id")
    return native.param_reset(tbl, row)
  end
  params.tables = native.param_tables
  params.fields = native.param_fields
  params.rows = native.param_rows
  kindle.params = params
end

-- ── player ──────────────────────────────────────────────────────────────────
if allowed.player then
  local player = {}
  --- Whether a character is in the world.
  player.loaded = native.player_loaded
  player.name = native.player_name
  --- The attributes: {vigor = ..., mind, endurance, strength, dexterity, intelligence, faith, arcane}.
  player.stats = native.player_stats
  for _, pool in ipairs({ "hp", "fp", "stamina" }) do
    player[pool] = function() return native.player_get(pool) end
    player["max_" .. pool] = function() return native.player_get("max_" .. pool) end
    --- Set within 0 (HP: 1) and the maximum, which follows the attributes.
    player["set_" .. pool] = function(v) native.player_set(pool, v) end
  end
  --- HP, FP and stamina to their maxima.
  function player.heal()
    for _, pool in ipairs({ "hp", "fp", "stamina" }) do native.player_set(pool, native.player_get("max_" .. pool)) end
  end
  --- The player's position: x, y (up), z, in the physics' frame (shifted from the map's own coordinates).
  player.position = native.player_position
  --- Move the player there (the coordinates position gives); they fall if nothing is under them.
  player.set_position = native.player_set_position
  --- Runes held, and set (0 to 999,999,999).
  player.runes = native.player_runes
  player.set_runes = native.player_set_runes
  function player.add_runes(n) native.player_set_runes(native.player_runes() + n) end
  --- Runes gained over the whole playthrough.
  player.total_runes = native.player_total_runes
  player.level = native.player_level
  --- Set the level (1 to 713), the attributes as they are.
  player.set_level = native.player_set_level
  --- Set an attribute (1 to 99); the level moves by as much unless keep_level.
  function player.set_stat(name, value, keep_level) native.player_set_attribute(name, value, keep_level) end
  --- Set several attributes at once: player.set_stats({vigor = 20, strength = 18})
  function player.set_stats(t, keep_level)
    check(type(t) == "table", "player.set_stats: a table of attributes")
    for name, value in pairs(t) do native.player_set_attribute(name, value, keep_level) end
  end
  --- Warp to a site of grace (its BonfireWarpParam row, or its name: "Church of Elleh"), as the game's
  --- own warps do: a load screen, then the player's start beside the grace. Runs on Kindle's next frame;
  --- on_done(done, err) as for events.run.
  function player.warp(grace, on_done) return native.player_warp(grace, on_done) end
  --- Every site of grace: {id (its BonfireWarpParam row), name, entity (its asset), map}.
  player.graces = native.player_graces
  --- Apply an SpEffect (SpEffectParam row: buffs, debuffs, states) to the player, as the game's own
  --- Set SpEffect does.
  function player.apply_effect(id) native.world_apply_effect(PLAYER, id) end
  --- End an active SpEffect on the player; how many instances were ended.
  function player.remove_effect(id) return native.world_remove_effect(PLAYER, id) end
  --- The player's active SpEffects (SpEffectParam rows), as the game lists them.
  function player.effects() return native.world_effects(PLAYER) end
  function player.has_effect(id)
    for _, e in ipairs(native.world_effects(PLAYER)) do
      if e == id then return true end
    end
    return false
  end
  -- Dark Souls III's names, for mods written for both (attunement and luck are mind and arcane).
  player.souls, player.set_souls, player.add_souls = player.runes, player.set_runes, player.add_runes
  player.total_souls, player.soul_level, player.set_soul_level = player.total_runes, player.level, player.set_level
  kindle.player = player
end

-- ── events (the game's event script commands) ──────────────────────────────
if allowed.events then
  local events = kindle.events
  --- Run one of the game's event script (EMEVD) commands: bank, index, then the arguments' format
  --- (one letter per value: b/B i8/u8, h/H i16/u16, i/I i32/u32, f f32) and values. Runs on Kindle's
  --- next frame, through the game's own handler, as a map's script would; an optional last argument
  --- on_done(done, err) gets whether the handler finished it. Entity 10000 is the player.
  function events.run(bank, index, format, ...)
    check(type(bank) == "number" and type(index) == "number", "events.run: bank and index must be numbers")
    local values, on_done = { ... }, nil
    if type(values[#values]) == "function" then on_done = tremove(values) end
    check(format == nil or type(format) == "string", "events.run: format must be a string")
    return native.events_run(bank, index, format or "", values, on_done)
  end
  -- Named commands: events.apply_effect(10000, 4330), events.banner("god_slain"), events.warp_player(...),
  -- each {bank, index, format, argument names} in events.commands. An optional last argument on_done.
  for name in pairs(EVENT_COMMANDS) do
    events[name] = function(...)
      local args, on_done = { ... }, nil
      if type(args[#args]) == "function" then on_done = tremove(args) end
      return run_named(name, args, on_done)
    end
  end
  events.commands = EVENT_COMMANDS
  events.PLAYER = PLAYER
  kindle.events = events
end

-- ── world (characters by entity id) ────────────────────────────────────────
if allowed.world then
  local world = {}
  -- A character is its entity id (from the map: 1042360800), 10000 the player, or the handle
  -- world.characters gives one without an entity id; a table from world.characters works too. One
  -- whose map isn't loaded isn't there. The game's commands (kill, AI, team ...) take entity ids only.
  local function key_of(c, what)
    if type(c) == "table" then c = c.entity or c.handle end
    check(math.type(c) == "integer", what .. ": a character is its entity id, its handle or its table")
    return c
  end
  local function entity_of(c, what)
    if type(c) == "table" then
      check(c.entity ~= nil, what .. ": this character has no entity id (the game's commands need one)")
      c = c.entity
    end
    check(type(c) == "number", what .. ": a character is its entity id")
    return c
  end
  --- Loaded characters, nearest first (those in a block the player's can't be measured from last).
  --- opts: radius (metres from the player), alive (default true: skip dead), player (default false),
  --- helpers (default false: skip the maps' invisible event characters), kind ("player": this game's
  --- or another's in a session, "npc", "enemy", "ally", "helper", "other": by its team). Each is {handle, entity, npc_id, think_id, name, model, kind, team,
  --- hp, max_hp, alive, map, is_player, x, y, z, distance}; entity and name are nil for some.
  function world.characters(opts)
    opts = opts or {}
    return native.world_characters(opts.radius, opts.alive ~= false, opts.player == true,
      opts.helpers == true, opts.kind)
  end
  --- One character, as world.characters lists it, or nil if it isn't loaded.
  function world.character(c) return native.world_character(key_of(c, "world.character")) end
  function world.exists(c) return native.world_character(key_of(c, "world.exists")) ~= nil end
  --- Its position: x, y (up), z, in the coordinates of the map block it's in (its table's map).
  function world.position(c) return native.world_position(key_of(c, "world.position")) end
  --- Set its HP (0 to its maximum).
  function world.set_hp(c, hp) native.world_set_hp(key_of(c, "world.set_hp"), hp) end
  function world.heal(c)
    local info = native.world_character(key_of(c, "world.heal"))
    if info then native.world_set_hp(info.entity or info.handle, info.max_hp) end
  end
  --- Kill it as the game's scripts do (runes for the player if give_runes), on Kindle's next frame.
  function world.kill(c, give_runes, on_done)
    return run_named("kill", { entity_of(c, "world.kill"), give_runes == true }, on_done)
  end
  --- Apply an SpEffect to it, end one (how many instances ended), its active ones.
  function world.apply_effect(c, id) native.world_apply_effect(key_of(c, "world.apply_effect"), id) end
  function world.remove_effect(c, id) return native.world_remove_effect(key_of(c, "world.remove_effect"), id) end
  function world.effects(c) return native.world_effects(key_of(c, "world.effects")) end
  --- Its AI on or off, its team (a number or events' team names: "enemy", "ally", "friendly_npc" ...),
  --- shown or hidden (set_enabled), immortal, invincible, its HP bar: the game's own commands, on
  --- Kindle's next frame.
  for _, name in ipairs({ "set_ai", "set_enabled", "set_immortal", "set_invincible", "set_hp_bar" }) do
    world[name] = function(c, on, on_done) return run_named(name, { entity_of(c, "world." .. name), on ~= false }, on_done) end
  end
  function world.set_team(c, team, on_done) return run_named("set_team", { entity_of(c, "world.set_team"), team }, on_done) end
  --- Make it forget its target and choose what to do again.
  function world.clear_aggro(c, on_done)
    local e = entity_of(c, "world.clear_aggro")
    run_named("clear_ai_target", { e })
    return run_named("ai_replan", { e }, on_done)
  end
  --- Distance between two characters (nil when their map blocks' coordinates don't meet: a legacy
  --- dungeon's and the open world's), or a character and a point {x, y, z} in its block's.
  function world.distance(a, b)
    local ca = native.world_character(key_of(a, "world.distance"))
    check(ca ~= nil, "world.distance: the first character isn't loaded")
    local bx, by, bz
    if type(b) == "table" and b.entity == nil and b.handle == nil then
      bx, by, bz = b.x or b[1], b.y or b[2], b.z or b[3]
    else
      local cb = native.world_character(key_of(b, "world.distance"))
      check(cb ~= nil, "world.distance: the second character isn't loaded")
      bx, by, bz = native.world_relative(cb.map, ca.map, cb.x, cb.y, cb.z)
      if bx == nil then return nil end
    end
    return math.sqrt((ca.x - bx) ^ 2 + (ca.y - by) ^ 2 + (ca.z - bz) ^ 2)
  end
  world.PLAYER = PLAYER
  kindle.world = world
end

-- ── ui (the game's own messages) ────────────────────────────────────────────
if allowed.ui then
  local ui = {}
  --- Show text as the game's own message: kind "blinking" (the default: a line mid-screen that
  --- fades, as "Somewhere, a heavy door has opened"), "full_screen" (the game waits for it to be
  --- dismissed), "status", "area" or "subarea" (a place's name, as entering it shows). On Kindle's
  --- next frame; on_done(done, err) as for events.run.
  function ui.message(text, kind, on_done)
    if type(kind) == "function" then kind, on_done = nil, kind end
    return native.ui_message(kind or "blinking", tostring(text), on_done)
  end
  --- One of the game's banners across the screen, by name (events.commands' banner names:
  --- "god_slain", "great_enemy_felled", "grace_discovered" ...) or type.
  function ui.game_banner(kind, on_done) return run_named("banner", { kind }, on_done) end
  --- The banner names.
  ui.banners = EVENT_NAMES.banner
  --- Kindle's own, drawn over the game (the overlay):
  --- Big centred text on a dark band. opts: seconds (3), color ("gold", "white", "red", "blue",
  --- "green" or "#rrggbb").
  function ui.banner(message, opts)
    opts = opts or {}
    native.ui_notice("banner", tostring(message), opts.seconds or 3, opts.color or "gold")
  end
  --- A small notification in the top-right corner. opts: seconds (4), color.
  function ui.toast(message, opts)
    opts = opts or {}
    native.ui_notice("toast", tostring(message), opts.seconds or 4, opts.color or "gold")
  end
  --- An action prompt at the bottom of the screen until cleared (nil), one per mod (the last set shows).
  function ui.prompt(text) native.ui_prompt(text) end
  --- A line across the screen that stays until cleared (nil), with a progress bar (0 to 1) if given:
  --- one per mod, stacked in the order they were first set.
  function ui.status(text, progress) native.ui_status(text, progress) end
  kindle.ui = ui
end

-- ── text ────────────────────────────────────────────────────────────────────
if allowed.text then
  local text = {}
  local function item_ref(item)
    if type(item) == "string" then return native.item_by_name(item, nil) end
    return item
  end
  --- What the game shows for (category, id), in its language: category is a name from
  --- text.categories ("weapon_name", "place_name" ...) or an FMG id. nil if the game has none.
  function text.get(category, id) return native.text_get(tostring(category), id) end
  --- Replace that text wherever the game shows it, or add it if the game has none (as the text of a
  --- row a mod added); how many of the game's tables changed. Undone when the mod unloads.
  function text.set(category, id, value) return native.text_set(tostring(category), id, tostring(value)) end
  --- The game's text back.
  function text.reset(category, id) return native.text_reset(tostring(category), id) end
  --- An item's "name", "info" or "description" text.
  function text.item(item, field) return native.text_item_get(item_ref(item), field or "name") end
  text.categories = native.text_categories()
  kindle.text = text
end
