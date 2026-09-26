# Drop-in levels

This directory is for user-created and external levels. Every `*.json` level or
`*.zip` level pack placed here is discovered at startup and appears in the Level
Select menu next to the shipped demo. Nothing in this directory is part of the
game's packaged content: `tools/package.sh` only ships committed `*.json`/`*.zip`
level files from here, so a fresh checkout and a packaged build both start with
`Places Demo` alone.

To add a level, drop its `.json` (or `.zip`) file here, or use the in-game
Import action from `import/`. See `assets/README.md` for the level format and
`assets/levels/README.md` for the shipped level.
