# Cookbook

Short, complete examples. Each one lists the permissions its `mod.toml` needs.

## Reward a boss kill

Permissions: `flags`, `items`

```lua
kindle.flags.on_set(kindle.flags.boss.iudex_gundyr, function()
  kindle.items.give("Ember") -- shows the game's "item received" box
end)
```

Every boss has a named flag (`kindle.flags.boss.*`, from the game's own event scripts) and
so does every bonfire (`kindle.flags.bonfire.firelink_shrine`, lit). Not sure of an item's
exact name? Search from the console: `kindle.items.find("estus")`.

## Take less damage, deal more

Permissions: none (hooks are core)

```lua
kindle.on("damage", function(e)
  if e.target and e.target.is_player then
    e.attack_power = e.attack_power * 0.5
  elseif e.attacker and e.attacker.is_player then
    e.attack_power = e.attack_power * 1.5
  end
end)
```

## Souls for every kill

Permissions: `player`

```lua
kindle.on("character_killed", function(c)
  if c.kind == "enemy" then
    kindle.player.add_souls(100)
  end
end)
```

## Warn at low HP

Permissions: `player`, `ui`

```lua
local warned = false
kindle.every(0.5, function()
  if not kindle.player.loaded() then return end
  local low = kindle.player.hp() < kindle.player.max_hp() * 0.25
  if low and not warned then
    kindle.ui.toast("Low HP!", { color = "red" })
  end
  warned = low
end)
```

## A bonfire menu option

Permissions: `ui`, `player`

```lua
-- The Handmaid's shop stock: ShopLineupParam rows 110000-119899.
local HANDMAID_FIRST, HANDMAID_LAST = 110000, 119899

kindle.ui.add_menu_option {
  text = "Kindle's blessing",
  where = "bonfire",
  on_select = function()
    kindle.ui.menu {
      { text = "Restore HP and FP", on_select = function() kindle.player.heal() end },
      { text = "Refill estus", on_select = function() kindle.player.refill_estus() end },
      { text = "Open the Handmaid's shop", on_select = function() kindle.ui.shop(HANDMAID_FIRST, HANDMAID_LAST) end },
    }
  end,
}
```

The option appears in every bonfire's menu; picking it opens a list in the game's own menu
style, and the shop entry opens the game's shop over those `ShopLineupParam` rows. With
`where = "npc"` it works the same in the menus of NPCs whose menu opens a screen (a shop,
reinforcing, levelling up ...): Andre, the Fire Keeper, the Shrine Handmaid and others.

## A menu of your own

Permissions: `ui`

```lua
local options = kindle.store.get_global("options") or { damage = 1.0, hard = false, motto = "" }
local function save() kindle.store.set_global("options", options) end

local function open_options()
  kindle.ui.screen {
    title = "Hard Mode",
    subtitle = "Changes apply at once",
    rows = {
      { header = "Enemies" },
      { text = "Damage", help = "How hard they hit.", slider = { min = 0.5, max = 3, step = 0.25 }, value = options.damage,
        on_change = function(v) options.damage = v; save() end },
      { text = "One life", toggle = options.hard, on_change = function(on) options.hard = on; save() end },
      { text = "Motto", input = options.motto, max = 24, on_change = function(text) options.motto = text; save() end },
      { header = "More" },
      { text = "Reset", on_select = function()
          kindle.ui.confirm { title = "Reset?", on_yes = function() options = { damage = 1.0, hard = false, motto = "" }; save() end }
        end },
    },
  }
end

kindle.ui.add_menu_option { text = "Hard Mode", where = "bonfire", on_select = open_options }
```

A screen of the mod's own, in the game's menu style, over everything: up and down move, left and right change
a switch, choice or slider, Enter (A) picks, Esc (B, a right click) goes back; the mouse hovers and clicks.
Rows are buttons (`on_select`), sub-screens (`screen = {...}` or a function returning one), switches, choices
(`choices = {...}`), sliders, text typed in (`input`), a key the player picks by pressing it (`key = "f6"`,
for a hotkey the mod then hears through `key_pressed`), lines of text (`info = true`, with a `value` on the right
for a stat) and headings. `screen:update(spec)` redraws it in place. `kindle.ui.choose` and `kindle.ui.confirm`
are two ready-made ones: a pick from a list, and a yes or no. Opened from a bonfire's menu, the screen is back
at that menu when it closes; `kindle.ui.title_entry("Hard Mode", "Its options.", open_options)` puts the same
screen on the title's main menu.

A longer screen can have tabs, as the game's own menus do: `tabs = {{title = "Enemies", rows = {...}},
{title = "Player", rows = {...}}}` in place of `rows`. LB and RB (Q and E, or a click on a tab's title) turn
them; `tab = 2` opens on the second, and `on_tab(index, title)` hears the player turn.

## Draw on the screen

Permissions: `ui`, `world`

```lua
local hud = kindle.ui.layer()
kindle.on("souls_changed", function(souls)
  hud:set { { text = "Souls: " .. souls, x = 1880, y = 40, align = "right", color = "#d8b46a" } }
end)

-- Bars over the enemies near the player, five times a second.
local marks = kindle.ui.layer()
kindle.every(0.2, function()
  local list = {}
  for _, enemy in ipairs(kindle.world.characters { radius = 20, kind = "enemy" }) do
    list[#list + 1] = { bar = { w = 100, h = 6, value = enemy.hp / enemy.max_hp }, color = "#b03020",
      at = { x = enemy.x, y = enemy.y + 2, z = enemy.z }, x = -50 }
  end
  marks:set(list)
end)
```

A layer draws text, boxes, lines, bars, circles and pictures until the mod changes it. `x` and `y` are on a
1920 x 1080 canvas that Kindle scales to the window; with `at`, they're an offset from where the camera sees that
point in the world. Walls don't hide a world point.

```lua
-- art/crest.png in the mod's folder, drawn 96 x 96 in the top-left corner.
kindle.ui.layer { { image = "art/crest.png", x = 40, y = 40, w = 96, h = 96 } }
```

A picture is a PNG file in the mod's folder; without `w` and `h` it's drawn at its own size in pixels, and a
`color` tints it. `kindle.input.mouse()` says where the cursor is on the same canvas, so a mod can tell when it's
over something it drew.

## A hotkey

Permissions: `input`, `player`

```lua
kindle.on("key_pressed", function(key)
  if key == "f6" and kindle.input.down("shift") then
    kindle.player.heal()
  end
end)
```

`key_pressed` names each key, controller button (`"pad_y"`) or mouse button (`"mouse_middle"`) as the player
presses it, but not while a screen or window of Kindle's is open. `kindle.input.down(name)` says whether one is
held now.

## Data in a file of the mod's own

```lua
-- drops.json, next to main.lua: [{"item": "Ember", "chance": 0.1}, ...]
local drops = kindle.json.decode(kindle.read_data("drops.json"))
for _, drop in ipairs(drops) do
  kindle.log.info("%s: %d%%", drop.item, drop.chance * 100)
end
```

`kindle.read_data` reads any file in the mod's folder (a path with `/` between folders); `kindle.json` turns
JSON text into tables and back. `kindle.date()` gives today's date, for a daily seed.

## Celebrate a boss kill

Permissions: `ui`

```lua
kindle.on("boss_defeated", function(kind)
  kindle.ui.toast(kind == "lord_of_cinder" and "A Lord of Cinder has fallen!" or "Boss down!")
end)
```

## Know when a boss fight starts

Permissions: `ui`

```lua
kindle.on("boss_bar", function(shown, name, entity)
  if shown then
    kindle.ui.toast("Fighting " .. (name or "a boss"))
  end
end)
```

`boss_bar` comes when the game's scripts show a boss's health bar (the fight starts) or hide it (the boss died,
or the player did and the fight reset).

## Rename an item

Permissions: `items`, `text`

```lua
kindle.items.set_name("Ember", "Spark of Kindle")
kindle.items.set_description("Ember", "Warm to the touch. Smells faintly of Lua.")
```

The new name shows everywhere the game shows the item, and works in `items.give("Spark of
Kindle")`. It's reverted when the mod unloads.

## Calm an enemy down

Permissions: `world`

```lua
-- The nearest enemy stops fighting: no target, AI off for 10 seconds.
local e = kindle.world.characters({ kind = "enemy" })[1]
if e then
  kindle.world.clear_aggro(e)
  kindle.world.set_ai(e, false)
  kindle.after(10, function() kindle.world.set_ai(e, true) end)
end
```

## Add a brand-new item

Permissions: `params`, `items`, `text`

```lua
-- A new goods item, a copy of Ember with its own id, name and effect. The id is yours to
-- choose (above every existing goods id); keep it fixed, saves remember it.
local KINDLE_EMBER = 4000000
local row = kindle.params.add_row("EquipParamGoods", KINDLE_EMBER, "Ember")
local item = kindle.items.goods(KINDLE_EMBER)
kindle.items.set_name(item, "Kindle Ember")
kindle.items.set_description(item, "An ember that remembers being a Lua script.")
kindle.items.give(item)
```

The new row starts as a copy of its template (stats, icon, behaviour, text) and is live like
any other: change fields on `row`. New rows last for the session; a save that holds a new
item needs the mod when it's loaded.

## Change weapon stats

Permissions: `params`

```lua
-- Every Uchigatana weighs nothing. Row ids are in the editor's completion and the overlay's
-- Params tab; kindle.items.info(item) returns an item's param id.
local _, row_id = kindle.items.info(kindle.items.weapon("Uchigatana"))
local row = kindle.params.get("EquipParamWeapon", row_id)
if row then
  row.weight = 0
end
```

Param changes are live and reverted when the mod unloads; they're never saved to the game's
files.

## Freeze the enemies around you for a few seconds

Permissions: `world`

```lua
-- Every enemy within 10 m stands still for 3 seconds.
local function freeze_nearby()
  local frozen = kindle.world.characters({ radius = 10, kind = "enemy" })
  for _, c in ipairs(frozen) do kindle.world.set_ai(c, false) end
  kindle.after(3, function()
    -- pcall: an enemy may have died or unloaded in the meantime.
    for _, c in ipairs(frozen) do pcall(kindle.world.set_ai, c, true) end
  end)
end
```

The same shape works for `set_animation_speed(c, 0.5)` (slowed), `set_gravity(c, false)`
(floating) or `set_visible(c, false)` (invisible), each undone with its opposite.

## A buff of your own

Permissions: `params`, `player`

```lua
-- A new SpEffect row (pick an id above every existing one), copied from the game's
-- 10-second +10% damage buff, with stronger numbers.
local MY_BUFF = 160900500
local fx = kindle.params.add_row("SpEffectParam", MY_BUFF, 120500)
fx.effectEndurance = 10
fx.physicsAttackPowerRate = 1.5

kindle.on("bonfire_rested", function()
  kindle.player.apply_effect(MY_BUFF) -- 10 seconds of +50% physical damage after resting
end)
```

New rows stay for the session; edits to them are reverted when the mod unloads. The game's own
SpEffect rows end at 160000030. Rows other mods add count too, and mods load in folder-name order,
so pick an id well apart from theirs.

## Change what a pickup or drop gives

Permissions: `params` (for the second example)

```lua
-- Vordt gives an Ember instead of his soul. Lot 2000 is his reward; lot 4550 is one Ember.
-- Item lot ids are in the overlay's Params tab (ItemLotParam).
kindle.on("item_lot", function(e)
  if e.lot == 2000 then
    e.lot = 4550
  end
end)

-- No Firebombs from anything: look at what the lot gives and cancel it.
local FIREBOMB = 292
kindle.on("item_lot", function(e)
  local lot = kindle.params.get("ItemLotParam", e.lot)
  if lot and lot.ItemLotId1 == FIREBOMB then
    return false
  end
end)
```

The game decides what a pickup holds before you reach it: an area's pickups are rolled all at
once as it loads, an enemy's drop as it dies, a reward as it's given. So:

- Register `item_lot` handlers when the mod loads (at the top of `main.lua`, as here). A handler
  added later, or a change of mind mid-session, reaches the current area's pickups only the next
  time the area loads (a warp, a death, back to the menu). Drops and rewards change at once.
- Decide from `e.lot` (and params). Pickups are rolled during loading screens, before the
  character exists, so don't rely on `kindle.player` here.
- A cancelled pickup still sparkles but gives nothing. The game marks the rolled lot as picked
  up, so a swapped pickup takes its new lot's pickup flag.

## Put another enemy in an area

Permissions: `maps`

```lua
-- Every enemy of one model in a map becomes an enemy of another map's type. The models here
-- are examples: list a map's parts with kindle.maps.parts(map) in the console to find yours.
local HERE, MODEL = "m30_00_00_00", "c1100"
local FROM_MAP, FROM_MODEL = "m31_00_00_00", "c1240"

local function first_enemy(map, model)
  for number, part in ipairs(kindle.maps.parts(map) or {}) do
    if part.kind == "enemy" and part.model == model then return number end
  end
end

kindle.on("map_loading", function(map, file)
  if map ~= HERE or file ~= "map" then return end
  local donor = first_enemy(FROM_MAP, FROM_MODEL)
  local changes = {}
  for number, part in ipairs(kindle.maps.parts(HERE)) do
    if donor and part.kind == "enemy" and part.model == MODEL then
      changes[#changes + 1] = { replace_enemy = number, from = { map = FROM_MAP, part = donor } }
    end
  end
  kindle.maps.edit(HERE, changes)
end)
```

The new enemy stands where the old one stood, on the same ground, and walks its route, but it's
its own type: its stats, AI, looks and drops. Its spot's event scripts (waking, ambushes) no
longer act on it. Changes made in `map_loading` are in place for the load that's starting; made
elsewhere, from the next time the area loads.

## Options on the character creation screen

Permissions: `ui`, `player`

```lua
-- A "Hard mode" row under the game's own; its page has one switch.
kindle.ui.character_rows {
  { label = "Hard mode", help = "Set up hard mode for this character", page = {
    { title = "Hard mode", icon = "skulls", switches = {
      { key = "on", label = "Hard mode", help = "Enemies hit harder.", default = false },
    } },
  } },
}

kindle.on("player_loaded", function()
  -- A character that has hardly played was just made: take what its rows held.
  if kindle.player.play_time() < 60 then
    local choices = kindle.ui.character_choices()
    kindle.ui.reset_character_choices()
    kindle.log.info("hard mode: %s", tostring(choices.on))
    -- Keep it with the character (in a flag or kindle.store) and apply it.
  end
end)
```

## Stop the player sprinting

Permissions: `hks`

```lua
-- The player's behaviour script sets these move speed variables every frame; refusing
-- its writes keeps the player from sprinting (other movement is unaffected).
local SPEED = { MoveSpeedLevelReal = true, MoveSpeedIndex = true }
kindle.hks.on_act(kindle.hks.act.SET_VARIABLE, function(e)
  if e.character and e.character.is_player and SPEED[e.args[1]] then
    return false
  end
end)
```

## Do something when the player rolls

Permissions: `player`

```lua
kindle.on("animation", function(e)
  if not e.character.is_player or not e.name then return end
  if e.name:find("^W_Rolling") then
    kindle.after(0, function()
      kindle.player.set_stamina(kindle.player.max_stamina())
    end)
  end
end)
```

`animation` fires very often, so the handler returns straight away for anything that isn't the
player rolling, and the actual work runs on the next frame.

## Remember things between sessions

Permissions: none (the store is always available)

```lua
-- Count this character's deaths across play sessions.
kindle.on("player_died", function()
  local deaths = kindle.store.get("deaths", 0) + 1
  kindle.store.set("deaths", deaths)
  kindle.log.info("Death number %d", deaths)
end)

-- A setting for the mod itself, the same for every character.
local volume = kindle.store.get_global("volume", 1.0)
```

`store.get`/`store.set` keep values for the loaded character (each save slot has its own);
`get_global`/`set_global` keep the mod's own settings. Values can be booleans, numbers,
strings, or tables that are lists or have string keys.

## Share functions between mods

Permissions: none

In `fate-core/main.lua`:

```lua
local fate = "none"
kindle.export {
  current = function() return fate end,
  set = function(new) fate = new end,
}
```

In another mod, with `depends = ["fate-core"]` in its `mod.toml`:

```lua
local core = kindle.mods.get("fate-core")
if core then
  core.set("lucky")
  kindle.log.info("fate is now %s", core.current())
end
```

Calls run in the exporting mod and pass plain data (no functions). For one-way news to any
mod that's listening, use `kindle.send(name, data)` and `kindle.on_message(name, fn)`.

## Clean up when the mod unloads

Kindle reverts a mod's param and text changes for it. Anything else the mod changed in the
running game stays, so undo it in an `unload` handler:

```lua
kindle.on("unload", function()
  kindle.player.set_animation_speed(1) -- don't leave the player sped up
end)
```
