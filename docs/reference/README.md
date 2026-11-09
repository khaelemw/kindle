<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# API reference

Everything a mod can call. Most modules need a permission in the mod's `mod.toml` (`[permissions] modules = [...]`); the core functions and `log` are always available.

| Module | Permission | |
|---|---|---|
| [`kindle`](core.md) | always | Events, timers and this mod's own info. |
| [`kindle.log`](log.md) | always | Write to Kindle's log (the overlay's Log tab and logs\kindle.log). |
| [`kindle.settings`](settings.md) | always | The mod's settings: options declared in mod.toml that players change in the overlay's Mods tab. |
| [`kindle.store`](store.md) | always | Values a mod keeps between sessions. |
| [`kindle.mods`](mods.md) | always | Other mods: their exports and who's running. |
| [`kindle.json`](json.md) | always | JSON text: for data a mod ships (kindle.read_data) or keeps as text. |
| [`kindle.flags`](flags.md) | `"flags"` | Read, set and watch the game's event flags (bosses killed, doors opened, NPC progress). |
| [`kindle.items`](items.md) | `"items"` | Find items by id or name; give, take and count them; rename them. |
| [`kindle.text`](text.md) | `"text"` | Read and replace any of the game's text, live. |
| [`kindle.ui`](ui.md) | `"ui"` | On-screen messages, drawn even when the overlay is closed. |
| [`kindle.player`](player.md) | `"player"` | The player: HP/FP/stamina, stats, effects, equipment, position, souls, animations. |
| [`kindle.world`](world.md) | `"world"` | Other characters: enemies, NPCs, players. |
| [`kindle.events`](events.md) | `"events"` | Run the game's own event script (EMEVD) commands, and change its event scripts (applied from the next time that map's events load). |
| [`kindle.hks`](hks.md) | `"hks"` | The character behaviour scripts (HavokScript, c0000.hks etc.). |
| [`kindle.params`](params.md) | `"params"` | Read and change the game's param tables (weapon stats, effects, ...) live. |
| [`kindle.maps`](maps.md) | `"maps"` | The game's map files (what stands where: enemies, objects, collisions) as the game has them, and changes to them, applied when the game reads the map's file (the next time that area loads; in "map_loading" for the load starting). |
| [`kindle.game`](game.md) | `"game"` | The catalogue: the game's decisions, values, functions and notifications Kindle names and reviews for mods. |
| [`kindle.net`](net.md) | `"net"` | The game's online session over Kindle's own connection (no Steam or FromSoftware server: a host shares a code, players join with it): hosting, joining, leaving, a lobby players gather in before a session (from the title screen on), and messages to this mod on the session's or lobby's other games (a mod reaches only itself there). |
| [`kindle.input`](input.md) | `"input"` | What the player presses, and keeping it from the game. |
| [`kindle.debug`](debug.md) | `"debug"` | Tools for finding things. |

Also: [Types](types.md), the tables and names functions take and return.
