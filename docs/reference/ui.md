<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->

# kindle.ui

> Needs the `"ui"` permission in `mod.toml`.

On-screen messages, drawn even when the overlay is closed.

## Functions

- [`message`](#message): Show text in the game's own "OK" message dialog (up to ~1000 characters).
- [`add_menu_option`](#add_menu_option): Add an option to the game's own list menus (NPC and bonfire menus).
- [`menu`](#menu): From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen: a shop, reinforcing, levelling up ...): show a list in the game's own menu.
- [`shop`](#shop): From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open the game's shop over ShopLineupParam rows first..last (inclusive), e.g.
- [`open`](#open): From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open one of the game's own screens: level up, attune spells, the storage box, travel (bonfire warp), reinforce weapon, infuse weapon, repair equipment or estus allotment.
- [`set_menu_option_text`](#set_menu_option_text): Change a menu option's text (the handle add_menu_option returned).
- [`banner`](#banner): Big centred text on a dark band, like the game's own banners.
- [`place_banner`](#place_banner): The game's own banner across the screen, as a place's name shows on arriving, with this text (the game's font: no colon).
- [`hide_game_banner`](#hide_game_banner): Hides one of the game's banners (a name as ui.game_banner takes, or its type) for some seconds: a mod playing the bonfire lighting's animation hides the "BONFIRE LIT" it shows.
- [`prompt`](#prompt): An action prompt at the bottom of the screen, as the game's own ("Rest at bonfire"), until cleared (nil).
- [`status`](#status): A line across the top of the screen that stays until cleared (nil), for a state the player has to see the whole time (hidden while a menu is open or the HUD is off), with a progress bar under it if `progress` (0 to 1) is given.
- [`screen`](#screen): A screen of this mod's own: full-screen, in the game's menu style (as the Mods screen), worked with the controller, the keyboard or the mouse; the game's input is the screen's while it's open.
- [`layer`](#layer): A layer of this mod's drawn over the game: text, boxes, lines, bars, circles and pictures (PNG files in the mod's folder, up to 4096 pixels a side, and the game's own icons) on a 1920 x 1080 canvas scaled to the window (a wider window keeps it centred), or pinned to a point in the world with `at` (a marker over a bonfire, a bar over an enemy) and drawn where the camera sees it.
- [`confirm`](#confirm): A question with two answers, on a screen of this mod's (kindle.ui.screen): the answer picked runs its function and closes it; backing out is no.
- [`choose`](#choose): One of a list, on a screen of this mod's (kindle.ui.screen).
- [`mod_tab`](#mod_tab): This mod's menu of buttons, opened from the game's Mods screen (Esc -> Mods, and at the title): its Mod menus tab lists each mod with a menu or settings, and picking one opens that mod's menu over it, these buttons and the mod's settings (rows past what the screen shows go on a More row).
- [`title_entry`](#title_entry): An entry of this mod's on the title's main menu, after Mods, opening this mod's menu (ui.mod_tab and its settings), or, given `open`, a screen of the mod's own over the menu (a kindle.ui.screen spec, or a function run when it's picked, which can return one).
- [`character_rows`](#character_rows): This mod's rows on the character creation screen, under the game's own (two places shared by all mods).
- [`character_choices`](#character_choices): What this mod's creation screen rows hold now, for the character being made: {key = value}, a switch true or false, a text row its text ("" when nothing was typed).
- [`reset_character_choices`](#reset_character_choices): This mod's creation screen rows back to their defaults (once it has taken the choices for a character, so the next one starts afresh).
- [`copy`](#copy): Puts text on the clipboard (1024 characters at most), as for a code to send someone; false if it couldn't.
- [`toast`](#toast): A small notification in the top-right corner (up to 6 stack).
- [`set_hud`](#set_hud): Change the HUD setting (Settings > HUD), as the settings menu does: menus keep working.
- [`hud`](#hud): The current HUD setting.
- [`game_banner`](#game_banner): One of the game's own banners, drawn by the game exactly like its own, by name or type.

### message

```lua
kindle.ui.message(text: string)
```

Show text in the game's own "OK" message dialog (up to ~1000 characters). Does nothing while some game menus are open.

**Parameters**

- `text` `string`

### add_menu_option

```lua
kindle.ui.add_menu_option(opts: {text: string, on_select?: fun(), screen?: kindle.screen_spec|fun(): kindle.screen_spec?, where?: "any"|"bonfire"|"npc"|fun(menu: kindle.menu): boolean}) -> integer
```

Add an option to the game's own list menus (NPC and bonfire menus). Picking it closes the menu (as if backed out) and runs on_select, or opens `screen` (a kindle.ui.screen spec, or a function returning one, built when it's picked).

**Parameters**

- `opts` `{text: string, on_select?: fun(), screen?: kindle.screen_spec|fun(): kindle.screen_spec?, where?: "any"|"bonfire"|"npc"|fun(menu: kindle.menu): boolean}`

**Returns**

- `handle` `integer`: kindle.cancel(handle) removes it

### menu

```lua
kindle.ui.menu(entries: {text: string, on_select?: fun()}[])
```

From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen: a shop, reinforcing, levelling up ...): show a list in the game's own menu. An entry's on_select can open another menu or a shop; one that opens nothing (or backing out) returns to the bonfire's or NPC's own menu. Up to 64 entries. Picking an entry closes and re-opens the list (the game's menus always close on a pick).

**Parameters**

- `entries` `{text: string, on_select?: fun()}[]`

### shop

```lua
kindle.ui.shop(first: integer, last: integer)
```

From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open the game's shop over ShopLineupParam rows first..last (inclusive), e.g. the Shrine Handmaid's 110000-119899. Closing it returns to the menu it was opened from.

**Parameters**

- `first` `integer`
- `last` `integer`

### open

```lua
kindle.ui.open(screen: kindle.screen)
```

From a menu option's on_select (bonfire menus, or NPCs whose menu opens a screen): open one of the game's own screens: level up, attune spells, the storage box, travel (bonfire warp), reinforce weapon, infuse weapon, repair equipment or estus allotment. Closing it returns to the menu it was opened from. `kindle.ui.add_menu_option{text = "Level Up", on_select = function() kindle.ui.open("level_up") end}`

**Parameters**

- `screen` [`kindle.screen`](types.md#kindlescreen)

### set_menu_option_text

```lua
kindle.ui.set_menu_option_text(handle: integer, text: string)
```

Change a menu option's text (the handle add_menu_option returned). Menus built after this show the new text: set it from the option's `where` function to show state, e.g. a price only until something is bought.

**Parameters**

- `handle` `integer`
- `text` `string`

### banner

```lua
kindle.ui.banner(message: string, opts: {seconds?: number, color?: kindle.color}?)
```

Big centred text on a dark band, like the game's own banners.

**Parameters**

- `message` `string`
- `opts` `{seconds?: number, color?: kindle.color}?`: defaults: 3 seconds, gold

### place_banner

```lua
kindle.ui.place_banner(text: string)
```

The game's own banner across the screen, as a place's name shows on arriving, with this text (the game's font: no colon).

**Parameters**

- `text` `string`

### hide_game_banner

```lua
kindle.ui.hide_game_banner(kind: string|integer, seconds: number)
```

Hides one of the game's banners (a name as ui.game_banner takes, or its type) for some seconds: a mod playing the bonfire lighting's animation hides the "BONFIRE LIT" it shows.

**Parameters**

- `kind` `string` | `integer`
- `seconds` `number`

### prompt

```lua
kindle.ui.prompt(text: string?)
```

An action prompt at the bottom of the screen, as the game's own ("Rest at bonfire"), until cleared (nil). Each mod has its own; when two have one, the one set last shows.

**Parameters**

- `text` `string?`

### status

```lua
kindle.ui.status(text: string?, progress: number?)
```

A line across the top of the screen that stays until cleared (nil), for a state the player has to see the whole time (hidden while a menu is open or the HUD is off), with a progress bar under it if `progress` (0 to 1) is given. Each mod has its own line: another mod's status shows above or below it (in the order they were first set), and setting it again changes this mod's line only. It goes when the mod unloads or stops.

**Parameters**

- `text` `string?`
- `progress` `number?`

### screen

```lua
kindle.ui.screen(spec: kindle.screen_spec) -> kindle.screen
```

A screen of this mod's own: full-screen, in the game's menu style (as the Mods screen), worked with the controller, the keyboard or the mouse; the game's input is the screen's while it's open. Rows are buttons (on_select; `value` shown on the right; `screen` opens a sub-screen that Back returns from), switches (`toggle`), choices (`choices` + `value`, 1-based or the text), sliders (`slider = {min, max, step}` + `value`), lines of text (`info = true`: greyed, or with a `value` shown on the right, as a stat; not selectable), text the player types (`input`, with a keyboard) and headings (`header`). A row's `help` shows at the bottom while it's selected; `disabled` greys it and stops it changing. A switch's, choice's or slider's on_change gets the new value (a choice its index and text), and the row keeps it. Up to 200 rows (the list scrolls). Screens opened from a screen stack: Back closes the top one (unless `closable = false`), and on_close runs. `kindle.ui.screen{title = "Hard Mode", rows = {{text = "Enemy damage", slider = {min = 1, max = 3, step = 0.5}, value = 1.5, on_change = function(v) damage = v end}}}` Tabs instead of rows put the rows on pages the player turns with LB/RB, Q/E or a click, as the game's own menus: `kindle.ui.screen{title = "Options", tabs = {{title = "Enemies", rows = {...}}, {title = "Player", rows = {...}}}}` (200 rows across them).

**Parameters**

- `spec` [`kindle.screen_spec`](types.md#kindlescreen_spec)

**Returns**

- [`kindle.screen`](types.md#kindlescreen)

### layer

```lua
kindle.ui.layer(elements: kindle.layer_element[]?) -> kindle.layer
```

A layer of this mod's drawn over the game: text, boxes, lines, bars, circles and pictures (PNG files in the mod's folder, up to 4096 pixels a side, and the game's own icons) on a 1920 x 1080 canvas scaled to the window (a wider window keeps it centred), or pinned to a point in the world with `at` (a marker over a bonfire, a bar over an enemy) and drawn where the camera sees it. A layer stays until it's changed: set it when its numbers change, or every frame for something that moves. Layers are drawn under Kindle's menus and screens, in the order they were added, and hidden on loading screens; they go when the mod unloads. Up to 500 elements. `local hud = kindle.ui.layer{{text = "Souls: 0", x = 1880, y = 40, align = "right"}}` `hud:set{{bar = {w = 300, h = 10, value = 0.5}, x = 810, y = 1000, color = "#c08030"}}` `kindle.ui.layer{{image = "art/crest.png", x = 40, y = 40, w = 96, h = 96}}` `local _, _, _, icon = kindle.items.info("Ember"); kindle.ui.layer{{image = icon, x = 40, y = 40, w = 64, h = 64}}`

**Parameters**

- `elements` [`kindle.layer_element[]?`](types.md#kindlelayer_element)

**Returns**

- [`kindle.layer`](types.md#kindlelayer)

### confirm

```lua
kindle.ui.confirm(opts: {title?: string, text?: string, yes?: string, no?: string, on_yes?: fun(), on_no?: fun()}) -> kindle.screen
```

A question with two answers, on a screen of this mod's (kindle.ui.screen): the answer picked runs its function and closes it; backing out is no. `kindle.ui.confirm{title = "Reset?", text = "Your run's numbers go back to 0.", on_yes = reset}`

**Parameters**

- `opts` `{title?: string, text?: string, yes?: string, no?: string, on_yes?: fun(), on_no?: fun()}`

**Returns**

- [`kindle.screen`](types.md#kindlescreen)

### choose

```lua
kindle.ui.choose(opts: {title?: string, text?: string, options: (string|{text: string, help?: string, value?: any, disabled?: boolean})[], on_pick?: fun(index: integer, option: any), on_cancel?: fun()}) -> kindle.screen
```

One of a list, on a screen of this mod's (kindle.ui.screen). An option is a text, or {text, help, value, disabled} (value shown on the right). Picking one closes it and runs on_pick(index, option); backing out runs on_cancel. `kindle.ui.choose{title = "Difficulty", options = {"Easy", "Normal", "Hard"}, on_pick = function(i, name) level = i end}`

**Parameters**

- `opts` `{title?: string, text?: string, options: (string|{text: string, help?: string, value?: any, disabled?: boolean})[], on_pick?: fun(index: integer, option: any), on_cancel?: fun()}`

**Returns**

- [`kindle.screen`](types.md#kindlescreen)

### mod_tab

```lua
kindle.ui.mod_tab(title: string, buttons: {label: string, help?: string, paste?: boolean, info?: boolean, on_press?: fun(pasted: string?)}[])
```

This mod's menu of buttons, opened from the game's Mods screen (Esc -> Mods, and at the title): its Mod menus tab lists each mod with a menu or settings, and picking one opens that mod's menu over it, these buttons and the mod's settings (rows past what the screen shows go on a More row). Each button: a label (48 characters), its help line, and on_press, run at Kindle's next frame. A paste button's on_press gets the clipboard's text, read as the player pressed it (a mod reads the clipboard no other way). A row with info = true is a line of information, shown greyed (no on_press). 1 to 16 rows; replaces this mod's earlier tab (the same rows again change nothing; this mod's menu, open in play, closes and opens again with the new rows, at the page it was on); gone when the mod unloads.

**Parameters**

- `title` `string`: 1 to 24 characters
- `buttons` `{label: string, help?: string, paste?: boolean, info?: boolean, on_press?: fun(pasted: string?)}[]`

### title_entry

```lua
kindle.ui.title_entry(label: string?, help: string?, open: kindle.screen_spec|fun():?)
```

An entry of this mod's on the title's main menu, after Mods, opening this mod's menu (ui.mod_tab and its settings), or, given `open`, a screen of the mod's own over the menu (a kindle.ui.screen spec, or a function run when it's picked, which can return one). Shown the next time the main menu is built; two mods' entries at most. nil removes it; gone when the mod unloads.

**Parameters**

- `label` `string?`: 1 to 24 characters
- `help` `string?`: at most 120 characters
- `open` `kindle.screen_spec|fun():?`: kindle.screen_spec?

### character_rows

```lua
kindle.ui.character_rows(rows: {label: string, help?: string, page?: {title: string, icon?: "candelabra"|"bell"|"skulls"|"equipment"|"shrine", switches: {key: string, label: string, help?: string, default?: boolean}[]}[], text?: {key: string, empty?: string}}[])
```

This mod's rows on the character creation screen, under the game's own (two places shared by all mods). A page row opens the System screen with the page's tabs of switches (1 to 4 tabs, each 1 to 24 switches); a text row opens the game's text box, and a toast shows what was typed (`empty` when nothing). Read what the player chose with ui.character_choices when the mod sees the new character. Replaces this mod's rows; gone when the mod unloads. Example: ui.character_rows{{label = "Hard mode", help = "...", page = {{title = "Hard mode", icon = "skulls", switches = {{key = "on", label = "On", help = "...", default = false}}}}}}

**Parameters**

- `rows` `{label: string, help?: string, page?: {title: string, icon?: "candelabra"|"bell"|"skulls"|"equipment"|"shrine", switches: {key: string, label: string, help?: string, default?: boolean}[]}[], text?: {key: string, empty?: string}}[]`

### character_choices

```lua
kindle.ui.character_choices() -> table<string, boolean|string>
```

What this mod's creation screen rows hold now, for the character being made: {key = value}, a switch true or false, a text row its text ("" when nothing was typed). Defaults where the player changed nothing.

**Returns**

- `table<string, boolean|string>`

### reset_character_choices

```lua
kindle.ui.reset_character_choices()
```

This mod's creation screen rows back to their defaults (once it has taken the choices for a character, so the next one starts afresh).

### copy

```lua
kindle.ui.copy(text: string) -> boolean
```

Puts text on the clipboard (1024 characters at most), as for a code to send someone; false if it couldn't.

**Parameters**

- `text` `string`

**Returns**

- `boolean`

### toast

```lua
kindle.ui.toast(message: string, opts: {seconds?: number, color?: kindle.color}?)
```

A small notification in the top-right corner (up to 6 stack).

**Parameters**

- `message` `string`
- `opts` `{seconds?: number, color?: kindle.color}?`: defaults: 4 seconds, gold

### set_hud

```lua
kindle.ui.set_hud(mode: "off"|"on"|"auto")
```

Change the HUD setting (Settings > HUD), as the settings menu does: menus keep working. The player's own setting comes back when the mod unloads.

**Parameters**

- `mode` `"off"` | `"on"` | `"auto"`: auto shows the HUD when something changes

### hud

```lua
kindle.ui.hud() -> "off"|"on"|"auto"
```

The current HUD setting.

**Returns**

- `"off"` | `"on"` | `"auto"`

### game_banner

```lua
kindle.ui.game_banner(banner: kindle.banner|integer)
```

One of the game's own banners, drawn by the game exactly like its own, by name or type. A mod's own banners don't fire the banner / boss_defeated events.

**Parameters**

- `banner` [`kindle.banner`](types.md#kindlebanner) | `integer`: a name, or a type 1-22 (3 and 8-11 show nothing)

