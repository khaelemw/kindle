-- Dark Souls III's part of the `kindle` API: its modules, built from the private `native` table.
-- Kindle's prelude (kindle-core's prelude.lua) runs it once per Lua state, after the sandbox and
-- Kindle's own API, with the `kindle` table so far and Kindle's argument check. Only the modules
-- listed in `allowed` are exposed.

local native, allowed, info, kindle, check = ...
local fmt, tostring, type, setmetatable = string.format, tostring, type, setmetatable
local pairs, ipairs, error, assert, pcall = pairs, ipairs, error, assert, pcall
local sqrt, tremove = math.sqrt, table.remove

--- Whether a DLC is installed: 1 Ashes of Ariandel, 2 The Ringed City (nil before the game
--- knows).
kindle.dlc_installed = native.dlc_installed

-- ── flags ───────────────────────────────────────────────────────────────────
if allowed.flags then
  local flags = {}
  --- A flag's state; set: "own" (the character's own flags) or "session" (the copy a player's
  --- game uses in a session, the host's world's), nil for the set in use.
  function flags.get(id, set) return native.flag_get(id, set) end
  --- Sets a flag (true by default); true if it now has that state. set as for flags.get.
  function flags.set(id, state, set)
    if state == nil then state = true end
    return native.flag_set(id, state, set)
  end
  --- The set of flags in use: "own", or "session" (a player in a session).
  function flags.set_in_use() return native.flag_set_in_use() end
  --- A set's block of 1000 flags from `first` as 32 words (highest bit first), or nil.
  function flags.block(first, set) return native.flag_block(first, set) end
  --- Writes a set's block of 1000 flags from `first` (32 words).
  function flags.set_block(first, words, set) native.flag_set_block(first, words, set) end
  --- Call fn when the flag turns on (edge-triggered, only while a character is loaded).
  function flags.on_set(id, fn) return native.flag_watch(id, true, fn) end
  --- Call fn when the flag turns off.
  function flags.on_clear(id, fn) return native.flag_watch(id, false, fn) end
  --- Boss defeat flags, from the game's event scripts (set when the boss dies).
  flags.boss = native.flag_bosses()
  --- Each bonfire's lit flag by name, e.g. flags.bonfire.firelink_shrine (read from the
  --- game's bonfire list the first time it's used).
  flags.bonfire = setmetatable({}, {
    __index = function(t, name)
      local all = native.flag_bonfires()
      setmetatable(t, nil)
      for k, v in pairs(all) do rawset(t, k, v) end
      return rawget(t, name)
    end,
  })
  kindle.flags = flags
end

-- ── items ───────────────────────────────────────────────────────────────────
if allowed.items then
  local items = {}
  -- Items can be given as an encoded item (from items.goods(500) etc.) or as a name
  -- ("Ember", "Dark Sword", "Shortsword +5"), looked up in the game's own text.
  local function resolve(item, kind)
    if type(item) == "string" then return native.item_by_name(item, kind) end
    return item
  end
  local function maker(kind)
    return function(x)
      if type(x) == "string" then return native.item_by_name(x, kind) end
      return native.item_make(kind, x)
    end
  end
  items.weapon, items.armor, items.ring, items.goods = maker("weapon"), maker("armor"), maker("ring"), maker("goods")
  --- The item with this exact name (any category). Errors if unknown or ambiguous.
  function items.id(name) return native.item_by_name(name, nil) end
  --- The item's name in the game's language (nil if it has none).
  function items.name(item) return native.item_name(resolve(item)) end
  --- Search names: list of {name, category, id, item}, exact matches first.
  function items.find(query, limit) return native.item_find(query, limit) end
  --- Give an item. opts.popup (default true) shows the game's "item received" box.
  function items.give(item, quantity, opts)
    local popup = true
    if opts and opts.popup == false then popup = false end
    native.item_give(resolve(item), quantity or 1, popup)
  end
  function items.remove(item, quantity) native.item_remove(resolve(item), quantity or 1) end
  function items.count(item) return native.item_count(resolve(item)) end
  function items.has(item, quantity) return native.item_count(resolve(item)) >= (quantity or 1) end
  --- Returns category ("weapon"/"armor"/"ring"/"goods"), param id and name of an item.
  function items.info(item) return native.item_info(resolve(item)) end
  if allowed.text then
    --- Rename an item (all menus and pop-ups). Reverted when the mod unloads.
    function items.set_name(item, name) native.text_item_set(resolve(item), "name", name) end
    --- Replace an item's long description. Reverted when the mod unloads.
    function items.set_description(item, text) native.text_item_set(resolve(item), "description", text) end
    --- Replace an item's short effect text (shown under its name).
    function items.set_info(item, text) native.text_item_set(resolve(item), "info", text) end
    --- Restore an item's original name, description and info.
    function items.reset_text(item)
      local it = resolve(item)
      for _, f in ipairs({ "name", "description", "info" }) do pcall(native.text_item_reset, it, f) end
    end
  end
  kindle.items = items
end

-- ── text ────────────────────────────────────────────────────────────────────
if allowed.text then
  local text = {}
  local function item_ref(item)
    if type(item) == "string" then return native.item_by_name(item, nil) end
    return item
  end
  --- Text the game shows for (category, id). category: a name from text.categories or an FMG id.
  function text.get(category, id) return native.text_get(tostring(category), id) end
  --- Replace that text everywhere the game shows it. Reverted when the mod unloads.
  function text.set(category, id, value) return native.text_set(tostring(category), id, tostring(value)) end
  --- Restore the original text.
  function text.reset(category, id) return native.text_reset(tostring(category), id) end
  --- An item's "name", "info" or "description" text.
  function text.item(item, field) return native.text_item_get(item_ref(item), field or "name") end
  text.categories = native.text_categories()
  kindle.text = text
end

-- ── ui ──────────────────────────────────────────────────────────────────────
if allowed.ui then
  local ui = {}
  --- Show text in the game's own "OK" message dialog (up to ~1000 characters). Does
  --- nothing while some game menus are open.
  function ui.message(text) native.ui_message(tostring(text)) end
  --- Add an option to the game's own list menus (NPC and bonfire menus). opts: text,
  --- on_select (runs when picked; the menu closes as if backed out), where ("any",
  --- "bonfire", "npc", or function(menu) -> bool with menu.entries {slot, text_id, text}
  --- and menu.bonfire). Returns a handle (kindle.cancel removes the option).
  function ui.add_menu_option(opts)
    assert(type(opts) == "table" and opts.text and (opts.on_select or opts.screen), "ui.add_menu_option: needs text and on_select (or screen)")
    local on_select = opts.on_select
    if opts.screen then
      -- A screen of the mod's (a spec, or a function returning one) opened when it's picked.
      on_select = function()
        local spec = opts.screen
        if type(spec) == "function" then spec = spec() end
        if spec then ui.screen(spec) end
      end
    end
    return native.ui_add_menu_option(tostring(opts.text), opts.where, on_select)
  end
  --- From a menu option's on_select (bonfire menus): show a list in the game's own menu.
  --- entries: { {text = "...", on_select = function() end}, ... } (up to 64). An entry's
  --- on_select can open another menu or a shop; one that opens nothing (or backing out)
  --- returns to the bonfire menu.
  function ui.menu(entries) native.ui_menu(entries) end
  --- Change a menu option's text (the handle add_menu_option returned); menus built after this
  --- show the new text (e.g. set it from the option's where function, just before it shows).
  function ui.set_menu_option_text(handle, text) native.ui_set_menu_option_text(handle, tostring(text)) end
  --- From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open
  --- the game's shop over ShopLineupParam rows first..last (inclusive), e.g. the Shrine
  --- Handmaid's 110000-119899. Closing it returns to the menu it was opened from.
  function ui.shop(first, last) native.ui_shop(first, last) end
  --- From a menu option's on_select: open one of the game's screens ("level_up", "attune",
  --- "storage", "travel", "reinforce", "infuse", "repair", "estus").
  function ui.open(screen) native.ui_open(screen) end
  --- Big centred text on a dark band. opts: seconds (3), color ("gold", "white", "red",
  --- "blue", "green" or "#rrggbb").
  function ui.banner(message, opts)
    opts = opts or {}
    native.ui_notice("banner", tostring(message), opts.seconds or 3, opts.color or "gold")
  end
  --- The game's own banner across the screen, as a place name shows, with this text.
  function ui.place_banner(text) native.ui_place_banner(tostring(text)) end
  --- Hides one of the game's banners (name or type) for some seconds.
  function ui.hide_game_banner(kind, seconds) native.ui_hide_game_banner(kind, seconds) end
  --- An action prompt at the bottom of the screen until cleared (nil), one per mod (the last set shows).
  function ui.prompt(text) native.ui_prompt(text) end
  --- A line across the screen that stays until cleared (nil), with a progress bar (0 to 1) if given:
  --- one per mod, stacked in the order they were first set.
  function ui.status(text, progress) native.ui_status(text, progress) end
  --- A tab of this mod's buttons in the game's Mods screen (Esc -> Mods): title, and rows
  --- {label, help, paste, on_press}, or {label, help, info = true} (a line shown greyed). on_press
  --- runs at Kindle's next frame; a paste button's gets the clipboard's text (as the player pressed
  --- it). Replaces this mod's earlier tab.
  function ui.mod_tab(title, buttons)
    check(type(title) == "string" and type(buttons) == "table", "ui.mod_tab: a title and a list of buttons")
    native.ui_mod_tab(title, buttons)
  end
  --- A screen of this mod's own, full-screen in the game's style, worked with the controller,
  --- the keyboard or the mouse (the game's input is the screen's while it's open). spec: title,
  --- subtitle, closable (true: Back closes it), on_close, and rows, each {text, help, disabled}
  --- plus one of: on_select (a button; value is shown on its right), screen (a table or a
  --- function returning one: a sub-screen, Back returns here), toggle = true/false, choices =
  --- {...} with value (1-based or the text), slider = {min, max, step} with value, info = true
  --- (a line of text: greyed, or with a value on the right), or {header = "Section"}. A switch's, choice's or slider's on_change gets
  --- the new value (a choice: its index and text); the row keeps it. Returns the screen:
  --- screen:update(spec) changes it in place, screen:close(), screen:is_open(). Instead of rows,
  --- tabs = {{title = "General", rows = {...}}, ...} (LB/RB, Q/E or a click turns them), tab = n
  --- the one it opens on, on_tab(index, title) when the player turns to another.
  local function screen_rows(spec)
    if type(spec.tabs) ~= "table" then return spec.rows end
    local all = {}
    for _, tab in ipairs(spec.tabs) do
      for _, r in ipairs(type(tab) == "table" and type(tab.rows) == "table" and tab.rows or {}) do all[#all + 1] = r end
    end
    return all
  end
  local function is_spec(spec) return type(spec) == "table" and (type(spec.rows) == "table" or type(spec.tabs) == "table") end
  function ui.screen(spec)
    check(is_spec(spec), "ui.screen: a table with rows = {...} or tabs = {...}")
    local current, rows = spec, screen_rows(spec)
    local handle = {}
    local function row(i) return rows[i] end
    local function dispatch(kind, i, value, text)
      if kind == "close" then
        handle.closed = true
        if current.on_close then current.on_close() end
        return
      end
      if kind == "tab" then
        local tab = current.tabs and current.tabs[i]
        if tab and current.on_tab then current.on_tab(i, tab.title) end
        return
      end
      local r = row(i)
      if not r then return end
      if kind == "select" then
        if r.screen then
          local sub = r.screen
          if type(sub) == "function" then sub = sub() end
          if sub then ui.screen(sub) end
        elseif r.on_select then
          r.on_select()
        end
      elseif kind == "change" then
        if r.toggle ~= nil then
          r.toggle = value ~= 0
          if r.on_change then r.on_change(r.toggle) end
        elseif r.choices then
          r.value = math.floor(value) + 1
          if r.on_change then r.on_change(r.value, r.choices[r.value]) end
        elseif r.slider then
          r.value = value
          if r.on_change then r.on_change(value) end
        end
      elseif kind == "text" then
        if r.key ~= nil then r.key = text else r.input = text end
        if r.on_change then r.on_change(text) end
      end
    end
    local id = native.ui_screen_open(spec, dispatch)
    handle.id = id
    --- Changes the screen in place (title, rows...); the selection stays.
    function handle:update(new_spec)
      check(is_spec(new_spec), "screen:update: a table with rows = {...} or tabs = {...}")
      native.ui_screen_update(id, new_spec)
      current, rows = new_spec, screen_rows(new_spec)
    end
    --- Closes it (and any screen opened from it); on_close doesn't run.
    function handle:close() return native.ui_screen_close(id) end
    function handle:is_open() return native.ui_screen_is_open(id) end
    return handle
  end
  --- A question with two answers, on a screen of this mod's: opts {title, text, yes = "Yes",
  --- no = "No", on_yes, on_no}. Backing out is no. Returns the screen.
  function ui.confirm(opts)
    check(type(opts) == "table", "ui.confirm: a table {title, text, on_yes, on_no}")
    local screen
    local function answer(fn)
      return function()
        screen:close()
        if fn then fn() end
      end
    end
    screen = ui.screen({
      title = opts.title or "",
      subtitle = opts.text,
      on_close = function() if opts.on_no then opts.on_no() end end,
      rows = {
        { text = opts.yes or "Yes", on_select = answer(opts.on_yes) },
        { text = opts.no or "No", on_select = answer(opts.on_no) },
      },
    })
    return screen
  end
  --- One of a list, on a screen of this mod's: opts {title, text, options, on_pick}. An option
  --- is a text, or {text, help, value} (value shown on the right). on_pick(index, option) runs
  --- once one is picked, and the screen closes; backing out runs opts.on_cancel. Returns the screen.
  function ui.choose(opts)
    check(type(opts) == "table" and type(opts.options) == "table", "ui.choose: a table {title, options, on_pick}")
    local screen
    local rows = {}
    for index, option in ipairs(opts.options) do
      local o = type(option) == "table" and option or { text = tostring(option) }
      rows[#rows + 1] = {
        text = o.text,
        help = o.help,
        value = o.value,
        disabled = o.disabled,
        on_select = function()
          screen:close()
          if opts.on_pick then opts.on_pick(index, option) end
        end,
      }
    end
    screen = ui.screen({
      title = opts.title or "",
      subtitle = opts.text,
      on_close = function() if opts.on_cancel then opts.on_cancel() end end,
      rows = rows,
    })
    return screen
  end
  --- A layer of this mod's drawn over the game: text, boxes, lines, bars and circles on a 1920 x
  --- 1080 canvas scaled to the window, or at a point in the world (`at`). It stays until changed:
  --- layer:set(elements) replaces what it draws, layer:show(on), layer:remove(). Layers hide on
  --- loading screens and under the menus Kindle draws.
  function ui.layer(elements)
    check(elements == nil or type(elements) == "table", "ui.layer: a list of elements (or nil)")
    local id = native.ui_layer_add(elements or {})
    local layer = { id = id }
    function layer:set(new) check(type(new) == "table", "layer:set: a list of elements"); native.ui_layer_set(id, new) end
    function layer:show(on) native.ui_layer_show(id, on ~= false) end
    function layer:remove() return native.ui_layer_remove(id) end
    return layer
  end
  --- An entry of this mod's on the title's main menu (after Mods) opening the Mods screen with only
  --- this mod's tab (ui.mod_tab), or, given `open`, running it: a function, or a screen spec (or a
  --- function returning one, as add_menu_option's `screen`) shown over the menu. nil removes it.
  function ui.title_entry(label, help, open)
    check(label == nil or type(label) == "string", "ui.title_entry: a label (or nil)")
    check(open == nil or type(open) == "function" or type(open) == "table", "ui.title_entry: open is a function or a screen spec")
    local on_select = nil
    if open ~= nil then
      on_select = function()
        local spec = open
        if type(spec) == "function" then spec = spec() end
        if type(spec) == "table" then ui.screen(spec) end
      end
    end
    native.ui_title_entry(label, help, on_select)
  end
  --- This mod's rows on the character creation screen, under the game's own (two places shared
  --- by all mods). A row is {label, help, page = {tabs}} or {label, help, text = {key, empty}}: a
  --- page opens the System screen with the tabs, each {title, icon, switches = {{key, label,
  --- help, default}}} (icon: "candelabra", "bell", "skulls", "equipment" or "shrine"); a text row
  --- opens the game's text box (what it says for no text: empty). Replaces this mod's rows.
  function ui.character_rows(rows)
    check(type(rows) == "table", "ui.character_rows: a list of rows")
    native.ui_character_rows(rows)
  end
  --- What this mod's creation screen rows hold now, for the character being made: {key = value}
  --- (a switch true or false, a text row's text; defaults where the player changed nothing).
  function ui.character_choices() return native.ui_character_choices() end
  --- The rows back to their defaults (once the mod has taken the choices for a character).
  function ui.reset_character_choices() native.ui_reset_character_choices() end
  --- Puts text on the clipboard (1024 characters at most); false if it couldn't.
  function ui.copy(text) return native.ui_copy(tostring(text)) end
  --- A small notification in the top-right corner. opts: seconds (4), color.
  function ui.toast(message, opts)
    opts = opts or {}
    native.ui_notice("toast", tostring(message), opts.seconds or 4, opts.color or "gold")
  end
  local HUD = { off = 0, on = 1, auto = 2 }
  local HUD_NAMES = { [0] = "off", [1] = "on", [2] = "auto" }
  --- The HUD setting (Settings > HUD): "off", "on" or "auto" (shown when something changes).
  --- Like the settings menu, so menus keep working; the player's own setting comes back
  --- when the mod unloads.
  function ui.set_hud(mode)
    check(HUD[mode] ~= nil, "ui.set_hud: mode must be \"off\", \"on\" or \"auto\"")
    native.ui_set_hud(HUD[mode])
  end
  --- The current HUD setting: "off", "on" or "auto".
  function ui.hud() return HUD_NAMES[native.ui_hud()] end
  --- One of the game's own banners by name ("bonfire_lit", "lord_of_cinder_fallen", ...) or
  --- type 1-22.
  function ui.game_banner(kind)
    check(type(kind) == "number" or type(kind) == "string", "ui.game_banner: give a banner name (e.g. \"bonfire_lit\") or type (1-22)")
    native.ui_game_banner(kind)
  end
  kindle.ui = ui
end

-- ── player ──────────────────────────────────────────────────────────────────
if allowed.player then
  local player = {}
  player.loaded = native.player_loaded
  player.name = native.player_name
  player.stats = native.player_stats
  for _, stat in ipairs({ "hp", "fp", "stamina" }) do
    player[stat] = function() return native.player_get(stat) end
    player["max_" .. stat] = function() return native.player_get("max_" .. stat) end
    player["set_" .. stat] = function(v) native.player_set(stat, v) end
  end
  function player.heal()
    native.player_set("hp", native.player_get("max_hp"))
    native.player_set("fp", native.player_get("max_fp"))
  end
  --- Apply an SpEffect (SpEffectParam row id) to the player: buffs, debuffs, states.
  player.apply_effect = native.player_apply_effect
  --- End an active SpEffect on the player; returns how many instances were ended.
  player.remove_effect = native.player_remove_effect
  --- Active SpEffects: list of {id, time_left, duration} (-1 = permanent).
  player.effects = native.player_effects
  --- The player's position: x, y (up), z.
  player.position = native.player_position
  --- Souls held.
  player.souls = native.player_souls
  --- Set souls held (clamped to 0..999,999,999).
  player.set_souls = native.player_set_souls
  function player.add_souls(n) native.player_set_souls(native.player_souls() + n) end
  player.soul_level = native.player_soul_level
  --- Set an attribute (1-99); soul level moves by the same amount unless keep_level.
  function player.set_stat(name, value, keep_level) native.player_set_attribute(name, value, keep_level) end
  --- Set several attributes at once: player.set_stats({vigor = 20, strength = 18})
  function player.set_stats(t, keep_level)
    for name, value in pairs(t) do native.player_set_attribute(name, value, keep_level) end
  end
  player.set_soul_level = native.player_set_soul_level
  --- {slot = item}: left1-3, right1-3, arrow1-2, bolt1-2, head, body, arms, legs, ring1-4,
  --- covenant, quick1-10, tool1-5. Empty slots are absent.
  player.equipment = native.player_equipment
  --- Equip an item from the inventory in a slot (same names as player.equipment()), using
  --- the game's own equip path. The item must be in the inventory (items.give it first).
  function player.equip(slot, item) native.player_equip(slot, item) end
  --- Empty a slot (weapons fall back to Fists, armour to the bare piece).
  function player.unequip(slot) native.player_equip(slot, nil) end
  --- Warp to a bonfire (table from world.bonfires(), name, or id) like the bonfire menu.
  --- Only lit, warpable bonfires unless force.
  function player.warp(bonfire, force) native.player_warp(bonfire, force) end
  --- Entity id of the bonfire last rested at.
  player.last_bonfire = native.player_last_bonfire
  --- Attuned spells (item ids), in slot order.
  player.spells = native.player_spells
  --- 0 on the first playthrough, 1 in NG+, ...
  player.ng_cycle = native.player_ng_cycle
  player.deaths = native.player_deaths
  --- Play time in seconds.
  player.play_time = native.player_play_time
  --- {poison|toxic|bleed|curse|frost = {value, max}}: build-up; the effect triggers at max.
  function player.status() return native.chr_status(nil) end
  --- current, max poise (most characters only have poise during certain attacks).
  function player.poise() return native.chr_poise(nil) end
  --- Name of the current animation state, e.g. "Idle".
  function player.animation() return native.chr_animation(nil) end
  function player.animation_speed() return native.chr_speed(nil) end
  --- 1 is normal; 0-10.
  function player.set_animation_speed(speed) native.chr_set_speed(nil, speed) end
  --- true: HP can't drop below 1.
  function player.set_no_death(on) native.chr_set_flag(nil, "no_death", on ~= false) end
  --- true: hits do no damage.
  function player.set_no_damage(on) native.chr_set_flag(nil, "no_damage", on ~= false) end
  function player.set_infinite_stamina(on) native.chr_set_flag(nil, "no_stamina_use", on ~= false) end
  function player.set_infinite_fp(on) native.chr_set_flag(nil, "no_fp_use", on ~= false) end
  --- true: using consumables doesn't use them up.
  function player.set_infinite_items(on) native.chr_set_flag(nil, "no_item_use", on ~= false) end
  --- Souls gained over the whole playthrough.
  player.total_souls = native.player_total_souls
  player.embered = native.player_embered
  --- true: become embered through the game's own transition; false: clear the state.
  player.set_embered = native.player_set_embered
  --- {hp, hp_max, fp, fp_max}: Estus / Ashen Estus charges and allotments.
  player.estus = native.player_estus
  --- Set charges (nil leaves one unchanged), clamped to the allotment.
  function player.set_estus(hp, fp) native.player_set_estus(hp, fp) end
  --- Move the player to a position (x, y up, z).
  function player.teleport(x, y, z) native.chr_teleport(nil, x, y, z) end
  --- Facing angle in radians.
  function player.facing() return native.chr_facing(nil) end
  --- The game camera: {x, y, z (its position), dx, dy, dz (where it looks), yaw, fov}.
  function player.camera() return native.player_camera() end
  --- The character the player is locked on to (a world.characters entry), or nil.
  function player.lock_target() return native.player_lock_target() end
  function player.set_lock_target(target)
    check(target == nil or (type(target) == "table" and math.type(target.handle) == "integer"),
      "player.set_lock_target: a character (from world.characters) or nil")
    return native.player_set_lock_target(target and target.handle)
  end
  function player.set_camera(cam)
    check(cam == nil or type(cam) == "table", "player.set_camera: a table {x, y, z, dx, dy, dz} or {x, y, z, look_at}, or nil")
    return native.player_set_camera(cam)
  end
  function player.set_facing(radians) native.chr_set_facing(nil, radians) end
  function player.invulnerable() return native.chr_flag(nil, "invulnerable") end
  function player.set_invulnerable(on) native.chr_set_flag(nil, "invulnerable", on ~= false) end
  function player.set_gravity(on) native.chr_set_flag(nil, "no_gravity", on == false) end
  --- Play an animation event by name (e.g. "W_ItemDrink") or id.
  function player.play_animation(anim) native.chr_play_animation(nil, anim) end
  --- {[id] = name} of every animation event the player's behaviour accepts.
  function player.animations() return native.chr_animations(nil) end
  function player.refill_estus()
    local e = native.player_estus()
    native.player_set_estus(e.hp_max, e.fp_max)
  end
  --- Whether an SpEffect is currently active on the player.
  function player.has_effect(id)
    for _, e in ipairs(native.player_effects()) do
      if e.id == id and (e.duration < 0 or e.time_left > 0) then return true end
    end
    return false
  end
  kindle.player = player
end

-- ── world (other characters) ────────────────────────────────────────────────
-- Event script commands by name: (bank, index, argument format, argument names). Argument
-- layouts follow the community's documentation of the event files (DarkScript3); names are Kindle's.
-- Booleans may be given as true/false. Shared by kindle.events and kindle.world.
local EVENT_COMMANDS = {
  award_item_lot = { 2003, 4, "i", "lot" },
  warp_player = { 2003, 14, "BBi", "area, block, start_entity" },
  show_boss_bar = { 2003, 11, "hiIi", "on, entity, slot, name_id" },
  handle_boss_defeat = { 2003, 12, "i", "entity" },
  boss_defeat_banner = { 2003, 74, "iB", "entity, banner" },
  set_respawn_point = { 2003, 23, "i", "entity" },
  -- The game's flag is "show" (0 hides the HUD and also blocks the pause menu).
  show_hud = { 2003, 71, "B", "shown" },
  set_ai = { 2004, 1, "iB", "entity, on" },
  set_team = { 2004, 2, "iB", "entity, team" },
  kill = { 2004, 4, "iB", "entity, give_souls" },
  set_enabled = { 2004, 5, "iB", "entity, on" },
  apply_effect = { 2004, 8, "ii", "entity, speffect" },
  set_immortal = { 2004, 12, "iB", "entity, on" },
  set_invincible = { 2004, 15, "iB", "entity, on" },
  clear_ai_target = { 2004, 16, "i", "entity" },
  ai_command = { 2004, 17, "iiB", "entity, command, slot" },
  set_ai_id = { 2004, 19, "ii", "entity, ai_id" },
  ai_replan = { 2004, 20, "i", "entity" },
  remove_effect = { 2004, 21, "ii", "entity, speffect" },
  set_hp_bar = { 2004, 30, "iB", "entity, on" },
  bonfire_recovery = { 2004, 47, "", "" },
  spawn_sfx = { 2006, 3, "iiii", "target_type, entity, dummypoly, sfx" },
  spawn_map_sfx = { 2006, 2, "i", "entity" },
  delete_map_sfx = { 2006, 1, "iB", "entity, root_only" },
  status_message = { 2007, 3, "iB", "text_id, pad" },
  area_name = { 2007, 11, "i", "text_id" },
  camera_shake = { 2008, 2, "iiiiff", "shake_id, target_type, entity, dummypoly, fade_start, fade_end" },
  play_sound = { 2010, 2, "iii", "entity, sound_type, sound_id" },
  set_map_part = { 2012, 1, "iB", "entity, on" },
  destroy_object = { 2005, 1, "ib", "entity, slot" },
  restore_object = { 2005, 2, "i", "entity" },
  set_object_active = { 2005, 3, "iB", "entity, on" },
  set_object_treasure = { 2005, 4, "iB", "entity, on" },
  set_obj_act = { 2005, 6, "iiB", "entity, obj_act, on" },
  object_animation = { 2005, 7, "ii", "entity, animation" },
}
-- Names some arguments accept instead of numbers.
local EVENT_NAMES = {
  target_type = { object = 0, area = 1, character = 2 },
  sound_type = { environment = 0, character = 1, menu = 2, object = 3, cutscene = 4, sfx = 5, bgm = 6, voice = 7, floor = 8, armor = 9, ghost = 10 },
  team = { disabled = 0, human = 1, white_phantom = 2, black_phantom = 3, hollow = 4, enemy = 6, strong_enemy = 7, ally = 8, hostile_ally = 9, friendly_enemy = 12, invader = 13, friendly_npc = 26, hostile_npc = 27, indiscriminate = 29 },
}
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

if allowed.world then
  local world = {}
  local function key(c)
    if type(c) == "table" then return c.handle end
    return c
  end
  --- Loaded characters, nearest first. opts: radius (metres from the player),
  --- alive (default true: skip dead), player (default false), helpers (default false:
  --- skip invisible event helpers), kind ("enemy", "npc", "player", "helper", "other").
  --- Each is {handle, entity_id, npc_id, name, model, kind, team, hp, max_hp, alive,
  --- x, y, z, distance}.
  function world.characters(opts)
    opts = opts or {}
    return native.world_characters(opts.radius, opts.alive ~= false, opts.player == true,
      opts.helpers == true, opts.kind)
  end
  --- The map's placed objects that are loaded (doors, levers, chests, ...), nearest first:
  --- {handle, entity_id, map, x, y, z, destroyed, distance}. opts: radius, map, entity_id.
  function world.objects(opts)
    opts = opts or {}
    check(type(opts) == "table", "world.objects: a table {radius, map, entity_id} or nothing")
    return native.world_objects(opts.radius, opts.map, opts.entity_id)
  end
  -- Object commands target objects by entity id: an object from world.objects or the id.
  local function object_entity(o, what)
    local entity = type(o) == "table" and o.entity_id or o
    check(math.type(entity) == "integer", what .. ": an object with an entity id (from world.objects), or the entity id")
    return entity
  end
  --- Break an object as a hit does (its pieces fall, its treasure drops), or put it back whole.
  function world.destroy_object(o) run_named("destroy_object", { object_entity(o, "world.destroy_object"), 1 }) end
  function world.restore_object(o) run_named("restore_object", { object_entity(o, "world.restore_object") }) end
  --- Turn an object's action on or off (a door that can't be opened, a lever that can't be pulled).
  function world.set_object_active(o, on)
    run_named("set_object_active", { object_entity(o, "world.set_object_active"), on ~= false })
  end
  --- The players' characters, alive or not: this game's first (is_player), then in a session
  --- the others' (other_player). Quick enough for every frame (characters() reads the area's).
  function world.players() return native.world_players() end
  --- One character by handle, entity id, or a table from world.characters; nil if not loaded.
  function world.character(c) return native.world_character(key(c)) end
  --- Create a character (experimental): {enemy = "black_knight", or model / npc_param /
  --- think_param / area; distance or x/y/z, angle, ai_enabled, on_spawn}. Returns it like
  --- world.character, or nil while its area's AI loads; on_spawn(c) runs once it exists.
  function world.spawn(opts)
    opts = opts or {}
    local chr, token = native.world_spawn(opts)
    if not token then
      if chr and opts.on_spawn then opts.on_spawn(chr) end
      return chr
    end
    local timer
    timer = kindle.every(0.1, function()
      local ok, c = pcall(native.world_spawn_poll, token)
      if not ok then
        kindle.cancel(timer)
        kindle.log.warn("world.spawn: %s", tostring(c))
      elseif c then
        kindle.cancel(timer)
        if opts.on_spawn then opts.on_spawn(c) end
      end
    end)
    return nil
  end
  --- Load an area's AI (e.g. "m33_00_00_00") so enemies from there act when spawned; true
  --- once ready (starts loading on the first call).
  function world.load_ai(area) return native.world_load_ai(area) end
  -- Event script commands target characters by entity id.
  local function entity_of(c, what)
    local chr = native.world_character(key(c))
    check(chr ~= nil, what .. ": that character isn't loaded")
    check(chr.entity_id ~= nil, what .. ": that character has no entity id (event commands can't target it)")
    return chr.entity_id
  end
  --- Turn a character on or off: a disabled character disappears and does nothing until
  --- enabled again.
  function world.set_enabled(c, on)
    run_named("set_enabled", { entity_of(c, "world.set_enabled"), on ~= false })
  end
  --- Turn a character's AI on or off: with it off, it stands still and won't attack. Off
  --- holds (the game's scripts can't wake it) until turned on or the mod unloads.
  function world.set_ai(c, on)
    local chr = native.world_character(key(c))
    check(chr ~= nil, "world.set_ai: that character isn't loaded")
    if chr.entity_id ~= nil then
      run_named("set_ai", { chr.entity_id, on ~= false })
    else
      -- No entity id (event commands can't target it): the flag the game's setter writes.
      native.chr_set_ai(chr.handle, on ~= false)
    end
  end
  --- Make a character forget its target (drop aggro) and choose what to do again.
  function world.clear_aggro(c)
    local chr = native.world_character(key(c))
    check(chr ~= nil, "world.clear_aggro: that character isn't loaded")
    if chr.entity_id ~= nil then
      run_named("clear_ai_target", { chr.entity_id })
      run_named("ai_replan", { chr.entity_id })
    else
      -- No entity id: what those two commands call for their character.
      native.chr_clear_aggro(chr.handle)
    end
  end
  function world.set_hp(c, hp) native.world_set_hp(key(c), hp) end
  function world.kill(c) native.world_set_hp(key(c), 0) end
  function world.heal(c)
    local info = native.world_character(key(c))
    if info then native.world_set_hp(info.handle, info.max_hp) end
  end
  --- Apply / remove an SpEffect on another character.
  function world.apply_effect(c, id) native.world_apply_effect(key(c), id) end
  function world.remove_effect(c, id) return native.world_remove_effect(key(c), id) end
  function world.effects(c) return native.world_effects(key(c)) end
  --- Move a character to a position.
  function world.teleport(c, x, y, z) native.chr_teleport(key(c), x, y, z) end
  function world.facing(c) return native.chr_facing(key(c)) end
  function world.set_facing(c, radians) native.chr_set_facing(key(c), radians) end
  function world.invulnerable(c) return native.chr_flag(key(c), "invulnerable") end
  function world.set_invulnerable(c, on) native.chr_set_flag(key(c), "invulnerable", on ~= false) end
  function world.set_gravity(c, on) native.chr_set_flag(key(c), "no_gravity", on == false) end
  --- Show or hide a character's model.
  function world.set_visible(c, on) native.chr_set_flag(key(c), "draw_model", on ~= false) end
  --- Play an animation event on a character by name or id.
  function world.play_animation(c, anim) native.chr_play_animation(key(c), anim) end
  function world.animations(c) return native.chr_animations(key(c)) end
  function world.status(c) return native.chr_status(key(c)) end
  function world.poise(c) return native.chr_poise(key(c)) end
  function world.animation(c) return native.chr_animation(key(c)) end
  function world.animation_speed(c) return native.chr_speed(key(c)) end
  function world.set_animation_speed(c, speed) native.chr_set_speed(key(c), speed) end
  function world.set_no_death(c, on) native.chr_set_flag(key(c), "no_death", on ~= false) end
  function world.set_no_damage(c, on) native.chr_set_flag(key(c), "no_damage", on ~= false) end
  --- Change a character's team: "enemy", "npc" (friendly), "player", or a raw number.
  function world.set_team(c, team)
    local teams = { player = 1, enemy = 6, helper = 8, npc = 26 }
    native.chr_set_team(key(c), teams[team] or team)
  end
  --- Every bonfire: {id, entity_id, name, lit, warpable, dlc, last_rested}.
  world.bonfires = native.world_bonfires
  --- Distance between two characters (tables/handles/entity ids) or a character and a point.
  function world.distance(a, b)
    local p = type(a) == "table" and a.x and a or native.world_character(key(a))
    local q = type(b) == "table" and b.x and b or native.world_character(key(b))
    if not p or not q then return nil end
    return sqrt((p.x - q.x)^2 + (p.y - q.y)^2 + (p.z - q.z)^2)
  end
  kindle.world = world
end

-- ── events (the game's event script commands) ─────────────────────────────────
if allowed.events then
  local events = kindle.events
  --- Run one of the game's event script (EMEVD) commands: bank, index, then the arguments'
  --- format (one letter per value: b/B i8/u8, h/H i16/u16, i/I i32/u32, f f32) and values.
  --- Runs on Kindle's next frame; an optional last argument on_done(done, err) gets whether
  --- the game's handler finished it.
  function events.run(bank, index, format, ...)
    check(type(bank) == "number" and type(index) == "number", "events.run: bank and index must be numbers")
    local values, on_done = { ... }, nil
    if type(values[#values]) == "function" then on_done = tremove(values) end
    check(format == nil or type(format) == "string", "events.run: format must be a string")
    return native.events_run(bank, index, format or "", values, on_done)
  end
  -- Named commands: events.award_item_lot(4000), events.play_sound(10000, "sfx", 123), ...
  -- An optional last argument on_done(done, err) as for run.
  for name in pairs(EVENT_COMMANDS) do
    events[name] = function(...)
      local args, on_done = { ... }, nil
      if type(args[#args]) == "function" then on_done = tremove(args) end
      return run_named(name, args, on_done)
    end
  end

  kindle.events = events
end

-- ── hks (character behaviour scripts) ─────────────────────────────────────────
if allowed.hks then
  local hks = {}
  --- Override the answer to an env(id, ...) question the character behaviour scripts (HKS)
  --- ask: fn(e) with e.character, e.id, e.args, e.result; return a number/boolean to change
  --- the answer (nil keeps it). Runs often: keep it quick.
  function hks.on_env(id, fn) return native.hks_on(false, id, fn) end
  --- Watch or block an act(id, ...) action of the behaviour scripts: fn(e) with e.character,
  --- e.id, e.args; return false to block it.
  function hks.on_act(id, fn) return native.hks_on(true, id, fn) end
  --- Known ids by name: hks.env.HP, hks.env.STAMINA, hks.act.SET_VARIABLE, ...
  hks.env, hks.act = native.hks_names()
  local function key(c) if c == nil then return nil end; if type(c) == "table" then return c.handle end; return c end
  --- Have a character's script perform act(id, ...) (character: table, handle or entity id;
  --- nil = the player). Runs at that script's next call (within a frame); on_done(result)
  --- is optional (result nil, plus a reason, if the character's script didn't run within ~5 s).
  function hks.run_act(c, id, ...)
    local args, on_done = { id, ... }, nil
    if type(args[#args]) == "function" then on_done = tremove(args) end
    return native.hks_request(key(c), true, args, on_done)
  end
  --- Ask a character's script env(id, ...); the answer goes to fn(result).
  function hks.ask_env(c, id, ...)
    local args = { id, ... }
    local fn = tremove(args)
    assert(type(fn) == "function", "hks.ask_env: the last argument must be a function for the answer")
    return native.hks_request(key(c), false, args, fn)
  end
  --- Run HKS source in a character's script state (its globals, its functions); the first
  --- value it returns goes to fn(result, err) (err set if it didn't compile or raised).
  --- Kindle removes io, os, package and debug from that state first.
  function hks.eval(c, source, fn)
    check(type(source) == "string", "hks.eval: source must be a string")
    check(type(fn) == "function", "hks.eval: the last argument must be a function for the result")
    return native.hks_eval(key(c), source, fn)
  end
  -- A value as HKS source (HKS is Lua 5.1: no hex floats).
  local function literal(v, what)
    local t = type(v)
    if t == "string" then return fmt("%q", v) end
    if t == "boolean" then return tostring(v) end
    if t == "number" and v == v and v ~= math.huge and v ~= -math.huge then return fmt("%.9g", v) end
    error(what .. ": value must be a number, boolean or string", 3)
  end
  local function noop() end
  --- Read a behaviour graph variable (e.g. "MoveSpeedLevelReal"): fn(value, err).
  function hks.get_variable(c, name, fn)
    check(type(name) == "string", "hks.get_variable: name must be a string")
    check(type(fn) == "function", "hks.get_variable: the last argument must be a function for the value")
    return native.hks_eval(key(c), "return hkbGetVariable(" .. literal(name) .. ")", fn)
  end
  --- Set a behaviour graph variable. The script sets many of them every frame; to hold a
  --- value against the script, block its own writes with on_act(act.SET_VARIABLE).
  function hks.set_variable(c, name, value, on_done)
    check(type(name) == "string", "hks.set_variable: name must be a string")
    local src = "hkbSetVariable(" .. literal(name) .. ", " .. literal(value, "hks.set_variable") .. ")"
    return native.hks_eval(key(c), src, on_done or noop)
  end
  local function global_name(name, what)
    check(type(name) == "string" and name:match("^[%a_][%w_]*$") ~= nil, what .. ": name must be a Lua identifier")
  end
  --- Read a global of the character's script (numbers, booleans, strings; nil otherwise).
  function hks.get_global(c, name, fn)
    global_name(name, "hks.get_global")
    check(type(fn) == "function", "hks.get_global: the last argument must be a function for the value")
    return native.hks_eval(key(c), fmt("return rawget(_G, %q)", name), fn)
  end
  --- Set a global of the character's script (nil removes it).
  function hks.set_global(c, name, value, on_done)
    global_name(name, "hks.set_global")
    local v = value == nil and "nil" or literal(value, "hks.set_global")
    return native.hks_eval(key(c), fmt("rawset(_G, %q, %s)", name, v), on_done or noop)
  end
  --- Handle kindle(name, ...) calls from edited behaviour scripts: fn(e) with e.character,
  --- e.name, e.args. The first number/boolean/string a handler returns goes back to the
  --- script. Runs on the game's script threads: keep it quick.
  function hks.on_call(name, fn)
    check(type(name) == "string", "hks.on_call: name must be a string")
    check(type(fn) == "function", "hks.on_call: handler must be a function")
    return native.hks_on_call(name, fn)
  end
  kindle.hks = hks
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
  -- In the item tables a row can be named by its item's name.
  local ITEM_KIND = { EquipParamGoods = "goods", EquipParamWeapon = "weapon", EquipParamProtector = "armor", EquipParamAccessory = "ring" }
  -- (Errors are raised at level 4: the mod's call to params.get/add_row.)
  local function row_id(tbl, id, what)
    if type(id) == "string" then
      local kind = ITEM_KIND[tbl]
      if not kind then error(what .. ": only item tables take an item name for the row (" .. tostring(tbl) .. " needs a row id)", 4) end
      return native.item_by_name(id, kind) & 0x0FFFFFFF
    end
    if math.type(id) ~= "integer" then error(what .. ": row must be an integer id (or an item name in an item table)", 4) end
    return id
  end
  --- A live row: read and assign fields by name. Edits are reverted when the mod unloads.
  --- In item tables the row can be an item name: params.get("EquipParamGoods", "Ember").
  function params.get(tbl, id)
    id = row_id(tbl, id, "params.get")
    if native.param_has(tbl, id) then return row_proxy(tbl, id) end
    return nil
  end
  --- Add a new row to a table, a copy of row copy_from, and return it (a live row, like get).
  --- The id must be above every existing id in the table; the row stays for the session.
  --- In item tables copy_from can be an item name: params.add_row("EquipParamGoods", MY_ID, "Ember").
  function params.add_row(tbl, id, copy_from)
    check(type(tbl) == "string", "params.add_row: table must be a string")
    check(math.type(id) == "integer", "params.add_row: id must be an integer")
    copy_from = row_id(tbl, copy_from, "params.add_row")
    native.param_add_row(tbl, id, copy_from)
    return row_proxy(tbl, id)
  end
  --- Add several rows to a table with one rebuild of it (add_row rebuilds the table for each: a second
  --- each, and every place the game holds it moves): `rows` is {{id = ID, copy_from = ROW}, ...}, ids
  --- ascending and above every existing id. Returns the live rows, in order.
  function params.add_rows(tbl, rows)
    check(type(tbl) == "string", "params.add_rows: table must be a string")
    check(type(rows) == "table", "params.add_rows: rows must be a list of {id = ..., copy_from = ...}")
    local ids, copies = {}, {}
    for i, r in ipairs(rows) do
      check(type(r) == "table" and math.type(r.id) == "integer", "params.add_rows: each row needs an integer id")
      ids[i] = r.id
      copies[i] = row_id(tbl, r.copy_from, "params.add_rows")
    end
    native.param_add_rows(tbl, ids, copies)
    local out = {}
    for i, r in ipairs(rows) do out[i] = row_proxy(tbl, r.id) end
    return out
  end
  --- Copies rows over others in one go: `pairs` is {{from, to}, ...}, each `to` getting what its
  --- `from` holds before any of them is copied, except the fields named in `keep` (e.g.
  --- {"getItemFlagId"}). Reverted when the mod unloads, as any write.
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

-- ── maps ────────────────────────────────────────────────────────────────────
if allowed.maps then
  local maps = {}
  local function map_name(map, what)
    if type(map) ~= "string" or not map:match("^m%d%d_%d%d_%d%d_%d%d$") then
      error(fmt("maps.%s: map must be a map's name like \"m30_00_00_00\"", what), 3)
    end
  end
  --- A map's parts as the game's file has them, in the file's order (part 1 first), or nil if
  --- the game has no such map (a DLC's that isn't installed). Each part: {name, type, kind
  --- ("enemy", "player", "collision", "object", "map_piece", ...), model ("c1100"), position
  --- {x, y, z}, rotation {x, y, z}, entity_id, entity_groups, and an enemy's think_param,
  --- npc_param and talk}. Read from the game's archives the first time (up to a second).
  function maps.parts(map)
    map_name(map, "parts")
    return native.maps_parts(map)
  end
  --- Reads maps' parts ahead, on a thread of Kindle's (the game goes on), so maps.parts gives
  --- them at once later; for a mod that needs many maps as an area loads.
  function maps.preload(list)
    check(type(list) == "table", "maps.preload: a list of map names")
    for _, map in ipairs(list) do map_name(map, "preload") end
    native.maps_preload(list)
  end
  --- Set this mod's changes to a map's file (replacing its earlier ones for that map), applied
  --- when the game next reads the file (the next time the area loads; change them in
  --- "map_loading" to have them for the load that's starting). A change:
  ---   {replace_enemy = n, from = {map =, part = m}, keep_ids = false}
  --- part n (an enemy) becomes an enemy of part m's type (of any map): it keeps its place, the
  --- ground it stands on and its walk route; its entity id and groups go unless keep_ids.
  ---   {replace_enemy = n, model = "c7200", npc_param = A, think_param = B, ai = "name", keep_ids = false}
  --- part n becomes an enemy of a model no map has (a mod's own, its files served by the mod)
  --- with the NpcParam row and AI row given (each only if given) and its goals in
  --- script/<ai>.luabnd (loaded as long as the map is); the model's name has as many characters as
  --- the part's own.
  function maps.edit(map, changes)
    map_name(map, "edit")
    check(type(changes) == "table", "maps.edit: changes must be a list")
    native.maps_edit(map, changes)
  end
  --- Drop this mod's changes to a map (to every map without one); how many there were.
  function maps.reset(map)
    if map ~= nil then map_name(map, "reset") end
    return native.maps_reset(map)
  end
  kindle.maps = maps
end

-- ── debug (tools for finding things; memory inspection for reverse engineering) ──
-- The memory and code functions (singletons, read, bytes, wstr, class_name, module, watch,
-- trace_next) exist only with raw access: the console, and the mods the player names in
-- kindle.toml [developer]. Without it, export writes only in kindle/exports/<mod id>/.
if allowed.debug then
  local debug_ = {}
  local raw = info.raw
  debug_.press = native.debug_press
  debug_.input_stats = native.debug_input_stats
  --- milliseconds per second each mod spent in Lua over the last second: { [mod id] = {
  --- total = ms, [handler] = ms } }, handlers named by event ("frame", "damage", ...)
  debug_.profile = native.debug_profile
  if raw then
    --- name -> address table of every game singleton
    debug_.singletons = native.debug_singletons
    debug_.class_name = native.debug_class_name
    --- instance address of a named singleton, or nil
    debug_.singleton = native.debug_singleton
    --- base, size of DarkSoulsIII.exe
    debug_.module = native.debug_module
    --- read a value: type is u8/i8/u16/i16/u32/i32/u64/ptr/f32/f64 (default u64); nil if unreadable
    debug_.read = native.debug_read
    --- hex dump of up to 4096 bytes
    debug_.bytes = native.debug_bytes
    --- read a null-terminated UTF-16 string
    debug_.wstr = native.debug_wstr
  end
  --- format a number as hex
  function debug_.hex(n) return string.format("0x%x", n) end
  --- Log the behaviour scripts' (HKS) env/act calls to kindle.log: each env id once per
  --- character, every act.
  function debug_.trace_hks(on) native.debug_trace_hks(on ~= false) end
  --- Log each distinct event script command the game runs: "event command 2007[02](1)" is
  --- bank 2007, index 2, its arguments as 32-bit words. For finding commands for events.run.
  function debug_.trace_events(on) native.debug_trace_events(on ~= false) end
  --- Count the game's "who is this player" checks (session guest, multiplayer, world owner,
  --- host, online, the per-type table's columns) by caller and answer; debug.predicates() lists
  --- them, and the co-op watchdog's reports too. Returns how many checks are hooked.
  function debug_.trace_predicates(on) return native.debug_trace_predicates(on ~= false) end
  debug_.predicates = native.debug_predicates
  if raw then
    --- Watch which game code touches [addr, addr+len) (within one page): debug.watch_hits()
    --- lists each instruction (offset in the exe), the address it touched and return offsets.
    --- Stops by itself after max (16) distinct instructions; debug.watch_stop() ends it.
    function debug_.watch(addr, len, max) native.debug_watch(addr, len, max) end
    debug_.watch_stop = native.debug_watch_stop
    --- record every instruction the next call of the game function at rva runs (logs/trace_<n>.bin)
    function debug_.trace_next(rva, max) native.debug_trace_next(rva, max) end
    debug_.watch_hits = native.debug_watch_hits
    --- How often the game ran each of Kindle's hooks since it was hooked, and on which threads:
    --- { area, name, calls, off_game_thread, threads, per_second }.
    debug_.census = native.debug_census
  end
  --- Save a game file from the game's archives, decrypted (as the game reads it): path like
  --- "/event/m40_00_00_00.emevd.dcx"; dest a file name, written in kindle/exports/<mod id>/
  --- (with raw access, any path). Returns which archive held it.
  debug_.export = native.debug_export
  --- Log every talk-script command and query (NPC, bonfire and shop menus) to kindle.log.
  function debug_.trace_talk(on) native.debug_trace_talk(on ~= false) end
  kindle.debug = debug_
end

-- ── net (co-op sessions: messages to the same mod on the other games) ────────────────────────
if allowed.input then
  local input = {}
  --- Keeps the player's actions from the game, or gives them back.
  function input.block_actions(on) native.input_block_actions(on ~= false) end
  --- While actions are blocked: keeps the interact button from the game too.
  function input.block_interact(on) native.input_block_interact(on ~= false) end
  --- Whether "left", "right" or "action" is held.
  function input.held(name) return native.input_held(name) end
  --- Whether the player uses a controller (rather than keyboard and mouse) lately.
  function input.using_pad() return native.input_using_pad() end
  --- Whether the player holds a key ("f5", "q", "num1"...), a controller button ("pad_a"...) or
  --- a mouse button ("mouse_left", "mouse_right", "mouse_middle").
  function input.mouse() return native.input_mouse() end
  function input.down(name)
    check(type(name) == "string", "input.down: a key or button's name")
    return native.input_down(name)
  end
  --- Holds a key or controller button down for the game.
  function input.press(key, seconds) native.input_press(key, seconds) end
  kindle.input = input
end

if allowed.net then
  local net = {}
  --- Sends a value (boolean, number, string or table, as kindle.store keeps them; 16 KB at most)
  --- to this mod on other games of the co-op session: to "host", "all" (every other game) or a
  --- member's id (kindle.net.members()). A player's goes through the host. Errors without a
  --- session.
  function net.send(to, channel, value)
    check(type(to) == "string" and type(channel) == "string", "net.send: to and channel are strings")
    native.net_send(to, channel, value)
  end
  --- fn(value, from) for this mod's messages on `channel` from the other games (on Kindle's frame).
  --- Returns a handle (kindle.cancel).
  function net.on(channel, fn)
    check(type(fn) == "function", "net.on: give a function")
    return native.net_on(channel, fn)
  end
  --- This game in the session: "host", "player", or nil without one.
  --- A host's join data: `value` (a value as net.send takes) goes to each player as it joins,
  --- before its game loads this world, and to the players in now; nil removes it. For what a
  --- player's game needs in place as it arrives (a randomizer's seed).
  function net.set_welcome(channel, value)
    check(type(channel) == "string", "net.set_welcome: channel is a string")
    native.net_set_welcome(channel, value)
  end
  --- A player in a host's world: the host's join data on `channel` (this mod's), or nil.
  function net.welcome(channel)
    check(type(channel) == "string", "net.welcome: channel is a string")
    return native.net_welcome(channel)
  end
  --- What players joining this host's world need: a list of DLCs ("ashes_of_ariandel",
  --- "ringed_city"); a player without one is refused with why. {} for none.
  function net.require_dlc(list)
    check(type(list) == "table", "net.require_dlc: a list of DLC names")
    native.net_require_dlc(list)
  end
  function net.role() return native.net_role() end
  --- This game's id, the host's, and every member's ("0x..."), or nil without a session.
  function net.me() return native.net_me() end
  function net.host() return native.net_host() end
  function net.members() return native.net_members() end
  function net.open_lobby() return native.net_open_lobby() end
  function net.join_lobby(code, fn) native.net_join_lobby(code, fn) end
  function net.leave_lobby() native.net_leave_lobby() end
  function net.lobby() return native.net_lobby() end
  function net.lobby_send(to, channel, value) native.net_lobby_send(to, channel, value) end
  function net.send_party_save(member) return native.net_send_party_save(member) end
  function net.player(id) return native.net_player(id) end
  --- Host a session (from the world). Its code: net.code().
  function net.host_session() native.net_host_session() end
  --- Join the session with this code, off the game's thread: fn(result, why) on Kindle's frame,
  --- result "joined", "gone", "refused" or "failed".
  function net.join(code, fn)
    check(type(code) == "string" and type(fn) == "function", "net.join: a code and a function")
    native.net_join(code, fn)
  end
  --- Leave the session (Kindle's connection and the game's). for_now: the player means to come back
  --- (following the host): Kindle's party keeps joining it.
  function net.leave(for_now) native.net_leave(for_now) end
  --- The game's session: "none", "creating", "create failed", "hosting", "joining", "join
  --- failed", "joined" or "leaving".
  function net.state() return native.net_state() end
  --- This session's code (a host's, or the one this player joined with), or nil; and whether it
  --- names an address on the host's own network only.
  function net.code() return native.net_code() end
  --- The other players' names and pings in ms, as the HUD shows them: { {name, ms}, ... }.
  function net.pings() return native.net_pings() end
  kindle.net = net
end
