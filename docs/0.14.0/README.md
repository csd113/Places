# Places 0.14.0

Unreleased development version.

- Add hosted application builds for macOS arm64 and Windows x64. Application ZIPs, gameplay maps and the illustrated guide remain separate assets.
- Add opt-in player and NPC health, a map-granted crowbar and a configurable player mannequin. Death on health-enabled maps uses the existing ragdoll solver and keeps the camera at the physical head. Existing maps keep health and the tool disabled by default.
- Add the encounter lab with authored spawn zones and an optional robot companion. Improve editor build and import handling, package dependency tracking, prop release, and NPC pursuit and obstruction recovery.
- On the Mac check of source `938c0b18`, the stock menu package was rejected for a mannequin asset identity mismatch. Separate gameplay checks used a compatible compact fixture.

[Changelog](../../CHANGELOG.md)
