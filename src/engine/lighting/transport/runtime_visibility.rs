//! Immutable opaque geometry for bounded runtime segment queries. The bake's
//! ray hierarchy and leaf order are reused; lighting values are never changed.
use super::TransportScene;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Node {
    pub minimum: [f32; 3],
    pub first: u32,
    pub maximum: [f32; 3],
    pub count: u32,
    /// Next node after this entire subtree; nodes are stored in preorder.
    pub escape: u32,
    pub ready: u32,
    pub padding: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Triangle {
    pub a_solid: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
}

impl TransportScene {
    /// Uploadable solid-world snapshot. Transmissive and alpha-tested faces
    /// retain their existing transport policy and are excluded from this
    /// opaque-only query, rather than turning their holes into solid blockers.
    #[must_use]
    pub fn runtime_visibility(&self) -> (Vec<Node>, Vec<Triangle>) {
        let mut nodes: Vec<_> = self
            .ray_nodes
            .iter()
            .map(|node| Node {
                minimum: node.min,
                first: node.first,
                maximum: node.max,
                count: node.count,
                escape: 0,
                ready: 1,
                padding: [0; 2],
            })
            .collect();
        // Assign each subtree's successor without recursion or a GPU stack.
        let mut pending = vec![(0_u32, u32::try_from(nodes.len()).unwrap_or(u32::MAX))];
        while let Some((index, escape)) = pending.pop() {
            let Some(node) = nodes.get_mut(usize::try_from(index).unwrap_or(usize::MAX)) else {
                continue;
            };
            node.escape = escape;
            if node.count == 0
                && let Some(source) = self
                    .ray_nodes
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
            {
                pending.push((source.right, escape));
                pending.push((source.first, source.right));
            }
        }
        let triangles = self
            .ray_triangles
            .iter()
            .map(|triangle| {
                let [a, b, c] = triangle.corners;
                let opaque = !triangle.transmissive
                    && self
                        .surface_alpha
                        .get(usize::try_from(triangle.surface).unwrap_or(usize::MAX))
                        .is_none_or(Option::is_none);
                Triangle {
                    a_solid: [a[0], a[1], a[2], f32::from(u8::from(opaque))],
                    b: [b[0], b[1], b[2], 0.0],
                    c: [c[0], c[1], c[2], 0.0],
                }
            })
            .collect();
        (nodes, triangles)
    }
}
