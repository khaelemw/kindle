-- Bonfire Fate: every few minutes fate pairs a random cause with a random effect. Until it
-- changes, every time the cause happens, the effect fires. Some fates are blessings, some
-- (if "banes" is on in the mod's settings) are curses.

local player, ui, world, settings = kindle.player, kindle.ui, kindle.world, kindle.settings

local COOLDOWN = 1.0 -- seconds between effects, so multi-hit attacks don't spam them

local WEAPON_POOL = {
  "Dagger", "Shortsword", "Longsword", "Broadsword", "Uchigatana", "Estoc",
  "Rapier", "Claymore", "Zweihander", "Greatsword", "Battle Axe", "Great Axe",
  "Mace", "Morning Star", "Spear", "Partizan", "Halberd", "Washing Pole",
  "Whip", "Caestus", "Sellsword Twinblades", "Dark Sword", "Sunlight Straight Sword",
  "Lothric Knight Sword", "Black Knight Sword", "Pickaxe", "Large Club",
}

local CONSUMABLE_POOL = {
  "Firebomb", "Black Firebomb", "Throwing Knife", "Green Blossom", "Ember",
  "Gold Pine Resin", "Charcoal Pine Resin", "Prism Stone", "Duel Charm",
  "Alluring Skull", "Homeward Bone", "Undead Hunter Charm",
}

-- Fate's Fury: a buff of the mod's own, a new SpEffect row (FURY, an id this mod picked
-- above every existing one) copied from the game's 10-second damage buff, with stronger numbers.
local GAME_DAMAGE_BUFF = 120500 -- SpEffect: +10% physical damage for 10 seconds
local FURY = 160900000
local fury_ready = pcall(function()
  local fx = kindle.params.add_row("SpEffectParam", FURY, GAME_DAMAGE_BUFF)
  fx.effectEndurance = 10
  fx.physicsAttackPowerRate = 1.3
  fx.magicAttackPowerRate = 1.3
  fx.fireAttackPowerRate = 1.3
  fx.thunderAttackPowerRate = 1.3
  fx.darkAttackPowerRate = 1.3
end)
if not fury_ready then kindle.log.warn("Fate's Fury unavailable (couldn't add its SpEffect row)") end

local function give_random(kind, pool)
  local name = pool[math.random(#pool)]
  local ok, err = pcall(function() kindle.items.give(kindle.items[kind](name), 1) end)
  if ok then
    ui.toast("Fate: received " .. name)
  else
    kindle.log.warn("random %s %q failed: %s", kind, name, tostring(err))
  end
end

local function nearby_enemies(radius)
  local out = {}
  for _, c in ipairs(world.characters({ radius = radius, kind = "enemy" })) do out[#out + 1] = c end
  return out
end

local function slow_for(speed, seconds)
  player.set_animation_speed(speed)
  kindle.after(seconds, function() player.set_animation_speed(1) end)
end

-- Do something to the enemies nearby for a few seconds, then undo it (for those still loaded).
local function enemies_for(radius, seconds, on, off)
  local hit = nearby_enemies(radius)
  for _, c in ipairs(hit) do pcall(on, c) end
  kindle.after(seconds, function()
    for _, c in ipairs(hit) do pcall(off, c) end
  end)
  return #hit
end

-- bane = true: only rolled when the "banes" setting is on.
local effects = {
  {
    text = "restore some HP",
    run = function()
      local max = player.max_hp()
      player.set_hp(math.min(max, player.hp() + math.floor(max * 0.15)))
    end,
  },
  { text = "gain 50 souls", run = function() player.add_souls(50) end },
  { text = "a burst of speed", run = function() slow_for(1.5, 2) end },
  { text = "refill your stamina", run = function() player.set_stamina(player.max_stamina()) end },
  {
    text = "a moment of invulnerability",
    run = function()
      player.set_invulnerable(true)
      kindle.after(2, function() player.set_invulnerable(false) end)
    end,
  },
  { text = "receive a random weapon", run = function() give_random("weapon", WEAPON_POOL) end },
  { text = "receive a random consumable", run = function() give_random("goods", CONSUMABLE_POOL) end },
  {
    text = "an Estus charge",
    run = function()
      local e = player.estus()
      player.set_estus(math.min(e.hp + 1, e.hp_max), nil)
    end,
  },
  {
    text = "Fate's Fury (+30% damage for 10 seconds)",
    available = function() return fury_ready end,
    run = function() player.apply_effect(FURY) end,
  },
  {
    text = "smite enemies around you",
    run = function()
      for _, c in ipairs(nearby_enemies(8)) do
        world.set_hp(c, math.max(0, c.hp - math.floor(c.max_hp * 0.25)))
      end
    end,
  },
  {
    text = "enemies around you forget you",
    run = function()
      for _, c in ipairs(nearby_enemies(15)) do world.clear_aggro(c) end
    end,
  },
  {
    text = "enemies around you slow down",
    run = function()
      enemies_for(12, 5, function(c) world.set_animation_speed(c, 0.5) end, function(c) world.set_animation_speed(c, 1) end)
    end,
  },
  {
    text = "enemies around you freeze",
    run = function()
      enemies_for(10, 3, function(c) world.set_ai(c, false) end, function(c) world.set_ai(c, true) end)
    end,
  },
  {
    text = "enemies around you float away",
    run = function()
      enemies_for(10, 3, function(c) world.set_gravity(c, false) end, function(c) world.set_gravity(c, true) end)
    end,
  },
  -- Banes.
  { text = "lose 100 souls", bane = true, run = function() player.add_souls(-math.min(100, player.souls())) end },
  { text = "slowed for 3 seconds", bane = true, run = function() slow_for(0.7, 3) end },
  { text = "your stamina drains", bane = true, run = function() player.set_stamina(0) end },
  {
    text = "lose some HP",
    bane = true,
    run = function() player.set_hp(math.max(1, player.hp() - math.floor(player.max_hp() * 0.1))) end,
  },
  {
    text = "enemies around you are healed",
    bane = true,
    run = function()
      for _, c in ipairs(nearby_enemies(12)) do pcall(world.heal, c) end
    end,
  },
  {
    text = "enemies around you speed up",
    bane = true,
    run = function()
      enemies_for(12, 5, function(c) world.set_animation_speed(c, 1.3) end, function(c) world.set_animation_speed(c, 1) end)
    end,
  },
}

local causes = {
  { id = "dodge", text = "When you dodge" },
  { id = "jump", text = "When you jump" },
  { id = "parry", text = "When you parry" },
  { id = "hurt", text = "When you take a hit" },
  { id = "strike", text = "When you land a hit" },
  { id = "kill", text = "When an enemy dies" },
  { id = "named_kill", text = "When a named foe falls" },
  { id = "item", text = "When you use an item" },
  { id = "estus", text = "When you drink Estus" },
  { id = "low_hp", text = "When your HP falls low" },
  { id = "rest", text = "When you rest at a bonfire" },
  { id = "boss", text = "When a boss falls" },
}

local fate -- { cause = ..., effect = ... }
local last_fired = -math.huge

local function pick_effect()
  local pool = {}
  for _, e in ipairs(effects) do
    if (not e.bane or settings.get("banes")) and (not e.available or e.available()) then pool[#pool + 1] = e end
  end
  return pool[math.random(#pool)]
end

local function roll_fate()
  fate = { cause = causes[math.random(#causes)], effect = pick_effect() }
  local text = fate.cause.text .. ": " .. fate.effect.text
  if settings.get("banner") then ui.banner(text, { seconds = 4 }) else ui.toast("Fate: " .. text) end
  kindle.log.info("fate: %s -> %s%s", fate.cause.text, fate.effect.text, fate.effect.bane and " (bane)" or "")
end

local function trigger(cause_id)
  if not fate or fate.cause.id ~= cause_id then return end
  local now = kindle.time()
  if now - last_fired < COOLDOWN then return end
  last_fired = now
  -- Run outside the game's own call (hit/animation processing) to keep it simple.
  kindle.after(0, function()
    fate.effect.run()
    ui.toast((fate.effect.bane and "Fate's curse: " or "Fate: ") .. fate.effect.text)
  end)
end

-- Player actions, from the animations they start. Only fate's current cause is looked at.
local ACTION = {
  dodge = function(name) return name:find("^W_Rolling") ~= nil or name:find("^W_BackStep") ~= nil end,
  jump = function(name) return name:find("^W_Jump") ~= nil end,
  parry = function(name) return name == "W_ParryLeftStart" or name == "W_ParryRightStart" end,
}
kindle.on("animation", function(e)
  local match = fate and ACTION[fate.cause.id]
  if match and e.character.is_player and e.name and match(e.name) then trigger(fate.cause.id) end
end)

kindle.on("damage", function(e)
  if e.target and e.target.is_player then trigger("hurt") end
  if e.attacker and e.attacker.is_player and e.target and not e.target.is_player then trigger("strike") end
end)

kindle.on("character_killed", function(c)
  trigger("kill")
  if c.name then trigger("named_kill") end
end)

kindle.on("item_used", function(e)
  if not (e.player and e.player.is_player) then return end
  if e.name and e.name:find("Estus") then trigger("estus") else trigger("item") end
end)

kindle.on("bonfire_rested", function() trigger("rest") end)
kindle.on("boss_defeated", function() trigger("boss") end)

-- Low HP: fires when HP drops below 30% (again only after it has risen back above).
local low = false
kindle.every(0.25, function()
  if not player.loaded() then return end
  local now_low = player.hp() < player.max_hp() * 0.3
  if now_low and not low then trigger("low_hp") end
  low = now_low
end)

local fate_timer
local function start_timer()
  if fate_timer then kindle.cancel(fate_timer) end
  -- Fate changes only while a character is in the world (not at the title or character creation).
  fate_timer = kindle.every(settings.get("interval"), function()
    if player.loaded() then roll_fate() end
  end)
end

kindle.on("player_loaded", function()
  ui.toast(("Bonfire Fate: your fate changes every %d seconds"):format(settings.get("interval")))
  roll_fate()
  start_timer()
end)

kindle.on("setting_changed", function(name)
  if name == "interval" and player.loaded() then start_timer() end
end)
