-- Hello Kindle: a tour of the API. Edit and save while the game runs to see hot reload.
local log, flags, items, player, params, ui = kindle.log, kindle.flags, kindle.items, kindle.player, kindle.params, kindle.ui

local util = require("util") -- mod-local module: util.lua next to this file

log.info("main.lua ran (Kindle %s)", kindle.version)

-- Params are available from the title screen onwards.
local tables = params.tables()
log.info("%d param tables available, e.g. %s", #tables, tables[1])

-- Text overrides are live and undone automatically when this mod unloads.
items.set_description("Ember", "An ember tended by Kindle. Gain strength of flame, boosting HP until death.")

kindle.on("player_loaded", function()
  ui.toast("Hello from Kindle, " .. player.name())
  local s = player.stats()
  log.info("%s entered the world: HP %d/%d, FP %d, %s",
    player.name(), player.hp(), player.max_hp(), player.fp(), util.describe_stats(s))
  log.info("carrying %d Ember(s)", items.count("Ember"))
end)

kindle.on("player_unloaded", function()
  log.info("character left the world")
end)

-- First boss arena cleared (Iudex Gundyr, or whatever the randomizer put there: replacements
-- inherit the original boss's flags). Check your save with:
--   kindle run "flags.get(flags.boss.iudex_gundyr)"
flags.on_set(flags.boss.iudex_gundyr, function()
  log.info("First boss defeated! Here's an Ember.")
  ui.banner("THE FLAME STIRS", { seconds = 4 })
  items.give("Ember")
end)

-- A heartbeat every five minutes while a character is loaded.
local HEARTBEAT_SECONDS = 5 * 60
kindle.every(HEARTBEAT_SECONDS, function()
  if player.loaded() then
    log.info("still burning: HP %d/%d", player.hp(), player.max_hp())
  end
end)

kindle.on_unload = function()
  log.info("hello-kindle unloading")
end
