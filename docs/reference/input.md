<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.input

> Needs the `"input"` permission in `mod.toml`.

What the player presses, and keeping it from the game.

## Functions

- [`block_actions`](#block_actions): Keeps the player's actions from the game (on, or nil), or gives them back (false): its character doesn't act, and the game doesn't see the D-pad or the arrow keys (they switch weapons, items and spells).
- [`block_interact`](#block_interact): While this mod blocks the player's actions: keeps the interact button and key (A, E) from the game too (on, or nil), or not (false).
- [`held`](#held): Whether the player holds "left" (the D-pad, left stick, left arrow or A), "right" (as left) or "action" (A on the controller, or E), the game seeing it or not.
- [`down`](#down): Whether the player holds a key, a controller button or a mouse button, the game seeing it or not (kindle.input.press's presses don't count).
- [`mouse`](#mouse): Where the mouse cursor is, on the 1920 x 1080 canvas kindle.ui.layer draws on (so a layer's rect can be hit-tested against it), or nil while it is outside the game's window.
- [`using_pad`](#using_pad): Whether the player has used the controller more lately than the keyboard and mouse (to name the right buttons in a text).
- [`press`](#press): Holds a key ("esc", "up", "1"...

### block_actions

```lua
kindle.input.block_actions(on: boolean?)
```

Keeps the player's actions from the game (on, or nil), or gives them back (false): its character doesn't act, and the game doesn't see the D-pad or the arrow keys (they switch weapons, items and spells). The camera, the menus and popups still work. One mod blocks at a time; its unload gives them back.

**Parameters**

- `on` `boolean?`

### block_interact

```lua
kindle.input.block_interact(on: boolean?)
```

While this mod blocks the player's actions: keeps the interact button and key (A, E) from the game too (on, or nil), or not (false). With them kept, the character doesn't take things, read messages or rest; the menus still take them.

**Parameters**

- `on` `boolean?`

### held

```lua
kindle.input.held(name: "left"|"right"|"action") -> boolean
```

Whether the player holds "left" (the D-pad, left stick, left arrow or A), "right" (as left) or "action" (A on the controller, or E), the game seeing it or not.

**Parameters**

- `name` `"left"` | `"right"` | `"action"`

**Returns**

- `boolean`

### down

```lua
kindle.input.down(name: kindle.key) -> boolean
```

Whether the player holds a key, a controller button or a mouse button, the game seeing it or not (kindle.input.press's presses don't count). For a hotkey, the key_pressed event says when one goes down. `if kindle.input.down("shift") then ... end`

**Parameters**

- `name` [`kindle.key`](types.md#kindlekey)

**Returns**

- `boolean`

### mouse

```lua
kindle.input.mouse() -> {x: number, y: number}?
```

Where the mouse cursor is, on the 1920 x 1080 canvas kindle.ui.layer draws on (so a layer's rect can be hit-tested against it), or nil while it is outside the game's window. While the player moves the camera with the mouse the game keeps the cursor where it put it; in menus and screens it follows the mouse. `local at = kindle.input.mouse() if at and at.x < 200 then ... end`

**Returns**

- `{x: number, y: number}?`

### using_pad

```lua
kindle.input.using_pad() -> boolean
```

Whether the player has used the controller more lately than the keyboard and mouse (to name the right buttons in a text).

**Returns**

- `boolean`

### press

```lua
kindle.input.press(key: string|integer, seconds: number?)
```

Holds a key ("esc", "up", "1"... or a DirectInput key code) or a controller button ("pad_a", "pad_down"...) down for the game for `seconds` (0.1).

**Parameters**

- `key` `string` | `integer`
- `seconds` `number?`

