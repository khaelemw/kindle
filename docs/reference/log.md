<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.log

Write to Kindle's log (the overlay's Log tab and logs\kindle.log). Always available.

## Functions

- [`debug`](#debug): Log with string.format-style arguments, e.g.
- [`info`](#info): Log at info level (arguments as for log.debug).
- [`warn`](#warn): Log a warning (arguments as for log.debug).
- [`error`](#error): Log an error (arguments as for log.debug).

### debug

```lua
kindle.log.debug(fmt: any, ...: any)
```

Log with string.format-style arguments, e.g. log.info("hp=%d", hp).

**Parameters**

- `fmt` `any`
- `...` `any`

### info

```lua
kindle.log.info(fmt: any, ...: any)
```

Log at info level (arguments as for log.debug).

**Parameters**

- `fmt` `any`
- `...` `any`

### warn

```lua
kindle.log.warn(fmt: any, ...: any)
```

Log a warning (arguments as for log.debug).

**Parameters**

- `fmt` `any`
- `...` `any`

### error

```lua
kindle.log.error(fmt: any, ...: any)
```

Log an error (arguments as for log.debug). The mod keeps running.

**Parameters**

- `fmt` `any`
- `...` `any`

