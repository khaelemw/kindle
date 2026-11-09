<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.json

JSON text: for data a mod ships (kindle.read_data) or keeps as text.

## Functions

- [`encode`](#encode): A value as JSON text (`pretty`: indented on several lines).
- [`decode`](#decode): JSON text as a value: objects and arrays as tables, null as nil.

### encode

```lua
kindle.json.encode(value: any, pretty: boolean?) -> string
```

A value as JSON text (`pretty`: indented on several lines). A table whose keys are 1..n is an array, any other an object (number keys become text); an empty table is `[]`.

**Parameters**

- `value` `any`
- `pretty` `boolean?`

**Returns**

- `string`

### decode

```lua
kindle.json.decode(text: string) -> any
```

JSON text as a value: objects and arrays as tables, null as nil. An error if it isn't JSON.

**Parameters**

- `text` `string`

**Returns**

- `any`

