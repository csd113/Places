//! Exact CPU certificates for the explicit opaque any-hit GPU experiment.
//! No solver query calls this module. A GPU miss is never a certificate.
use super::super::{
    PreparedRay, TransportScene, TransportTriangle, parallel_map, ray_slab_entry, safe_inverse,
    slab_entry,
};
use serde::{Deserialize, Serialize};

/// Deliberately smaller than production limits: one bounded developer capture.
pub const MAX_TRIANGLES: usize = 131_072;
/// Largest captured ray batch; device queues use smaller reusable tiles.
pub const MAX_RAYS: usize = 16_384;

/// Portable ray input; the limit has already received the caller's endpoint policy.
pub type Ray = ([f32; 3], [f32; 3], f32);
/// GPU traversal node, copied from the CPU hierarchy.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
    /// Lower bound.
    pub min: [f32; 3],
    /// Upper bound.
    pub max: [f32; 3],
    /// First primitive or left child.
    pub first: u32,
    /// Primitive count, zero for an interior node.
    pub count: u32,
    /// Right child.
    pub right: u32,
}
/// Versioned diagnostic snapshot. IDs name source triangles, never packed slots.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    /// Schema version; old captures cannot certify canonical eligibility.
    pub version: u32,
    /// Opaque source geometry in its original canonical order.
    pub source_triangles: Vec<[[f32; 3]; 3]>,
    /// Geometry in ray-BVH leaf order.
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Canonical source ID for each packed triangle.
    pub canonical_ids: Vec<u32>,
    /// Ray hierarchy.
    pub nodes: Vec<Node>,
    /// Captured origins, directions, limits.
    pub rays: Vec<Ray>,
    /// Independent CPU reference captured at export time.
    pub expected: Vec<u32>,
    /// Historical reference timing, never used as a current baseline.
    pub cpu_12_workers_ms: f64,
    /// Excluded faces: this is filtered opaque geometry, not full transport.
    pub excluded_nonopaque_triangles: usize,
}

/// Rebuilt exact production predicates and checked canonical identity.
pub struct Oracle {
    scene: TransportScene,
    snapshot: Snapshot,
    packed_by_id: Vec<usize>,
    leaf_by_slot: Vec<usize>,
    parents: Vec<usize>,
}

impl Snapshot {
    /// Construct a small explicit CPU fixture without map preparation or assets.
    ///
    /// # Errors
    /// Rejects malformed or oversized geometry/rays.
    pub fn fixture(source_triangles: &[[[f32; 3]; 3]], rays: Vec<Ray>) -> Result<Self, String> {
        let scene = build_scene(source_triangles)?;
        if rays.is_empty() || rays.len() > MAX_RAYS || !rays.iter().all(valid_ray) {
            return Err("invalid bounded probe rays".into());
        }
        let expected = rays
            .iter()
            .map(|(o, d, l)| u32::from(scene.any_hit(*o, *d, *l)))
            .collect();
        Ok(capture(&scene, rays, expected, 0.0, 0))
    }
}

pub(super) fn capture(
    scene: &TransportScene,
    rays: Vec<Ray>,
    expected: Vec<u32>,
    cpu_ms: f64,
    excluded: usize,
) -> Snapshot {
    Snapshot {
        version: 2,
        source_triangles: scene.triangles.iter().map(|t| [t.p0, t.p1, t.p2]).collect(),
        triangles: scene.ray_triangles.iter().map(|t| t.corners).collect(),
        canonical_ids: scene.ray_triangles.iter().map(|t| t.surface).collect(),
        nodes: scene
            .ray_nodes
            .iter()
            .map(|n| Node {
                min: n.min,
                max: n.max,
                first: n.first,
                count: n.count,
                right: n.right,
            })
            .collect(),
        rays,
        expected,
        cpu_12_workers_ms: cpu_ms,
        excluded_nonopaque_triangles: excluded,
    }
}

fn valid_ray((o, d, l): &Ray) -> bool {
    o.iter().chain(d).all(|v| v.is_finite())
        && l.is_finite()
        && *l > 0.0
        && d.iter().any(|v| *v != 0.0)
}

fn build_scene(source: &[[[f32; 3]; 3]]) -> Result<TransportScene, String> {
    if source.is_empty() || source.len() > MAX_TRIANGLES {
        return Err("probe geometry exceeds bounded experiment".into());
    }
    let triangles = source
        .iter()
        .map(|[a, b, c]| TransportTriangle::new(*a, *b, *c, [0.5; 3]))
        .collect::<Option<Vec<_>>>()
        .ok_or("invalid probe triangle")?;
    TransportScene::new(triangles, Vec::new()).ok_or_else(|| "invalid probe scene".into())
}

impl Oracle {
    /// Rebuild the production CPU oracle, refusing stale geometry/IDs/hierarchies.
    ///
    /// # Errors
    /// Rejects legacy, malformed, oversized or incompatible snapshots.
    pub fn new(snapshot: Snapshot) -> Result<Self, String> {
        if snapshot.version != 2
            || snapshot.rays.is_empty()
            || snapshot.rays.len() > MAX_RAYS
            || !snapshot.rays.iter().all(valid_ray)
        {
            return Err("versioned finite bounded ray capture required".into());
        }
        let scene = build_scene(&snapshot.source_triangles)?;
        if snapshot.triangles.len() != scene.ray_triangles.len()
            || snapshot.canonical_ids.len() != scene.ray_triangles.len()
            || snapshot.nodes.len() != scene.ray_nodes.len()
            || snapshot.expected.len() != snapshot.rays.len()
        {
            return Err("incompatible probe capture lengths".into());
        }
        for ((corners, id), t) in snapshot
            .triangles
            .iter()
            .zip(&snapshot.canonical_ids)
            .zip(&scene.ray_triangles)
        {
            if *id != t.surface
                || !corners
                    .iter()
                    .flatten()
                    .zip(t.corners.iter().flatten())
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            {
                return Err("canonical primitive identity mismatch".into());
            }
        }
        for (a, b) in snapshot.nodes.iter().zip(&scene.ray_nodes) {
            if a.first != b.first
                || a.count != b.count
                || a.right != b.right
                || !a
                    .min
                    .iter()
                    .chain(&a.max)
                    .zip(b.min.iter().chain(&b.max))
                    .all(|(x, y)| x.to_bits() == y.to_bits())
            {
                return Err("ray hierarchy mismatch".into());
            }
        }
        let mut parents = vec![usize::MAX; scene.ray_nodes.len()];
        let mut leaf_by_slot = vec![usize::MAX; scene.ray_triangles.len()];
        let mut packed_by_id = vec![usize::MAX; scene.ray_triangles.len()];
        for (index, node) in scene.ray_nodes.iter().enumerate() {
            if node.count == 0 {
                for child in [node.first, node.right] {
                    *parents
                        .get_mut(usize::try_from(child).map_err(|e| e.to_string())?)
                        .ok_or("invalid child")? = index;
                }
            } else {
                let first = usize::try_from(node.first).map_err(|e| e.to_string())?;
                let end =
                    first.saturating_add(usize::try_from(node.count).map_err(|e| e.to_string())?);
                for leaf in leaf_by_slot
                    .get_mut(first..end)
                    .ok_or("invalid leaf range")?
                {
                    *leaf = index;
                }
            }
        }
        for (slot, t) in scene.ray_triangles.iter().enumerate() {
            *packed_by_id
                .get_mut(usize::try_from(t.surface).map_err(|e| e.to_string())?)
                .ok_or("invalid canonical ID")? = slot;
        }
        Ok(Self {
            scene,
            snapshot,
            packed_by_id,
            leaf_by_slot,
            parents,
        })
    }
    /// Checked immutable inputs for the isolated device tool.
    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    /// Evaluate the unchanged full production CPU any-hit query.
    #[must_use]
    pub fn reference(&self, ray: Ray) -> bool {
        self.scene.any_hit(ray.0, ray.1, ray.2)
    }
    /// Exact candidate-hit certificate, including all ray ancestors and original
    /// canonical leaf eligibility. This does not certify nearest identity or misses.
    #[must_use]
    pub fn certifies(&self, ray: Ray, canonical_id: u32) -> bool {
        if !valid_ray(&ray) {
            return false;
        }
        let Some(slot) = usize::try_from(canonical_id)
            .ok()
            .and_then(|id| self.packed_by_id.get(id))
            .copied()
        else {
            return false;
        };
        let Some(triangle) = self.scene.ray_triangles.get(slot) else {
            return false;
        };
        let Some(prepared) = PreparedRay::new(ray.0, ray.1) else {
            return false;
        };
        let inverse = ray.1.map(safe_inverse);
        let Some(t) = prepared.corners(triangle.corners) else {
            return false;
        };
        if t <= super::super::RAY_EPS_M || t >= ray.2 {
            return false;
        }
        if self
            .scene
            .nodes
            .get(usize::try_from(triangle.canonical_leaf).unwrap_or(usize::MAX))
            .is_none_or(|leaf| slab_entry(leaf, ray.0, inverse, ray.2).is_none())
        {
            return false;
        }
        let mut node = self.leaf_by_slot.get(slot).copied().unwrap_or(usize::MAX);
        for _ in 0_usize..64 {
            if !self.scene.ray_nodes.get(node).is_some_and(|n| {
                ray_slab_entry(n, ray.0, inverse, ray.2, prepared.parallel).is_some()
            }) {
                return false;
            }
            if node == 0 {
                return true;
            }
            node = self.parents.get(node).copied().unwrap_or(usize::MAX);
        }
        false
    }
    /// Resolve one untrusted GPU result; misses and every uncertified hit run CPU.
    /// The boolean pair is (exact result, full-query fallback used).
    #[must_use]
    pub fn resolve(
        &self,
        ray: Ray,
        candidate: Option<u32>,
        ambiguous: bool,
        verify_all: bool,
    ) -> (bool, bool) {
        if !verify_all && !ambiguous && candidate.is_some_and(|id| self.certifies(ray, id)) {
            (true, false)
        } else {
            (self.reference(ray), true)
        }
    }
    /// Timed callers use the same bounded production scheduling as the capture.
    ///
    /// # Errors
    /// Returns the production worker failure/cancellation reason.
    pub fn batch(
        &self,
        candidates: Option<&[(Option<u32>, bool)]>,
        verify_all: bool,
        workers: usize,
    ) -> Result<Vec<(bool, bool)>, String> {
        if candidates.is_some_and(|values| values.len() != self.snapshot.rays.len()) {
            return Err("candidate count mismatch".into());
        }
        parallel_map(self.snapshot.rays.len(), workers, None, |index| {
            let ray = self.snapshot.rays[index];
            let (id, ambiguous) = candidates
                .and_then(|values| values.get(index))
                .copied()
                .unwrap_or((None, true));
            self.resolve(ray, id, ambiguous, verify_all)
        })
        .map_err(|e| e.name().to_string())
    }
}
