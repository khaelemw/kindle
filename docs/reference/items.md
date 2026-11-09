<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.items

> Needs the `"items"` permission in `mod.toml`.

Find items by id or name; give, take and count them; rename them.

## Functions

- [`weapon`](#weapon): A weapon by EquipParamWeapon row id or by name.
- [`armor`](#armor): Armor by EquipParamProtector row id or by name.
- [`ring`](#ring): A ring by EquipParamAccessory row id or by name.
- [`goods`](#goods): Goods (consumables, key items, spells) by EquipParamGoods row id or by name.
- [`id`](#id): The item with this exact name in any category.
- [`name`](#name): The item's name in the game's language, or nil.
- [`find`](#find): Search item names (case-insensitive substring), exact matches first.
- [`give`](#give): Give an item.
- [`remove`](#remove): Take an item from the player's inventory.
- [`count`](#count): How many the player carries.
- [`has`](#has): Whether the player carries at least `quantity` of an item.
- [`info`](#info): An item's category, param row id, name and icon (the game's icon id: a layer's image or a screen row's icon shows it).
- [`set_name`](#set_name): Rename an item everywhere the game shows it.
- [`set_description`](#set_description): Replace an item's long description (needs "text").
- [`set_info`](#set_info): Replace an item's short effect line (needs "text").
- [`reset_text`](#reset_text): Restore an item's original name, description and info (needs "text").

### weapon

```lua
kindle.items.weapon(id_or_name: integer|kindle.weapon_name|string) -> kindle.item
```

A weapon by EquipParamWeapon row id or by name.

**Parameters**

- `id_or_name` `integer` | [`kindle.weapon_name`](types.md#kindleweapon_name) | `string`

**Returns**

- [`kindle.item`](types.md#kindleitem)

### armor

```lua
kindle.items.armor(id_or_name: integer|kindle.armor_name|string) -> kindle.item
```

Armor by EquipParamProtector row id or by name.

**Parameters**

- `id_or_name` `integer` | [`kindle.armor_name`](types.md#kindlearmor_name) | `string`

**Returns**

- [`kindle.item`](types.md#kindleitem)

### ring

```lua
kindle.items.ring(id_or_name: integer|kindle.ring_name|string) -> kindle.item
```

A ring by EquipParamAccessory row id or by name.

**Parameters**

- `id_or_name` `integer` | [`kindle.ring_name`](types.md#kindlering_name) | `string`

**Returns**

- [`kindle.item`](types.md#kindleitem)

### goods

```lua
kindle.items.goods(id_or_name: integer|kindle.goods_name|string) -> kindle.item
```

Goods (consumables, key items, spells) by EquipParamGoods row id or by name.

**Parameters**

- `id_or_name` `integer` | [`kindle.goods_name`](types.md#kindlegoods_name) | `string`

**Returns**

- [`kindle.item`](types.md#kindleitem)

### id

```lua
kindle.items.id(name: kindle.item_name|string) -> kindle.item
```

The item with this exact name in any category. Errors if unknown or ambiguous (then use items.weapon/armor/ring/goods(name) to pick the category).

**Parameters**

- `name` [`kindle.item_name`](types.md#kindleitem_name) | `string`

**Returns**

- [`kindle.item`](types.md#kindleitem)

### name

```lua
kindle.items.name(item: kindle.itemref) -> string?
```

The item's name in the game's language, or nil.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)

**Returns**

- `string?`

### find

```lua
kindle.items.find(query: string, limit: integer?) -> {name: string, category: "weapon"|"armor"|"ring"|"goods", id: integer, item: kindle.item}[]
```

Search item names (case-insensitive substring), exact matches first.

**Parameters**

- `query` `string`
- `limit` `integer?`: default 25

**Returns**

- `{name: string, category: "weapon"|"armor"|"ring"|"goods", id: integer, item: kindle.item}[]`

### give

```lua
kindle.items.give(item: kindle.itemref, quantity: integer?, opts: {popup: boolean}?)
```

Give an item. Shows the game's "item received" box unless opts.popup = false.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `quantity` `integer?`: default 1
- `opts` `{popup: boolean}?`

### remove

```lua
kindle.items.remove(item: kindle.itemref, quantity: integer?)
```

Take an item from the player's inventory.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `quantity` `integer?`: default 1

### count

```lua
kindle.items.count(item: kindle.itemref) -> integer
```

How many the player carries.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)

**Returns**

- `integer`

### has

```lua
kindle.items.has(item: kindle.itemref, quantity: integer?) -> boolean
```

Whether the player carries at least `quantity` of an item.

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `quantity` `integer?`: default 1

**Returns**

- `boolean`

### info

```lua
kindle.items.info(item: kindle.itemref) -> "weapon"|"armor"|"ring"|"goods", integer, string?, integer?
```

An item's category, param row id, name and icon (the game's icon id: a layer's image or a screen row's icon shows it).

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)

**Returns**

- `category` `"weapon"` | `"armor"` | `"ring"` | `"goods"`
- `param_id` `integer`
- `name` `string?`
- `icon` `integer?`

### set_name

```lua
kindle.items.set_name(item: kindle.itemref, name: string)
```

Rename an item everywhere the game shows it. Needs the "text" permission too. Reverted when the mod unloads; the new name also works in items.give("New Name").

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `name` `string`

### set_description

```lua
kindle.items.set_description(item: kindle.itemref, text: string)
```

Replace an item's long description (needs "text").

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `text` `string`

### set_info

```lua
kindle.items.set_info(item: kindle.itemref, text: string)
```

Replace an item's short effect line (needs "text").

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)
- `text` `string`

### reset_text

```lua
kindle.items.reset_text(item: kindle.itemref)
```

Restore an item's original name, description and info (needs "text").

**Parameters**

- `item` [`kindle.itemref`](types.md#kindleitemref)

