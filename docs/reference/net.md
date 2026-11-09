<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.net

> Needs the `"net"` permission in `mod.toml`.

The game's online session over Kindle's own connection (no Steam or FromSoftware server: a host shares a code, players join with it): hosting, joining, leaving, a lobby players gather in before a session (from the title screen on), and messages to this mod on the session's or lobby's other games (a mod reaches only itself there).

## Functions

- [`send`](#send): Send a value (boolean, number, string or table, as kindle.store keeps them; 16 KB at most as JSON) to "host", "all" (every other game) or a member's id (kindle.net.members()).
- [`on`](#on): Call `fn(value, from)` for this mod's messages on `channel` (on Kindle's frame).
- [`set_welcome`](#set_welcome): A host's join data: `value` (as net.send takes) goes to each player as it joins, before its game loads this world, and to the players in now; nil removes it.
- [`welcome`](#welcome): A player in a host's world: the host's join data on `channel` (this mod's), or nil.
- [`require_dlc`](#require_dlc): What players joining this host's world need: a list of DLCs; a player without one is refused and told why.
- [`role`](#role): This game in the session: "host", "player", or nil without one.
- [`me`](#me): This game's id in the session ("0x..."), or nil.
- [`host`](#host): The host's id ("0x..."; this game's when it hosts), or nil.
- [`members`](#members): Every member's id ("0x..."), or nil without a session.
- [`open_lobby`](#open_lobby): Opens a lobby this game hosts: players gather in it before a session, from the title screen on (it has the co-op port; a session hosted later shares it).
- [`join_lobby`](#join_lobby): Joins a lobby with its code (from the title screen or anywhere): Kindle connects off the game's thread, then fn(result, why): "joined", "refused" (why: the host's answer) or "failed".
- [`leave_lobby`](#leave_lobby): Leaves the lobby; a host closes it (its members' links end).
- [`lobby`](#lobby): The lobby this game is in, or nil: its role ("host" or "member"), code (the host's), me and host (ids, as members() names them), and members: {id, name (the character's, or "Player" and an id's end at the title), place ("title", "loading" or "world")}.
- [`lobby_send`](#lobby_send): Sends a value to this mod on the lobby's other games ("host", "all" or a member's id), as net.send does, over the lobby even while there's a session (members not in it get it too).
- [`send_party_save`](#send_party_save): The lobby's host: sends a member its party save for this game's world (its character there, kept by this game, or none yet), and keeps what the member's game writes back to it.
- [`player`](#player): A member's character in this game's world ("host" or a member's id; this game's own too): its handle (for kindle.world) and the character's name, or nil while it isn't there.
- [`host_session`](#host_session): Host a session (from the world, with a character loaded and no session).
- [`join`](#join): Join the session with this code (from the world, with no session), without holding up the game: Kindle connects to the host on its own thread, then the game joins it (summoned to the host's place), and `fn(result, why)` follows on Kindle's frame: result "joined", "gone" (nothing answers at the host's address), "refused" (the host turned this player away for now: why, as it says, such as "the host is on a loading screen" or a DLC this game doesn't have) or "failed" (why).
- [`leave`](#leave): Leave the session: Kindle's connection (the others see this game leave) and the game's (a player's game goes home; a host's session ends).
- [`pings`](#pings): The other players' names and pings in milliseconds, as the HUD shows them (a player's ping to another player is through the host).
- [`state`](#state): The game's session: "none", "creating", "create failed", "hosting", "joining", "join failed", "joined" or "leaving".
- [`code`](#code): This session's code: a host's, or the one this player joined with; nil without one.

### send

```lua
kindle.net.send(to: string, channel: string, value: any)
```

Send a value (boolean, number, string or table, as kindle.store keeps them; 16 KB at most as JSON) to "host", "all" (every other game) or a member's id (kindle.net.members()). A player's goes through the host. Without a session, over the lobby (net.lobby); an error without either.

**Parameters**

- `to` `string`
- `channel` `string`: 1 to 64 bytes
- `value` `any`

### on

```lua
kindle.net.on(channel: string, fn: fun(value: any, from: string)) -> integer
```

Call `fn(value, from)` for this mod's messages on `channel` (on Kindle's frame).

**Parameters**

- `channel` `string`
- `fn` `fun(value: any, from: string)`

**Returns**

- `handle` `integer`

### set_welcome

```lua
kindle.net.set_welcome(channel: string, value: any)
```

A host's join data: `value` (as net.send takes) goes to each player as it joins, before its game loads this world, and to the players in now; nil removes it. For what a player's game needs in place as it arrives, e.g. a randomizer's seed for the maps it loads.

**Parameters**

- `channel` `string`: 1 to 64 bytes
- `value` `any`

### welcome

```lua
kindle.net.welcome(channel: string) -> any
```

A player in a host's world: the host's join data on `channel` (this mod's), or nil. There from the start of the join, before the host's world loads.

**Parameters**

- `channel` `string`

**Returns**

- `any`

### require_dlc

```lua
kindle.net.require_dlc(dlcs: ("ashes_of_ariandel"|"ringed_city")[])
```

What players joining this host's world need: a list of DLCs; a player without one is refused and told why. {} for none.

**Parameters**

- `dlcs` `("ashes_of_ariandel"|"ringed_city")[]`

### role

```lua
kindle.net.role() -> "host"|"player"|nil
```

This game in the session: "host", "player", or nil without one.

**Returns**

- `"host"` | `"player"` | `nil`

### me

```lua
kindle.net.me() -> string?
```

This game's id in the session ("0x..."), or nil.

**Returns**

- `string?`

### host

```lua
kindle.net.host() -> string?
```

The host's id ("0x..."; this game's when it hosts), or nil.

**Returns**

- `string?`

### members

```lua
kindle.net.members() -> string[]?
```

Every member's id ("0x..."), or nil without a session.

**Returns**

- `string[]?`

### open_lobby

```lua
kindle.net.open_lobby() -> string
```

Opens a lobby this game hosts: players gather in it before a session, from the title screen on (it has the co-op port; a session hosted later shares it). Its code, for the others to join with.

**Returns**

- `string`

### join_lobby

```lua
kindle.net.join_lobby(code: string, fn: fun(result: "joined"|"refused"|"failed", why: string?))
```

Joins a lobby with its code (from the title screen or anywhere): Kindle connects off the game's thread, then fn(result, why): "joined", "refused" (why: the host's answer) or "failed".

**Parameters**

- `code` `string`
- `fn` `fun(result: "joined"|"refused"|"failed", why: string?)`

### leave_lobby

```lua
kindle.net.leave_lobby()
```

Leaves the lobby; a host closes it (its members' links end). A member playing its party save has its own saves again at the main menu.

### lobby

```lua
kindle.net.lobby() -> {role: "host"|"member", code: string?, me: string, host: string, world: string?, members: {id: string, name: string, place: "title"|"loading"|"world"}[]}?
```

The lobby this game is in, or nil: its role ("host" or "member"), code (the host's), me and host (ids, as members() names them), and members: {id, name (the character's, or "Player" and an id's end at the title), place ("title", "loading" or "world")}. The host's also has world: the world its party plays in (its character's, or at the title the one its Continue will load: the last it played; nil if Kindle has seen none). While there's no session, net.send and net.on carry the mod's messages between the lobby's games.

**Returns**

- `{role: "host"|"member", code: string?, me: string, host: string, world: string?, members: {id: string, name: string, place: "title"|"loading"|"world"}[]}?`

### lobby_send

```lua
kindle.net.lobby_send(to: string, channel: string, value: any)
```

Sends a value to this mod on the lobby's other games ("host", "all" or a member's id), as net.send does, over the lobby even while there's a session (members not in it get it too). Received with net.on.

**Parameters**

- `to` `string`
- `channel` `string`
- `value` `any`

### send_party_save

```lua
kindle.net.send_party_save(member: string) -> string
```

The lobby's host: sends a member its party save for this game's world (its character there, kept by this game, or none yet), and keeps what the member's game writes back to it. The member's game plays it from its main menu (the events saves.party_save_ready and the others in kindle.game.catalogue()). The world: the host's character's, or at the title the one its Continue will load (net.lobby().world), so a member's is in place before the host loads. The world it was for.

**Parameters**

- `member` `string`

**Returns**

- `world` `string`

### player

```lua
kindle.net.player(id: string) -> {handle: integer, name: string}?
```

A member's character in this game's world ("host" or a member's id; this game's own too): its handle (for kindle.world) and the character's name, or nil while it isn't there.

**Parameters**

- `id` `string`

**Returns**

- `{handle: integer, name: string}?`

### host_session

```lua
kindle.net.host_session()
```

Host a session (from the world, with a character loaded and no session). Its code is kindle.net.code() once it's up.

### join

```lua
kindle.net.join(code: string, fn: fun(result: "joined"|"gone"|"refused"|"failed", why: string?))
```

Join the session with this code (from the world, with no session), without holding up the game: Kindle connects to the host on its own thread, then the game joins it (summoned to the host's place), and `fn(result, why)` follows on Kindle's frame: result "joined", "gone" (nothing answers at the host's address), "refused" (the host turned this player away for now: why, as it says, such as "the host is on a loading screen" or a DLC this game doesn't have) or "failed" (why). One join at a time; an error if one can't start.

**Parameters**

- `code` `string`
- `fn` `fun(result: "joined"|"gone"|"refused"|"failed", why: string?)`

### leave

```lua
kindle.net.leave(for_now: boolean?)
```

Leave the session: Kindle's connection (the others see this game leave) and the game's (a player's game goes home; a host's session ends). The catalogue's event session.left follows. for_now: the player means to come back (following the host to its new place): Kindle's party keeps joining it to the host's world.

**Parameters**

- `for_now` `boolean?`

### pings

```lua
kindle.net.pings() -> {name: string, ms: integer}[]
```

The other players' names and pings in milliseconds, as the HUD shows them (a player's ping to another player is through the host). Empty without a session.

**Returns**

- `{name: string, ms: integer}[]`

### state

```lua
kindle.net.state() -> string
```

The game's session: "none", "creating", "create failed", "hosting", "joining", "join failed", "joined" or "leaving".

**Returns**

- `string`

### code

```lua
kindle.net.code() -> string?, boolean
```

This session's code: a host's, or the one this player joined with; nil without one. And whether it names an address on the host's own network only (for the internet, kindle.toml's [coop] public_address).

**Returns**

- `code` `string?`
- `is_local` `boolean`

