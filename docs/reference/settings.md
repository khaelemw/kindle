<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.settings

The mod's settings: options declared in mod.toml that players change in the overlay's Mods tab. Always available. Declare each as a table:

```toml
[settings.chance]
type = "float"            # "bool", "int", "float" or "choice"
label = "Effect chance"   # shown in the overlay (default: the name)
description = "How often fate strikes."   # tooltip
default = 0.5
min = 0.0                 # int/float: range (sliders when both are set)
max = 1.0
# choice: choices = ["easy", "normal", "hard"]
```



Values are saved for the mod (all characters). kindle.on("setting_changed", fn(name, value)) runs when the player changes one.

## Functions

- [`get`](#get): A setting's current value: the player's choice, or the default.
- [`set`](#set): Change a setting from the mod (checked against its declaration; ranges clamp).
- [`names`](#names): The declared settings' names, in mod.toml order.

### get

```lua
kindle.settings.get(name: string) -> boolean|number|string
```

A setting's current value: the player's choice, or the default.

**Parameters**

- `name` `string`

**Returns**

- `boolean` | `number` | `string`

### set

```lua
kindle.settings.set(name: string, value: boolean|number|string)
```

Change a setting from the mod (checked against its declaration; ranges clamp).

**Parameters**

- `name` `string`
- `value` `boolean` | `number` | `string`

### names

```lua
kindle.settings.names() -> string[]
```

The declared settings' names, in mod.toml order.

**Returns**

- `string[]`

