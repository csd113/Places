# Drop-in levels

This directory is for user-created and external levels. Every `*.json` level or
`*.zip` level pack placed here is discovered at startup and appears in the Level
Select menu next to the shipped demo. A level's `format_version` must be `2`; a
file with any other version is rejected by name at discovery. Nothing in this
directory is part of the game's packaged content: `tools/package.sh` only ships
committed `*.json`/`*.zip` level files from here, so a fresh checkout and a
packaged build both start with `Places Demo` alone.

To add a level, drop its `.json` (or `.zip`) file here, or use the in-game
Import action from `import/`. `Places Demo` demonstrates the current authoring
surface — interactive doors, a wall switch that drives a door and its label, a
cedar sauna door, solid glass panes and sauna steam emitters. See
`docs/MAP_AUTHORING_GUIDE.md` for the level format and `assets/levels/README.md`
for the shipped levels.
