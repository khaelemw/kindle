# Mod tours: six mods built step by step

Each tour builds one mod from an empty folder to the finished example in Kindle's `examples` folder, a step at a
time. Each step adds a few lines and says what to try. Where there is more than one way, a **Choices** note
says what the other ways are and why this one was picked. Read the finished mod's README alongside it for the
details.

| Tour | Builds | You learn |
|---|---|---|
| [0. The loop](#tour-0-the-loop) | an empty mod | make, check, load, reload, read the log |
| [1. Hard mode](#tour-1-hard-mode) | [ex-hard-mode](https://github.com/khaelemw/kindle-examples/blob/main/ex-hard-mode/README.md) | settings of every kind, the damage hook, a timer, an hks block, the store |
| [2. Run timer](#tour-2-run-timer) | [ex-run-timer](https://github.com/khaelemw/kindle-examples/blob/main/ex-run-timer/README.md) | the game's clock, events for splits, the two stores, a mod menu |
| [3. Practice tool](#tour-3-practice-tool) | [ex-practice-tool](https://github.com/khaelemw/kindle-examples/blob/main/ex-practice-tool/README.md) | the player's and enemies' state, entity ids, buttons, input, cleanup |
| [4. New weapon](#tour-4-new-weapon) | [ex-new-weapon](https://github.com/khaelemw/kindle-examples/blob/main/ex-new-weapon/README.md) | a new param row, a weapon's fields, item text, done-once |
| [5. Pickup shuffle](#tour-5-pickup-shuffle) | [ex-pickup-shuffle](https://github.com/khaelemw/kindle-examples/blob/main/ex-pickup-shuffle/README.md) | walking a table, item lots, a seeded shuffle, `params.copy` |
| [6. One mod for both games](#tour-6-one-mod-for-both-games) | [ex-kill-streak](https://github.com/khaelemw/kindle-examples/blob/main/ex-kill-streak/README.md) | `games = ["ds3", "er"]`, what's shared, telling the games apart, porting |

The tours are for Dark Souls III, except tour 6. Each finished mod passes `kindle check`, and each README's
**Tested** section says what was checked in the game. For the same goal done other ways,
see [Ways to do it](mod-options.md). For the messages when something goes wrong, see
[When a mod goes wrong](mod-errors.md).

## Tour 0: the loop

Every tour uses this loop. It is quick, and the rest builds on it.

**Make the mod.** `kindle new my-mod --mods <the game's kindle\mods folder>` makes three files:

| File | Holds |
|---|---|
| `mod.toml` | the mod's id, name, version, `api = 1`, `games = ["ds3"]`, and `[permissions] modules`, the parts of the API it may use |
| `main.lua` | the code; this one logs "Hello, <name>!" when a character loads |
| `.luarc.json` | points the Lua language server at Kindle's API stubs (the `stubs` folder next to `kindle.exe`, or next to the mods folder), so the editor autocompletes `kindle.` and flags misspellings. With neither found, its library list is empty: add the path to `dist\stubs` yourself |

Leave out `--mods` and it goes into `mods\` next to `kindle.exe`.

**Check it.** `kindle check my-mod` reads `mod.toml`, parses every Lua file and looks up every `kindle.` name
and event against the API. It prints `my-mod: ok (1 Lua file(s))` or the errors (listed in
[When a mod goes wrong](mod-errors.md)). Run it after every step. It takes a fraction of a second, and it
catches most mistakes before the game sees them.

**Load it.** Start the game with Kindle. Mods in `kindle\mods` load at the start, in folder-name order. With the
game running, a new folder loads as soon as it appears, and saving any of the mod's files reloads it:
`unload` handlers run, everything the mod changed is undone, and `main.lua` runs again.

**Watch it.** `kindle.log.info("...")` writes to `logs\kindle.log` in the kindle folder. With **Developer
overlay** on (Mods, the Kindle tab), F1 in the game opens the developer overlay, whose Log tab shows the same
lines live. `kindle console` gives you a Lua prompt in the running game (turn it on first: `console = true`
under `[runtime]` in `kindle\kindle.toml`): try a call there before writing it into the mod.

**Choices.** A mod can live anywhere while you write it. Point `--mods` at the game's folder so a save reloads
it at once, or write it elsewhere and copy it in. `kindle pack my-mod` checks the mod and zips it to share.

## Tour 1: Hard mode

A challenge run: hits hurt more, Estus is capped, sprinting is off, and deaths are counted on screen. Each part
is a setting, so the player picks their own challenge. The finished mod:
[ex-hard-mode](https://github.com/khaelemw/kindle-examples/blob/main/ex-hard-mode/README.md).

### Step 1: the first setting and the damage hook

`mod.toml`:

```toml
[mod]
id = "ex-hard-mode"
name = "Example: Hard Mode"
version = "1.0.0"
api = 1
games = ["ds3"]

[permissions]
modules = ["player"]

[settings.damage_taken]
type = "float"
label = "Damage taken"
description = "Multiplies every hit on you."
default = 1.5
min = 1.0
max = 5.0
```

`main.lua`:

```lua
kindle.on("damage", function(hit)
  if hit.target and hit.target.is_player then
    hit.attack_power = hit.attack_power * kindle.settings.get("damage_taken")
  end
end)
```

The `damage` hook runs as every hit lands, on anyone. `hit.target` is the character hit, and `is_player` is
true for your own. Assigning `hit.attack_power` changes the hit before the target's defence is applied. The
setting is read on every hit, so a change in the Mods screen applies to the next one.

**Try it.** Check, load, take a hit. Open Esc, then Mods, and the setting is a slider from 1.0 to 5.0.

**Choices.** Another way to make the game harder is params: raise every NpcParam row's attack, or lower the
player's defence. That applies everywhere at once, but each change is a table walk. The hook is one function,
and it can decide per hit: only bosses (`hit.attacker.name`), only one attack (`hit.attack_id`).

### Step 2: the Estus cap, with a timer

```toml
[settings.estus_cap]
type = "int"
label = "Estus at most"
description = "Estus Flask charges you can hold; extra charges are taken away. 15 leaves them alone."
default = 3
min = 0
max = 15
```

```lua
kindle.every(1, function()
  if not kindle.player.loaded() then
    return
  end
  local cap = math.tointeger(kindle.settings.get("estus_cap")) or 0
  local estus = kindle.player.estus()
  if estus.hp > cap then
    kindle.player.set_estus(cap, nil)
  end
end)
```

`player.estus()` is `{hp, hp_max, fp, fp_max}`: the charges of the Estus and Ashen Estus Flasks and their
allotments. `set_estus(hp, nil)` sets the first and leaves the second alone.

**Choices.** There's no event for "the flask was refilled". Charges come back at a bonfire, from an Estus
Shard, or from a script. So the mod looks once a second and corrects. Three things looked possible:

- `item_used` returning false: it keeps the item, but the drink still heals. No good for blocking Estus.
- `bonfire_rested`: catches the refill at a bonfire, but not the others.
- `kindle.every(1, ...)`: catches all of them within a second, for one call a second. Chosen.

`kindle.player.loaded()` guards the title screen and loading screens, where there is no player to read.

### Step 3: no sprinting, with an hks block that comes and goes

```toml
[settings.no_sprint]
type = "bool"
label = "No sprinting"
description = "The sprint button does nothing: you run at most."
default = true
```

Add `"hks"` to the modules, then:

```lua
local SPEED_VARIABLES = { MoveSpeedLevelReal = true, MoveSpeedIndex = true }
local sprint_block = nil

local function apply_sprint_setting()
  local wanted = kindle.settings.get("no_sprint")
  if wanted and not sprint_block then
    sprint_block = kindle.hks.on_act(kindle.hks.act.SET_VARIABLE, function(call)
      if call.character and call.character.is_player and SPEED_VARIABLES[call.args[1]] then
        return false
      end
    end)
  elseif not wanted and sprint_block then
    kindle.cancel(sprint_block)
    sprint_block = nil
  end
end

kindle.on("setting_changed", function(name)
  if name == "no_sprint" then
    apply_sprint_setting()
  end
end)

apply_sprint_setting()
```

The character's behaviour script (HavokScript, `c0000.hks` for the player) picks the move speed with
`act(SET_VARIABLE, name, value)`. `hks.on_act` sees each of those calls, and returning false refuses it. With
`MoveSpeedLevelReal` and `MoveSpeedIndex` refused, the speed stays at a run.

**Choices.**

- The handler could stay registered and check the setting inside. Instead it's added and removed with the
  setting (`kindle.cancel(handle)`), because these calls are frequent and run on the game's script threads.
  With the setting off, the mod adds nothing to them.
- `kindle.debug.trace_hks(true)` logs the player's script calls with their names. That is how to find the
  call behind any other move you want to block: turn it on, do the move, read the log.
- `input.block_actions(true)` would block every action button, not just sprint.

### Step 4: deaths on screen, kept with the character

```toml
[settings.deaths_shown]
type = "choice"
label = "Deaths shown"
description = "Where the death count shows: a line that stays on screen, a toast after each death, or nowhere."
choices = ["status line", "toast", "off"]
default = "toast"
```

Add `"ui"` to the modules, then:

```lua
local function show_deaths()
  local deaths = kindle.store.get("deaths", 0)
  local where = kindle.settings.get("deaths_shown")
  if where == "status line" then
    kindle.ui.status(string.format("Hard mode: %d deaths", deaths))
  else
    kindle.ui.status(nil)
    if where == "toast" then
      kindle.ui.toast(string.format("Hard mode: %d deaths", deaths), { color = "red" })
    end
  end
end

kindle.on("player_died", function()
  kindle.store.set("deaths", kindle.store.get("deaths", 0) + 1)
  show_deaths()
end)
kindle.on("player_loaded", show_deaths)
kindle.on("player_unloaded", function() kindle.ui.status(nil) end)
```

Add `elseif name == "deaths_shown" and kindle.player.loaded() then show_deaths()` to the `setting_changed`
handler.

**Choices.**

- **Where the count lives.** `kindle.store` is per character, saved with the save slot, so each character has
  its own count. `kindle.store.get_global` would be one count for all characters. `kindle.player.deaths()` is
  the game's own death count, but it holds every death before the mod was installed too.
- **How it shows.** A status line is always visible. A toast appears once per death and goes. Both are
  offered, and the choice setting picks one.

**Try it.** Die. The toast shows the count. Quit, load again, die again: the count went on.

## Tour 2: Run timer

A speedrun clock, splits at bosses and first bonfire rests, best times, and a menu of its own in the Mods screen. The finished mod:
[ex-run-timer](https://github.com/khaelemw/kindle-examples/blob/main/ex-run-timer/README.md).

### Step 1: a clock that survives quitting

```lua
local run = nil

local function clock(seconds)
  seconds = math.floor(seconds)
  local hours, minutes = seconds // 3600, seconds % 3600 // 60
  if hours > 0 then
    return string.format("%d:%02d:%02d", hours, minutes, seconds % 60)
  end
  return string.format("%d:%02d", minutes, seconds % 60)
end

kindle.on("player_loaded", function()
  run = kindle.store.get("run")
  if not run then
    run = { start = kindle.player.play_time(), splits = {} }
    kindle.store.set("run", run)
  end
end)

kindle.on("player_unloaded", function()
  run = nil
  kindle.ui.status(nil)
end)

kindle.every(0.5, function()
  if run then
    kindle.ui.status(clock(kindle.player.play_time() - run.start))
  end
end)
```

**Choices.** Which clock?

- `kindle.player.play_time()` is the save's play time. It survives quitting, stops in the title screen and is
  the same number the game shows. Chosen.
- `kindle.time()` is seconds since Kindle started. It keeps counting in menus and starts over with the game.
- `kindle.clock()` is the computer's clock (seconds since 1970) and `kindle.date()` its date. Right for a daily
  seed or a date in a log; wrong for a run's time, which shouldn't count the hours the game was closed. (A mod's
  Lua has no `os` library; these two are its wall clock.)

`every(0.5)` rather than every frame: the text changes once a second, so twice a second is enough to keep it
from looking late.

### Step 2: splits from game events

```lua
local BOSS_SPLITS = { heir_of_fire = "Heir of fire destroyed", lord_of_cinder = "Lord of cinder fallen" }

local function split(name)
  if not run then return end
  local time = kindle.player.play_time() - run.start
  run.splits[#run.splits + 1] = { name = name, time = time }
  kindle.store.set("run", run)
  kindle.ui.toast(string.format("%s  %s", name, clock(time)))
end

kindle.on("boss_defeated", function(kind)
  split(BOSS_SPLITS[kind] or kind)
end)

kindle.on("bonfire_rested", function(bonfire)
  if bonfire and bonfire.name then
    split("Bonfire: " .. bonfire.name)
  end
end)
```

`boss_defeated` comes from the game's own banner, so it fires for every boss the game announces.
`bonfire_rested` passes the `kindle.bonfire` (id, name, lit, flag ...).

**Choices.**

- **Bosses by banner or by flag.** `boss_defeated` gives no name. For named splits, `flags.on_set(kindle.flags.boss.iudex_gundyr, fn)`
  fires on one boss's defeat flag (needs `flags`). Banners suit "every boss"; flags suit a route.
- **Only the first rest.** The finished mod skips a bonfire already in the run's splits, and has a setting
  to turn bonfire splits off.

**Try it.** Rest at two bonfires, then rest at the first again. Two splits, not three, once the first-rest
check is in.

### Step 3: best times, for every character

```lua
local best = kindle.store.get_global("best", {})
local previous = best[name]
if previous then
  local difference = time - previous
  kindle.ui.toast(string.format("%s  %s (%s%s)", name, clock(time), difference <= 0 and "-" or "+",
    clock(math.abs(difference))), { color = difference <= 0 and "green" or "red" })
end
if not previous or time < previous then
  best[name] = time
  kindle.store.set_global("best", best)
end
```

This goes into `split`. The run is the character's (`store.get`); the best times are yours, whichever
character set them (`store.get_global`). Store tables need string keys or a list, and split names are strings.

### Step 4: the menu

```lua
local function show_tab()
  local rows = {}
  for index = math.max(1, #run.splits - 11), #run.splits do
    local entry = run.splits[index]
    rows[#rows + 1] = { label = string.format("%s  %s", clock(entry.time), entry.name), info = true }
  end
  rows[#rows + 1] = { label = "Restart the clock", on_press = function()
    run = { start = kindle.player.play_time(), splits = {} }
    kindle.store.set("run", run)
    show_tab()
  end }
  kindle.ui.mod_tab("Run timer", rows)
end
```

Call `show_tab()` from `player_loaded` and at the end of `split`. A tab has 1 to 16 rows. Calling `mod_tab`
again replaces it; if the player has the menu open, it opens again with the new rows.

**Choices.** A tab, a bonfire menu option (`ui.add_menu_option`) or a setting? The tab is reachable anywhere
with Esc and holds a list. A menu option is only at bonfires. A setting holds a value, not a list. For a reset
button and a list of splits, the tab fits.

## Tour 3: Practice tool

Save a spot (you and the enemies near you), go back to it whenever you like. The finished mod:
[ex-practice-tool](https://github.com/khaelemw/kindle-examples/blob/main/ex-practice-tool/README.md).

### Step 1: save and load where you stand

`modules = ["player", "ui"]`:

```lua
local function capture()
  local x, y, z = kindle.player.position()
  local estus = kindle.player.estus()
  return { x = x, y = y, z = z, facing = kindle.player.facing(), hp = kindle.player.hp(), fp = kindle.player.fp(),
    estus_hp = estus.hp, estus_fp = estus.fp }
end

local function restore(spot)
  kindle.player.teleport(spot.x, spot.y, spot.z)
  kindle.player.set_facing(spot.facing)
  kindle.player.set_hp(spot.hp)
  kindle.player.set_fp(spot.fp)
  kindle.player.set_estus(spot.estus_hp, spot.estus_fp)
end
```

**Choices.** `player.teleport(x, y, z)` moves you within the loaded map, at once. `player.warp(bonfire)` goes
through a loading screen to a bonfire, the game's own way. For practising a fight, teleport. For "back to the
start of the area", a warp.

### Step 2: the enemies, by entity id

Add `"world"`:

```lua
local enemies = {}
for _, enemy in ipairs(kindle.world.characters({ radius = 40, kind = "enemy" })) do
  if enemy.entity_id then
    enemies[#enemies + 1] = { entity_id = enemy.entity_id, x = enemy.x, y = enemy.y, z = enemy.z, hp = enemy.hp,
      facing = kindle.world.facing(enemy) }
  end
end
```

and on load, for each saved enemy:

```lua
local enemy = kindle.world.character(saved.entity_id)
if enemy then
  kindle.world.teleport(enemy, saved.x, saved.y, saved.z)
  kindle.world.set_facing(enemy, saved.facing)
  kindle.world.set_hp(enemy, saved.hp)
  kindle.world.clear_aggro(enemy)
end
```

**Choices.** How to find the same enemy later:

- `enemy.handle` is stable only while the enemy is loaded, so it breaks across an area reload or quitting.
- `enemy.entity_id` is the map's own id, the same at every load. Chosen. Event scripts use the same ids.
- Position only: two enemies close together can't be told apart.

Enemies a mod spawned have no entity id, and the spot leaves them out.

### Step 3: buttons whose labels follow state

```lua
local training = { no_damage = false }

local function show_tab()
  kindle.ui.mod_tab("Practice", {
    { label = "Save spot 1", on_press = function() kindle.store.set("spot", capture()) end },
    { label = "Go to spot 1", on_press = function()
      local spot = kindle.store.get("spot")
      if spot then restore(spot) end
    end },
    { label = string.format("No damage: %s", training.no_damage and "on" or "off"), on_press = function()
      training.no_damage = not training.no_damage
      kindle.player.set_no_damage(training.no_damage)
      show_tab()
    end },
  })
end
```

A switch is a button that flips a value, calls the game, and rebuilds the tab so its label shows the new state.
The finished mod has three spots, a refill and an endless stamina switch.

### Step 4: a shortcut, and cleanup

Add `"input"`:

```lua
local held_for = 0
kindle.on("frame", function(seconds)
  if not kindle.player.loaded() then return end
  if kindle.input.held("left") and kindle.input.held("right") then
    held_for = held_for + seconds
    if held_for >= 1 then
      held_for = -math.huge -- once per hold
      local spot = kindle.store.get("spot")
      if spot then restore(spot) end
    end
  else
    held_for = 0
  end
end)

kindle.on("unload", function()
  if kindle.player.loaded() then
    kindle.player.set_no_damage(false)
  end
end)
```

`input.held` knows `"left"`, `"right"` and `"action"`, from the pad or the keyboard. Both directions at once
is a combination nothing in the game uses. Setting the counter to minus infinity makes it fire once per hold,
then resets when you let go.

**Choices.** Why clean up on `unload`: Kindle undoes params, text and hooks itself, but `set_no_damage` is a
state of the character. A mod that turns something on that Kindle doesn't track turns it off as it goes.

## Tour 4: New weapon

A longsword of your own, made from the Long Sword. The finished mod:
[ex-new-weapon](https://github.com/khaelemw/kindle-examples/blob/main/ex-new-weapon/README.md).

### Step 1: the row

`modules = ["params"]`:

```lua
local SWORD_ID = 95000000
local row = kindle.params.add_row("EquipParamWeapon", SWORD_ID, "Long Sword")
kindle.log.info("weight %.1f, attack %d", row.weight, row.atkBasePhysics)
```

`add_row(table, id, copy_from)` adds a row that starts as a copy. `copy_from` is a row id or, in the item
tables, an item's name. The new id must be above every row in the table: the game's weapons end at 90060000.

**Choices.** Which id? Weapon ids carry the upgrade level in the last digits (+1 is id + 1) and the infusion in
the hundreds (Heavy is + 100). A round number far above the game's leaves room for both, and for other mods'
rows (mods load in folder-name order, and each new id must be above all the others).

### Step 2: the numbers

```lua
local CHANGES = { weight = 2.0, atkBasePhysics = 128, correctStrength = 30.0, correctAgility = 60.0,
  properStrength = 8, properAgility = 14, durability = 120, sellValue = 500 }
for field, value in pairs(CHANGES) do
  row[field] = value
end
```

Every field of a row is there by its paramdef name. To find them, call
`kindle.params.fields("EquipParamWeapon")` in the console, or read `paramdefs/darksouls3/EquipParamWeapon.xml`
in Kindle's repository.
A value outside the paramdef's limits stops the mod with a message naming the range.

**Choices.** A new row or a changed one? `params.get("EquipParamWeapon", 2010000).weight = 2.0` changes every
Long Sword, including the ones players already have. A new row is a new weapon next to the old.

### Step 3: its name and text

Add `"items"` and `"text"`:

```lua
local sword = kindle.items.weapon(SWORD_ID)
kindle.items.set_name(sword, "Kindled Longsword")
kindle.items.set_info(sword, "A longsword reforged in the bonfire's flame")
kindle.items.set_description(sword, "A longsword, lighter than most, its edge reforged\nin a bonfire's flame.")
```

A new item row starts with its template's text. Without these three it would be a second "Long Sword".

### Step 4: give it once

```lua
kindle.on("player_loaded", function()
  if kindle.store.get("given") then
    return
  end
  kindle.store.set("given", true)
  kindle.items.give(sword)
end)
```

**Choices.** How the player gets it:

- `items.give` on load: simple, once per character. Chosen.
- A shop: a ShopLineupParam row (`equipType = 0` for a weapon) and `ui.shop` from a bonfire option, as
  [ex-new-item-shop](https://github.com/khaelemw/kindle-examples/blob/main/ex-new-item-shop/README.md) does for goods.
- A pickup in the world: an ItemLotParam row's slot pointing at it (category 0 for weapons).
- A reward: `events.award_item_lot` with a lot holding it.

**Keep in mind.** Rows added with `add_row` stay for the session even after the mod unloads, because a
weapon someone holds must keep its row. A save holding the sword needs the mod loaded.

## Tour 5: Pickup shuffle

Items lying in the world trade places, from a seed. The finished mod:
[ex-pickup-shuffle](https://github.com/khaelemw/kindle-examples/blob/main/ex-pickup-shuffle/README.md).

### Step 1: find the pickups

`modules = ["params"]`:

```lua
local function int(value) return math.tointeger(value) or 0 end

local lots = {}
for _, id in ipairs(kindle.params.rows("ItemLotParam")) do
  local row = kindle.params.get("ItemLotParam", id)
  if int(row.getItemFlagId) > 0 and int(row.LotItemCategory01) == 0x40000000 then
    lots[#lots + 1] = id
  end
end
kindle.log.info("%d one-time goods lots", #lots)
```

An item lot is an ItemLotParam row: up to eight slots (`ItemLotId1..8` the item, `LotItemCategory01..08` its
kind, `LotItemNum1..8` how many, `LotItemBasePoint01..08` its weight in the lottery) and `getItemFlagId`, the
flag set when it's picked up. A flag above 0 means "given once": a pickup, a one-time drop or a reward.
`0x40000000` in the category means goods.

**Choices.** Which lots? The finished mod also requires:

- slot 1 only, so a lot is one item;
- the goods row's `goodsType` in the chosen kinds (0 consumables, 2 materials, 5 spells), never 1 (key items),
  so the game stays finishable;
- not a flask or multiplayer item (goods below 240), and not a boss soul (700 to 799);
- no row at id - 1 or id + 1: the game gives a lot's following rows with it, and a chain shouldn't be split.

Each rule is a line of `if`. Run the mod after each and watch the count in the log: 552 lots for consumables.

### Step 2: a seeded shuffle

```lua
local function generator(seed)
  local state = seed
  return function(limit)
    state = state ~ (state << 13)
    state = state ~ (state >> 7)
    state = state ~ (state << 17)
    return (state >> 1) % limit + 1
  end
end

local function shuffled(list, seed)
  local next_number = generator(seed)
  local order = table.move(list, 1, #list, 1, {})
  for i = #order, 2, -1 do
    local j = next_number(i)
    order[i], order[j] = order[j], order[i]
  end
  return order
end
```

**Choices.** `math.random` with `math.randomseed(seed)` would work too. The mod's own xorshift is a few lines,
gives the same numbers for a seed on every PC and every Lua build, and isn't moved by any other use of
`math.random` in the mod. For a shared seed ("play seed 42"), that matters. The shuffle is Fisher-Yates: each
order equally likely.

### Step 3: move the contents, keep the flags

Add a `seed` int setting, then:

```lua
local sources = shuffled(lots, math.tointeger(kindle.settings.get("seed")))
local moves = {}
for i, lot in ipairs(lots) do
  moves[i] = { sources[i], lot }
end
kindle.params.copy("ItemLotParam", moves, { "getItemFlagId" })
```

`params.copy(table, pairs, keep)` copies each `{from, to}`, every `to` getting what its `from` held before any
of them changed, except the fields in `keep`. Keeping `getItemFlagId` keeps "picked up" with the place. A place
you've already looted stays looted, and nothing can be picked up twice.

**Choices.** Three ways to move pickups:

- **Copy rows** (`params.copy`), as here: one call, the lottery of each lot moves whole, undone on unload.
- **Write slots** (`row.ItemLotId1 = ...`): for putting items of your choosing in places. Kindle's own
  randomizer writes single-item lots this way when an item moves into a lot from a shop.
- **The `item_lot` hook** (`e.lot = other`): decided per roll. The rolled lot sets its own flag, so
  for one-time pickups it needs a new lot per place carrying the original's flag. That suits drops that
  come back better than pickups.

### Step 4: when it applies, and a new seed

```lua
local function apply()
  kindle.params.reset("ItemLotParam")
  -- find the lots, shuffle, copy (steps 1 to 3)
end

kindle.on("setting_changed", function(name)
  if name == "seed" or name == "kinds" then apply() end
end)

apply()
```

The game rolls an area's pickups as the area loads. The mod's first `apply()` runs at its start, before any
area loads, so the whole game is shuffled. A seed changed later applies as each area next loads.
`params.reset("ItemLotParam")` puts back what this mod wrote, so a new seed shuffles the game's world and not
the last shuffle.

**Try it.** Turn on the spoiler setting and read the list in the log. Pick up the first item of the game.
Change the seed, rest (the area reloads), and the next pickups are different.

## Tour 6: One mod for both games

A kill streak that pays souls in Dark Souls III and runes in Elden Ring, from one folder. The finished mod:
[ex-kill-streak](https://github.com/khaelemw/kindle-examples/blob/main/ex-kill-streak/README.md).

Kindle for Elden Ring is in development and not part of this release; `kindle check` already checks a mod
against Elden Ring's names, so a mod written this way is ready for it.

### Step 1: games = ["ds3", "er"]

```toml
[mod]
id = "ex-kill-streak"
api = 1
games = ["ds3", "er"]

[permissions]
modules = ["player", "ui"]
```

Kindle loads a mod only in the games its `games` names. The other game skips it with a line in the log. A mod
without `games` is taken as Dark Souls III's.

### Step 2: write with what both have

```lua
local streak = 0

kindle.on("character_killed", function(character)
  if character.kind ~= "enemy" or not kindle.player.loaded() then return end
  streak = streak + 1
  kindle.ui.status(string.format("Streak: %d", streak))
  if streak % 10 == 0 then
    kindle.player.add_souls(streak * 50)
    kindle.ui.toast(string.format("%d in a row", streak), { color = "gold" })
  end
end)

kindle.on("player_died", function()
  streak = 0
  kindle.ui.status(nil)
end)
```

The two APIs share the core (`kindle.on`, `after`, `every`, `settings`, `store`, `log`, `keep`, `export`,
`send`), most game events (`frame`, `player_loaded`, `player_unloaded`, `player_died`, `character_killed`,
`character_spawned`, `item_gained`, `item_lost`, `effect_applied`, `effect_expired`, `level_up`,
`setting_changed`, `unload`), `ui.toast`, `status`, `prompt`, `banner`, `game_banner`, and much of `player`,
`items`, `params`, `flags` and `text`. Elden Ring keeps `souls()` and `add_souls(n)` as names for its runes, for
mods like this one.

### Step 3: tell the games apart

```lua
local ELDEN_RING = kindle.player["runes"] ~= nil
local CURRENCY = ELDEN_RING and "runes" or "souls"
```

Only Elden Ring's player has `runes`. The mod tests for the thing it needs, not for a game id (the `kindle`
table has none).

**Why the string index.** `kindle check` checks a mod for both games against Dark Souls III's names, and
`kindle.player.runes` isn't one of them: `kindle.player.runes doesn't exist`. Indexed by string, the checker
leaves it alone. Use this only for a test like this one, where the name is meant to be missing in one game.

### Step 4: what differs, and checking both

| | Dark Souls III | Elden Ring |
|---|---|---|
| a character's map id | `entity_id` | `entity` |
| `character.kind` | player, enemy, npc, helper, other | also `ally` (spirit summons) |
| hooks (`damage`, `animation`, `effect`, `item_used`, `item_lot`) | yes | no |
| `bonfire_rested`, `boss_defeated`, `shop_buy`, `region_changed`, `map_loading` | yes | no; `map_changed(new, old)` instead of `region_changed` |
| `ui.mod_tab`, `add_menu_option`, `menu`, `shop`, `open` | yes | no |
| `ui.message` | `message(text)` | `message(text, kind)` |
| travel | `player.warp(bonfire)`, `world.bonfires()` | `player.warp(grace, on_done)`, `player.graces()` |
| currency | `souls`, `add_souls` | `runes`, `add_runes` (and the DS3 names) |

```lua
local function entity_of(character)
  return character.entity_id or character["entity"]
end
```

To check the mod against both: `kindle check ex-kill-streak` checks it against Dark Souls III's names. Copy it,
set `games = ["er"]` in the copy, and `kindle check` the copy against Elden Ring's. Both must pass.

**Game-only parts.** Put them behind the test, reached by string, so both checks pass and only the right game
runs them:

```lua
if not ELDEN_RING then
  local DAMAGE = "damage"
  kindle.on(DAMAGE, function(hit) end)
  kindle.ui["mod_tab"]("Streaks", { { label = "Best: 0", info = true } })
end
```

### Porting a Dark Souls III mod to Elden Ring

1. Set `games = ["er"]` (or both) and run `kindle check`. Every name Elden Ring lacks is an error with its
   line: that's the list of work.
2. Replace what has an equivalent: `region_changed` by `map_changed`, `bonfires` by `graces`, `souls` by
   `runes`, `entity_id` by `entity`.
3. For what has none (hooks, mod menus, bonfire menus), decide: drop the feature, or keep it for Dark
   Souls III behind the test.
4. Param and text changes carry the same way, but Elden Ring's tables, rows and fields are its own: look them
   up again for Elden Ring. A Dark Souls III row id means nothing there.
5. Check both ways, then test in each game.
