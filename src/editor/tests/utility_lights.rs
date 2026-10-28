//! Protects measured mount/emission anchors for five floor-origin GLBs and
//! circuit-aware document persistence. Five small model parses, no GPU/bake.
use crate::editor::document::{Document, ObjectKind};
use crate::engine::{
    assets::AssetCatalog, lighting::fixture_profile, loader::PropCatalog, props::PropAssets,
};

#[test]
fn utility_light_mounts_follow_delivered_lenses_and_save_as_circuit_fixtures() -> Result<(), String>
{
    let path = std::env::temp_dir().join(format!("places-utility-lights-{}", std::process::id()));
    let mut doc = Document::create(&path, "utility_lights", "Utility lights")?;
    let result = workflow(&mut doc, &path);
    let _cleanup = std::fs::remove_dir_all(&path);
    result
}
fn workflow(doc: &mut Document, path: &std::path::Path) -> Result<(), String> {
    let initial_fixtures = doc.level().ceiling_lights.len();
    let catalog = AssetCatalog::load_default();
    let props = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    for variant in [
        "recessed",
        "surface",
        "hanging",
        "damaged",
        "missing_diffuser",
    ] {
        let id = format!("backrooms:light_fluorescent_{variant}");
        let profile = fixture_profile(&id);
        let model = profile.model.ok_or("model mapping")?;
        let definition = props.get(model);
        let asset = assets.resolve(definition.model.as_deref().ok_or("GLB")?)?;
        let (_min, max) = asset.model.bounds().ok_or("bounds")?;
        assert!(
            (max[1] + profile.model_offset_y).abs() < 0.0001,
            "actual GLB top meets the mounting plane"
        );
        let emitter = profile.emission_y(3.0);
        assert!(
            emitter < 3.0 && emitter > 3.0 + profile.model_offset_y,
            "emitter lies at lens/tube height below mount"
        );
        assert_eq!(
            profile.emissive_slots,
            &[2],
            "housing, broken diffuser and service label must stay passive"
        );
        let placed =
            doc.add_with_catalog(ObjectKind::Light, [2.0, 3.0, 2.0], Some(&id), &catalog)?;
        doc.mount_to_surface(placed, None)?;
        assert!(doc.undo(), "mount is reversible");
        assert!(doc.redo());
    }
    assert_eq!(
        doc.level().ceiling_lights.len(),
        initial_fixtures.saturating_add(5)
    );
    doc.save()?;
    assert_eq!(
        Document::open(path)?.raw(),
        doc.raw(),
        "operational fixture IDs survive reopening"
    );
    Ok(())
}
