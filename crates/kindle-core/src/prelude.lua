-- Kindle prelude: builds the `kindle` API for one mod from the private `native` table: the sandbox
-- and Kindle's own API here, then the game's modules, from the prelude of the game's runtime
-- (bindings.rs), run last with the `kindle` table so far. Runs once per Lua state, before the mod's
-- main.lua. `native` is not reachable from mod code afterwards; only the modules listed in
-- `allowed` are exposed.

local native, allowed, info, game_modules = ...
local fmt, select, tostring, type, setmetatable = string.format, select, tostring, type, setmetatable
local pairs, ipairs, error, assert, pcall = pairs, ipairs, error, assert, pcall
local concat, sqrt, tremove = table.concat, math.sqrt, table.remove
local orig_load = load

-- ── sandbox hardening ───────────────────────────────────────────────────────
dofile, loadfile = nil, nil
-- text chunks only; binary chunks could break memory safety
load = function(chunk, name, _mode, env)
  return orig_load(chunk, name, "t", env or _ENV)
end

local inspect = native.inspect

-- Tables print their contents, everything else as tostring would.
local function show(v)
  if type(v) == "table" then return inspect(v) end
  return tostring(v)
end

local function tostr_all(...)
  local n = select("#", ...)
  local parts = {}
  for i = 1, n do parts[i] = show((select(i, ...))) end
  return concat(parts, "\t")
end

print = function(...) native.print(tostr_all(...)) end

-- mod-local require: require("util.math") loads <mod dir>/util/math.lua once
local loaded, LOADING = {}, {}
require = function(name)
  if type(name) ~= "string" then error("require expects a module name", 2) end
  local cached = loaded[name]
  if cached == LOADING then error("require: circular require of " .. fmt("%q", name), 2) end
  if cached ~= nil then return cached end
  local src, chunkname = native.read_mod_file(name)
  local fn = assert(orig_load(src, chunkname, "t", _ENV))
  loaded[name] = LOADING
  local ok, result = pcall(fn, name)
  if not ok then
    loaded[name] = nil
    error(result, 0)
  end
  if result == nil then result = true end
  loaded[name] = result
  return result
end

-- ── kindle core (always available) ──────────────────────────────────────────
local kindle = {}
kindle.version = native.version
kindle.api = native.api
kindle.mod = info
--- A readable string for any value: tables show their contents (`depth` levels, default 3).
function kindle.inspect(value, depth) return inspect(value, depth) end

local function log_fn(level)
  return function(f, ...)
    local msg
    if select("#", ...) > 0 then
      -- Tables passed for %s show their contents.
      local args = { ... }
      for i = 1, select("#", ...) do
        if type(args[i]) == "table" then args[i] = inspect(args[i]) end
      end
      msg = fmt(f, table.unpack(args, 1, select("#", ...)))
    else
      msg = show(f)
    end
    native.log(level, msg)
  end
end
kindle.log = { debug = log_fn("debug"), info = log_fn("info"), warn = log_fn("warn"), error = log_fn("error") }

-- Errors in argument checks point at the mod's line, not the prelude's.
local function check(ok, msg)
  if not ok then error(msg, 3) end
end

--- Subscribe to an event (unknown names are an error). Runtime: "frame" (dt),
--- "player_loaded", "player_unloaded", "unload". Dark Souls III's: game state (polled): "player_died",
--- "region_changed", "character_killed", "character_spawned", "item_gained", "item_lost", "effect_applied",
--- "effect_expired", "level_up". Hooked (handlers can change the outcome): "damage", "animation",
--- "effect", "item_used", "item_lot"; and "bonfire_rested" (bonfire), "shop_buy" (item, count, row), "banner" (name, type),
--- "boss_defeated" (kind), "boss_bar" (shown, name, entity), "travel" (map, entity, how). See the stubs for each event's arguments.
function kindle.on(event, fn)
  check(type(event) == "string", "kindle.on: event name must be a string")
  check(type(fn) == "function", "kindle.on: handler must be a function")
  return native.on(event, fn)
end

--- Run fn once after `seconds`. Returns a handle for kindle.cancel.
function kindle.after(seconds, fn)
  check(type(seconds) == "number" and seconds >= 0, "kindle.after: seconds must be a number >= 0")
  check(type(fn) == "function", "kindle.after: handler must be a function")
  return native.timer(seconds, false, fn)
end

--- Run fn every `seconds`. Returns a handle for kindle.cancel.
function kindle.every(seconds, fn)
  check(type(seconds) == "number" and seconds > 0, "kindle.every: seconds must be a number > 0")
  check(type(fn) == "function", "kindle.every: handler must be a function")
  return native.timer(seconds, true, fn)
end

--- Cancel an event handler, timer or flag watcher by handle.
kindle.cancel = native.cancel
--- Seconds since Kindle started.
kindle.time = native.time
--- Seconds since 1970-01-01 00:00 UTC, from the computer's clock (kindle.time is since Kindle
--- started). For daily seeds, dates and times across sessions.
kindle.clock = native.clock
--- The calendar date and time of `seconds` (a kindle.clock() value; default now), in the
--- player's local time, or UTC with `utc` true: {year, month, day, hour, minute, second,
--- weekday (1 Monday .. 7 Sunday), yearday}.
function kindle.date(seconds, utc) return native.date(seconds, utc) end
--- A file of the mod's own folder, as text (bytes, for any file): `path` relative to the folder,
--- with / between folders. For data a mod ships: kindle.json.decode(kindle.read_data("drops.json")).
function kindle.read_data(path)
  check(type(path) == "string", "kindle.read_data: a path in the mod's folder")
  return native.read_data(path)
end
--- JSON text: encode(value, pretty) and decode(text). A table whose keys are 1..n is an array,
--- any other an object; an empty table is []. JSON's null reads as nil.
kindle.json = {}
function kindle.json.encode(value, pretty) return native.json_encode(value, pretty) end
function kindle.json.decode(text)
  check(type(text) == "string", "kindle.json.decode: JSON text")
  return native.json_decode(text)
end

--- Frames since Kindle started.
kindle.frame = native.frame

-- ── other mods ───────────────────────────────────────────────────────────────
--- Offer functions to other mods: kindle.export{ greet = function(name) ... end }. Calls
--- from other mods run here, in this mod; arguments and results are copied (plain data:
--- nil, booleans, numbers, strings, tables). Calling it again replaces the exports.
function kindle.export(functions)
  check(type(functions) == "table", "kindle.export: expects a table of functions")
  native.export(functions)
end
kindle.mods = {}
--- Another mod's exports, as a table of functions (nil if that mod isn't running).
--- Calls error if the mod has since stopped. List it under depends in mod.toml to be sure
--- it loads first.
function kindle.mods.get(id)
  check(type(id) == "string", "kindle.mods.get: id must be a string")
  local names = native.mod_exports(id)
  if names == nil then return nil end
  local proxy = {}
  for _, name in ipairs(names) do
    proxy[name] = function(...) return native.mod_call(id, name, ...) end
  end
  return setmetatable(proxy, { __index = function(_, name)
    error(fmt("mod %q exports no function %q", id, tostring(name)), 2)
  end })
end
--- Ids of the running mods.
function kindle.mods.running() return native.mods_running() end
--- Send a message to every other mod (delivered next frame): their
--- kindle.on_message(name, fn) handlers get fn(data, from_mod_id). data is plain data.
function kindle.send(name, data)
  check(type(name) == "string", "kindle.send: name must be a string")
  native.send(name, data)
end
--- Handle messages other mods send with kindle.send(name, data): fn(data, from).
function kindle.on_message(name, fn)
  check(type(name) == "string", "kindle.on_message: name must be a string")
  check(type(fn) == "function", "kindle.on_message: handler must be a function")
  return native.on_message(name, fn)
end

-- ── settings (declared in mod.toml, changed in the overlay) ───────────────────
kindle.settings = {}
--- A setting's current value ([settings.<name>] in mod.toml): what the player chose in the
--- overlay's Mods tab, or the default. kindle.on("setting_changed", fn(name, value)) runs
--- when the player changes one.
function kindle.settings.get(name)
  check(type(name) == "string", "settings.get: name must be a string")
  return native.settings_get(name)
end
--- Change a setting from the mod (checked against its declaration; ranges clamp).
function kindle.settings.set(name, value)
  check(type(name) == "string", "settings.set: name must be a string")
  native.settings_set(name, value)
end
--- The declared settings' names, in mod.toml order.
function kindle.settings.names() return native.settings_names() end

-- ── store (persistent values) ────────────────────────────────────────────────
do
  local store = {}
  local function key_check(key, what)
    check(type(key) == "string", what .. ": key must be a string")
  end
  --- A value saved for the loaded character (nil, or default if it was never set).
  function store.get(key, default)
    key_check(key, "store.get")
    local v = native.store_get(false, key)
    if v == nil then return default end
    return v
  end
  --- Save a value for the loaded character (nil removes it): booleans, numbers, strings,
  --- and tables that are lists or have string keys.
  function store.set(key, value)
    key_check(key, "store.set")
    native.store_set(false, key, value)
  end
  --- The loaded character's saved keys.
  function store.keys() return native.store_keys(false) end
  --- A value saved for the mod itself, the same for every character (settings).
  function store.get_global(key, default)
    key_check(key, "store.get_global")
    local v = native.store_get(true, key)
    if v == nil then return default end
    return v
  end
  --- Save a value for the mod itself (nil removes it).
  function store.set_global(key, value)
    key_check(key, "store.set_global")
    native.store_set(true, key, value)
  end
  --- The mod's global keys.
  function store.global_keys() return native.store_keys(true) end
  kindle.store = store

  --- A value that survives the mod being reloaded (for this session): the one kept when the
  --- mod last unloaded, else `default`. Kindle saves it as the mod unloads, so change a kept
  --- table in place rather than replacing it.
  function kindle.keep(key, default)
    key_check(key, "keep")
    return native.keep(key, default)
  end
end

-- ── game (the catalogue: Kindle's named, reviewed hook points, fields, functions, events) ──
if allowed.game then
  local game = {}
  --- Every entry: { hooks = { [name] = { doc, answers, answer } }, fields = { [name] = { doc, kind,
  --- character, writable } }, functions = { [name] = { doc, args } }, events = { [name] = { doc,
  --- args } } }.
  game.catalogue = native.game_catalogue
  --- Answers a hook point (one of its answers; its first, the game's own, gives it back). One mod
  --- holds a hook point at a time; it goes back to the game's answer when the mod unloads.
  function game.decide(name, answer) native.game_decide(name, answer) end
  --- Answers each call of a hook point: fn(...) gets the call's values and returns an answer, or
  --- nil for the switch's. Only calls on the game's thread ask; elsewhere the switch answers.
  function game.on_decide(name, fn) return native.game_on_decide(name, fn) end
  --- A field's value: name, and a character's handle for a character's field.
  function game.get(name, handle) return native.game_get(name, handle) end
  --- Sets a field (checked against its kind and range).
  function game.set(name, value, handle) native.game_set(name, value, handle) end
  --- Calls a game function (arguments checked against their kinds and ranges).
  function game.call(name, ...) return native.game_call(name, ...) end
  --- A catalogue event's handler (as kindle.on); its values come as arguments.
  function game.on(name, fn)
    native.game_listen(name)
    return kindle.on(name, fn)
  end
  kindle.game = game
end

-- ── events (changes to the game's event files; the game's prelude adds its commands) ─────
if allowed.events then
  local events = {}
  -- Changing the game's event scripts: maps are named by their event file ("m40_00_00_00",
  -- "common", "common_func"); changes apply from the next time that map's events load
  -- (loading into the area), in the order mods make them. Command numbers start at 1 and count
  -- the event's commands as they are by then.
  local function map_event(name, map, id)
    check(type(map) == "string", fmt("events.%s: map must be a string like \"m40_00_00_00\"", name))
    check(type(id) == "number", fmt("events.%s: the event id must be a number", name))
  end
  local function at(name, n)
    check(type(n) == "number" and n >= 1 and n == math.floor(n), fmt("events.%s: the command number must be 1 or more", name))
    return n - 1
  end
  --- The game's own event: {id, rest, commands = {{bank, index, args = {32-bit words}, bytes,
  --- layer}}, parameters = {{command, target, source, length}}} (the event's arguments copied
  --- into a command's bytes), or nil.
  function events.script(map, id)
    map_event("script", map, id)
    return native.events_script(map, id)
  end
  --- The event as the game will run it: the game's, with every change mods and Kindle make to
  --- it now (as events.script), or nil.
  function events.view(map, id)
    map_event("view", map, id)
    return native.events_view(map, id)
  end
  --- Changes an event, all or nothing. Each step names a command by its number in the event as
  --- it is before this edit (1 = the first; insert at the count + 1 adds at the end):
  ---   {check = n, is = {bank, index, byte, ...}}  command n must be that (its first bytes)
  ---   {remove = n}
  ---   {replace = n, bank =, index =, format =, values = {...}}
  ---   {insert = n, bank =, index =, format =, values = {...}}  before command n
  --- Any step can say what its command must be (is); a new command can take the event's
  --- arguments (params = {{target =, source =, length =}}, bytes) and command m's layer
  --- (layer_of = m; a replacement keeps its command's by default); a replacement can keep its
  --- command's argument entries (keep_params = true). If a check fails, nothing of the edit
  --- applies and the log says why (with the reason given).
  function events.edit(map, id, steps, why)
    map_event("edit", map, id)
    check(type(steps) == "table", "events.edit: steps must be a list")
    check(why == nil or type(why) == "string", "events.edit: the reason must be a string")
    native.events_edit(map, id, steps, why)
  end
  --- Every command bank[index] in a map's event file as the game has it: {{event, command,
  --- args = {32-bit words}, bytes}} (command: its number in the event). E.g. every boss health
  --- bar a map shows: events.find("m30_00_00_00", 2003, 11).
  function events.find(map, bank, index)
    check(type(map) == "string", "events.find: map must be a string")
    check(math.type(bank) == "integer" and math.type(index) == "integer", "events.find: bank and index must be integers")
    return native.events_find(map, bank, index)
  end
  --- Drops this mod's changes to a map's events (from the next time the area loads); how many.
  function events.reset(map)
    check(type(map) == "string", "events.reset: map must be a string")
    return native.events_reset(map)
  end
  --- The event does nothing (it ends at once).
  function events.disable(map, id)
    map_event("disable", map, id)
    native.events_disable(map, id)
  end
  --- Removes the event's command number n.
  function events.remove(map, id, n)
    map_event("remove", map, id)
    native.events_remove(map, id, at("remove", n))
  end
  --- Inserts a command before number n (n = the count + 1 adds it at the end).
  function events.insert(map, id, n, bank, index, format, ...)
    map_event("insert", map, id)
    native.events_insert(map, id, at("insert", n), bank, index, format or "", { ... })
  end
  --- Replaces the event's command number n.
  function events.replace(map, id, n, bank, index, format, ...)
    map_event("replace", map, id)
    native.events_replace(map, id, at("replace", n), bank, index, format or "", { ... })
  end
  --- Adds a new event: commands = {{bank =, index =, format =, values = {...}}, ...}; rest is
  --- what happens when the map reloads (0 restart, the default; 1 end; 2 end and restart).
  function events.add(map, id, commands, rest)
    map_event("add", map, id)
    check(type(commands) == "table", "events.add: commands must be a list")
    native.events_add(map, id, rest or 0, commands)
  end
  --- Starts an event when the map's events load (as the game's event 0 does): slot, then its
  --- arguments' format and values.
  function events.start(map, id, slot, format, ...)
    map_event("start", map, id)
    native.events_start(map, id, slot or 0, format or "", { ... })
  end
  kindle.events = events
end

-- ── the game's modules (its prelude: bindings.rs) ──────────────────────────────
game_modules(native, allowed, info, kindle, check)

_G.kindle = kindle
