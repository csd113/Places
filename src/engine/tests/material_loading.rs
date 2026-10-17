//! A four-byte budget proves early rejection and alias charging with three
//! numeric 1x1 PNGs; no whole map, filesystem asset tree or GPU is needed.

use std::collections::HashMap;
use std::sync::Arc;

use super::{
    AssetCatalog, LevelDef, PackMaterials, RawImage, TextureCache, resolve_materials_with_budget,
};
use crate::engine::materials::encode_png;

#[test]
fn material_budget_stops_before_later_decodes_and_charges_shared_images_once() -> Result<(), String>
{
    let mut level = LevelDef::from_json(
        r#"{"format_version":3,"id":"material_budget","name":"Budget","spawn":{"x":0,"z":0},
        "defaults":{"wall":"pack:a","floor":"pack:b","ceiling":"pack:c"}}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut textures = HashMap::new();
    for (name, value) in [("a.png", 1_u8), ("b.png", 2_u8), ("c.png", 3_u8)] {
        let bytes = encode_png(&RawImage::new(1, 1, vec![value, 0, 0, 255]))?;
        drop(textures.insert(name.to_string(), Arc::<[u8]>::from(bytes)));
    }
    let pack = PackMaterials::new("budget", Some(r#"{"pack:alias":"a.png"}"#), textures);
    assert_eq!(
        pack.cache_key("textures/a.png"),
        pack.cache_key("a.png"),
        "filename fallback must use the matched entry's shared texture identity"
    );
    let catalog = AssetCatalog::builtin();
    let mut cache = TextureCache::new();
    let error = resolve_materials_with_budget(&level, &catalog, Some(&pack), None, &mut cache, 4)
        .err()
        .ok_or("over-budget table unexpectedly resolved")?;
    assert!(
        error.contains("8 (limit 4)"),
        "the second unique RGBA texel crosses the budget: {error}"
    );
    assert_eq!(
        cache.decoded_count(),
        2,
        "budget failure must stop before the third texture decodes"
    );

    level.defaults.floor = "pack:alias".into();
    level.defaults.ceiling = "pack:a".into();
    cache.clear();
    let shared = resolve_materials_with_budget(&level, &catalog, Some(&pack), None, &mut cache, 4)?;
    assert_eq!(
        shared.decoded_texture_bytes(),
        4,
        "two material ids share one charged texture"
    );
    assert_eq!(cache.decoded_count(), 1, "shared source pixels decode once");
    Ok(())
}
