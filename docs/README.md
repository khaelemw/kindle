# Kindle documentation

Kindle runs Lua mods inside Dark Souls III (version 1.15.2). A mod is a folder with a
`mod.toml` and Lua files; Kindle loads it when the game starts and reloads it the moment you
save a file.

All of these docs are also one page you can open in a browser and search:
`kindle-docs.html` here, installed next to Kindle as `docs.html`.

## Guides

- [Getting started](getting-started.md): install Kindle, make a mod, set up your editor,
  use the console.
- [Events and hooks](events.md): reacting to the game, and changing what it does.
- [Cookbook](cookbook.md): short, complete examples to copy from.
- [Mod tours](mod-tours.md): six mods built step by step from an empty folder.
- [Ways to do it](mod-options.md): each common goal, every way Kindle offers and when to pick each.
- [When a mod goes wrong](mod-errors.md): every message, its cause and the fix.

## Reference

- [API reference](reference/README.md): every module and function, generated from the same
  type stubs your editor uses for autocomplete.
- [Types](reference/types.md): the tables functions take and return, and the names the API
  accepts.
