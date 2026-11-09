<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.params

> Needs the `"params"` permission in `mod.toml`.

Read and change the game's param tables (weapon stats, effects, ...) live.

## Functions

- [`get`](#get): A live row, or nil if the row doesn't exist.
- [`tables`](#tables): Every param table's name.
- [`add_row`](#add_row): Add a brand-new row to a table, starting as a copy of row copy_from, and return it (a live row, like params.get).
- [`add_rows`](#add_rows): Add several rows to a table with one rebuild of it: add_row rebuilds the table for each row (a second each, and every place the game holds the table moves), so a mod adding many rows adds them together.
- [`copy`](#copy): Copies rows over others in one go: each `to` gets what its `from` holds before any of them is copied (so rows can trade contents), except the fields named in `keep`.
- [`reset`](#reset): Puts back what this mod wrote to the game's params: one row's fields (table and row), a table's, or everything (no arguments).
- [`fields`](#fields): A table's fields: name, type, byte offset, and array or bitfield size.
- [`rows`](#rows): A table's row ids, in table order.

### get

```lua
kindle.params.get(table: `T`, row: integer|kindle.item_name|string) -> T?
```

A live row, or nil if the row doesn't exist. Writes are reverted when the mod unloads. The row is typed by its table (e.g. EquipParamWeapon), so its fields autocomplete. In the item tables the row can also be the item's name: params.get("EquipParamGoods", "Ember").

**Parameters**

- `table` ``T``: e.g. "EquipParamGoods"
- `row` `integer` | [`kindle.item_name`](types.md#kindleitem_name) | `string`: row id, or an item's name in an item table

**Returns**

- `T?`

### tables

```lua
kindle.params.tables() -> kindle.param_table[]
```

Every param table's name.

**Returns**

- [`kindle.param_table[]`](types.md#kindleparam_table)

### add_row

```lua
kindle.params.add_row(table: `T`, id: integer, copy_from: integer|kindle.item_name|string) -> T
```

Add a brand-new row to a table, starting as a copy of row copy_from, and return it (a live row, like params.get). The id must be above every existing id in the table, including rows other mods added (mods load in folder-name order): pick ids well apart. For item tables (EquipParamGoods, EquipParamWeapon, EquipParamProtector, EquipParamAccessory) the new item also gets its template's name, effect line and description, until the mod sets its own (items.set_name etc.). Rows stay for the rest of the session, even after the mod unloads (an item the player holds must keep its row); adding the same id again returns that row. A save holding a new item needs the mod when it's loaded. copy_from can be an item's name in the item tables. Example, a new goods item based on Ember: `local MY_ITEM = 4000000` (your own id), then params.add_row("EquipParamGoods", MY_ITEM, "Ember")

**Parameters**

- `table` ``T``
- `id` `integer`: above every existing id
- `copy_from` `integer` | [`kindle.item_name`](types.md#kindleitem_name) | `string`: the row to copy, or an item's name in an item table

**Returns**

- `T`

### add_rows

```lua
kindle.params.add_rows(table: `T`, rows: {id: integer, copy_from: integer|kindle.item_name|string}[]) -> T[]
```

Add several rows to a table with one rebuild of it: add_row rebuilds the table for each row (a second each, and every place the game holds the table moves), so a mod adding many rows adds them together. `rows` are {id = ID, copy_from = ROW}, ids ascending and above every existing id; the rules of add_row hold. Returns the live rows, in order. Edits made to a table's rows stay through later additions to it. Example: params.add_rows("AtkParam_Npc", {{id = 12000000, copy_from = 2110010}, {id = 12000001, copy_from = 2110020}})

**Parameters**

- `table` ``T``
- `rows` `{id: integer, copy_from: integer|kindle.item_name|string}[]`

**Returns**

- `T[]`

### copy

```lua
kindle.params.copy(table: kindle.param_table, pairs: integer[][], keep: string[]?)
```

Copies rows over others in one go: each `to` gets what its `from` holds before any of them is copied (so rows can trade contents), except the fields named in `keep`. Reverted when the mod unloads, as any write. Example, two item lots trading contents but keeping their pickup flags: params.copy("ItemLotParam", {{4000, 4010}, {4010, 4000}}, {"getItemFlagId"})

**Parameters**

- `table` [`kindle.param_table`](types.md#kindleparam_table)
- `pairs` `integer[][]`: {{from, to}, ...}
- `keep` `string[]?`: field names left as they are

### reset

```lua
kindle.params.reset(table: kindle.param_table?, row: integer?) -> integer
```

Puts back what this mod wrote to the game's params: one row's fields (table and row), a table's, or everything (no arguments). Returns how many fields.

**Parameters**

- `table` [`kindle.param_table?`](types.md#kindleparam_table)
- `row` `integer?`

**Returns**

- `integer`

### fields

```lua
kindle.params.fields(table: kindle.param_table) -> {name: string, type: string, offset: integer, count?: integer, bit?: integer, bits?: integer}[]
```

A table's fields: name, type, byte offset, and array or bitfield size.

**Parameters**

- `table` [`kindle.param_table`](types.md#kindleparam_table)

**Returns**

- `{name: string, type: string, offset: integer, count?: integer, bit?: integer, bits?: integer}[]`

### rows

```lua
kindle.params.rows(table: kindle.param_table) -> integer[]
```

A table's row ids, in table order.

**Parameters**

- `table` [`kindle.param_table`](types.md#kindleparam_table)

**Returns**

- `integer[]`

