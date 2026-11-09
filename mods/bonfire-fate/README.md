# Bonfire Fate

A [Kindle](#kindle) mod for Dark Souls III (version 1.15.2).

Cause and effect. Every 2 minutes (you can change this) fate pairs a random **cause** with a random
**effect**, and a banner announces it: *"When you take a hit: a burst of speed"*. Until the next
fate, whenever the cause happens, the effect fires (at most once a second). Some fates are curses;
you can turn those off.

| Causes | Blessings | Curses |
|---|---|---|
| When you dodge (roll or backstep) | Restore 15% of your HP | Lose 100 souls |
| When you jump | Gain 50 souls | Slowed for 3 seconds |
| When you parry | A burst of speed (1.5x for 2 seconds) | Your stamina drains |
| When you take a hit | Refill your stamina | Lose 10% of your HP |
| When you land a hit | 2 seconds of invulnerability | Enemies within 12 m are healed |
| When an enemy dies | Receive a random weapon | Enemies within 12 m speed up (5 seconds) |
| When a named foe falls | Receive a random consumable | |
| When you use an item | An Estus charge | |
| When you drink Estus | Fate's Fury: +30% damage for 10 seconds | |
| When your HP falls low (below 30%) | Smite enemies within 8 m (a quarter of their HP) | |
| When you rest at a bonfire | Enemies within 15 m forget you | |
| When a boss falls | Enemies within 12 m slow down (5 seconds) | |
| | Enemies within 10 m freeze (3 seconds) | |
| | Enemies within 10 m float away (3 seconds) | |

Your first fate comes as soon as you load in.

## Settings

In the game (**Mods**, the **Kindle** tab, **All mod settings**, then Bonfire Fate) or in Kindle
Launcher while the game is closed:

- **Fate changes every**: 30 seconds to 10 minutes (default 2 minutes).
- **Curses** (on): include the curses.
- **Big banner** (on): announce each new fate with a banner, or with a small message.

## Switching it on

Bonfire Fate comes with Kindle and is off until you switch it on: at the title screen or in the
pause menu, choose **Mods**, open the **Mods** tab and turn **Bonfire Fate** on. It starts at once.
Kindle Launcher's mod list does the same while the game is closed.

To install a copy on its own, put this folder in the game's `Game\kindle\mods` folder, so you
have `Game\kindle\mods\bonfire-fate\mod.toml`.

## Removing it

Switch it off in **Mods**, or delete `Game\kindle\mods\bonfire-fate`. It stops at once. Nothing
stays in your save beyond what fate gave you while it ran (souls, weapons and consumables stay,
like any others).

## Good to know

- Fate's Fury is an effect the mod adds to the game while it runs; nothing about it is written to
  your save.
- Kindle keeps the game off FromSoftware's servers while it runs (their terms don't allow modded
  play online). Kindle's Co-op mod connects players itself.

## Kindle

Kindle is a Lua modding runtime for Dark Souls III: mods are folders of Lua scripts, switched on
and off in game. This mod is plain Lua in `main.lua`; edit it with the game running and Kindle
reloads it when you save. Causes and effects are lists at the top of the file, so adding your own
is a few lines each.
