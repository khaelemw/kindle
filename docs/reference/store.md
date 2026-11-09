<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.store

Values a mod keeps between sessions. Always available. `get`/`set` keep values for the loaded character (saved per save slot; a new character in the slot starts empty); `get_global`/`set_global` keep values for the mod itself, the same for every character (settings). Kindle writes them to disk (kindle\data\<mod id>) about once a second and when the mod unloads or the character leaves the world.

## Functions

- [`get`](#get): A value saved for the loaded character, or `default` if it was never set.
- [`set`](#set): Save a value for the loaded character (nil removes it).
- [`keys`](#keys): The keys saved for the loaded character.
- [`get_global`](#get_global): A value saved for the mod itself, or `default` if it was never set.
- [`set_global`](#set_global): Save a value for the mod itself (nil removes it).
- [`global_keys`](#global_keys): The keys saved for the mod itself.

### get

```lua
kindle.store.get(key: string, default: any?) -> any
```

A value saved for the loaded character, or `default` if it was never set. Errors if no character is loaded.

**Parameters**

- `key` `string`
- `default` `any?`

**Returns**

- `any`

### set

```lua
kindle.store.set(key: string, value: kindle.storable)
```

Save a value for the loaded character (nil removes it).

**Parameters**

- `key` `string`
- `value` [`kindle.storable`](types.md#kindlestorable)

### keys

```lua
kindle.store.keys() -> string[]
```

The keys saved for the loaded character.

**Returns**

- `string[]`

### get_global

```lua
kindle.store.get_global(key: string, default: any?) -> any
```

A value saved for the mod itself, or `default` if it was never set.

**Parameters**

- `key` `string`
- `default` `any?`

**Returns**

- `any`

### set_global

```lua
kindle.store.set_global(key: string, value: kindle.storable)
```

Save a value for the mod itself (nil removes it).

**Parameters**

- `key` `string`
- `value` [`kindle.storable`](types.md#kindlestorable)

### global_keys

```lua
kindle.store.global_keys() -> string[]
```

The keys saved for the mod itself.

**Returns**

- `string[]`

