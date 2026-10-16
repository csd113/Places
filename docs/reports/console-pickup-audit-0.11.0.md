# Developer console pickup audit — 0.11.0

Audit date: October 16, 2026. Scope: the complete installed `assets/catalog.json`, its 370 model identities (366 unique GLBs; five streamer aliases share one file), and reusable physical overrides in `assets/gameplay.json`.

## Contract and outcome

The explicit allowlist is `src/game/pickup_catalog.rs::ITEMS`: 58 reviewed stable catalog IDs and their display names, in lexicographic ID order. `find(id)` resolves an exact member. Command listing and give execution must use these same entries. Names, category, `solid`, file presence, and an existing physics profile do not grant eligibility. Aliases belong to command parsing and must resolve to an approved exact ID.

The audit parsed all 370 catalog models at their scene-node rest transforms with `tools/props/glb.py::read_glb`, measured their actual bounds, inspected rig/clip/morph declarations, and reviewed their documented use. Of the 58 approved choices, 57 are rigid; the lit cake is the one explicit animated-item exception, retaining its embedded candle clip. All approved actual extents are positive, at least 1 mm and at most 1.5 m per axis. All fit the existing pickup-reach calculation; the largest conservative model-bounds radius is 0.710141 m (the Beach crate).

Every approved entry has a strict-valid dynamic profile with `pickup: true`. This adds 42 profiles and preserves all 23 pre-existing gameplay metadata entries exactly by JSON value, including ground friction, existing bowling/cup/duck/drum tuning, and the fixed gutter compound. The gutter is deliberately excluded from the pickup allowlist. This audit changes no catalog visual entries, GLBs, PNGs, maps or baked packages.

## Complete coverage

| Catalog population | Count | Decision |
| --- | ---: | --- |
| Reviewed pickup choices | 58 | Explicit allowlist below |
| Other model entries | 312 | Exact-ID exclusion ledger below |
| `decal` entries | 18 | Non-model assets; cannot be given as a pickup |
| `light` entries | 6 | Non-model assets; cannot be given as a pickup |
| `material` entries | 100 | Non-model assets; cannot be given as a pickup |
| `texture` entries | 99 | Non-model assets; cannot be given as a pickup |
| **Total catalog entries** | **593** | **Every entry accounted for** |

Non-model exclusions are all entries of the listed catalog types, not a partial sample. They comprise 100 material definitions, 99 PNG textures, six fixture-face light assets and 18 decals. They have no independent physical model. Trees/buildings/roads are never admitted because their dimensions happen to pass a generic bound.

## Reviewed pickup ledger

Sizes below are measured occupied GLB spans in X × Y × Z metres, not guesses from catalog dimensions. Colliders are contact approximations; source artwork and model geometry remain intact. Explicit collider dimensions and coefficients are recorded in the sidecar. `new` means a profile was added by this audit; `existing` means its previous values were retained.

| Stable ID | Display name | Measured size (m) | Collider | Mass (kg) | Profile |
| --- | --- | --- | --- | ---: | --- |
| `arcade:bowling_ball_black` | Bowling Ball Black | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_burgundy` | Bowling Ball Burgundy | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_cobalt` | Bowling Ball Cobalt | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_green` | Bowling Ball Green | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_marbled` | Bowling Ball Marbled | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_purple` | Bowling Ball Purple | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_red` | Bowling Ball Red | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_pin` | Bowling Pin | 0.120000 × 0.380000 × 0.120000 | existing compound hulls | 1.5 | existing |
| `arcade:cat_plush` | Tuxedo Cat Plush | 0.192400 × 0.360000 × 0.157609 | cuboid | 0.12 | new |
| `arcade:litter_bottle_fallen` | Litter Bottle Fallen | 0.096000 × 0.096000 × 0.280000 | cuboid | 0.35 | new |
| `arcade:litter_can` | Litter Can | 0.066000 × 0.120000 × 0.065937 | cylinder | 0.018 | new |
| `arcade:litter_cup` | Litter Cup | 0.088871 × 0.140000 × 0.089991 | cylinder | 0.025 | existing |
| `arcade:litter_paper` | Litter Paper | 0.210000 × 0.009900 × 0.148000 | cuboid | 0.008 | new |
| `arcade:litter_wrapper` | Litter Wrapper | 0.120000 × 0.010200 × 0.085000 | cuboid | 0.004 | new |
| `arcade:party_balloon_weight` | Party Balloon Weight | 0.070000 × 0.054418 × 0.070000 | cylinder | 0.15 | new |
| `arcade:party_cake_lit` | Party Cake Lit | 0.360000 × 0.236000 × 0.360000 | compound plate/body cylinders | 1.4 | new |
| `arcade:party_cake_unlit` | Party Cake Unlit | 0.360000 × 0.218000 × 0.360000 | compound plate/body cylinders | 1.4 | new |
| `arcade:party_cup_balloons` | Party Cup Balloons | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_confetti` | Party Cup Confetti | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_geometric` | Party Cup Geometric | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_stars` | Party Cup Stars | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_gift_arcade` | Party Gift Arcade | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_confetti` | Party Gift Confetti | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_green` | Party Gift Green | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_red` | Party Gift Red | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_stars` | Party Gift Stars | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_stripes` | Party Gift Stripes | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_hat_confetti` | Party Hat Confetti | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_dots` | Party Hat Dots | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_shapes` | Party Hat Shapes | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_stars` | Party Hat Stars | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_stripes` | Party Hat Stripes | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_plate_balloons` | Party Plate Balloons | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_border` | Party Plate Border | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_confetti` | Party Plate Confetti | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_stars` | Party Plate Stars | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:soda_bottle` | Soda Bottle | 0.096000 × 0.280000 × 0.096000 | cylinder | 0.35 | existing |
| `beach:crate` | Beach Crate | 0.820000 × 0.820000 × 0.820000 | cuboid | 12 | new |
| `core:cardboard_box` | Cardboard Box | 0.500000 × 0.500000 × 0.500000 | cuboid | 1.5 | new |
| `core:crate` | Crate | 0.600000 × 0.600000 × 0.600000 | cuboid | 8 | new |
| `core:plant` | Plant | 0.412423 × 0.985000 × 0.418366 | cuboid | 4 | new |
| `core:rubber_duck` | Rubber Duck | 0.098000 × 0.120000 × 0.145000 | cuboid | 0.065 | existing |
| `core:tv` | Television | 1.100000 × 0.700000 × 0.100000 | cuboid | 12 | new |
| `core:washer_drum` | Washer Drum | 0.420000 × 0.300000 × 0.420000 | cuboid | 6.4 | existing |
| `home:book_stack` | Two Stacked Books | 0.250000 × 0.055000 × 0.180000 | cuboid | 1.1 | new |
| `home:bowl` | Cereal Bowl | 0.150000 × 0.065000 × 0.150000 | cylinder | 0.25 | new |
| `home:crt_tv` | CRT Television | 0.550000 × 0.480000 × 0.460000 | cuboid | 9 | new |
| `home:cushion` | Checked Throw Cushion | 0.330000 × 0.320000 × 0.115000 | cuboid | 0.25 | new |
| `home:fork` | Table Fork | 0.026000 × 0.012000 × 0.196000 | cuboid | 0.045 | new |
| `home:kettle` | Home Kettle | 0.222500 × 0.201000 × 0.162000 | cuboid | 0.9 | new |
| `home:knife` | Table Knife | 0.021000 × 0.016000 × 0.215000 | cuboid | 0.065 | new |
| `home:landscape_frame` | Small Framed Landscape | 0.450000 × 0.500000 × 0.030000 | cuboid | 0.6 | new |
| `home:mug` | Ceramic Coffee Mug | 0.117000 × 0.090000 × 0.078000 | cuboid | 0.32 | new |
| `home:plant_table` | Small Potted Plant | 0.130000 × 0.280484 × 0.130000 | cuboid | 0.45 | new |
| `home:plate` | Dinner Plate | 0.220000 × 0.022000 × 0.220000 | cylinder | 0.35 | new |
| `home:spoon` | Table Spoon | 0.036000 × 0.020000 × 0.185000 | cuboid | 0.05 | new |
| `home:toaster` | Home Toaster | 0.252000 × 0.161000 × 0.168000 | cuboid | 1.2 | new |
| `pool:pool_potted_plant` | Pool Potted Plant | 0.588164 × 0.860127 × 0.561670 | cuboid | 8 | new |

The list includes every four party-cup prints, all four party-plate prints, five hats, six gifts, both cakes, seven bowling-ball finishes, the bowling pin, upright and fallen bottles, litter cup/can/paper/wrapper, tableware/cutlery, household loose objects, three portable potted plants, two wooden crates plus the cardboard box, the washer drum, rubber duck, and cat plush. The two-book stack stays one rigid assembly. Cushion/plush deformation, liquid motion in the mug, and leaf deformation are outside the existing rigid-item approximation.

Masses are deliberate gameplay approximations: 4–65 g for lightweight litter/cutlery, 25 g party paper, 0.7 kg boxed gifts, 1.4 kg cakes, 5 kg bowling balls, and up to 12 kg crates/televisions. Bow and flame details do not inflate their load-bearing colliders. For the horizontal fallen bottle, a cuboid follows its horizontal measured envelope instead of incorrectly using the local-Y upright cylinder.

Hollow/light items use explicit source-volume displacement where needed: paper 8 cm³, wrapper 4 cm³, empty can 20 cm³, paper plate 30 cm³ and paper hat 40 cm³. These values avoid treating economical solid contact envelopes as sealed displacement. The retained duck/drum water tuning remains intact. Every primitive profile was checked against density ≥10 kg/m³ and the existing 120 Hz buoyancy stiffness bound; all pass. Party hats use bounded nondegenerate nine-point cone hulls, and cake/gift compounds use only one or two ordinary primitives.

## Lit-cake dependency review

`arcade:party_cake_lit` retains the accepted GLB unchanged: 1,164 triangles, eight root scene nodes/meshes, no skin and no morph targets. Node 0 contains the cake, plate and seven candle shafts. Nodes 1–7 (`candle_flame_00` through `candle_flame_06`) contain the tiny separate flame geometry, already embedded inside this GLB. There is no separate catalog flame/candle model to create or spawn.

Its `candle_flicker` clip has seven LINEAR scale channels, targeting only the flame nodes. It never translates the cake body. Flame material slot 2, `party_tiny_candle_flame.002`, retains embedded emissive factor `[0.800000012, 0.439999998, 0.071999997]`. The cake spans `[-0.180000007, 0, -0.180000007]` to `[0.180000007, 0.235999994, 0.180000007]`; the unlit export stops at Y=0.217999995. `rigid_runtime_model_bounds` alone cannot accept the lit model because it has a clip. Runtime integration must explicitly use the supported animated-item/character transform path for this entry, follow the physics item in full orientation, and play `candle_flicker`.

Both cakes use the same two-part model-origin compound: a plate cylinder at `[0, 0.0035, 0]`, radius 0.18 and half-height 0.0035; and a cake-body cylinder at `[0, 0.0695, 0]`, radius 0.155 and half-height 0.0625. This preserves floor contact at Y=0 and body top Y=0.132 without inventing a broad collider around thin candle shafts/flames.

The reference Arcade placement (`party_cake_cake` in `levels/retro_arcade_demo.json`) also adds a separate `glow` component: colour `[1, 0.49, 0.16]`, intensity 0.28, range 0.8, offset `[0, 0.218, 0]`, enabled, no fade. That illumination is not contained in the GLB material. The console stage retains it through `pickup_catalog::runtime_glow` and the renderer's bounded attached-character light path, transformed with the physical cake. Failed light allocation rolls back the whole spawn; removal and suspension cleanly remove or hide its light. The embedded visible emissive flame material and clip remain unchanged.

## Exclusion rationale

| Code | Count | Reason |
| --- | ---: | --- |
| B | 1 | Balloon and long string use dedicated attachment/animation behavior; measured height 1.53 m also exceeds give limit. |
| C | 12 | Character/creature rig with entity lifecycle; use the separate representation choices, not give pickup. |
| D | 13 | Draped cloth or hanging paper decoration; preserve its installation rather than make a stiff pickup shell. |
| E | 2 | Campfire scenery includes flames and map-owned illumination; not a loose carryable item. |
| F | 56 | Furniture or installed appliance/equipment assembly; outside loose handheld pickup scope. |
| L | 17 | Mounted or wired light fixture with map-owned illumination/attachment. |
| M | 15 | Installed fitting/sign/large artwork with a wall, ceiling, plumbing or decal contract. |
| S | 162 | Fixed architectural, road, railing, containment or surface module. |
| T | 34 | Terrain, rooted vegetation, environmental ice/snow/rock, or outdoor seating scenery. |

Some excluded objects fit the dimensional ceiling: chairs/tables, fixed monitors, mounted dispensers, light housings, rails, floor panels, individual icicles and snow caps are examples. Their installed or environmental use, rather than a broad dimension/category filter, keeps them out. Small framed landscape artwork is self-contained and portable; the large frame instead depends on its separate wall-art/decal composition. Portable potted plants are distinct from rooted trees, grass and shrub scenery.

The ledger records the maximum measured dimension to make size decisions inspectable. A value over 1.5 m is an additional rejection even if the reason code already excludes it. There are no unclassified model IDs.

| Excluded stable ID | Maximum measured dimension (m) | Reason |
| --- | ---: | --- |
| `arcade:ball_rack` | 1.100000 | F |
| `arcade:ball_return` | 1.810000 | F |
| `arcade:bench_three_seat_blue_red_blue` | 1.980000 | F |
| `arcade:bench_three_seat_red_blue_red` | 1.980000 | F |
| `arcade:blacklight_fixture` | 1.240000 | L |
| `arcade:cabinet_action` | 2.040000 | F |
| `arcade:cabinet_fighting` | 2.040000 | F |
| `arcade:cabinet_racing` | 2.040000 | F |
| `arcade:cabinet_scifi` | 2.040000 | F |
| `arcade:cabinet_shooter` | 2.040000 | F |
| `arcade:change_machine` | 1.460000 | F |
| `arcade:fixed_chair_blue` | 0.920000 | F |
| `arcade:fixed_chair_red` | 0.920000 | F |
| `arcade:gutter_segment` | 3.000000 | S |
| `arcade:lane_divider` | 3.000000 | S |
| `arcade:neon_bowl` | 1.820000 | L |
| `arcade:party_balloon` | 1.530000 | B |
| `arcade:party_cloth_rect_arcade` | 2.220000 | D |
| `arcade:party_cloth_rect_balloons` | 2.220000 | D |
| `arcade:party_cloth_rect_confetti` | 2.220000 | D |
| `arcade:party_cloth_rect_stars` | 2.220000 | D |
| `arcade:party_cloth_round_arcade` | 1.292000 | D |
| `arcade:party_cloth_round_balloons` | 1.292000 | D |
| `arcade:party_cloth_round_confetti` | 1.292000 | D |
| `arcade:party_cloth_round_stars` | 1.292000 | D |
| `arcade:party_streamer_blue` | 0.700120 | D |
| `arcade:party_streamer_orange` | 0.700120 | D |
| `arcade:party_streamer_pink` | 0.700120 | D |
| `arcade:party_streamer_purple` | 0.700120 | D |
| `arcade:party_streamer_yellow` | 0.700120 | D |
| `arcade:party_table_rect` | 2.100000 | F |
| `arcade:party_table_round` | 1.200000 | F |
| `arcade:pedestal_table` | 0.900000 | F |
| `arcade:pinball_comet` | 1.940000 | F |
| `arcade:pinball_cosmic` | 1.940000 | F |
| `arcade:restroom_grab_bar` | 0.930000 | M |
| `arcade:restroom_mirror_frame` | 0.968000 | M |
| `arcade:restroom_paper_holder` | 0.191000 | M |
| `arcade:restroom_sink` | 0.680000 | M |
| `arcade:restroom_soap_dispenser` | 0.240000 | M |
| `arcade:restroom_stall_divider` | 2.500000 | S |
| `arcade:restroom_stall_door` | 1.680000 | S |
| `arcade:restroom_stall_front` | 1.996000 | S |
| `arcade:restroom_stall_front_wide` | 1.996000 | S |
| `arcade:restroom_toilet` | 0.883000 | M |
| `arcade:score_monitor` | 1.120000 | F |
| `arcade:score_monitor_inactive` | 1.120000 | F |
| `arcade:vending_machine` | 2.030000 | F |
| `beach:bunting` | 4.381779 | S |
| `beach:coastal_rock` | 1.200000 | T |
| `beach:coastal_rock_wide` | 2.600000 | T |
| `beach:cove_foam` | 48.000000 | T |
| `beach:crab` | 0.832090 | C |
| `beach:dock` | 4.400000 | S |
| `beach:fish` | 0.638000 | C |
| `beach:grassy_bank` | 8.000000 | T |
| `beach:hut_brown` | 3.550000 | S |
| `beach:island` | 10.000000 | T |
| `beach:kiosk` | 3.640000 | S |
| `beach:lighthouse` | 7.700000 | S |
| `beach:lounge_chair` | 1.460459 | F |
| `beach:palm` | 6.000000 | T |
| `beach:palm_small` | 4.200000 | T |
| `beach:sand_patch` | 8.000000 | T |
| `beach:sea_arch` | 13.000000 | T |
| `beach:seagull` | 1.096000 | C |
| `beach:shoreline_foam` | 8.000000 | T |
| `beach:signpost` | 1.760000 | S |
| `beach:stilt_house_blue` | 6.080000 | S |
| `beach:town_arch` | 3.600000 | S |
| `beach:town_house_blue` | 5.450000 | S |
| `beach:town_house_coral` | 4.000000 | S |
| `beach:town_house_cream` | 5.400000 | S |
| `beach:town_parapet` | 2.400000 | S |
| `beach:town_shrub` | 1.300000 | T |
| `beach:town_stairs` | 4.200000 | S |
| `beach:town_terrace` | 4.030000 | S |
| `beach:town_terrace_base` | 4.000000 | S |
| `beach:umbrella` | 2.550000 | F |
| `carved-pumpkin` | 0.631126 | C |
| `core:armchair` | 0.900000 | F |
| `core:bed` | 2.000000 | F |
| `core:bookshelf` | 1.800000 | F |
| `core:cabinet` | 0.900000 | F |
| `core:ceiling_diffuser_01` | 0.600000 | M |
| `core:chair` | 0.900000 | F |
| `core:couch` | 2.000000 | F |
| `core:desk` | 1.600000 | F |
| `core:exit_sign` | 0.570000 | M |
| `core:fridge` | 1.800000 | F |
| `core:hot_tub` | 2.670000 | F |
| `core:lamp` | 1.500000 | L |
| `core:office_filing_cabinet_tall` | 1.320000 | F |
| `core:office_fluorescent_panel_housing` | 1.200000 | L |
| `core:office_noticeboard` | 1.000000 | M |
| `core:office_reception_desk` | 2.400000 | F |
| `core:office_snack_machine` | 1.900000 | F |
| `core:painting_frame_landscape_01` | 1.900000 | M |
| `core:pool_chair` | 0.850000 | F |
| `core:pool_curtain_corner` | 2.600000 | S |
| `core:pool_curtain_end` | 2.600000 | S |
| `core:pool_curtain_straight` | 2.600000 | S |
| `core:pool_guardrail_corner` | 1.050000 | S |
| `core:pool_guardrail_end` | 1.050000 | S |
| `core:pool_guardrail_straight` | 2.000000 | S |
| `core:pool_ladder` | 2.195000 | S |
| `core:pool_light_round_housing` | 0.500000 | L |
| `core:pool_light_wall_housing` | 0.400000 | L |
| `core:pool_table` | 0.800000 | F |
| `core:rug` | 2.000000 | S |
| `core:sink` | 1.100000 | F |
| `core:stop_sign` | 1.800000 | M |
| `core:stove` | 0.900000 | F |
| `core:table` | 1.400000 | F |
| `core:table_sideways` | 1.400000 | F |
| `core:vending_machine` | 1.900000 | F |
| `core:washing_machine` | 0.850000 | F |
| `core:water_cooler` | 1.100000 | F |
| `entity:cockroach` | 0.132125 | C |
| `frutiger_aero:accent_panel` | 2.200000 | S |
| `frutiger_aero:atrium_dome` | 12.000000 | S |
| `frutiger_aero:banner_atrium` | 2.400000 | S |
| `frutiger_aero:banner_corridor` | 2.400000 | S |
| `frutiger_aero:banner_reception` | 2.400000 | S |
| `frutiger_aero:bubble_sculpture` | 2.700000 | S |
| `frutiger_aero:ceiling_ring` | 1.300000 | S |
| `frutiger_aero:city_backdrop` | 23.850000 | S |
| `frutiger_aero:display_kiosk` | 2.400000 | S |
| `frutiger_aero:double_doors` | 3.200000 | S |
| `frutiger_aero:double_doors_open` | 3.200000 | S |
| `frutiger_aero:fountain_basin` | 3.200000 | S |
| `frutiger_aero:glass_canopy` | 3.600000 | S |
| `frutiger_aero:glass_partition` | 3.495000 | S |
| `frutiger_aero:green_backdrop` | 24.000000 | S |
| `frutiger_aero:light_pod` | 3.600000 | S |
| `frutiger_aero:lime_arch` | 3.600000 | S |
| `frutiger_aero:portal_spandrel` | 3.600000 | S |
| `frutiger_aero:reception_counter` | 4.500000 | S |
| `frutiger_aero:reception_soffit` | 5.200000 | S |
| `frutiger_aero:seating_pod` | 2.400000 | S |
| `frutiger_aero:tree_planter` | 3.008870 | S |
| `frutiger_aero:wall_bay` | 3.600000 | S |
| `frutiger_aero:wall_bay_tall` | 4.200000 | S |
| `home:armchair` | 0.925000 | F |
| `home:ball_light` | 0.799484 | L |
| `home:bookshelf` | 1.800000 | F |
| `home:cabinet_base` | 0.900000 | F |
| `home:cabinet_strip` | 0.550000 | L |
| `home:cabinet_wall` | 0.720000 | F |
| `home:coffee_table` | 1.400000 | F |
| `home:dining_chair` | 0.902000 | F |
| `home:dining_chair_refined` | 0.902000 | F |
| `home:dining_table` | 1.400000 | F |
| `home:door_casing` | 2.180000 | S |
| `home:floor_lamp` | 1.500000 | L |
| `home:outlet` | 0.120000 | M |
| `home:rug` | 2.000000 | S |
| `home:sink` | 1.089000 | F |
| `home:sofa` | 2.000000 | F |
| `home:stove` | 0.916000 | F |
| `home:tv_console` | 1.550000 | F |
| `home:wall_switch` | 0.120000 | M |
| `mannequin` | 1.723035 | C |
| `outdoor:boundary_ridge` | 20.000000 | T |
| `outdoor:bush_low` | 1.700000 | T |
| `outdoor:bush_round` | 1.550000 | T |
| `outdoor:campfire_static` | 1.300000 | E |
| `outdoor:collision_peg` | 0.060000 | S |
| `outdoor:concrete_step` | 1.400000 | S |
| `outdoor:fence_post` | 1.050000 | S |
| `outdoor:fence_two_rail` | 2.000000 | S |
| `outdoor:fence_two_rail_post` | 1.050000 | S |
| `outdoor:fence_two_rail_section` | 1.780000 | S |
| `outdoor:grass_patch_large` | 0.950000 | T |
| `outdoor:grass_patch_small` | 0.550000 | T |
| `outdoor:house_01_corner_trim` | 2.700000 | S |
| `outdoor:house_01_gable` | 3.000000 | S |
| `outdoor:house_01_porch_deck` | 3.000000 | S |
| `outdoor:house_01_porch_post` | 2.300000 | S |
| `outdoor:house_01_roof_ridge` | 3.400000 | S |
| `outdoor:house_01_roof_slope` | 3.400000 | S |
| `outdoor:house_01_wall_doorway` | 3.000000 | S |
| `outdoor:house_01_wall_solid` | 3.000000 | S |
| `outdoor:house_01_wall_window` | 3.000000 | S |
| `outdoor:house_02_corner_trim` | 2.700000 | S |
| `outdoor:house_02_gable` | 3.000000 | S |
| `outdoor:house_02_porch_deck` | 3.000000 | S |
| `outdoor:house_02_porch_post` | 2.300000 | S |
| `outdoor:house_02_roof_ridge` | 3.400000 | S |
| `outdoor:house_02_roof_slope` | 3.400000 | S |
| `outdoor:house_02_wall_doorway` | 3.000000 | S |
| `outdoor:house_02_wall_solid` | 3.000000 | S |
| `outdoor:house_02_wall_window` | 3.000000 | S |
| `outdoor:house_03_corner_trim` | 2.700000 | S |
| `outdoor:house_03_gable` | 3.000000 | S |
| `outdoor:house_03_porch_deck` | 3.000000 | S |
| `outdoor:house_03_porch_post` | 2.300000 | S |
| `outdoor:house_03_roof_ridge` | 3.400000 | S |
| `outdoor:house_03_roof_slope` | 3.400000 | S |
| `outdoor:house_03_wall_doorway` | 3.000000 | S |
| `outdoor:house_03_wall_solid` | 3.000000 | S |
| `outdoor:house_03_wall_window` | 3.000000 | S |
| `outdoor:house_04_corner_trim` | 2.700000 | S |
| `outdoor:house_04_gable` | 3.000000 | S |
| `outdoor:house_04_porch_deck` | 3.000000 | S |
| `outdoor:house_04_porch_post` | 2.300000 | S |
| `outdoor:house_04_roof_ridge` | 3.400000 | S |
| `outdoor:house_04_roof_slope` | 3.400000 | S |
| `outdoor:house_04_wall_doorway` | 3.000000 | S |
| `outdoor:house_04_wall_solid` | 3.000000 | S |
| `outdoor:house_04_wall_window` | 3.000000 | S |
| `outdoor:house_05_corner_trim` | 2.700000 | S |
| `outdoor:house_05_gable` | 3.000000 | S |
| `outdoor:house_05_porch_deck` | 3.000000 | S |
| `outdoor:house_05_porch_post` | 2.300000 | S |
| `outdoor:house_05_roof_ridge` | 3.400000 | S |
| `outdoor:house_05_roof_slope` | 3.400000 | S |
| `outdoor:house_05_wall_doorway` | 3.000000 | S |
| `outdoor:house_05_wall_solid` | 3.000000 | S |
| `outdoor:house_05_wall_window` | 3.000000 | S |
| `outdoor:house_corner_trim` | 2.700000 | S |
| `outdoor:house_roof_ridge` | 3.400000 | S |
| `outdoor:house_roof_slope` | 3.400000 | S |
| `outdoor:house_wall_doorway` | 3.000000 | S |
| `outdoor:house_wall_solid` | 3.000000 | S |
| `outdoor:house_wall_window` | 3.000000 | S |
| `outdoor:lamp_fence` | 0.420000 | L |
| `outdoor:lamp_stand` | 1.050000 | L |
| `outdoor:lamp_wall` | 0.520000 | L |
| `outdoor:masonry_pier` | 1.200000 | S |
| `outdoor:porch_canopy` | 3.400000 | S |
| `outdoor:porch_post` | 1.050000 | S |
| `outdoor:porch_railing_corner` | 0.910000 | S |
| `outdoor:porch_railing_end` | 1.050000 | S |
| `outdoor:porch_railing_straight` | 1.800000 | S |
| `outdoor:road_barrier` | 3.400000 | S |
| `outdoor:showcase_boulder` | 1.400000 | T |
| `outdoor:showcase_campfire` | 1.300000 | E |
| `outdoor:showcase_curb` | 6.000000 | S |
| `outdoor:showcase_curb_ramp` | 2.400000 | S |
| `outdoor:showcase_road_dash` | 1.500000 | S |
| `outdoor:showcase_road_gate` | 8.000000 | S |
| `outdoor:showcase_road_straight` | 8.000000 | S |
| `outdoor:showcase_road_unmarked` | 8.000000 | S |
| `outdoor:showcase_rock_face` | 5.000000 | T |
| `outdoor:showcase_rock_face_variant` | 4.800000 | T |
| `outdoor:showcase_sidewalk` | 6.000000 | S |
| `outdoor:showcase_sidewalk_corner` | 2.400000 | S |
| `outdoor:showcase_sidewalk_transition` | 2.400000 | S |
| `outdoor:showcase_stump_seat` | 0.700000 | T |
| `outdoor:streetlight` | 6.400000 | L |
| `outdoor:tree_01` | 6.400000 | T |
| `outdoor:tree_02` | 6.200000 | T |
| `outdoor:tree_03` | 6.800000 | T |
| `pool:pool_basin_tile_module` | 0.500000 | S |
| `pool:pool_bench` | 1.600000 | F |
| `pool:pool_ceiling_panel_module` | 0.600000 | S |
| `pool:pool_deck_tile_module` | 0.500000 | S |
| `pool:pool_drain` | 0.600000 | M |
| `pool:pool_entry_steps` | 1.200000 | S |
| `pool:pool_safety_sign` | 0.400000 | M |
| `pool:pool_service_door` | 2.100000 | S |
| `pool:pool_wall_tile_module` | 0.500000 | S |
| `pumpkin-skeleton` | 1.822028 | C |
| `rat` | 0.462770 | C |
| `sheet-ghost` | 1.619323 | C |
| `sheet-ghost-cat` | 0.593000 | C |
| `skeleton` | 1.722652 | C |
| `spooner-man` | 0.543769 | C |
| `winter:boulder_snow` | 1.400000 | T |
| `winter:door_hood_snow` | 1.940000 | S |
| `winter:drift_fence` | 1.800000 | T |
| `winter:drift_large` | 4.400000 | T |
| `winter:drift_medium` | 2.400000 | T |
| `winter:drift_small` | 1.200000 | T |
| `winter:drift_wall` | 3.000000 | T |
| `winter:entrance_frame` | 2.420000 | S |
| `winter:fence_post_snow` | 1.126000 | S |
| `winter:fence_two_rail_post_snow` | 1.127000 | S |
| `winter:fence_two_rail_section_snow` | 1.780000 | S |
| `winter:ice_fragment` | 1.100000 | T |
| `winter:icicle_cluster_mixed` | 1.200000 | S |
| `winter:icicle_cluster_sparse` | 1.200000 | S |
| `winter:icicle_long` | 0.650000 | S |
| `winter:icicle_medium` | 0.360000 | S |
| `winter:icicle_short` | 0.180000 | S |
| `winter:masonry_pier_snow` | 1.250000 | S |
| `winter:mound_irregular` | 2.100000 | T |
| `winter:railing_snow_corner` | 0.976000 | S |
| `winter:railing_snow_corner_assembly` | 1.124000 | S |
| `winter:railing_snow_end` | 1.126000 | S |
| `winter:railing_snow_straight` | 1.800000 | S |
| `winter:railing_snow_straight_assembly` | 1.800000 | S |
| `winter:rock_face_snow` | 5.160000 | T |
| `winter:snow_awning` | 2.600000 | S |
| `winter:snow_door_overhang` | 1.600000 | S |
| `winter:snow_ledge` | 1.200000 | S |
| `winter:snow_porch_edge` | 1.800000 | S |
| `winter:snow_post_cap` | 0.120000 | S |
| `winter:snow_rail_top` | 1.200000 | S |
| `winter:snow_roof_blanket` | 6.811420 | S |
| `winter:snow_roof_edge` | 2.400000 | S |
| `winter:snow_roof_slope` | 3.400000 | S |
| `winter:snow_stair_edge` | 0.800000 | S |
| `winter:stone_wall_snow` | 2.400000 | S |
| `winter:string_lights_long` | 6.650000 | L |
| `winter:string_lights_medium` | 4.050000 | L |
| `winter:string_lights_short` | 2.850000 | L |
| `winter:timber_lantern` | 3.200000 | L |
| `winter:tree_snow_01` | 6.800000 | T |
| `winter:tree_snow_02` | 6.800000 | T |
| `winter:window_frame` | 1.260000 | S |

## Verification and outstanding native evidence

- `python3 tools/assets/validate.py`: passed; 593 assets / 370 placeable; zero warnings.
- `python3 tools/props/build.py --check`: passed for all 370 model identities; unique family image residency 77.793 MiB (exact-PNG identity). Existing reviewed geometry/art-budget notes remain.
- Nonmutating complete GLB inventory: all 370 parsed; zero missing/failed models; 16 carry skin/animation declarations. Only the reviewed flame-animated cake enters this pickup list.
- Focused sidecar/list audit: 58 unique sorted exact IDs; every entry exists, has positive bounded measured geometry, strict-valid dynamic pickup metadata and a conservative radius within pickup reach; primitive water density/stiffness checks passed.
- `rustfmt --edition 2024 src/game/pickup_catalog.rs`: passed.

Full Cargo/Clippy/release gates and native execution belong to the integrating parent task; this subtask did not run a competing build or acquire the game window. No Rust tests were added: the work changes a reviewed data list and physical profiles, and the essential integration evidence is real give/list/pickup/drop behavior using the renderer and physics.

**Pending native acceptance:** give and pick up/drop at least a representative party cup, party plate, bottle, bowling ball, unlit cake and lit cake. The lit-cake check must show its original textured cake and embedded bright candle flames, active flicker, correct body orientation while carried/dropped, no orphaned static flame copy, and honest treatment of nearby glow/spill. Recheck the selected lightweight paper/hat profile in a water map. The outstanding item is execution evidence in the integrated runtime, not missing artwork or metadata.

Review identity: `assets/catalog.json` SHA-256 `020afb28566d36b5f096cd203826d281abc357aaf1b18cc83cbae3f851e82c62`.
