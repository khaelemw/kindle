Kindle for Dark Souls III (version 1.15.2)
==========================================

Kindle runs mods written in Lua inside Dark Souls III. It comes with one mod, Bonfire Fate
(every few minutes, a random cause paired with a random effect), as an example. Its README is
in kindle\mods\bonfire-fate.

More mods, each in its own repo with a zip to unzip into Game\kindle\mods:

  Randomizer       items in new places (the game can always be finished) and enemies of other types
                   https://github.com/khaelemw/kindle-randomizer
  Chaos            every so often something random happens for a while
                   https://github.com/khaelemw/kindle-chaos
  Practice Tool    save a spot with its enemies and go back to it
                   https://github.com/khaelemw/kindle-practice-tool
  Run Timer        a speedrun clock with splits for bosses and bonfires
                   https://github.com/khaelemw/kindle-run-timer
  Photo Mode       freeze the scene and fly the camera around it
                   https://github.com/khaelemw/kindle-photo-mode
  Pickup Shuffle   the world's pickups trade places, from a seed
                   https://github.com/khaelemw/kindle-pickup-shuffle
  Co-op (beta)     play the whole game together
                   https://github.com/khaelemw/kindle-coop

Example mods for making your own: https://github.com/khaelemw/kindle-examples


INSTALLING

1. Open Kindle Launcher.exe in the unzipped folder and press Install. It finds Dark Souls III
   through Steam (or Browse... to its folder) and copies dinput8.dll and the kindle folder into
   the game's Game folder. To update, do the same with a newer zip: your settings and your own
   mods stay. A dinput8.dll of another mod that was there is kept as dinput8.dll.before-kindle.
2. Or copy them yourself (on Linux, say): in Steam, right-click Dark Souls III, then Manage,
   Browse local files, open the Game folder, and copy dinput8.dll and the kindle folder from the
   zip into it, so that you have Game\dinput8.dll and Game\kindle\kindle.dll.
3. Start the game from Steam as usual. On Linux (Proton), first set the game's launch options in
   Steam to:   WINEDLLOVERRIDES="dinput8=n,b" %command%
4. The title screen now has Mods. Open it, then the Mods tab, and switch on the mods you want.
   They are all off to begin with.

While Kindle runs, the game stays off FromSoftware's servers: if the game's Launch Setting
(Network settings) is Play Online, Kindle changes it to Play Offline and asks you to restart the
game once. The Co-op mod connects players itself.

Each time the game starts with mods on, Kindle copies your save to kindle\backups.


SETTINGS

Mods' settings: Mods, the Kindle tab, All mod settings. Or Kindle Launcher.exe, in the kindle
folder, while the game is closed (it can also start the game with other DLL mods).
Kindle's own settings: kindle\kindle.toml.


REMOVING

Delete Game\dinput8.dll and the Game\kindle folder (keep kindle\backups if you want the copies of
your saves).


PROBLEMS

Kindle's log is kindle\logs\kindle.log: include it when you report a problem. Kindle runs only
with Dark Souls III 1.15.2, and says so in its log on any other version.


MAKING MODS

kindle\docs.html is the guide and the full reference, in one page. "kindle\kindle.exe new my-mod"
makes a mod to start from, in kindle\mods\my-mod.


LICENSE

Kindle is MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE); what it includes is listed in
THIRD_PARTY_NOTICES.md. Kindle is not made or endorsed by FromSoftware or Bandai Namco.
