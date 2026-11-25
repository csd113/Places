# Places documentation

Places started as a small experiment in making spaces worth walking through. The earliest builds were about getting coloured lights, damaged walls and changes in floor height to look right. Later versions added the means to build and share those spaces without editing the game itself: a native editor, a map compiler and a standalone player. The setting is still the point, whether it is a quiet apartment, a fluorescent office after hours or a pool with nobody in it.

## Where to start

- **Building a map?** Start with the illustrated beginner’s guide and editable example linked from [0.13.1](0.13.1/README.md). The [editor reference](EDITOR.md) covers project settings, saving and exporting.
- **Installing Places or importing a map?** See [Installation and file locations](INSTALLATION.md). Editable projects belong in the editor; compiled `.placesmap` files are what the player loads.
- **Following development?** The [changelog](../CHANGELOG.md) records the larger changes. The summaries below describe the version they came from, not what is necessarily current. Several early milestones were never published as downloadable releases.

## How Places changed

**From rooms to environments.** The Residence, Quiet Apartments and After the Leak were early attempts at making ordinary interiors feel distinct. The Office and Pool later pushed the same geometry in opposite directions: raised floors and stairs on one side, a recessed basin on the other. The two eventually met in Places Demo, linked by stairs and an elevated window.

**From a bundled demo to independent maps.** The move from OpenGL to wgpu came alongside a bigger change: geometry, collision, navigation and baked lighting could be prepared before the game loaded a map. By 0.9.0, Places could start without the demo maps installed. The 0.12.0 native editor then made it possible to build multi-map projects, connect them and give them their own Luau scripts.

**From spaces to encounters.** The Quiet Collection gave otherwise separate environments a shared hub. The Long Way Home took a different approach: a route through several spaces with a Feltling guide, a Tall Guest pursuit and a duck to bring home. In 0.14.0 development, individual maps can opt into health and grant the player a crowbar. Older walking maps don’t inherit either feature by default.

## Version summaries

| Version | What changed |
| --- | --- |
| [0.14.0](0.14.0/README.md) · in development | Opt-in health, a map-granted crowbar, the customizable mannequin and an encounter lab. |
| [0.13.1](0.13.1/README.md) | The Long Way Home, Feltling and Tall Guest authoring, checkpoints and the illustrated editor guide. |
| [0.13.0](0.13.0/README.md) | A live main menu, per-level builds, fixed-step movement and articulated ragdolls. |
| [0.12.0](0.12.0/README.md) | The native editor, multi-map projects, Luau and the connected Quiet Collection. |
| [0.11.0](0.11.0/README.md) | Spatial audio and material footsteps; clearer separation between engine, game and compiler. |
| [0.10.0](0.10.0/README.md) | Props you can carry and throw, a flashlight, working bowling resets and a compiler GUI. |
| [0.9.0](0.9.0/README.md) | Cosmic Lanes & Arcade, a multi-floor Office, HDR lighting and startup without installed maps. |
| [0.8.0](0.8.0/README.md) | SDL3 and wgpu, offline map compilation, swimming, ladders and Lantern Hollow. |
| [0.7.0](0.7.0/README.md) | The final desktop OpenGL baseline, with baked lightmaps, graphics presets and saved controls. |
| [0.6.0](0.6.0/README.md) | Places Demo connects the Office and Pool; packaging finds assets beside the executable. |
| [0.5.3](0.5.3/README.md) | Light stops leaking through solid walls without blocking real doors and windows. |
| [0.5.2](0.5.2/README.md) | The first Office and Pool showcases, with furnished rooms and solid prop collision. |
| [0.5.1](0.5.1/README.md) | PNG material catalogs replace hardcoded surface art. |
| [0.5.0](0.5.0/README.md) | Raised and recessed floors, transition faces and gabled ceilings. |
| [0.4.0](0.4.0/README.md) | An asset catalog, Office and Pool themes, and shared organization for entities such as Spooner-Man. |
| [0.3.2](0.3.2/README.md) | WASD movement and arrow-key look become the defaults. |
| [0.3.1](0.3.1/README.md) | Places moves into its own repository and workspace. |
| [0.3.0](0.3.0/README.md) | Editable lit decals and a fix for overlapping water-damaged wall surfaces. |
| [0.2.0](0.2.0/README.md) | Mixed-colour fixture lighting and genuinely dark unlit rooms. |
| [0.1.0](0.1.0/README.md) | The first App Manager-ready milestone and three early interiors. |

The [changelog](../CHANGELOG.md) also records the earlier prototypes and the separate PocketCHIP edition, which do not follow this version sequence.
