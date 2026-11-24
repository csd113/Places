//! Conservative joint-cluster proxies for the current skinned body.
use std::sync::OnceLock;

use glam::Vec3;

use crate::engine::collision::DoorCollider;
use crate::engine::gltf::PropModel;

pub(super) struct Clusters {
    joint_vertices: Vec<Vec<u16>>,
    posed_colliders: OnceLock<Vec<DoorCollider>>,
}

impl Clusters {
    pub(super) fn new(model: &PropModel) -> Self {
        let count = model.skin.as_ref().map_or(0, |skin| skin.joints.len());
        let mut joint_vertices = vec![Vec::new(); count];
        for face in model.indices.as_chunks::<3>().0 {
            let [first, ..] = *face;
            let vertex = usize::from(first);
            let Some((joints, weights)) = model.joints.get(vertex).zip(model.weights.get(vertex))
            else {
                continue;
            };
            // Preserve the first vertex's strongest joint and the original tie
            // break. The whole triangle contributes, including boundary faces.
            let Some(cluster) = weights
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .and_then(|(i, _)| joints.get(i))
                .and_then(|slot| joint_vertices.get_mut(usize::from(*slot)))
            else {
                continue;
            };
            cluster.extend_from_slice(face);
        }
        Self {
            joint_vertices,
            posed_colliders: OnceLock::new(),
        }
    }

    // Call whenever the world-space skin is refilled, including a rollback or
    // checkpoint restore. Lifecycle changes gate access to this same skin.
    pub(super) fn invalidate(&mut self) {
        let _previous = self.posed_colliders.take();
    }

    pub(super) fn colliders(&self, vertices: &[Vec3]) -> &[DoorCollider] {
        self.posed_colliders.get_or_init(|| {
            self.joint_vertices
                .iter()
                .filter_map(|indices| cluster_collider(indices, vertices))
                .collect()
        })
    }
}

fn cluster_collider(indices: &[u16], vertices: &[Vec3]) -> Option<DoorCollider> {
    let (lo, hi) = indices
        .iter()
        .filter_map(|index| vertices.get(usize::from(*index)))
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &point| (lo.min(point), hi.max(point)),
        );
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    Some(DoorCollider {
        npc_body: true,
        hinge_x: lo.x,
        hinge_y: lo.y,
        hinge_z: lo.z.midpoint(hi.z),
        dir_x: 1.0,
        dir_z: 0.0,
        width: (hi.x - lo.x).max(0.001),
        height: (hi.y - lo.y).max(0.001),
        thickness: (hi.z - lo.z).max(0.001),
    })
}
