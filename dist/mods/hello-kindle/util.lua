-- Loaded by main.lua with require("util").
local M = {}

function M.describe_stats(s)
  return string.format("VIG %d ATT %d END %d VIT %d STR %d DEX %d INT %d FTH %d LCK %d",
    s.vigor, s.attunement, s.endurance, s.vitality, s.strength, s.dexterity,
    s.intelligence, s.faith, s.luck)
end

return M
