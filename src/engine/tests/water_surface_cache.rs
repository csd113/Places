//! Essential cache safety: hints address the canonical GPU priority tree;
//! overlaps/nearby planes retain full queries and preparation has a hard cap.
//! One tiny CPU fixture (three rectangles, no GPU, assets or map bake; <1 ms).
use super::{Node, leaf, surface_hints, tree};
use crate::engine::level::{CausticsDef, LevelDef, WaterVolumes};

#[test]
fn cached_surface_ownership_preserves_priority_and_overlap_fallback() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"water_cache","name":"Water cache","spawn":{"x":0,"z":0},"rooms":[{"x":-1,"z":-1,"width":12,"depth":4,"floor_y":-1,"height":4}],"water":[{"x":0,"z":0,"width":2,"depth":2,"surface_y":0,"bottom_y":-1},{"x":2,"z":0,"width":2,"depth":2,"surface_y":0,"bottom_y":-1},{"x":7,"z":0,"width":2,"depth":2,"surface_y":0,"bottom_y":-1}]}"#).map_err(|error| error.to_string())?;
    let hints = surface_hints(&level);
    assert_eq!(hints.len(), 3);
    let volumes = WaterVolumes::from_level(&level);
    let mut leaves: Vec<_> = volumes
        .volumes()
        .iter()
        .enumerate()
        .map(|(index, volume)| leaf(volume, index, CausticsDef::default(), [0; 4]))
        .collect();
    let mut gpu_nodes: Vec<Node> = Vec::new();
    tree(&mut leaves, &mut gpu_nodes);
    for (index, hint) in hints.iter().enumerate() {
        let leaf = usize::from(*hint)
            .checked_sub(1)
            .and_then(|offset| gpu_nodes.get(offset))
            .ok_or("Unambiguous surface lost its GPU leaf")?;
        assert_eq!(
            leaf.meta[2],
            u32::try_from(index)
                .map_err(|error| error.to_string())?
                .saturating_add(1)
        );
    }
    let overlap_definition = level.water.get_mut(1).ok_or("second water missing")?;
    overlap_definition.x = 1.0;
    let overlapping = surface_hints(&level);
    assert_eq!(
        overlapping.get(..2),
        Some([0_u16, 0].as_slice()),
        "Both overlapping surfaces must retain pixel priority selection"
    );
    assert!(overlapping.get(2).is_some_and(|hint| *hint > 0));
    let nearby_definition = level.water.get_mut(1).ok_or("second water missing")?;
    nearby_definition.surface_y = 0.0007;
    assert_eq!(
        surface_hints(&level).get(..2),
        Some([0_u16, 0].as_slice()),
        "The shader plane tolerance must not create an overlap shortcut"
    );
    let separated_definition = level.water.get_mut(1).ok_or("second water missing")?;
    separated_definition.surface_y = 2.0;
    assert!(
        surface_hints(&level).iter().all(|hint| *hint > 0),
        "Disjoint planes are independently cacheable"
    );
    let template = level.water.first().ok_or("first water missing")?.clone();
    level.water = std::iter::repeat_n(template, 257).collect();
    assert!(
        surface_hints(&level).is_empty(),
        "Large authored sets use full queries without quadratic cache preparation"
    );
    Ok(())
}
