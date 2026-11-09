<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.maps

> Needs the `"maps"` permission in `mod.toml`.

The game's map files (what stands where: enemies, objects, collisions) as the game has them, and changes to them, applied when the game reads the map's file (the next time that area loads; in "map_loading" for the load starting).

## Functions

- [`parts`](#parts): A map's parts as the game's file has them, in the file's order (part 1 first), or nil if the game has no such map (a DLC's that isn't installed).
- [`preload`](#preload): Reads maps' parts ahead on a thread of Kindle's (the game goes on), so maps.parts gives them at once later.
- [`edit`](#edit): Sets this mod's changes to a map's file (replacing its earlier ones for that map).
- [`reset`](#reset): Drops this mod's changes to a map (to every map without one); how many there were.

### parts

```lua
kindle.maps.parts(map: string) -> kindle.map_part[]?
```

A map's parts as the game's file has them, in the file's order (part 1 first), or nil if the game has no such map (a DLC's that isn't installed). Read from the game's archives the first time (up to a second; maps.preload reads ahead).

**Parameters**

- `map` `string`: e.g. "m30_00_00_00"

**Returns**

- [`kindle.map_part[]?`](types.md#kindlemap_part)

### preload

```lua
kindle.maps.preload(maps: string[])
```

Reads maps' parts ahead on a thread of Kindle's (the game goes on), so maps.parts gives them at once later.

**Parameters**

- `maps` `string[]`

### edit

```lua
kindle.maps.edit(map: string, changes: ({replace_enemy: integer, from: {map: string, part: integer}, keep_ids: boolean?}|{replace_enemy: integer, model: string, npc_param: integer?, think_param: integer?, ai: string?, keep_ids: boolean?})[])
```

Sets this mod's changes to a map's file (replacing its earlier ones for that map). A change: {replace_enemy = n, from = {map =, part = m}, keep_ids = false}: part n (an enemy) becomes an enemy of part m's type (of any map): it keeps its place, the ground it stands on and its walk route; its entity id and groups go (its spot's event scripts no longer act on it) unless keep_ids. Kindle loads the AI of the maps the new enemies come from. A change {replace_enemy = n, model = "c7200", npc_param = A, think_param = B, ai = "name", keep_ids = false} makes part n an enemy of a model no map has (a mod's own, its files served by the mod) with the NpcParam row and AI row given (each only if given), its goals in script/<ai>.luabnd (loaded as long as the map is); the model's name has as many characters as the part's own. A changed map file is kept in kindle\cache for the next session.

**Parameters**

- `map` `string`
- `changes` `({replace_enemy: integer, from: {map: string, part: integer}, keep_ids: boolean?}|{replace_enemy: integer, model: string, npc_param: integer?, think_param: integer?, ai: string?, keep_ids: boolean?})[]`

### reset

```lua
kindle.maps.reset(map: string?) -> integer
```

Drops this mod's changes to a map (to every map without one); how many there were.

**Parameters**

- `map` `string?`

**Returns**

- `integer`

