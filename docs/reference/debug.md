<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.debug

> Needs the `"debug"` permission in `mod.toml`.

Tools for finding things. The memory and code functions (singletons, singleton, class_name, module, read, bytes, wstr, watch, watch_stop, watch_hits, trace_next, census) exist only with raw access: for the mods the player names in kindle.toml [developer] (raw = true, raw_mods), and the console.

## Functions

- [`watch`](#watch): Watch which game code touches [addr, addr+len) (within one 4 KB page): debug.watch_hits() lists each instruction (offset in the exe), the address it touched and return offsets.
- [`watch_stop`](#watch_stop): Stop the current watch.
- [`trace_next`](#trace_next): Record every instruction the next call of the game function at `rva` runs (an offset in DarkSoulsIII.exe; the function's first instruction), the game's protected code included, up to `max` (2,000,000).
- [`press`](#press): Hold a key or controller button down for the game for a moment, as if the player pressed it (menus, title screen, testing).
- [`input_stats`](#input_stats): How often the game has read input since Kindle started: {keyboard reads, failed keyboard reads, mouse reads, controller reads, controller reads with none connected}.
- [`profile`](#profile): Milliseconds per second each mod spent in its Lua over the last second, in total and by handler (named by event: "frame", "timer", "damage", "message", ...): { [mod id] = { total = ms, [handler] = ms } }.
- [`census`](#census): How often the game ran each of Kindle's hooks since it was hooked, and on which threads (raw access only).
- [`watch_hits`](#watch_hits): What the watch saw.
- [`export`](#export): Save a game file from the game's archives, decrypted (as the game reads it), e.g.
- [`singletons`](#singletons): name -> static address of every game singleton: the ones Kindle uses, plus every static in the game's data that points to an object the game's type information names (found by scanning, so it takes a moment).
- [`class_name`](#class_name): The class of the object at an address, from the game's type information (e.g.
- [`singleton`](#singleton): Instance address of a named singleton (e.g.
- [`module`](#module): Base address and size of DarkSoulsIII.exe.
- [`read`](#read): Read a value; nil if the address is unreadable (never crashes).
- [`bytes`](#bytes): Hex dump of up to 4096 bytes.
- [`wstr`](#wstr): Read a null-terminated UTF-16 string.
- [`hex`](#hex): Format a number as hexadecimal, e.g.
- [`trace_hks`](#trace_hks): Log the player's behaviour script calls (env/act, with known ids named): each env question and its answer once, act calls when their arguments change.
- [`trace_events`](#trace_events): Log each distinct event script command the game runs, e.g.
- [`trace_talk`](#trace_talk): Log the talk script (NPC and bonfire menu) commands and queries as they run.
- [`trace_predicates`](#trace_predicates): Count the game's "who is this player" checks (session guest, multiplayer, world owner, host, online, the per-character-type table's columns) by the game code calling them and their answers, from now; debug.predicates() lists them, as do the co-op watchdog's reports.
- [`predicates`](#predicates): The counts since debug.trace_predicates(true), most called first: the check, the caller (an offset in DarkSoulsIII.exe), the character type asked about (the type table's), how many calls and how many answered true.

### watch

```lua
kindle.debug.watch(addr: integer, len: integer?, max: integer?)
```

Watch which game code touches [addr, addr+len) (within one 4 KB page): debug.watch_hits() lists each instruction (offset in the exe), the address it touched and return offsets. Stops by itself after max (16) distinct instructions.

**Parameters**

- `addr` `integer`
- `len` `integer?`: default 1
- `max` `integer?`: default 16

### watch_stop

```lua
kindle.debug.watch_stop()
```

Stop the current watch.

### trace_next

```lua
kindle.debug.trace_next(rva: integer, max: integer?)
```

Record every instruction the next call of the game function at `rva` runs (an offset in DarkSoulsIII.exe; the function's first instruction), the game's protected code included, up to `max` (2,000,000). The trace is saved to logs/trace_<n>.bin: the game module's base (u64), then for each instruction its address (u64) and its 16 bytes. For finding what a protected function really does: most of it is ordinary code the trace names.

**Parameters**

- `rva` `integer`
- `max` `integer?`

### press

```lua
kindle.debug.press(key: string|integer, seconds: number?)
```

Hold a key or controller button down for the game for a moment, as if the player pressed it (menus, title screen, testing). Keys: "enter", "esc", "e", "q", "f", "space", "up", "down", "left", "right", letters, digits, "f1"-"f5", or a DirectInput key code. Controller: "pad_a", "pad_b", "pad_x", "pad_y", "pad_start", "pad_back", "pad_up", "pad_down", "pad_left", "pad_right", "pad_lb", "pad_rb", "pad_ls", "pad_rs" (reported on controller 1, connected if none is, and for a minute after the last press so the game keeps reading it; with no controller Kindle tells the game one arrived and the first press starts 0.6 s late, once the game reads it). Controller presses work at the title screen even when the game ignores the keyboard there. Needs the overlay enabled (its input hooks).

**Parameters**

- `key` `string` | `integer`
- `seconds` `number?`: default 0.1

### input_stats

```lua
kindle.debug.input_stats() -> integer[]
```

How often the game has read input since Kindle started: {keyboard reads, failed keyboard reads, mouse reads, controller reads, controller reads with none connected}.

**Returns**

- `integer[]`

### profile

```lua
kindle.debug.profile() -> table<string, table<string, number>>
```

Milliseconds per second each mod spent in its Lua over the last second, in total and by handler (named by event: "frame", "timer", "damage", "message", ...): { [mod id] = { total = ms, [handler] = ms } }. The overlay's Mods tab shows the same when you hover a mod's ms/s.

**Returns**

- `table<string, table<string, number>>`

### census

```lua
kindle.debug.census() -> {area: string, name: string, calls: integer, off_game_thread: integer, threads: integer, per_second: number?}[]
```

How often the game ran each of Kindle's hooks since it was hooked, and on which threads (raw access only).

**Returns**

- `{area: string, name: string, calls: integer, off_game_thread: integer, threads: integer, per_second: number?}[]`

### watch_hits

```lua
kindle.debug.watch_hits() -> {rip: integer, address: integer, returns: integer[]}[]
```

What the watch saw.

**Returns**

- `{rip: integer, address: integer, returns: integer[]}[]`

### export

```lua
kindle.debug.export(path: string, dest: string) -> string
```

Save a game file from the game's archives, decrypted (as the game reads it), e.g. debug.export("/event/m40_00_00_00.emevd.dcx", "m40.emevd.dcx"): dest is a file name, written in kindle/exports/<mod id>/ (with raw access, any path). Returns which archive held it and its size.

**Parameters**

- `path` `string`
- `dest` `string`

**Returns**

- `string`

### singletons

```lua
kindle.debug.singletons() -> table<string, integer>
```

name -> static address of every game singleton: the ones Kindle uses, plus every static in the game's data that points to an object the game's type information names (found by scanning, so it takes a moment).

**Returns**

- `table<string, integer>`

### class_name

```lua
kindle.debug.class_name(addr: integer) -> string?
```

The class of the object at an address, from the game's type information (e.g. "WorldChrMan"), or nil.

**Parameters**

- `addr` `integer`

**Returns**

- `string?`

### singleton

```lua
kindle.debug.singleton(name: string) -> integer?
```

Instance address of a named singleton (e.g. "MsgRepository"), or nil.

**Parameters**

- `name` `string`

**Returns**

- `integer?`

### module

```lua
kindle.debug.module() -> integer, integer
```

Base address and size of DarkSoulsIII.exe.

**Returns**

- `base` `integer`
- `size` `integer`

### read

```lua
kindle.debug.read(addr: integer, type: "u8"|"i8"|"u16"|"i16"|"u32"|"i32"|"u64"|"ptr"|"f32"|"f64"?) -> number?
```

Read a value; nil if the address is unreadable (never crashes).

**Parameters**

- `addr` `integer`
- `type` `"u8"` | `"i8"` | `"u16"` | `"i16"` | `"u32"` | `"i32"` | `"u64"` | `"ptr"` | `"f32"` | `"f64"?`: default "u64"

**Returns**

- `number?`

### bytes

```lua
kindle.debug.bytes(addr: integer, len: integer) -> string?
```

Hex dump of up to 4096 bytes.

**Parameters**

- `addr` `integer`
- `len` `integer`

**Returns**

- `string?`

### wstr

```lua
kindle.debug.wstr(addr: integer, max: integer?) -> string?
```

Read a null-terminated UTF-16 string.

**Parameters**

- `addr` `integer`
- `max` `integer?`: default 256

**Returns**

- `string?`

### hex

```lua
kindle.debug.hex(n: integer) -> string
```

Format a number as hexadecimal, e.g. for addresses.

**Parameters**

- `n` `integer`

**Returns**

- `string`

### trace_hks

```lua
kindle.debug.trace_hks(on: boolean?)
```

Log the player's behaviour script calls (env/act, with known ids named): each env question and its answer once, act calls when their arguments change.

**Parameters**

- `on` `boolean?`: default true

### trace_events

```lua
kindle.debug.trace_events(on: boolean?)
```

Log each distinct event script command the game runs, e.g. "event command 2007[02](1)" (bank 2007, index 2, arguments as 32-bit words). For finding commands for events.run.

**Parameters**

- `on` `boolean?`: default true

### trace_talk

```lua
kindle.debug.trace_talk(on: boolean?)
```

Log the talk script (NPC and bonfire menu) commands and queries as they run.

**Parameters**

- `on` `boolean?`: default true

### trace_predicates

```lua
kindle.debug.trace_predicates(on: boolean?) -> integer
```

Count the game's "who is this player" checks (session guest, multiplayer, world owner, host, online, the per-character-type table's columns) by the game code calling them and their answers, from now; debug.predicates() lists them, as do the co-op watchdog's reports. Returns how many of the checks are hooked.

**Parameters**

- `on` `boolean?`: default true

**Returns**

- `integer`

### predicates

```lua
kindle.debug.predicates() -> {predicate: string, caller: integer, type: integer?, calls: integer, yes: integer}[]
```

The counts since debug.trace_predicates(true), most called first: the check, the caller (an offset in DarkSoulsIII.exe), the character type asked about (the type table's), how many calls and how many answered true.

**Returns**

- `{predicate: string, caller: integer, type: integer?, calls: integer, yes: integer}[]`

