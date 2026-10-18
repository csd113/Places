//! One four-node fixture protects wrong-floor smoothing and the PNV2 boundary.
//! The short body fits both close storeys, exposing the former additive slope
//! tolerance bug. No bake/assets/device: the test costs four graph nodes.

use crate::engine::nav::{NavDoorState, NavMesh, NoDoors};
use crate::engine::package::navigation::{
    CELL_SURFACE, NO_NAV_LINK, NO_PORTAL, NavClass, NavGrid, read_navigation, write_navigation,
};
use glam::Vec3;

fn close_storeys() -> Result<NavGrid, String> {
    let class = NavClass::new(0.03, 0.2, 0.1, 2.6667).ok_or("invalid class")?;
    let mut links = vec![[NO_NAV_LINK; 8]; 4];
    for (node, direction, next) in [(0, 1, 2), (2, 0, 0), (1, 1, 3), (3, 0, 1)] {
        *links
            .get_mut(node)
            .and_then(|row| row.get_mut(direction))
            .ok_or("missing fixture link")? = next;
    }
    Ok(NavGrid {
        cell_m: 0.2,
        origin_x: 0.0,
        origin_z: 0.0,
        cells_x: 2,
        cells_z: 1,
        column_offsets: vec![0, 2, 4],
        links: vec![links],
        classes: vec![class],
        cell_y: vec![0.0, 0.6, 0.0, 0.6],
        cell_flags: vec![CELL_SURFACE; 4],
        cell_headroom_cm: vec![60; 4],
        cell_portal: vec![NO_PORTAL; 4],
        portals: vec![],
        walkable: vec![vec![15]],
        region: vec![vec![0, 1, 0, 1]],
    })
}

struct UpperBlocked;
impl NavDoorState for UpperBlocked {
    fn door_open(&self, _id: &str) -> bool {
        true
    }
    fn door_locked(&self, _id: &str) -> Option<bool> {
        Some(false)
    }
    fn door_leaf(&self, _id: &str) -> Option<crate::engine::collision::DoorCollider> {
        None
    }
    fn obstacle_blocked(&self, position: Vec3, _radius: f32, _height: f32) -> bool {
        position.y > 0.3
    }
}

#[test]
fn close_storeys_keep_endpoint_identity_and_reject_obsolete_or_forged_records() -> Result<(), String>
{
    let grid = close_storeys()?;
    let bytes = write_navigation(&grid)?;
    assert_eq!(
        read_navigation(&bytes)?,
        grid,
        "layered codec must preserve offsets and exact links"
    );
    let mesh = NavMesh::from_record(grid.clone())?;
    let lower = Vec3::new(0.1, 0.0, 0.1);
    let upper = Vec3::new(0.3, 0.6, 0.1);
    assert!(
        !mesh.segment_clear(0, lower, upper, &NoDoors, false),
        "slope tolerance cannot switch floor identity"
    );
    assert!(
        !mesh.segment_clear(0, lower, upper, &UpperBlocked, false),
        "a blocked upper endpoint cannot fall through to a lower span"
    );
    let mut obsolete = bytes;
    obsolete
        .get_mut(..4)
        .ok_or("missing magic")?
        .copy_from_slice(b"PNV1");
    assert!(
        read_navigation(&obsolete)
            .err()
            .is_some_and(|error| error.contains("recompile")),
        "old navigation requires a named safe recompilation refusal"
    );
    let mut forged = grid;
    *forged
        .links
        .first_mut()
        .and_then(|links| links.first_mut())
        .and_then(|links| links.get_mut(1))
        .ok_or("missing fixture edge")? = 3;
    assert!(
        NavMesh::from_record(forged).is_err(),
        "cross-floor forged link with no reciprocal traversal is rejected"
    );
    Ok(())
}
