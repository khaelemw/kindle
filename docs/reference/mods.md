<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.mods

Other mods: their exports and who's running. Always available.

## Functions

- [`get`](#get): Another mod's exported functions (see kindle.export), or nil if that mod isn't running.
- [`running`](#running): Ids of the running mods.

### get

```lua
kindle.mods.get(id: string) -> table<string, function>?
```

Another mod's exported functions (see kindle.export), or nil if that mod isn't running. List the mod under `depends` in mod.toml so it's loaded first. Example: local fate = kindle.mods.get("bonfire-fate"); if fate then fate.reroll() end

**Parameters**

- `id` `string`

**Returns**

- `table<string, function>?`

### running

```lua
kindle.mods.running() -> string[]
```

Ids of the running mods.

**Returns**

- `string[]`

