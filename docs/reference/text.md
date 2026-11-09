<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.text

> Needs the `"text"` permission in `mod.toml`.

Read and replace any of the game's text, live.

## Fields

| Field | Type | Description |
|---|---|---|
| `categories` | `string[]` | named categories (DLC tables are searched automatically) |

## Functions

- [`get`](#get): The text the game shows for (category, id), or nil.
- [`set`](#set): Replace that text everywhere the game shows it.
- [`reset`](#reset): Restore the original text.
- [`item`](#item): An item's "name" (default), "info" or "description" text as currently shown.

### get

```lua
kindle.text.get(category: kindle.text_category, id: integer) -> string?
```

The text the game shows for (category, id), or nil.

**Parameters**

- `category` [`kindle.text_category`](types.md#kindletext_category): name, or a raw FMG id
- `id` `integer`

**Returns**

- `string?`

### set

```lua
kindle.text.set(category: kindle.text_category, id: integer, value: string) -> integer
```

Replace that text everywhere the game shows it. Gives the id an entry if the game has none (a new bonfire's name, say). Reverted when the mod unloads.

**Parameters**

- `category` [`kindle.text_category`](types.md#kindletext_category)
- `id` `integer`
- `value` `string`

**Returns**

- `changed` `integer`: number of tables changed

### reset

```lua
kindle.text.reset(category: kindle.text_category, id: integer)
```

Restore the original text.

**Parameters**

- `category` [`kindle.text_category`](types.md#kindletext_category)
- `id` `integer`

### item

```lua
kindle.text.item(item: kindle.itemref, field: "name"|"info"|"description"?) -> string?
```

An item's "name" (default), "info" or "description" text as currently shown.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `field` `"name"` | `"info"` | `"description"?`

**Returns**

- `string?`

