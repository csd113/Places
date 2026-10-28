//! Essential bounded CPU regression: regeneration/joins, opening bands and
//! contact subtraction are not covered by the former primitive paths. These
//! tiny scenes perform no bake, texture decode, GPU work or external I/O.
use crate::engine::{
    assets::AssetCatalog,
    level::{LevelDef, LevelSurfaces},
    loader::validate_level,
};
use crate::map_compiler::prepare_level;
use serde_json::json;

fn catalog() -> Result<AssetCatalog, String> {
    AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[
      {"id":"wall","display_name":"Wall","asset_class":"core","asset_type":"material","source":"definition","texture":"tex","baseboard":"trim"},
      {"id":"trim","display_name":"Trim","asset_class":"core","asset_type":"material","source":"definition","texture":"tex"},
      {"id":"tex","display_name":"PNG","asset_class":"core","asset_type":"texture","source":"file","model":"unused.png"}
    ]}"#,
    )
}
fn scene() -> serde_json::Value {
    json!({"schema":"places-level","format_version":1_u32,"id":"trim","name":"Trim fit",
        "spawn":{"x":1.0_f32,"z":1.0_f32},"defaults":{"wall":"wall"},
        "rooms":[{"width":8.0_f32,"depth":8.0_f32,"height":3.0_f32}]})
}
fn parse(raw: serde_json::Value) -> Result<LevelDef, String> {
    let level = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    validate_level(&level)?;
    Ok(level)
}
fn front_point(board: &crate::engine::level::BaseboardDef, end: bool) -> (f32, f32) {
    let (low, high) = board.span_at(board.thickness());
    let along = if end { high } else { low };
    let (dx, dz) = board.direction();
    let (ax, az) = board.across();
    (
        dx.mul_add(along, ax.mul_add(board.thickness(), board.x)),
        dz.mul_add(along, az.mul_add(board.thickness(), board.z)),
    )
}

fn assert_front_texture_phase(pair: &[crate::engine::level::BaseboardDef]) -> Result<(), String> {
    let [current, next] = pair else {
        return Err("Expected adjacent trim faces".into());
    };
    let (_, end) = current.span_at(current.thickness());
    let (start, _) = next.span_at(next.thickness());
    assert!(
        (current.uv_offset + end - next.uv_offset - start).abs() < 0.00001_f32,
        "mitered fronts share a continuous physical texture phase"
    );
    Ok(())
}

#[test]
fn attached_rings_close_at_every_corner_and_regenerate_without_stacking() -> Result<(), String> {
    let catalog = catalog()?;
    for (placement, bands) in [("none", 0_usize), ("bottom", 1), ("top", 1), ("both", 2)] {
        let mut raw = scene();
        drop(raw.as_object_mut().ok_or("Expected source object")?.insert("columns".into(), json!([{"x":2.0_f32,"z":2.0_f32,"width":0.6_f32,"depth":1.1_f32,"trim":{"placement":placement,"material":"trim"}}])));
        drop(raw.as_object_mut().ok_or("Expected source object")?.insert("pillars".into(), json!([{"x":5.0_f32,"z":4.0_f32,"radius":0.45_f32,"segments":16_u32,"trim":{"placement":placement,"height":0.12_f32,"thickness":0.03_f32,"material":"trim"}}])));
        let mut level = parse(raw)?;
        prepare_level(&mut level, &catalog, None);
        assert_eq!(
            level.baseboards.len(),
            20_usize.saturating_mul(bands),
            "each perimeter has exactly one requested band"
        );
        for ring in level
            .baseboards
            .get(..4_usize.saturating_mul(bands))
            .unwrap_or_default()
            .as_chunks::<4>()
            .0
            .iter()
            .map(<[crate::engine::level::BaseboardDef; 4]>::as_slice)
            .chain(
                level
                    .baseboards
                    .get(4_usize.saturating_mul(bands)..)
                    .unwrap_or_default()
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .map(<[crate::engine::level::BaseboardDef; 16]>::as_slice),
            )
        {
            for pair in ring.windows(2) {
                assert_front_texture_phase(pair)?;
            }
            for (current, next) in ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .take(ring.len())
            {
                let a = front_point(current, true);
                let b = front_point(next, false);
                assert!(
                    (a.0 - b.0).hypot(a.1 - b.1) < 0.00001_f32,
                    "mitered outer edges meet without gaps or overlap"
                );
                assert_eq!(
                    current.material.as_deref(),
                    Some("trim"),
                    "material selection survives generation"
                );
            }
        }
        let once = serde_json::to_value(&level).map_err(|e| e.to_string())?;
        prepare_level(&mut level, &catalog, None);
        assert_eq!(
            serde_json::to_value(&level).map_err(|e| e.to_string())?,
            once,
            "repeat preparation replaces output without changing it"
        );
        if let Some(pillar) = level.pillars.first_mut() {
            pillar.x += 0.5_f32;
            pillar.radius = 0.7_f32;
        }
        prepare_level(&mut level, &catalog, None);
        assert_eq!(
            level.baseboards.len(),
            20_usize.saturating_mul(bands),
            "move and radius edits regenerate rather than stack"
        );
        assert!(
            level
                .collision_aabbs()
                .iter()
                .any(|solid| solid.min_x > 4.0_f32 && solid.max_x < 7.0_f32),
            "round pillar uses existing round collision decomposition"
        );
    }
    Ok(())
}

#[test]
fn wall_trim_splits_only_intersecting_openings_and_refuses_gapped_contacts() -> Result<(), String> {
    let catalog = catalog()?;
    let mut raw = scene();
    drop(raw.as_object_mut().ok_or("Expected source object")?.insert(
        "walls".into(),
        json!([{"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":0.2_f32,"height":3.0_f32,
        "openings":[{"offset":2.0_f32,"width":1.0_f32,"height":2.1_f32}],
        "trim":{"placement":"both","material":"trim"}}]),
    ));
    let mut level = parse(raw.clone())?;
    prepare_level(&mut level, &catalog, None);
    assert_eq!(
        level.baseboards.len(),
        3,
        "doorway splits bottom band and leaves ceiling band continuous"
    );
    assert_eq!(
        level
            .baseboards
            .iter()
            .filter(|b| b.base_y(&LevelSurfaces::new(&level)) < 0.1_f32)
            .count(),
        2,
        "only floor-reaching opening cuts the bottom band"
    );
    *raw.pointer_mut("/walls/0/trim/placement")
        .ok_or("Missing wall attachment")? = json!("none");
    let mut disabled = parse(raw.clone())?;
    prepare_level(&mut disabled, &catalog, None);
    assert!(
        disabled.baseboards.is_empty(),
        "explicit None disables the catalog's automatic trim too"
    );
    *raw.pointer_mut("/walls/0/trim/placement")
        .ok_or("Missing wall attachment")? = json!("both");
    drop(
        raw.pointer_mut("/walls/0")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("Missing wall")?
            .insert("y".into(), json!(0.02_f32)),
    );
    assert!(
        parse(raw).is_err(),
        "a visible floor gap must be refused before applying or compiling"
    );
    Ok(())
}
