//! Focused audit for the regional-fog and void-wall contracts (workstream 5).
//!
//! Runs the small validation scene
//! `tests/fixtures/levels/fog_void_validation.json` through the whole offline
//! path — parse, validate, compile to a `.placesmap` package, load the package
//! back — and pins the authored contract at every step: the resolved regional
//! fog values, the preset upload prefix, the emitted void-wall geometry, the
//! collision/bake participation flags, and the package round-trip.
//!
//! This is deliberately a *small validation scene* rather than a shipped map:
//! it exists so the fog/void contract has one end-to-end regression that does
//! not depend on the demo's content.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are
// idiomatic in tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use crate::level::{FogRegionDef, LevelDef, MAX_FOG_REGIONS};
use crate::quality::QualityLevel;
use crate::render::{FogRegion, LevelFog};

/// The validation scene source, kept beside the other level fixtures.
const VALIDATION_SCENE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/levels/fog_void_validation.json"
));

/// Parses and validates the scene exactly as the loader would.
fn scene() -> LevelDef {
    let level = LevelDef::from_json(VALIDATION_SCENE).expect("the validation scene parses");
    crate::loader::validate_level(&level).expect("the validation scene validates");
    level
}

/// The authored regional fog resolves into the documented world values: an
/// omitted field means the box floor/ceiling, the global colour and the
/// default falloff, and every region keeps its authoring order.
#[test]
fn fog_void_validation_scene_resolves_its_regions() {
    let level = scene();
    assert_eq!(
        level.fog_regions.len(),
        2,
        "the scene authors a low mist and a taller denser layer"
    );
    let fog = LevelFog::from_level(&level);
    assert_eq!(fog.regions.len(), 2);
    let mist: FogRegion = fog.regions[0];
    assert_eq!(mist.ground_y, -0.1, "the authored ground layer survives");
    assert_eq!(mist.top_y, 0.5, "the authored top survives");
    assert_eq!(mist.falloff_m, 3.0);
    // A different camera position never changes a fragment's regional term:
    // the contribution is a pure function of the world position.
    let inside = mist.contribution([18.0, 0.2, 0.0]);
    assert!(inside > 0.0, "the yard fragment is inside the mist layer");
    assert_eq!(
        mist.contribution([-4.0, 0.2, 0.0]),
        0.0,
        "the indoor room stays outside the mist"
    );
    // The low preset uploads the first two regions; the scene authors exactly
    // two, so every preset shows both.
    for quality in [QualityLevel::Low, QualityLevel::Medium, QualityLevel::High] {
        assert_eq!(fog.uploaded_region_count(quality), 2, "{quality:?}");
    }
}

/// The void-wall flags are independent: the sight-only shell draws but never
/// collides or occludes, the under-plate draws, collides and occludes.
#[test]
fn fog_void_validation_scene_keeps_the_void_flags_independent() {
    let level = scene();
    assert_eq!(level.void_walls.len(), 2);
    let shell = level
        .void_walls
        .iter()
        .find(|wall| wall.id.as_deref() == Some("yard_shell"))
        .expect("the shell exists");
    assert!(!shell.solid && !shell.occludes, "the shell is sight only");
    assert_eq!(shell.faces().len(), 6, "one quad per box face");
    let plate = level
        .void_walls
        .iter()
        .find(|wall| wall.id.as_deref() == Some("yard_underplate"))
        .expect("the under-plate exists");
    assert!(plate.solid && plate.occludes);
    let colliders = level.collision_aabbs();
    let plate_box = plate.resolved_box().expect("a real box");
    assert!(
        colliders.iter().any(|aabb| {
            (aabb.min_x - plate_box.min[0]).abs() < 1e-4
                && (aabb.min_y - plate_box.min[1]).abs() < 1e-4
                && (aabb.min_z - plate_box.min[2]).abs() < 1e-4
                && (aabb.max_x - plate_box.max[0]).abs() < 1e-4
                && (aabb.max_y - plate_box.max[1]).abs() < 1e-4
                && (aabb.max_z - plate_box.max[2]).abs() < 1e-4
        }),
        "the solid box collides"
    );
    let shell_box = shell.resolved_box().expect("a real box");
    assert!(
        !colliders.iter().any(|aabb| {
            (aabb.min_x - shell_box.min[0]).abs() < 1e-4
                && (aabb.min_y - shell_box.min[1]).abs() < 1e-4
                && (aabb.min_z - shell_box.min[2]).abs() < 1e-4
                && (aabb.max_x - shell_box.max[0]).abs() < 1e-4
                && (aabb.max_y - shell_box.max[1]).abs() < 1e-4
                && (aabb.max_z - shell_box.max[2]).abs() < 1e-4
        }),
        "the sight-only shell adds no collider"
    );
}

/// The scene compiles offline to a package (no GPU probe capture) and loads
/// back through the package reader with its fog and void content intact.
#[test]
fn fog_void_validation_scene_round_trips_through_a_package() {
    let root = std::env::temp_dir().join(format!("places-fog-void-package-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp dir");
    let source = root.join("fog_void_validation.json");
    std::fs::write(&source, VALIDATION_SCENE).expect("write the source");
    let out = root.join("fog_void_validation.placesmap");
    let request = crate::compiler::BuildRequest {
        source,
        out: out.clone(),
        asset_root: std::path::PathBuf::from("assets"),
        variants: vec![crate::quality::LightmapQuality::Off],
        workers: 1,
        force: true,
        capture_probes: false,
    };
    crate::compiler::build(&request).expect("the validation scene compiles offline");
    assert!(out.exists(), "the package was published");

    let manager = crate::loader::LevelManager::with_paths(
        root.join("assets/levels"),
        root.clone(),
        root.join("import"),
    );
    let entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == "fog_void_validation")
        .cloned()
        .expect("the package is discovered");
    let loaded = manager.load_level(&entry).expect("the package loads");
    assert_eq!(loaded.level.fog_regions.len(), 2);
    assert_eq!(loaded.level.void_walls.len(), 2);
    assert_eq!(
        loaded.level.fog_regions[0].id, "yard_mist",
        "authoring order survives the round trip"
    );
    // The referenced void-wall materials resolve, so the emitted faces bind a
    // real texture rather than the diagnostic fallback.
    let ids: Vec<&str> = loaded
        .materials
        .entries()
        .iter()
        .map(|material| material.id.as_str())
        .collect();
    assert!(
        ids.contains(&"outdoor:dirt_gravel_01"),
        "the shell material resolves: {ids:?}"
    );
    assert!(
        ids.contains(&"outdoor:concrete_pavement_01"),
        "the under-plate material resolves: {ids:?}"
    );
    assert!(
        loaded.level.fog_regions.len() <= MAX_FOG_REGIONS,
        "the scene is inside the shader budget"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The fog-region and void-wall caps are the shader/uniform contract; the
/// scene stays well inside both, and the serde defaults elide the optional
/// keys exactly as documented.
#[test]
fn fog_void_validation_scene_is_inside_the_shader_budget() {
    let level = scene();
    assert!(level.fog_regions.len() <= MAX_FOG_REGIONS);
    let def = FogRegionDef {
        id: "half_space".into(),
        min: [0.0, 0.0, 0.0],
        max: [1.0, 1.0, 1.0],
        density: 0.1,
        color: None,
        falloff_m: None,
        ground_y: Some(1.0),
        top_y: Some(0.5),
    };
    let resolved = FogRegion::resolve(&def, [0.2, 0.3, 0.4]);
    assert_eq!(
        resolved.vertical_factor(0.75),
        1.0,
        "a degenerate layer is a half-space below its base"
    );
    assert_eq!(resolved.vertical_factor(1.25), 0.0);
}
