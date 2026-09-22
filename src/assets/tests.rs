//! Unit tests for the asset catalog.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
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
    assert_eq!(fixture.source, AssetSource::Generated);
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

    let themes: Vec<&str> = catalog
        .themes()
        .iter()
        .map(|theme| theme.id.as_str())
        .collect();
    assert_eq!(themes, ["office", "pool"]);
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
        "the old assets/props/models/spooner-man.glb must not survive the migration"
    );
    assert!(
        !root.join("catalog.json").is_dir() && root.join("catalog.json").is_file(),
        "the catalog lives at the asset root"
    );
}

#[test]
fn shipped_levels_only_reference_catalog_ids() {
    let catalog = shipped_catalog();
    let mut levels = 0;
    let mut props = 0;
    let mut spooner_levels = 0;

    for dir in ["assets/levels", "levels"] {
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
    assert!(levels >= 10, "expected the shipped and custom levels");
    assert!(props >= 1, "expected placed props");
    assert!(
        spooner_levels >= 1,
        "at least one shipped level must still reference `spooner-man`"
    );
}

#[test]
fn renderer_and_catalog_agree_on_surface_and_decal_ids() {
    let catalog = shipped_catalog();

    // Every catalogued surface material must resolve through the material
    // table to a decoded PNG: no material is allowed to depend on a
    // renderer-known id or a code-generated sheet any more.
    let level = LevelDef::from_json(include_str!("../../assets/levels/level1.json"))
        .expect("level1 parses");
    let mut cache = crate::materials::TextureCache::new();
    let root = resolve_asset_root().expect("assets/ is discoverable");
    let table =
        crate::materials::resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
    assert!(
        table.errors().is_empty(),
        "level1 materials must all resolve: {:?}",
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

    // Every catalogued light fixture must have a built-in appearance, so a
    // catalog entry can never silently render as some other fixture.
    let mut fixtures = 0usize;
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
            AssetSource::Generated,
            "{}: the built-in fixtures are generated resources",
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
    }
}

#[test]
fn level_material_references_cover_floor_regions_too() {
    let catalog = shipped_catalog();
    let mut checked = 0usize;
    for dir in ["assets/levels", "levels"] {
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
