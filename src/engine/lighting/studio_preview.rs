//! Neutral studio lighting for the editor’s Inspect view.

use crate::engine::render::Vertex;

/// Shade the original linear RGB albedo without changing normals or geometry.
pub fn illuminate_preview_vertices(vertices: &mut [Vertex]) {
    let direction = glam::Vec3::new(-0.35, 0.8, 0.45).normalize();
    for vertex in vertices {
        let shade = 0.4_f32.mul_add(
            glam::Vec3::from_array(vertex.normal)
                .dot(direction)
                .max(0.0),
            0.55,
        );
        for (channel, albedo) in vertex.color.iter_mut().zip(vertex.caustics_albedo) {
            *channel = albedo * shade;
        }
    }
}
