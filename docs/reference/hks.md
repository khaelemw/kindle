<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.hks

> Needs the `"hks"` permission in `mod.toml`.

The character behaviour scripts (HavokScript, c0000.hks etc.). They ask the game questions with env(id, ...) and act with act(id, ...); mods can change the answers and block actions without editing the scripts. Use kindle.debug.trace_hks(true) to see the player's calls (known ids are named). Example: blocking act SET_VARIABLE for "MoveSpeedLevelReal" stops the player sprinting.

## Functions

- [`on_env`](#on_env): Change the answer to env(id, ...): return a number/boolean (nil keeps the game's answer).
- [`run_act`](#run_act): Have a character's script perform act(id, ...): it runs at that script's next call (within a frame).
- [`ask_env`](#ask_env): Ask a character's script env(id, ...); the last argument is fn(result) for the answer (nil if the character's script didn't run within ~5 seconds).
- [`eval`](#eval): Run HKS source in a character's script state, with access to its globals and functions (the script's own Lua, not Kindle's).
- [`get_variable`](#get_variable): Read a behaviour graph variable (the values the animation graph runs on, e.g.
- [`set_variable`](#set_variable): Set a behaviour graph variable.
- [`get_global`](#get_global): Read a global of the character's script, e.g.
- [`set_global`](#set_global): Set a global of the character's script (nil removes it).
- [`on_call`](#on_call): Handle kindle(name, ...) calls from edited behaviour scripts.
- [`on_act`](#on_act): Watch or block act(id, ...): return false to block the action.

### env

`kindle.hks.env`: `kindle.hks_env_ids`

| Field | Type | Description |
|---|---|---|
| `HP` | `integer` | 1000: current HP |
| `STAMINA` | `integer` | 1001: current stamina |
| `FP` | `integer` | 2016: current FP |
| `HAS_SPEFFECT` | `integer` | 1116: env(1116, id): 1 if the character has SpEffect id |
| `WEAPON_MOTION_CATEGORY` | `integer` | 225: env(225, hand): the hand's weapon motion category (wepmotionCategory; 42 = fists; hand 0 left, 1 right) |
| `GUARD_MOTION_CATEGORY` | `integer` | 278: env(278, hand): the hand's guard motion category (GUARD_STYLE_*: 0 default, 1 small, 2 large, 3 torch) |
| `WEAPON_ART_CATEGORY` | `integer` | 345: env(345, hand): the hand's weapon art category (spAtkcategory) |
| `HAND_STYLE` | `integer` | 207: how weapons are held (HAND_*: 1 right one-handed, 2 two-handing left, 3 two-handing right) |
| `EQUIP_WEIGHT` | `integer` | 257: equip load class (WEIGHT_*: 0 light, 5 normal, 10 heavy, 15 overweight, 16 super light) (unconfirmed) |
| `ACTION_REQUEST` | `integer` | 1106: env(1106, action): 1 when that action's input is requested (ACTION_ARM_*: 0 R1, 1 R2, 2 L1, 3 L2, 7 use item, 12 backstep, 13 roll, 14 jump) (unconfirmed) |
| `ACTION_HELD` | `integer` | 1107: env(1107, action): 1 while that action's input is held (unconfirmed) |
| `ACTION_HOLD_TIME` | `integer` | 1108: env(1108, action): how long that action's input has been held (unconfirmed) |
| `ANY_ACTION_REQUEST` | `integer` | 1109: 1 when any action input is requested (IsActionRequest) (unconfirmed) |
| `HAS_CONDITION` | `integer` | 305: env(305, type): 1 if the character is in that condition (CONDITION_TYPE_*: 117 stone, 136 crystal, 154 no estus, 268 hollowing death, 286/287 gravity) (unconfirmed) |
| `ITEM_ANIM_TYPE` | `integer` | 231: animation type of the item being used (ITEM_*: 10 drink, 19 drink FP, 21 drink sake) (unconfirmed) |
| `WEAPON_ART_INFO` | `integer` | 326: env(326, hand): weapon art info for the hand (GetSwordArtsInfo) (unconfirmed) |
| `MOVE_CANCEL_TIMING` | `integer` | 2000: 1 when the current animation can be cancelled into movement (unconfirmed) |
| `GUARD_CANCEL_TIMING` | `integer` | 243: 1 when the current animation can be cancelled into guarding (unconfirmed) |
| `FALL_START` | `integer` | 200: asked when a fall may start (ExecFallStart) (unconfirmed) |
| `FALLING_FOR` | `integer` | 315: env(315, seconds): 1 once the character has been falling that long (the player's script kills it at 12) |
| `FRAME_TIME` | `integer` | 333: frame time (GetDeltaTime; observed 18-25) (unconfirmed) |
| `LADDER_COMMAND` | `integer` | 105: env(105, n): ladder event command (-1 none) (unconfirmed) |
| `IS_FEMALE` | `string` | "女性か": 1 if the character is female (unconfirmed) |
| `IS_DUAL_BLADE` | `string` | "双剣か": env(name, hand): 1 if the hand holds a paired weapon (unconfirmed) |
| `WEAPON_ART_CATEGORY_BY_NAME` | `string` | "装備武器特殊カテゴリ番号取得": env(name, hand): the hand's weapon art category (spAtkcategory) |
| `WEAPON_ART_CANCEL_TYPE` | `string` | "剣戟キャンセルタイプを取得": env(name, hand): weapon art cancel type (unconfirmed) |
| `CAN_CANCEL_WEAPON_ART` | `string` | "剣戟キャンセル可能か": 1 when the weapon art can be cancelled (unconfirmed) |
| `IS_EQUIP_MENU_OPEN` | `string` | "装備メニュー開いているか": 1 while the equipment menu is open (unconfirmed) |
| `HAS_EVENT_ANIM_REQUEST` | `string` | "イベントアニメ再生要求があるか": 1 when an event animation is requested (unconfirmed) |
| `IDLE_ANIM_CATEGORY` | `string` | "待機アニメカテゴリ取得": idle animation category (unconfirmed) |
| `IS_ANIM_END` | `string` | "アニメ終了か": env(name, layer): 1 when the animation has ended (unconfirmed) |
| `WEAPON_STORAGE_TYPE` | `string` | "武器格納場所タイプ取得": env(name, hand): where the weapon is stowed (back, waist, shoulder) (unconfirmed) |
| `UPPER_ARM_OUTER_ANGLE` | `string` | "上腕制御外側角度": upper arm control: outward angle (unconfirmed) |
| `UPPER_ARM_VERTICAL_ANGLE` | `string` | "上腕制御上下角度": upper arm control: vertical angle (unconfirmed) |

### act

`kindle.hks.act`: `kindle.hks_act_ids`

| Field | Type | Description |
|---|---|---|
| `SET_VARIABLE` | `integer` | 148: act(148, name, value): set a behaviour graph variable |
| `APPLY_SPEFFECT` | `integer` | 2002: act(2002, id): apply SpEffect id |
| `ADD_STAMINA` | `integer` | 1001: act(1001, amount): add (negative: spend) stamina (unconfirmed) |
| `SET_HAND_STYLE` | `integer` | 104: act(104, style): change how weapons are held (HAND_*) (unconfirmed) |
| `SET_THROW_ATTACK_INVALID` | `integer` | 102: act(102, n): disable throw attacks (SetThrowAtkInvalid) (unconfirmed) |
| `SET_WEAPON_ART_CANCEL_TYPE` | `integer` | 2011: act(2011, type): weapon art cancel type (unconfirmed) |
| `SET_AI_ACTION_STATE` | `integer` | 9103: act(9103, state): AI action state (unconfirmed) |
| `RESET_REQUEST` | `integer` | 9101: clear pending action requests (ResetRequest) (unconfirmed) |
| `LOCKON_TURN_LIMIT` | `string` | "ロックオン時システム旋回不可角度": act(name, a, b): angles the system won't turn to while locked on (unconfirmed) |
| `LOCKON_RELEASE_ANGLE_LOCK` | `string` | "ロックオン中角度固定解除": release the angle lock while locked on (unconfirmed) |
| `AUTO_TARGET_SET` | `string` | "自動捕捉対象設定": set the auto-aim target (unconfirmed) |
| `AUTO_TARGET_CLEAR` | `string` | "自動捕捉対象クリア": clear the auto-aim target (unconfirmed) |
| `AI_NOTIFY_ATTACK_TYPE` | `string` | "AI通知攻撃タイプ": act(name, type): attack type told to AI (unconfirmed) |
| `SET_4WAY_MOVE_THRESHOLD` | `string` | "4方向移動閾値設定": act(name, a, b): four-way movement thresholds (unconfirmed) |
| `DISABLE_AVARICE_DRAIN` | `string` | "貪欲者の印スリップダメージ無効": pause the Symbol of Avarice HP drain (done at bonfires) (unconfirmed) |
| `SET_IN_ITEM_ANIM` | `string` | "アイテムアニメ中か設定": mark the character as in an item animation (unconfirmed) |
| `DISABLE_ITEM_SWITCH` | `string` | "道具ID切り替え無効": disable switching the selected item (unconfirmed) |
| `SET_WEAPON_PARAM_HAND` | `string` | "武器パラメータ参照先": act(name, hand): which hand's weapon params attacks use (unconfirmed) |

### on_env

```lua
kindle.hks.on_env(id: integer|string, fn: fun(e: kindle.hks_call): (number|boolean|nil)) -> integer
```

Change the answer to env(id, ...): return a number/boolean (nil keeps the game's answer). Called very often, from the game's script threads: keep handlers quick.

**Parameters**

- `id` `integer` | `string`
- `fn` `fun(e: kindle.hks_call): (number|boolean|nil)`

**Returns**

- `handle` `integer`

### run_act

```lua
kindle.hks.run_act(c: kindle.charref|nil, id: integer|string, ...: any) -> integer
```

Have a character's script perform act(id, ...): it runs at that script's next call (within a frame). An optional last argument on_done(result, err) gets the result (nil, with a reason in err, if the character's script didn't run within ~5 seconds). Example: kindle.hks.run_act(nil, kindle.hks.act.SET_VARIABLE, "MoveSpeedLevelReal", 2)

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `id` `integer` | `string`
- `...` `any`: arguments, then optionally on_done

**Returns**

- `request` `integer`

### ask_env

```lua
kindle.hks.ask_env(c: kindle.charref|nil, id: integer|string, ...: any) -> integer
```

Ask a character's script env(id, ...); the last argument is fn(result) for the answer (nil if the character's script didn't run within ~5 seconds).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `id` `integer` | `string`
- `...` `any`: arguments, then fn(result)

**Returns**

- `request` `integer`

### eval

```lua
kindle.hks.eval(c: kindle.charref|nil, source: string, fn: fun(result: number|boolean|string|nil, err: string?)) -> integer
```

Run HKS source in a character's script state, with access to its globals and functions (the script's own Lua, not Kindle's). The first value it returns (number, boolean, string or nil) goes to fn(result, err); err is set if it didn't compile or raised an error. Before the first chunk runs, Kindle removes the io, os, package and debug libraries (and require, dofile, loadfile) from that script state: the game's scripts don't use them. Source text only: a precompiled chunk is refused, as loadstring and load refuse one inside it. Example: kindle.hks.eval(nil, "return env(1000)", function(hp) print(hp) end)

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `source` `string`
- `fn` `fun(result: number|boolean|string|nil, err: string?)`

**Returns**

- `request` `integer`

### get_variable

```lua
kindle.hks.get_variable(c: kindle.charref|nil, name: string, fn: fun(value: number|boolean|nil, err: string?)) -> integer
```

Read a behaviour graph variable (the values the animation graph runs on, e.g. "MoveSpeedLevelReal", "MoveType"); fn(value, err), err set for an unknown name.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `name` `string`
- `fn` `fun(value: number|boolean|nil, err: string?)`

**Returns**

- `request` `integer`

### set_variable

```lua
kindle.hks.set_variable(c: kindle.charref|nil, name: string, value: number|boolean, on_done: fun(result: nil, err: string?)?) -> integer
```

Set a behaviour graph variable. The script sets many of them every frame, so a value can be overwritten right away; to hold one, block the script's own writes with kindle.hks.on_act(kindle.hks.act.SET_VARIABLE, ...).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `name` `string`
- `value` `number` | `boolean`
- `on_done` `fun(result: nil, err: string?)?`

**Returns**

- `request` `integer`

### get_global

```lua
kindle.hks.get_global(c: kindle.charref|nil, name: string, fn: fun(value: number|boolean|string|nil, err: string?)) -> integer
```

Read a global of the character's script, e.g. one of its constants: fn(value, err). Tables and functions read as nil.

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `name` `string`
- `fn` `fun(value: number|boolean|string|nil, err: string?)`

**Returns**

- `request` `integer`

### set_global

```lua
kindle.hks.set_global(c: kindle.charref|nil, name: string, value: number|boolean|string|nil, on_done: fun(result: nil, err: string?)?) -> integer
```

Set a global of the character's script (nil removes it).

**Parameters**

- `c` [`kindle.charref`](types.md#kindlecharref) | `nil`: nil = the player
- `name` `string`
- `value` `number` | `boolean` | `string` | `nil`
- `on_done` `fun(result: nil, err: string?)?`

**Returns**

- `request` `integer`

### on_call

```lua
kindle.hks.on_call(name: string, fn: fun(e: kindle.hks_script_call): (number|boolean|string|nil)) -> integer
```

Handle kindle(name, ...) calls from edited behaviour scripts. Kindle adds a global `kindle` function to every script; a script calls e.g. `if kindle then kindle("dodged", 1) end`. Every handler for that name runs; the first number/boolean/string one returns is what the script gets back. Runs on the game's script threads: keep it quick.

**Parameters**

- `name` `string`
- `fn` `fun(e: kindle.hks_script_call): (number|boolean|string|nil)`

**Returns**

- `handle` `integer`

### on_act

```lua
kindle.hks.on_act(id: integer|string, fn: fun(e: kindle.hks_call): (boolean|nil)) -> integer
```

Watch or block act(id, ...): return false to block the action.

**Parameters**

- `id` `integer` | `string`
- `fn` `fun(e: kindle.hks_call): (boolean|nil)`

**Returns**

- `handle` `integer`

