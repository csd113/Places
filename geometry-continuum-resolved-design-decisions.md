# Geometry Continuum — Resolved Design Decisions

**Status:** Design decisions captured for future implementation  
**Project:** Geometry Continuum  
**Purpose:** Consolidate decisions resolved from the open-questions section of the design draft.

---

## 1. Final Game Title

**Geometry Continuum**

The game should stand on its own as a normal PC title while still beginning life on PocketCHIP.

The title should not be tied directly to Vitrallis branding or retro hardware.

---

## 2. World Scale and Liminal Proportions

Geometry Continuum uses a deliberately **mixed-scale** visual language.

Some architectural elements, props, furniture, fixtures, doors, corridors, and other objects may be:

- oversized,
- undersized,
- or correctly proportioned.

This inconsistency is intentional and should contribute to the feeling that a space is subtly wrong.

The engine and editor must **not automatically normalize or correct scale** simply because an object does not match expected real-world proportions.

Real-world units may still be used internally for consistency, but creators are free to deliberately violate proportion.

---

## 3. Player Spawn Position and Facing

There is **no global spawn-facing rule**.

Each level, experience, campaign, or generated space defines its own:

- spawn position,
- facing direction,
- and starting orientation.

This allows the opening composition to be tailored to the experience.

Geometry Continuum is expected to evolve beyond simple isolated levels into:

- standalone experiences,
- connected campaigns,
- procedurally generated spaces,
- bounded spaces,
- directly connected/inter-transitioning spaces,
- objective-driven experiences,
- minigames,
- and creator-defined tasks.

---

## 4. Movement and Look Feel

The default movement model should feel like **natural walking**.

Goals:

- grounded,
- human-scale,
- stable,
- believable,
- not floaty,
- not arcade-like.

The surreal or unsettling feeling should come primarily from the environment rather than from deliberately strange locomotion.

The game should remain walking-focused by default.

---

## 5. Input Support

Long-term input support should include both:

- keyboard-based look controls,
- mouse look.

Where practical, both should be available on all supported platforms.

PocketCHIP-style keyboard controls remain valid and supported, while desktop users may use conventional mouse look.

---

## 6. Visual Style, Texture Resolution, and Quality Tiers

Geometry Continuum should retain a deliberate **PS2-era visual identity** across all hardware tiers.

This is an **art-direction choice**, not simply a restriction to very low-resolution textures.

The style should come from choices such as:

- low-poly geometry,
- restrained texture detail,
- simple materials,
- controlled filtering,
- lighting style,
- UV treatment,
- limited effects,
- and overall scene composition.

### Texture Resolution Strategy

Texture resolution should be **flexible per texture** rather than globally fixed.

Different assets may use different resolutions depending on purpose.

The game should support **multiple quality tiers**.

### Hardware Profiles

A deliberate low-end profile should exist for:

- PocketCHIP,
- VideoCore IV Raspberry Pi-class hardware,
- similarly constrained devices.

Higher profiles should be available for progressively more capable hardware.

Higher settings should produce a cleaner or sharper version of the same visual identity, not turn the game into a modern photorealistic renderer.

---

## 7. Supported Texture Formats

Both **PNG** and **WebP** are first-class supported texture source formats.

### PNG

Use cases:

- maximum compatibility,
- simple editing workflows,
- lossless artwork,
- creator friendliness.

### WebP

Use cases:

- smaller asset packages,
- optional lossy compression,
- optional lossless compression,
- transparency support.

The engine should decode both formats into the same internal texture representation.

Creators should be free to choose either format.

---

## 8. Material System

Materials should be **simple but structured**.

A material should contain more than just a texture reference while remaining lightweight and easy to author.

Potential material metadata includes:

- base texture,
- UV scale/repeat,
- tint,
- filtering mode,
- simple surface properties,
- optional future audio/surface category information.

### Overlay System

Materials should support optional texture overlays for variations such as:

- cracks,
- broken wall sections,
- stains,
- water marks,
- grime,
- signs,
- damage,
- discoloration.

Overlays should be composited into a derived texture when an experience loads rather than requiring an entirely new full texture for every variation.

Identical base-material + overlay combinations should be cached and reused where possible.

The implementation should guard against excessive numbers of unique baked combinations, especially on low-memory hardware.

---

## 9. Fixture System

Fixtures should be implemented as **generic placeable architectural objects**, not as light-only objects.

Examples include:

- ceiling lights,
- wall lights,
- exit signs,
- vents,
- fans,
- alarms,
- mounted devices,
- architectural decorations,
- other wall/ceiling-mounted elements.

Fixtures may optionally emit light.

Fixture metadata may include:

- model,
- dimensions,
- mounting type,
- orientation rules,
- whether it emits light,
- light intensity,
- light colour/tint.

Example:

A green exit sign should be able to emit green-tinted light.

Mounting and emission behavior should be data-driven rather than implemented as separate hardcoded object classes.

---

## 10. Custom Prop and Asset Format

### Model Format

**GLTF/GLB** is the standard model format.

This matches the format already used by the project.

### Asset Packaging

Custom props should support both:

1. **Unpacked folder-based assets**
   - easy to create,
   - easy to inspect,
   - easy to edit during development.

2. **Packaged single-file assets**
   - easier to distribute,
   - less vulnerable to missing files,
   - safer for shared experiences.

Both forms should resolve to the same internal asset definition.

---

## 11. Prop Collision

Prop collision should use a **hybrid system**.

### Default

Use simple collision primitives such as:

- boxes,
- spheres,
- capsules.

These are preferred for:

- performance,
- simplicity,
- predictable behavior,
- PocketCHIP compatibility.

### Advanced Collision

Unusual props may supply a dedicated simplified low-poly collision mesh.

### Explicit Non-Goal

The engine should **not automatically use the full visible render mesh as collision**.

### Placement Rule

Collision must respect intentional creator placement.

The engine must not automatically correct:

- clipping,
- partial sinking,
- intersecting props,
- unusual vertical offsets.

---

## 12. Arbitrary User Meshes

Geometry Continuum should support arbitrary user meshes through a **hybrid advanced path**.

### Recommended Path

Structured level geometry remains the normal foundation:

- rooms,
- walls,
- floors,
- ceilings,
- openings,
- fixtures,
- props.

This path is preferred for:

- compatibility,
- lighting,
- collision,
- editing,
- validation,
- PocketCHIP performance.

### Advanced Path

Advanced creators may add arbitrary GLTF/GLB structural meshes where needed.

These are supplemental custom geometry rather than the default replacement for the structured level format.

---

## 13. Community Experience Import and Installation

Use a **hybrid community-content model**.

### Initial System

Provide:

- a strong local experience library,
- manual import of downloaded experience packages.

### Future System

Add an optional built-in online community browser with content such as:

- screenshots,
- descriptions,
- authors,
- versions,
- downloadable experiences.

Manually imported and online-downloaded content should appear together in the same library.

Community content should be treated as first-class:

- campaigns,
- experiences,
- levels,

rather than as second-class "mods."

---

## 14. Creator Suite and Level Editor

Geometry Continuum should use a **hybrid creator suite**.

### External Editor

The external editor remains the main serious authoring tool and may provide:

- 2D editing,
- 3D editing,
- inspectors,
- asset placement,
- lighting controls,
- validation,
- campaign creation,
- objective scripting,
- export/package tools.

### In-Game Editing

A lighter in-game creation/editing mode may be added later for simpler tasks such as:

- moving props,
- changing materials,
- adjusting lights,
- placing simple objectives,
- making quick environmental changes.

### Open Source Content

All official:

- levels,
- included assets,
- included models

are intended to be open source.

Creators should be able to inspect, learn from, modify, and reuse official content according to the project's eventual licensing terms.

---

## 15. Baked Lightmaps

Real baked lightmaps should be supported **across hardware tiers**, including low-memory systems such as PocketCHIP.

They should not be treated as a desktop-only feature.

Because Geometry Continuum uses intentionally simple:

- geometry,
- textures,
- materials,

baked lighting can remain practical on constrained hardware if carefully budgeted.

### Low-End Profiles

Use conservative:

- lightmap resolution,
- atlas size,
- memory budgets,
- number of unique baked surfaces.

### Higher-End Profiles

May use:

- higher-resolution lightmaps,
- larger lighting budgets,
- improved baked-light detail.

Baked lightmaps should complement the existing lightweight fixture/ambient lighting system rather than require a completely separate level design.

---

## 16. Audio System

Geometry Continuum should eventually provide a **full audio system**.

It should support:

- ambient sound beds,
- positional/3D environmental audio,
- music,
- scripted audio cues,
- UI sounds,
- minigame sounds,
- dialogue/voice,
- creator-authored audio behavior.

Individual experiences may choose to use as much or as little of the system as they want.

### Sound Captions / Subtitles

Meaningful non-ambient sounds should support optional captions/subtitles.

Examples:

- cat meows,
- doors slamming,
- alarms,
- glass breaking,
- distant knocks,
- mechanical sounds,
- other meaningful sound cues.

These captions should be toggleable in game settings.

This accessibility system should not be limited to spoken dialogue.

---

## 17. Official Experience Structure

Official Geometry Continuum content should use a **mixed structure**.

The project should support official content such as:

- standalone spaces,
- connected campaigns,
- experiences with direct transitions,
- procedurally generated spaces,
- objective-driven experiences,
- unusual experimental formats.

The game should not force all official content into a single level or campaign structure.

---

## 18. Prop Library

The official prop library should use a **hybrid structure**.

### Core Library

Maintain a reusable general-purpose prop collection.

### Themed Packs

Expand the library with themed sets such as:

- office,
- school,
- hotel,
- industrial,
- retail,
- residential,
- hospital,
- maintenance,
- and other environment-specific collections.

Official prop assets should remain open source and reusable by creators.

---

## 19. Room-Wide Brightness and Tint

Room-wide lighting should use a **hybrid approach**.

Normal illumination should come primarily from:

- fixtures,
- baked lighting,
- the normal lighting model.

Rooms may optionally apply ambient modifiers such as:

- brightness multiplier,
- colour/tint modifier.

These can be used for:

- mood,
- emergency lighting,
- surreal effects,
- unusual environmental conditions,
- deliberate creator intent.

Room-wide modifiers should complement rather than replace the normal lighting system.

---

## 20. Save and Progress System

Use a **hybrid persistence system**.

### Built-In Persistence

The engine should provide standard support for common needs such as:

- current campaign/experience,
- checkpoints,
- completed objectives,
- standard progress state.

### Creator-Defined Persistence

Advanced experiences may define additional persistent state for:

- tasks,
- minigames,
- switches,
- moved objects,
- custom objectives,
- inventory-like systems,
- campaign variables,
- other scripted logic.

The system should allow simple experiences to use persistence without scripting while still supporting complex campaigns later.

---

# Broader Direction

Geometry Continuum should remain:

- lightweight enough to run on PocketCHIP-class hardware,
- visually rooted in a PS2-era aesthetic,
- open and creator-friendly,
- flexible enough to grow into a full desktop game,
- capable of supporting both simple exploration and increasingly complex community experiences.

The architecture should favor graceful scaling rather than splitting the project into separate low-end and high-end games.

Features should degrade by:

- quality,
- resolution,
- memory budget,
- complexity,

rather than disappearing entirely whenever practical.

---

# Implementation Status

These items represent **resolved design direction**, not necessarily completed implementation.

They should be used as reference when planning future:

- engine work,
- editor work,
- asset schema work,
- content packaging,
- community features,
- accessibility features,
- rendering improvements,
- audio,
- campaign systems,
- persistence.

