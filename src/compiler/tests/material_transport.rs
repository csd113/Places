//! Offline compilation must retain source colour and numeric alpha for transport.
use super::*;

#[test]
fn offline_materials_include_real_glass_coverage_and_wall_reflectance() -> Result<(), String> {
    let level = LevelDef::from_json(include_str!(
        "../../../tests/fixtures/levels/art_style_hero.json"
    ))
    .map_err(|error| error.to_string())?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let catalog = crate::loader::PropCatalog::load_from_path(&root.join("catalog.json"))
        .ok_or_else(|| "hero catalog is unavailable".to_owned())?;
    let materials = resolve_build_materials(&level, &catalog, &root)?;
    let glass = materials
        .entry_of("core:glass_window_clear_01")
        .ok_or_else(|| "hero glass material is unavailable".to_owned())?;
    assert_eq!(glass.alpha.mode, crate::materials::AlphaMode::Blend);
    let image = glass
        .image
        .as_ref()
        .ok_or_else(|| "offline glass has no decoded PNG".to_owned())?;
    assert!(
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| { pixel.get(3).is_some_and(|alpha| *alpha > 0 && *alpha < 128) }),
        "clear glass must preserve its partial numeric PNG coverage"
    );
    let wall = materials
        .entry_of("home:wall_paint_warm_01")
        .ok_or_else(|| "hero wall material is unavailable".to_owned())?;
    assert!(
        wall.image.is_some(),
        "offline bounce must read wall PNG colour"
    );
    assert!(
        wall.error.is_none() && glass.error.is_none(),
        "the hero must resolve source assets rather than diagnostic fallbacks"
    );
    Ok(())
}
