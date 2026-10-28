//! One CPU-only junction regression protects the new contact subtraction before
//! atlas creation: a straight face meeting a polygonal circle and coplanar trim
//! cap ownership. No rendering, solve, files or assets are involved.
use super::{Contacts, area};
use crate::engine::{
    level::{LevelDef, LevelSurfaces},
    render::Vertex,
};
use glam::Vec3;

fn polygon(points: [[f32; 3]; 4]) -> Vec<Vertex> {
    points
        .into_iter()
        .map(|pos| Vertex {
            pos,
            ..Vertex::UNLIT
        })
        .collect()
}

#[test]
fn circular_junction_cuts_buried_faces_and_trim_caps_have_one_owner() -> Result<(), String> {
    let level: LevelDef = serde_json::from_value(serde_json::json!({
      "schema":"places-level","format_version":1_u32,"id":"contact","name":"Contact",
      "spawn":{"x":1.0_f32,"z":1.0_f32},"rooms":[{"width":6.0_f32,"depth":6.0_f32,"height":3.0_f32}],
      "walls":[{"x":0.0_f32,"z":1.9_f32,"width":4.0_f32,"depth":0.2_f32,"height":3.0_f32,"trim":{"placement":"none"}}],
      "pillars":[{"x":3.0_f32,"z":2.0_f32,"radius":0.5_f32,"height":3.0_f32,"segments":32_u32}],
      "baseboards":[
        {"x":0.0_f32,"z":4.0_f32,"length":2.0_f32,"thickness":0.1_f32,"height":0.1_f32},
        {"x":1.9_f32,"z":4.0_f32,"length":2.0_f32,"thickness":0.1_f32,"height":0.1_f32,"rotation_degrees":270.0_f32},
        {"x":0.0_f32,"z":4.0_f32,"length":2.0_f32,"thickness":0.1_f32,"height":0.1_f32}
      ]
    })).map_err(|error|error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    let surfaces = LevelSurfaces::new(&level);
    let contacts = Contacts::new(&level, &surfaces);
    let pieces = contacts.clip(
        polygon([
            [0.0_f32, 0.0_f32, 2.1_f32],
            [4.0_f32, 0.0_f32, 2.1_f32],
            [4.0_f32, 3.0_f32, 2.1_f32],
            [0.0_f32, 3.0_f32, 2.1_f32],
        ]),
        Vec3::Z,
        None,
        false,
    );
    let visible_area: f32 = pieces.iter().map(|piece| area(piece)).sum();
    assert!(
        (9.0..9.2).contains(&visible_area),
        "wall must stop on the rendered circular polygon, retaining both exposed sides"
    );
    assert!(
        pieces.iter().flatten().all(|vertex| {
            let x = vertex.pos[0];
            x <= 2.52_f32 || x >= 3.48_f32
        }),
        "a wall face buried in the pillar cannot leave a protruding interior cap"
    );
    let first = contacts.clip(
        polygon([
            [0.0_f32, 0.1_f32, 4.0_f32],
            [2.0_f32, 0.1_f32, 4.0_f32],
            [2.0_f32, 0.1_f32, 4.1_f32],
            [0.0_f32, 0.1_f32, 4.1_f32],
        ]),
        Vec3::Y,
        Some(0),
        false,
    );
    let second = contacts.clip(
        polygon([
            [1.9_f32, 0.1_f32, 4.0_f32],
            [1.8_f32, 0.1_f32, 4.0_f32],
            [1.8_f32, 0.1_f32, 6.0_f32],
            [1.9_f32, 0.1_f32, 6.0_f32],
        ]),
        Vec3::Y,
        Some(1),
        false,
    );
    let union_area: f32 = first
        .iter()
        .chain(second.iter())
        .map(|piece| area(piece))
        .sum();
    assert!(
        (union_area - 0.39_f32).abs() < 0.00001_f32,
        "an overlapping trim cap is owned exactly once, without z-fighting or a hole; area={union_area}"
    );
    assert!(
        roundoff_fragment_is_removed(&contacts),
        "sub-micrometre intersection residue cannot reach mesh/chart publication"
    );
    assert!(
        (duplicate_front_area(&contacts, 0) + duplicate_front_area(&contacts, 2) - 0.19_f32).abs()
            < 0.00001_f32,
        "manual and automatic coplanar trim fronts have one owner, like their caps"
    );
    bare_floor_excludes_hidden_receivers(&level);
    Ok(())
}

fn duplicate_front_area(contacts: &Contacts, owner: usize) -> f32 {
    contacts
        .clip(
            polygon([
                [0.0, 0.0, 4.1],
                [2.0, 0.0, 4.1],
                [2.0, 0.1, 4.1],
                [0.0, 0.1, 4.1],
            ]),
            Vec3::Z,
            Some(owner),
            false,
        )
        .iter()
        .map(|piece| area(piece))
        .sum()
}

fn bare_floor_excludes_hidden_receivers(level: &LevelDef) {
    let mut bare = level.clone();
    bare.pillars.clear();
    bare.baseboards.clear();
    for wall in &mut bare.walls {
        wall.trim = None;
    }
    let bare_contacts = Contacts::new(&bare, &LevelSurfaces::new(&bare));
    let floor = bare_contacts.clip(
        polygon([
            [0.0, 0.0, 3.0],
            [4.0, 0.0, 3.0],
            [4.0, 0.0, 1.9],
            [0.0, 0.0, 1.9],
        ]),
        Vec3::Y,
        None,
        true,
    );
    let exposed_area: f32 = floor.iter().map(|piece| area(piece)).sum();
    assert!(
        (exposed_area - 3.6).abs() < 0.00001,
        "floor charts must exclude the buried receiver strip at a bare wall contact"
    );
    assert!(
        floor
            .iter()
            .flatten()
            .all(|vertex| vertex.pos[2] >= 2.1 - 0.00001),
        "visible floor interpolation cannot include a receiver inside the wall"
    );
}

fn roundoff_fragment_is_removed(contacts: &Contacts) -> bool {
    // The second case retains the measured near-coincident contact corners
    // from native ceiling validation, translated onto this tiny fixture.
    [
        [
            [3.1, 0.0, 2.49],
            [3.1, 0.09, 2.49],
            [3.1, 0.09, 2.490_000_2],
            [3.1, 0.0, 2.490_000_2],
        ],
        [
            [3.361_115_5, 3.0, 2.490_458_3],
            [3.459_622_4, 3.0, 2.409_615_8],
            [3.459_615_7, 3.0, 2.409_623_4],
            [3.459_615_7, 3.0, 2.409_623_4],
        ],
    ]
    .into_iter()
    .all(|points| {
        let [corner_a, corner_b, corner_c, corner_d] = points.map(|pos| Vertex {
            pos,
            ..Vertex::UNLIT
        });
        let mut vertices = vec![corner_a, corner_b, corner_c, corner_a, corner_c, corner_d];
        super::stamp(
            contacts,
            None,
            &mut vertices,
            0,
            crate::engine::lighting::lightmap::PatchKind::Wall,
            None,
            None,
        );
        vertices.is_empty()
    })
}
