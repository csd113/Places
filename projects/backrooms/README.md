# The Backrooms

One editable project and one connected map, the_backrooms. Open this folder
with the native editor. Room geometry, walls, stairs, columns, half-walls,
fixtures and every prop remain ordinary current-format source data. No teleport,
quest, actor, custom rendering path or alternate map format is used.

The 140 x 140 m lower envelope contains two useful floors, at 0 and 3.55 m,
with 3.4 m clear ceilings. Explicit shafts contain four ordinary stair flights.
Source floor tiles follow shared 31.5 m chart boundaries to bound preparation
cost; architecture and stable instance prefixes identify the authored zones.

| Zone | Composition | Original Library identity |
|---|---|---|
| A Chevron Weave | Staggered wall masses, central framed connection, wrapping passages | libfile_21ffc8d40b008191981ff8d7543ea448 |
| B Offset Colonnade | Pale dominant column, offset cross-bays, right corridor | libfile_b0c6b281f97081919ab0f8f2c02e63a2 |
| C Pillar Field | Largest continuous ochre chamber, regular chunky columns | libfile_14eb906b38ec8191aea3b031788036a2 |
| D Half-wall Reception | Two cores, capped partitions and figure-eight circulation | libfile_80579f061268819182dc8506c6cf672e |
| E Chair Gallery | Sparse green-seat ladder-back chairs and parallel return | libfile_d8221c161f148191b7069b5fb1eb7bc2 |
| F Stool Court | Small court, isolated green stool, paired panels and vent | libfile_84d15a2a0af88191984ab2f589cd7d08 |
| G Recess Hall | Pier-divided shallow recesses and a high passage sightline | libfile_6e578697e21081918269f01f2a27ab55 |
| H Office Gallery | Furniture bay, CRTs, cabinet block and deep right return | libfile_92c8837e9df08191871dd02326b47084 |

A cloud visual designer inspected all eight originals and authored the blueprint.
Windows implementation consumed that blueprint; Windows did not view the
original pixels. Native A-H views are reviewed against the originals separately.

Lower ring A-B-C-D-A and upper ring E-F-G-H-E have separated A-E, B-G,
C-H and D-F stairs, plus local returns and offset upper circulation. A decorative
high aperture is a sightline, not a required climbing route.

Prop anomaly audit includes the eight real GLB troffers. Separate generated
fluorescent fixtures are catalog lights. Explicit IDs preserve ordinary versus
anomalous transforms. Deliberate placement intersections require individual
review; do not bulk move or normalize them.

tools/levels/build_backrooms.py is the explicit bootstrap and focused model
source. It overwrites this project's saved scene, so normal editor changes take
ownership and must not be regenerated accidentally. Its required arguments keep
the destination and source wallpaper explicit. assets/references/backrooms
retains the generated wallpaper master and prompt. Existing content is preserved.

Validation, measured area, topology, native evidence and limitations belong in
the one canonical docs/reports/release-0.13.0.md report.
