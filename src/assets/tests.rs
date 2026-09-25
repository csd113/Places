//! Unit tests for the asset catalog.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::map_unwrap_or,
    clippy::needless_raw_string_hashes,
    clippy::panic,
    clippy::too_many_lines
)]

use super::*;
use crate::level::LevelDef;

fn shipped_catalog() -> AssetCatalog {
    let catalog = AssetCatalog::load_default();
    assert!(
        !catalog.is_empty(),
        "assets/catalog.json must be discoverable from the crate root"
    );
    catalog
}

#[test]
fn shipped_catalog_classifies_environments_themes_and_entities() {
    let catalog = shipped_catalog();

    let office = catalog.get("core:desk").expect("the office desk");
    assert_eq!(office.asset_class.as_str(), AssetClass::ENVIRONMENT);
    assert_eq!(
        office.theme.as_ref().map(AssetTheme::as_str),
        Some("office")
    );
    assert_eq!(office.asset_type.as_str(), AssetType::PROP);
    assert_eq!(office.source, AssetSource::File);
    assert!(office.is_placeable());
    assert_eq!(
        office.model.as_deref(),
        Some("environment/office/props/models/desk.glb")
    );

    let fixture = catalog
        .get("core:fluorescent_panel_01")
        .expect("the office fixture");
    assert_eq!(fixture.asset_type.as_str(), AssetType::LIGHT);
    assert_eq!(
        fixture.theme.as_ref().map(AssetTheme::as_str),
        Some("office")
    );
    assert_eq!(fixture.source, AssetSource::File);
    assert_eq!(
        catalog.fixture_sheet_path("core:fluorescent_panel_01"),
        Some("environment/office/textures/lights/fluorescent_panel_01.png")
    );
    assert!(!fixture.is_placeable());

    let generic = catalog.get("core:couch").expect("the shared couch");
    assert_eq!(generic.asset_class.as_str(), AssetClass::ENVIRONMENT);
    assert!(
        generic.theme.is_none(),
        "generic props stay generic instead of being forced into a theme"
    );

    let decal = catalog
        .get("core:decal_test_01")
        .expect("the diagnostic decal");
    assert_eq!(decal.asset_class.as_str(), AssetClass::DIAGNOSTIC);
    assert_eq!(decal.asset_type.as_str(), AssetType::DECAL);

    let carpet = catalog
        .get("core:carpet_beige_01")
        .expect("the office carpet material");
    assert_eq!(carpet.asset_type.as_str(), AssetType::MATERIAL);
    assert_eq!(carpet.source, AssetSource::Definition);
    assert_eq!(carpet.texture.as_deref(), Some("core:tex_carpet_beige_01"));
    assert_eq!(carpet.tile_metres, Some(2.0));
    assert_eq!(
        catalog.material_texture("core:carpet_beige_01"),
        Some("core:tex_carpet_beige_01")
    );

    let texture = catalog
        .get("core:tex_carpet_beige_01")
        .expect("the carpet texture");
    assert_eq!(texture.asset_type.as_str(), AssetType::TEXTURE);
    assert_eq!(texture.source, AssetSource::File);
    assert_eq!(
        catalog.texture_path("core:tex_carpet_beige_01"),
        Some("environment/office/textures/floors/carpet_beige_01.png")
    );

    // The Home theme's own fixture is catalogued with its visible face, exactly
    // like the office and pool families, so a Home level draws a real textured
    // lamp rather than the fallback sheet.
    assert_eq!(
        catalog.fixture_sheet_path("home:ceiling_light_round"),
        Some("environment/home/textures/lights/ceiling_light_round_01.png")
    );
    assert_eq!(
        catalog.texture_path("home:tex_hardwood_oak_01"),
        Some("environment/home/textures/floors/hardwood_oak_01.png")
    );

    let themes: Vec<&str> = catalog
        .themes()
        .iter()
        .map(|theme| theme.id.as_str())
        .collect();
    assert_eq!(themes, ["office", "pool", "home"]);
    assert!(
        catalog
            .themes()
            .iter()
            .all(|theme| !theme.display_name.is_empty())
    );
}

#[test]
fn spooner_man_is_an_entity_not_a_theme() {
    let catalog = shipped_catalog();
    let spooner = catalog.get("spooner-man").expect("spooner-man");
    assert_eq!(spooner.id, "spooner-man", "the logical id is the anchor");
    assert_eq!(spooner.asset_class.as_str(), AssetClass::ENTITY);
    assert_eq!(spooner.asset_type.as_str(), AssetType::ENTITY);
    assert_eq!(spooner.entity_type.as_deref(), Some("character"));
    assert!(
        spooner.theme.is_none(),
        "entities are a class, never a theme"
    );
    assert!(
        spooner.is_placeable(),
        "entities place through the prop pipeline"
    );
    assert_eq!(
        spooner.model.as_deref(),
        Some("entities/spooner-man/model/spooner-man.glb")
    );
    assert!(
        !catalog
            .themes()
            .iter()
            .any(|theme| theme.id.as_str() == "entities"),
        "entities are a class, so there must be no `entities` theme"
    );
}

#[test]
fn duplicate_logical_ids_are_rejected() {
    let json = r##"{
        "assets": [
            { "id": "spooner-man", "asset_class": "entity", "asset_type": "entity",
              "model": "entities/spooner-man/model/spooner-man.glb" },
            { "id": "spooner-man", "asset_class": "environment", "asset_type": "prop",
              "model": "core/props/models/spooner-man.glb" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(json).expect_err("duplicate ids must fail");
    assert!(
        error.contains("duplicate asset id"),
        "unexpected error: {error}"
    );
    assert!(error.contains("spooner-man"), "unexpected error: {error}");

    let duplicate_theme = r#"{ "themes": [{ "id": "pool" }, { "id": "pool" }], "assets": [] }"#;
    let error =
        AssetCatalog::from_json_str(duplicate_theme).expect_err("duplicate themes must fail");
    assert!(
        error.contains("duplicate theme id"),
        "unexpected error: {error}"
    );

    // Duplicate texture ids and duplicate material ids are the same
    // catalog-level error, never last-one-wins.
    let duplicate_texture = r##"{
        "assets": [
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "environment/office/textures/walls/a.png" },
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "environment/office/textures/walls/b.png" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(duplicate_texture).expect_err("duplicate texture");
    assert!(
        error.contains("duplicate asset id") && error.contains("core:tex_a"),
        "unexpected error: {error}"
    );

    let duplicate_material = r##"{
        "assets": [
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "environment/office/textures/walls/a.png" },
            { "id": "core:mat_a", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_a" },
            { "id": "core:mat_a", "asset_class": "environment", "asset_type": "material",
              "source": "definition", "texture": "core:tex_a" }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(duplicate_material).expect_err("duplicate material");
    assert!(
        error.contains("duplicate asset id") && error.contains("core:mat_a"),
        "unexpected error: {error}"
    );
}

#[test]
fn future_themes_and_classes_parse_without_an_engine_change() {
    let json = r##"{
        "themes": [{ "id": "hotel", "display_name": "Hotel" }],
        "assets": [
            { "id": "hotel:chair", "display_name": "Hotel Chair", "asset_class": "environment",
              "theme": "hotel", "asset_type": "prop", "model": "environment/hotel/props/models/chair.glb",
              "size": [0.5, 0.9, 0.5], "category": "Furniture", "solid": true },
            { "id": "future:cart", "asset_class": "utility", "asset_type": "vehicle",
              "model": "core/vehicles/models/cart.glb" }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("a future catalog parses");
    let chair = catalog
        .get("hotel:chair")
        .expect("the hotel chair resolves");
    assert_eq!(chair.theme.as_ref().map(AssetTheme::as_str), Some("hotel"));
    assert!(chair.is_placeable());
    let cart = catalog
        .get("future:cart")
        .expect("the future class resolves");
    assert_eq!(cart.asset_class.as_str(), "utility");
    assert!(!cart.asset_class.is_known());
    assert!(!cart.is_placeable());
    assert_eq!(
        catalog.themes()[0].display_name,
        "Hotel",
        "a new theme is declared by data, not by code"
    );
}

#[test]
fn malformed_catalog_entries_are_rejected_with_a_useful_message() {
    let cases = [
        (
            r#"{ "assets": [{ "id": "  ", "asset_class": "environment", "asset_type": "prop" }] }"#,
            "empty id",
        ),
        (
            r#"{ "assets": [{ "id": "future:thing", "asset_type": "prop", "model": "a.glb" }] }"#,
            "missing `asset_class`",
        ),
        (
            r#"{ "assets": [{ "id": "future:thing", "asset_class": "environment", "model": "a.glb" }] }"#,
            "missing `asset_type`",
        ),
        (
            r#"{ "assets": [{ "id": "future:thing", "asset_class": "environment",
                "asset_type": "prop", "theme": "Not A Theme", "model": "a.glb" }] }"#,
            "invalid theme",
        ),
        (
            r#"{ "assets": [{ "id": "future:thing", "asset_class": "environment",
                "asset_type": "prop", "model": "../outside.glb" }] }"#,
            "relative path",
        ),
        (
            r#"{ "assets": [{ "id": "future:thing", "asset_class": "environment",
                "asset_type": "prop", "source": "magic", "model": "a.glb" }] }"#,
            "expected `file`, `generated` or `definition`",
        ),
    ];
    for (json, needle) in cases {
        let error =
            AssetCatalog::from_json_str(json).expect_err(&format!("must be rejected: {json}"));
        assert!(
            error.contains(needle),
            "error {error:?} does not mention {needle:?}"
        );
    }
}

#[test]
fn legacy_props_shaped_catalog_still_parses() {
    let json = r##"{
        "format_version": 1,
        "props": [
            { "id": "core-crate", "name": "Legacy Crate", "category": "Utility",
              "size": [0.6, 0.6, 0.6], "color": "#7a6244", "model": "models/crate.glb", "solid": true },
            { "id": "core:nameless" }
        ]
    }"##;
    let catalog = AssetCatalog::from_json_str(json).expect("the legacy shape parses");
    assert_eq!(catalog.len(), 2);
    let legacy = catalog.get("core-crate").expect("legacy id");
    assert_eq!(legacy.display_name, "Legacy Crate");
    assert_eq!(legacy.asset_class.as_str(), AssetClass::ENVIRONMENT);
    assert_eq!(legacy.asset_type.as_str(), AssetType::PROP);
    assert!(legacy.theme.is_none());
    assert!(legacy.is_placeable());
}

#[test]
fn every_file_asset_exists_exactly_once() {
    let catalog = shipped_catalog();
    let root = resolve_asset_root().expect("assets/ is discoverable");
    let mut claimed: HashMap<&str, &str> = HashMap::new();

    for entry in catalog.entries() {
        if entry.source != AssetSource::File {
            assert!(
                entry.model.is_none(),
                "{}: generated assets must not name a file",
                entry.id
            );
            continue;
        }
        let model = entry
            .model
            .as_deref()
            .unwrap_or_else(|| panic!("{}: file assets must name a model", entry.id));
        if let Some(other) = claimed.insert(model, &entry.id) {
            panic!("{} and {other} claim the same resource {model}", entry.id);
        }
        assert!(
            root.join(model).is_file(),
            "{}: {} is missing below {}",
            entry.id,
            model,
            root.display()
        );
    }

    // One canonical Spooner-Man resource: no duplicate legacy copy remains.
    assert_eq!(
        claimed["entities/spooner-man/model/spooner-man.glb"],
        "spooner-man"
    );
    assert!(
        !root.join("props/models/spooner-man.glb").exists(),
        "the removed assets/props/models/spooner-man.glb must not be present"
    );
    assert!(
        !root.join("catalog.json").is_dir() && root.join("catalog.json").is_file(),
        "the catalog lives at the asset root"
    );
}

#[test]
fn levels_only_reference_catalog_ids() {
    let catalog = shipped_catalog();
    let mut levels = 0;
    let mut props = 0;
    let mut spooner_levels = 0;

    for dir in ["assets/levels", "tests/fixtures/levels"] {
        let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{dir} must exist: {error}"));
        for file in entries.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let content = fs::read_to_string(&path).expect("level is readable");
            let level = LevelDef::from_json(&content)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            levels += 1;

            let check = |id: &str, what: &str| {
                assert!(
                    catalog.contains(id),
                    "{}: {what} `{id}` is not in the asset catalog",
                    path.display()
                );
            };
            check(&level.defaults.wall, "wall material");
            check(&level.defaults.floor, "floor material");
            check(&level.defaults.ceiling, "ceiling material");
            for room in level.room_iter() {
                if let Some(material) = &room.material {
                    check(material, "room floor material");
                }
                if let Some(material) = &room.ceiling_material {
                    check(material, "room ceiling material");
                }
            }
            for wall in &level.walls {
                if let Some(material) = &wall.material {
                    check(material, "wall material");
                }
                for material in wall.faces.values() {
                    check(material, "wall face material");
                }
            }
            for patch in &level.floor_patches {
                check(&patch.material, "floor patch material");
            }
            for light in &level.ceiling_lights {
                check(&light.fixture, "light fixture");
            }
            for decal in &level.decals {
                check(&decal.material, "decal sheet");
            }
            for prop in &level.props {
                props += 1;
                assert!(
                    catalog.placeable(&prop.model).is_some(),
                    "{}: prop `{}` is not a placeable catalog asset",
                    path.display(),
                    prop.model
                );
                if prop.model == "spooner-man" {
                    spooner_levels += 1;
                }
            }
        }
    }
    assert!(
        levels >= 5,
        "expected the shipped level and the regression fixtures"
    );
    assert!(props >= 1, "expected placed props");
    assert!(
        spooner_levels >= 1,
        "at least one level must still reference `spooner-man`"
    );
}

#[test]
fn renderer_and_catalog_agree_on_surface_and_decal_ids() {
    let catalog = shipped_catalog();

    // Every catalogued surface material must resolve through the material
    // table to a decoded PNG: no material is allowed to depend on a
    // renderer-known id or a code-generated sheet any more.
    let level = LevelDef::from_json(include_str!("../../assets/levels/places_demo.json"))
        .expect("places_demo parses");
    let mut cache = crate::materials::TextureCache::new();
    let root = resolve_asset_root().expect("assets/ is discoverable");
    let table =
        crate::materials::resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    assert!(
        table.errors().is_empty(),
        "the demo's materials must all resolve: {:?}",
        table.errors()
    );
    for material in table.entries() {
        assert!(
            material.image.is_some(),
            "{}: no decoded texture image",
            material.id
        );
        assert_eq!(
            material.origin,
            crate::materials::TextureOrigin::Catalog,
            "{}: expected a catalog PNG",
            material.id
        );
    }
    for legacy in [
        "core:wallpaper_yellow_01",
        "core:carpet_beige_01",
        "core:ceiling_panel_01",
    ] {
        assert!(
            catalog.contains(legacy),
            "{legacy} must stay a stable catalogued material id"
        );
    }

    for sheet in crate::render::DECAL_MATERIALS {
        let entry = catalog
            .get(sheet)
            .unwrap_or_else(|| panic!("{sheet} must be catalogued"));
        assert_eq!(entry.asset_type.as_str(), AssetType::DECAL);
        assert!(
            crate::render::decal_material_slot(sheet).is_some(),
            "{sheet} must resolve to a decal sheet slot"
        );
    }
    // Every catalogued decal must be one the renderer can actually draw:
    // either a generated atlas pattern or a file-backed PNG sheet that
    // resolves through the catalog like a surface texture.
    for entry in catalog.entries() {
        if entry.asset_type.as_str() != AssetType::DECAL {
            continue;
        }
        let generated = crate::render::decal_material_slot(&entry.id).is_some();
        if generated {
            continue;
        }
        assert_eq!(
            entry.source,
            AssetSource::File,
            "{}: a decal must be a generated pattern or a file-backed PNG sheet",
            entry.id
        );
        let model = entry
            .model
            .as_deref()
            .unwrap_or_else(|| panic!("{}: a file decal needs a PNG model", entry.id));
        assert!(
            model.to_ascii_lowercase().ends_with(".png"),
            "{}: decal sheet `{model}` must be a PNG",
            entry.id
        );
        assert!(
            resolve_asset_root()
                .map(|root| root.join(model).is_file())
                .unwrap_or(false),
            "{}: decal sheet `{model}` is missing below assets/",
            entry.id
        );
    }

    // Every catalogued light fixture must have a built-in appearance *and* a
    // visible face the renderer can load: a fixture is generated geometry, but
    // what it shows is external artwork, exactly like a decal sheet.
    let mut fixtures = 0usize;
    let mut sheets: Vec<&str> = Vec::new();
    for entry in catalog.entries() {
        if entry.asset_type.as_str() != AssetType::LIGHT {
            continue;
        }
        fixtures += 1;
        assert!(
            crate::lighting::LIGHT_FIXTURE_IDS.contains(&entry.id.as_str()),
            "{}: the renderer has no fixture appearance for this light",
            entry.id
        );
        assert_eq!(
            entry.source,
            AssetSource::File,
            "{}: a built-in fixture names its visible-face PNG",
            entry.id
        );
        let model = catalog
            .fixture_sheet_path(&entry.id)
            .unwrap_or_else(|| panic!("{}: a light fixture needs a .png sheet", entry.id));
        assert!(
            !sheets.contains(&model),
            "{}: sheet `{model}` is already claimed by another fixture",
            entry.id
        );
        sheets.push(model);
        assert!(
            resolve_asset_root()
                .map(|root| root.join(model).is_file())
                .unwrap_or(false),
            "{}: fixture sheet `{model}` is missing below assets/",
            entry.id
        );
    }
    assert!(
        fixtures >= crate::lighting::LIGHT_FIXTURE_IDS.len(),
        "the catalog must declare every built-in fixture"
    );
    for id in crate::lighting::LIGHT_FIXTURE_IDS {
        let entry = catalog
            .get(id)
            .unwrap_or_else(|| panic!("{id} must be catalogued"));
        assert_eq!(entry.asset_type.as_str(), AssetType::LIGHT, "{id}");
        assert!(
            catalog.fixture_sheet_path(id).is_some(),
            "{id}: every built-in fixture must resolve a sheet"
        );
    }
}

#[test]
fn level_material_references_cover_floor_regions_too() {
    let catalog = shipped_catalog();
    let mut checked = 0usize;
    for dir in ["assets/levels", "tests/fixtures/levels"] {
        let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{dir}: {error}"));
        for file in entries.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let content = fs::read_to_string(&path).expect("level is readable");
            let level = LevelDef::from_json(&content)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for region in &level.floor_regions {
                for (what, material) in [
                    ("region floor material", region.material.as_deref()),
                    ("region edge material", region.edge_material.as_deref()),
                ] {
                    if let Some(material) = material {
                        checked += 1;
                        assert!(
                            catalog.contains(material),
                            "{}: {what} `{material}` is not in the asset catalog",
                            path.display()
                        );
                    }
                }
            }
        }
    }
    assert!(
        checked > 0,
        "at least one shipped level must exercise a region material"
    );
}

#[test]
fn shipped_texture_policy_accepts_the_upgraded_art_and_rejects_breaches() {
    // The upgraded Office/Pool surfaces are 1024x1024: square, exactly at the
    // hard dimension, and exactly the per-sheet decoded byte budget.
    ShippedTextureKind::Surface
        .check_dimensions(1024, 1024)
        .expect("a 1024x1024 surface is the shipped contract");
    assert_eq!(decoded_rgba_bytes(1024, 1024), MAX_SURFACE_TEXTURE_BYTES);
    assert!(decoded_rgba_bytes(1024, 1024) <= MAX_SURFACE_TEXTURE_BYTES);

    // Over-preferred is a soft warning, never an error.
    assert!(ShippedTextureKind::is_over_preferred(1024, 1024));
    assert!(ShippedTextureKind::is_over_preferred(512, 256));
    assert!(!ShippedTextureKind::is_over_preferred(256, 256));

    // One pixel over the hard limit is rejected.
    let error = ShippedTextureKind::Surface
        .check_dimensions(1025, 1024)
        .expect_err("dimensions over the hard limit must be rejected");
    assert!(error.contains("hard limit"), "unexpected error: {error}");

    // Zero-sized sheets are rejected for every class.
    for kind in [
        ShippedTextureKind::Surface,
        ShippedTextureKind::FixtureFace,
        ShippedTextureKind::DecalSheet,
    ] {
        let error = kind
            .check_dimensions(0, 128)
            .expect_err("a zero-sized sheet must be rejected");
        assert!(error.contains("non-zero"), "unexpected error: {error}");
    }

    // A surface sheet must be square: the renderer samples it as a square
    // tile_metres cell, so a 2:1 sheet would stretch.
    let error = ShippedTextureKind::Surface
        .check_dimensions(256, 128)
        .expect_err("a non-square surface must be rejected");
    assert!(error.contains("square"), "unexpected error: {error}");

    // Surfaces are not power-of-two constrained: a square NPOT sheet loads and
    // satisfies the surface contract.
    ShippedTextureKind::Surface
        .check_dimensions(96, 96)
        .expect("a square NPOT surface satisfies the surface contract");

    // Fitted sheets are power-of-two constrained for an exact mip chain.
    for kind in [
        ShippedTextureKind::FixtureFace,
        ShippedTextureKind::DecalSheet,
    ] {
        let error = kind
            .check_dimensions(96, 96)
            .expect_err("a non-power-of-two fitted sheet must be rejected");
        assert!(error.contains("power-of-two"), "unexpected error: {error}");
        // The shipped fixture faces and decal sheets satisfy the contract.
        kind.check_dimensions(256, 128)
            .expect("the office panel face is a valid fixture sheet");
        kind.check_dimensions(1024, 1024)
            .expect("the upgraded pool sign is a valid decal sheet");
    }

    // The 96x64 diagnostic is the deliberate NPOT exception: the policy
    // rejects it for every class, and the call site that loads it asserts the
    // exact dimensions instead of weakening the classes above.
    for kind in [
        ShippedTextureKind::Surface,
        ShippedTextureKind::FixtureFace,
        ShippedTextureKind::DecalSheet,
    ] {
        assert!(
            kind.check_dimensions(96, 64).is_err(),
            "{kind:?} must not silently accept the NPOT diagnostic"
        );
    }
}

/// Every shipped file-backed sheet satisfies the dimension contract of the
/// class it draws as: surfaces are square (`tile_metres` cells are square on
/// both axes), fitted sheets are power-of-two on both edges (mipmapped fitted
/// sampling), and every sheet stays inside the decoder's
/// hard edge limit.
///
/// The diagnostic sheets are exempt: their whole purpose is to prove that
/// non-standard dimensions decode, and the 96x64 `alt` sheet is the deliberate
/// NPOT exception the policy documents rather than a production contract.
///
/// A texture used only as an emissive mask is exempt too: the shader samples a
/// mask with the material's own UVs and never compares its dimensions to the
/// albedo, so the engine imposes no shape on it. A texture that is also an
/// albedo or a normal map is still held to the surface contract.
#[test]
fn every_shipped_sheet_satisfies_its_texture_kind_contract() {
    let catalog = shipped_catalog();
    let root = resolve_asset_root().expect("assets/ is discoverable");
    let entries = catalog.entries();
    let mask_only: Vec<&str> = entries
        .iter()
        .filter_map(|entry| entry.emissive_mask.as_deref())
        .filter(|mask| {
            !entries.iter().any(|entry| {
                entry.texture.as_deref() == Some(*mask)
                    || entry.normal_texture.as_deref() == Some(*mask)
            })
        })
        .collect();
    let mut checked = 0usize;
    for entry in &entries {
        let kind = match entry.asset_type.as_str() {
            AssetType::TEXTURE => {
                if entry.asset_class.as_str() == AssetClass::DIAGNOSTIC
                    || mask_only.contains(&entry.id.as_str())
                {
                    continue;
                }
                ShippedTextureKind::Surface
            }
            AssetType::DECAL => ShippedTextureKind::DecalSheet,
            AssetType::LIGHT => ShippedTextureKind::FixtureFace,
            _ => continue,
        };
        if entry.source != AssetSource::File {
            continue;
        }
        let model = entry
            .model
            .as_deref()
            .unwrap_or_else(|| panic!("{}: a file-backed sheet needs a model", entry.id));
        let bytes = std::fs::read(root.join(model))
            .unwrap_or_else(|error| panic!("{}: cannot read `{model}`: {error}", entry.id));
        let image = crate::materials::decode_png(&bytes)
            .unwrap_or_else(|error| panic!("{}: `{model}` does not decode: {error}", entry.id));
        kind.check_dimensions(image.width, image.height)
            .unwrap_or_else(|error| panic!("{}: `{model}`: {error}", entry.id));
        checked += 1;
    }
    assert!(
        checked >= 20,
        "the shipped sheet set is unexpectedly small: only {checked} sheets checked"
    );
}

/// The renderer's shared untextured sheet is a real committed PNG.
///
/// It used to be synthesized from a hard-coded pixel array in `src/render.rs`;
/// it is now an ordinary catalogued texture, so it is covered by the same
/// validator and test set as every shipped sheet. This test is the guard that
/// the file (not a code fallback) is the sheet's source: it fails if the PNG
/// disappears, stops being opaque white, or stops being a legal sheet. The
/// exact resolution is texture policy (the production sheet is authored at the
/// 1024 hard budget), so the test pins the flat-fill properties and the
/// dimension contract rather than a historical placeholder size.
#[test]
fn the_shared_white_sheet_is_a_committed_opaque_white_png() {
    let catalog = shipped_catalog();
    let entry = catalog
        .get("core:tex_white_01")
        .expect("the shared white sheet must be catalogued");
    assert_eq!(entry.asset_class.as_str(), AssetClass::CORE);
    assert_eq!(entry.asset_type.as_str(), AssetType::TEXTURE);
    assert_eq!(entry.source, AssetSource::File);
    assert_eq!(
        catalog.texture_path("core:tex_white_01"),
        Some("core/textures/white_01.png")
    );

    let root = resolve_asset_root().expect("assets/ is discoverable");
    let bytes = std::fs::read(root.join("core/textures/white_01.png"))
        .expect("the committed white sheet must exist on disk");
    assert!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "the white sheet must be a PNG"
    );
    let image = crate::materials::decode_png(&bytes).expect("the white sheet decodes");
    assert!(
        image.width.is_power_of_two() && image.height.is_power_of_two(),
        "the white sheet must be power-of-two, found {}x{}",
        image.width,
        image.height
    );
    assert_eq!(
        image.width, image.height,
        "the white sheet must be a square flat fill, found {}x{}",
        image.width, image.height
    );
    assert!(
        image.width >= PREFERRED_TEXTURE_DIMENSION,
        "the white sheet must not be a placeholder-size fill, found {}x{}",
        image.width,
        image.height
    );
    assert!(
        image.width <= MAX_TEXTURE_DIMENSION && image.height <= MAX_TEXTURE_DIMENSION,
        "the white sheet must stay within the {MAX_TEXTURE_DIMENSION}-texel hard limit, \
         found {}x{}",
        image.width,
        image.height
    );
    // The production sheet is a flat white fill with sub-1% dither (every
    // channel sits in 252..=255); what the renderer needs is a sheet that
    // reads as opaque white with no pattern or stain, so the test pins the
    // opaque near-white band rather than byte equality with a 2x2 fill.
    for texel in image.rgba.as_chunks::<4>().0 {
        assert_eq!(
            texel[3], 255,
            "every texel of the shared white sheet must be opaque"
        );
        for channel in &texel[..3] {
            assert!(
                *channel >= 250,
                "the shared white sheet must stay within 5/255 of white, found {texel:?}"
            );
        }
    }
}

/// A minimal catalog with an albedo texture and a mask texture; `extra`
/// appends fields to the third entry, the material.
fn emissive_catalog_json(extra: &str) -> String {
    format!(
        r##"{{
            "assets": [
                {{ "id": "core:tex_albedo", "asset_class": "environment", "asset_type": "texture",
                  "source": "file",
                  "model": "environment/office/textures/ceilings/ceiling_panel_01.png" }},
                {{ "id": "core:tex_mask", "asset_class": "environment", "asset_type": "texture",
                  "source": "file",
                  "model": "environment/office/textures/floors/carpet_beige_01.png" }},
                {{ "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_albedo"{extra} }}
            ]
        }}"##
    )
}

#[test]
fn emissive_materials_parse_and_are_exposed_only_through_material_accessors() {
    let json = emissive_catalog_json(
        r#", "emissive": [0.25, 0.5, 1.0], "emissive_intensity": 2.0,
           "emissive_mask": "core:tex_mask""#,
    );
    let catalog = AssetCatalog::from_json_str(&json).expect("emissive catalog parses");
    let material = catalog.material("core:mat").expect("material");
    assert_eq!(material.emissive, Some([0.25, 0.5, 1.0]));
    assert_eq!(material.emissive_intensity, Some(2.0));
    assert_eq!(material.emissive_mask.as_deref(), Some("core:tex_mask"));
    assert_eq!(
        catalog.material_emissive("core:mat"),
        Some([0.25, 0.5, 1.0])
    );
    assert_eq!(catalog.material_emissive_intensity("core:mat"), Some(2.0));
    assert_eq!(
        catalog.material_emissive_mask("core:mat"),
        Some("core:tex_mask")
    );

    // An authored colour may stand alone: intensity and mask stay unauthored.
    let bare = emissive_catalog_json(r#", "emissive": [1.0, 0.0, 0.0]"#);
    let catalog = AssetCatalog::from_json_str(&bare).expect("bare emissive catalog parses");
    assert_eq!(catalog.material_emissive("core:mat"), Some([1.0, 0.0, 0.0]));
    assert_eq!(catalog.material_emissive_intensity("core:mat"), None);
    assert_eq!(catalog.material_emissive_mask("core:mat"), None);

    // Emission can never be read off a non-material, authored or not.
    for id in ["core:tex_albedo", "core:tex_mask", "core:absent"] {
        assert_eq!(catalog.material_emissive(id), None, "{id}");
        assert_eq!(catalog.material_emissive_intensity(id), None, "{id}");
        assert_eq!(catalog.material_emissive_mask(id), None, "{id}");
    }
}

#[test]
fn catalog_rejects_malformed_emission_values() {
    let cases = [
        (r#", "emissive": [0.1, 0.2]"#, "exactly three channels"),
        (
            r#", "emissive": [0.1, 0.2, 0.3, 0.4]"#,
            "exactly three channels",
        ),
        (r#", "emissive": [1.5, 0.0, 0.0]"#, "between 0.0 and 1.0"),
        (r#", "emissive": [-0.1, 0.0, 0.0]"#, "between 0.0 and 1.0"),
        (r#", "emissive_intensity": 2.0"#, "requires an `emissive`"),
        (
            r#", "emissive_mask": "core:tex_mask""#,
            "requires an `emissive`",
        ),
        (
            r#", "emissive": [1.0, 1.0, 1.0], "emissive_intensity": 9.0"#,
            "emissive_intensity",
        ),
        (
            r#", "emissive": [1.0, 1.0, 1.0], "emissive_intensity": -1.0"#,
            "emissive_intensity",
        ),
        (
            r#", "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core tex mask""#,
            "well-formed logical asset id",
        ),
    ];
    for (extra, needle) in cases {
        let json = emissive_catalog_json(extra);
        let error =
            AssetCatalog::from_json_str(&json).expect_err(&format!("must be rejected: {extra}"));
        assert!(
            error.contains(needle),
            "error {error:?} does not mention {needle:?}"
        );
    }
}

#[test]
fn catalog_rejects_non_finite_emission_values() {
    // JSON cannot carry NaN or infinity, so the boundary values are injected
    // into the parsed file directly.
    let json = emissive_catalog_json(r#", "emissive": [1.0, 0.0, 0.0], "emissive_intensity": 1.0"#);

    let mut file: CatalogFile = serde_json::from_str(&json).expect("catalog JSON parses");
    file.assets[2].emissive_intensity = Some(f32::NAN);
    let error = file.assets[2].convert(false).expect_err("NaN intensity");
    assert!(error.contains("emissive_intensity"), "error: {error}");

    let mut file: CatalogFile = serde_json::from_str(&json).expect("catalog JSON parses");
    file.assets[2].emissive = Some(vec![f32::INFINITY, 0.0, 0.0]);
    let error = file.assets[2].convert(false).expect_err("infinite channel");
    assert!(error.contains("emissive"), "error: {error}");
}

#[test]
fn catalog_rejects_emission_outside_material_definitions() {
    let on_prop = r##"{
        "assets": [
            { "id": "core:prop", "asset_class": "environment", "asset_type": "prop",
              "source": "file", "model": "core/props/models/couch.glb",
              "emissive": [1.0, 1.0, 1.0] }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(on_prop).expect_err("emission on a prop");
    assert!(error.contains("only a `material`"), "error: {error}");

    // A material whose source is not `definition` may not emit either.
    let generated = r##"{
        "assets": [
            { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
              "source": "file", "model": "environment/office/textures/walls/a.png" },
            { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
              "source": "generated", "texture": "core:tex_a",
              "emissive": [1.0, 1.0, 1.0] }
        ]
    }"##;
    let error = AssetCatalog::from_json_str(generated).expect_err("emission on a generated asset");
    assert!(
        error.contains("only a `material` `definition`"),
        "error: {error}"
    );
}

#[test]
fn catalog_rejects_emissive_masks_that_are_not_loadable_textures() {
    let dangling =
        emissive_catalog_json(r#", "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core:tex_nope""#);
    let error = AssetCatalog::from_json_str(&dangling).expect_err("dangling mask");
    assert!(error.contains("core:tex_nope"), "error: {error}");
    assert!(error.contains("emissive mask"), "error: {error}");

    // A mask must name a texture, not another material.
    let wrong_type =
        emissive_catalog_json(r#", "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core:mat""#);
    let error = AssetCatalog::from_json_str(&wrong_type).expect_err("non-texture mask");
    assert!(error.contains("not a texture"), "error: {error}");
}

#[test]
fn catalog_rejects_an_emissive_mask_without_a_png_file() {
    let json =
        emissive_catalog_json(r#", "emissive": [1.0, 1.0, 1.0], "emissive_mask": "core:tex_mask""#);
    let mut catalog = AssetCatalog::from_json_str(&json).expect("catalog parses");
    let mask = catalog
        .entries
        .get_mut("core:tex_mask")
        .expect("mask entry");
    mask.source = AssetSource::Generated;
    let material = catalog.entries.get("core:mat").expect("material");
    let error = check_emissive_mask(material, &catalog).expect_err("mask without a file");
    assert!(error.contains("no PNG file"), "error: {error}");
}
