//! Static water surfaces for authored `water` volumes.
//!
//! A level's water bodies are described by [`crate::level::WaterVolumeDef`]
//! entries and resolved once by [`crate::level::WaterVolumes`]. The renderer
//! draws the *surface* only: one flat quad per rectangular volume and one
//! closed fan per circular volume at its `surface_y`, covering the shape's own
//! footprint, into the static mesh's floor bucket. The basin's own tile
//! geometry already owns the walls and the bottom, so no side or bottom face is
//! emitted here.
//!
//! The surface is deliberately vertex-lit even on a lightmapped build: it
//! carries the baked light of its own points in the vertex colour exactly like
//! a prop or a fixture face (`LIGHTMAP_NONE`), which keeps it out of the
//! lightmap atlas — the surface belongs to the volume, not to a room's chart.
//! Its vertex alpha carries the volume's opacity while the catalog material
//! stays `opacity: 1.0`, so one catalog material serves every level's water and
//! the authored per-volume opacity is what blends. The blend classification is
//! the existing material `alpha_mode: "blend"` path: a floor key lands in the
//! sorted back-to-front translucent pass with depth writes off, and the
//! renderer draws the pass two-sided, so the surface is visible from above and
//! below the waterline without any new pipeline state.

use crate::level::{MaterialRef, WaterShape, WaterVolumes};
use crate::spatial::SpatialBuckets;

use super::geometry::EmitContext;
use super::{MaterialSlot, SurfaceKey, Vertex, count_to_f32, shade, tiled_uv};

/// Fan segments one circular water surface draws with.
///
/// A fixed, deterministic count: a hot tub reads perfectly round at 48
/// segments, the fan is tiny next to the level's own geometry budget, and the
/// same authored circle always produces the same vertices.
pub const WATER_DISC_SEGMENTS: usize = 48;

/// Emits one translucent surface per water volume: a quad for a rectangle and
/// a closed fan for a circle.
///
/// Volumes are resolved through [`WaterVolumes::from_level`] — the same
/// resolution the player controller samples — so the drawn shape, surface
/// height, material and opacity can never drift from the water the player
/// swims in. Malformed or non-finite volumes are skipped by that resolution and
/// therefore draw nothing; a level with no `water` array resolves nothing at
/// all and keeps the historical mesh byte for byte.
pub fn emit_water(
    context: &EmitContext<'_, '_>,
    buckets: &mut SpatialBuckets<SurfaceKey>,
    scratch: &mut Vec<Vertex>,
) {
    // A dry level is the historical level: its mesh must stay byte-identical,
    // so do not even resolve (or allocate) an empty volume set.
    if context.level.water.is_empty() {
        return;
    }
    let volumes = WaterVolumes::from_level(context.level);
    for volume in volumes.volumes() {
        let key = context
            .materials
            .key(MaterialSlot::Floor, MaterialRef::id(volume.material_id()));
        let tile = context.materials.tile_metres(key);
        let tint = context.materials.tint(key);
        let y = volume.surface_y;
        match volume.shape {
            WaterShape::Rect => {
                // Floor winding (`p0 -> p1 -> p2` gives `+Y`), so the surface's
                // front faces up and its underside is the same quad seen from
                // below.
                let corners = [
                    [volume.x0, y, volume.z1],
                    [volume.x1, y, volume.z1],
                    [volume.x1, y, volume.z0],
                    [volume.x0, y, volume.z0],
                ];
                // The surface is never lightmapped, so the baked light rides
                // in the vertex colour: tint × the light sampled at each
                // corner, exactly like the vertex-lit floor path.
                let colours = corners
                    .map(|corner| shade(tint, context.lighting.sample(corner[0], y, corner[2])));
                // World-space UVs at the material's tiling, like every floor
                // sheet: a quad's UVs continue the pool deck's metre grid
                // instead of restarting at each volume.
                let uv = corners.map(|[x, _, z]| tiled_uv(x, z, tile));
                scratch.clear();
                add_water_quad(scratch, corners, colours, uv, volume.opacity);
                buckets.add_quads(key, scratch);
            }
            WaterShape::Circle => {
                let (centre_x, centre_z) = volume.center();
                let radius = volume.radius;
                let centre = [centre_x, y, centre_z];
                // The baked light of the rim (plus the centre for a smooth
                // gradient) rides the vertex colour, exactly like the
                // rectangle's corners.
                let centre_colour = shade(tint, context.lighting.sample(centre_x, y, centre_z));
                let centre_uv = tiled_uv(centre_x, centre_z, tile);
                let rim = disc_rim(centre_x, centre_z, radius, y, WATER_DISC_SEGMENTS);
                scratch.clear();
                for pair in rim.windows(2) {
                    let [current, next] = pair else {
                        continue;
                    };
                    let current_colour =
                        shade(tint, context.lighting.sample(current[0], y, current[2]));
                    let next_colour = shade(tint, context.lighting.sample(next[0], y, next[2]));
                    let current_uv = tiled_uv(current[0], current[2], tile);
                    let next_uv = tiled_uv(next[0], next[2], tile);
                    // The fan triangle is `(centre, next, current)`: with the
                    // rim running counter-clockwise seen from above, its
                    // derived normal is `+Y`, the same front every floor
                    // emitter has.
                    add_water_triangle(
                        scratch,
                        centre,
                        centre_colour,
                        centre_uv,
                        *next,
                        next_colour,
                        next_uv,
                        *current,
                        current_colour,
                        current_uv,
                        volume.opacity,
                    );
                }
                buckets.add_quads(key, scratch);
            }
        }
    }
}

/// The closed rim ring of a circular surface: `segments + 1` points at `y`, in
/// the XZ plane, counter-clockwise seen from above.
///
/// The closing point repeats the first, so `windows(2)` walks every fan
/// segment including the one that closes the circle.
fn disc_rim(centre_x: f32, centre_z: f32, radius: f32, y: f32, segments: usize) -> Vec<[f32; 3]> {
    let count = segments.max(3);
    (0..=count)
        .map(|index| {
            // The closing point reuses angle zero exactly, so the fan's last
            // vertex is bit-identical to its first and the seam has no
            // duplicate near-twin.
            let step = if index == count { 0 } else { index };
            let angle = std::f32::consts::TAU * count_to_f32(step) / count_to_f32(count);
            [
                radius.mul_add(angle.cos(), centre_x),
                y,
                radius.mul_add(angle.sin(), centre_z),
            ]
        })
        .collect()
}

/// Writes one quad whose vertex alpha is preserved.
///
/// The shared `add_quad` / `add_quad_flat` helpers force alpha to `1.0`, which
/// is right for every opaque emitter and wrong for the water surface: the
/// volume's opacity is exactly what the translucent pass blends with. The
/// geometry is the same six-vertex quad shape (`p0, p1, p2` then `p0, p2, p3`),
/// so indexing and the surface-frame derivation stay identical to every other
/// emitter.
fn add_water_quad(
    vertices: &mut Vec<Vertex>,
    [p0, p1, p2, p3]: [[f32; 3]; 4],
    [c0, c1, c2, c3]: [[f32; 3]; 4],
    [uv0, uv1, uv2, uv3]: [[f32; 2]; 4],
    alpha: f32,
) {
    let vertex = |pos: [f32; 3], colour: [f32; 3], uv: [f32; 2]| Vertex {
        pos,
        color: [colour[0], colour[1], colour[2], alpha],
        uv,
        ..Vertex::UNLIT
    };
    vertices.push(vertex(p0, c0, uv0));
    vertices.push(vertex(p1, c1, uv1));
    vertices.push(vertex(p2, c2, uv2));
    vertices.push(vertex(p0, c0, uv0));
    vertices.push(vertex(p2, c2, uv2));
    vertices.push(vertex(p3, c3, uv3));
}

/// Writes one alpha-carrying triangle in the shared six-vertex run shape.
///
/// The run repeats its last two vertices (`p0, p1, p2, p0, p1, p2`), which the
/// index builder recognises as a triangle encoded in the quad form: its
/// degenerate second triangle is dropped and the first is indexed normally, so
/// a fan segment costs three distinct vertices like every other triangle in
/// the mesh.
#[expect(
    clippy::too_many_arguments,
    reason = "one triangle's three points, colours and uvs"
)] // one triangle's three points, colours and uvs
fn add_water_triangle(
    vertices: &mut Vec<Vertex>,
    p0: [f32; 3],
    c0: [f32; 3],
    uv0: [f32; 2],
    p1: [f32; 3],
    c1: [f32; 3],
    uv1: [f32; 2],
    p2: [f32; 3],
    c2: [f32; 3],
    uv2: [f32; 2],
    alpha: f32,
) {
    let vertex = |pos: [f32; 3], colour: [f32; 3], uv: [f32; 2]| Vertex {
        pos,
        color: [colour[0], colour[1], colour[2], alpha],
        uv,
        ..Vertex::UNLIT
    };
    vertices.push(vertex(p0, c0, uv0));
    vertices.push(vertex(p1, c1, uv1));
    vertices.push(vertex(p2, c2, uv2));
    vertices.push(vertex(p0, c0, uv0));
    vertices.push(vertex(p1, c1, uv1));
    vertices.push(vertex(p2, c2, uv2));
}

#[cfg(test)]
mod tests {
    // Test code: exact float comparisons and indexing are idiomatic here; the
    // production lints stay enforced everywhere else.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[2].mul_add(-b[1], a[1] * b[2]),
            a[0].mul_add(-b[2], a[2] * b[0]),
            a[1].mul_add(-b[0], a[0] * b[1]),
        ]
    }

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    /// A circular surface is a closed fan whose points all sit on or inside
    /// the authored radius, and every fan triangle faces up.
    #[test]
    fn a_circle_is_a_closed_upward_fan_within_its_radius() {
        let (centre_x, centre_z) = (10.0_f32, -4.0_f32);
        let radius = 2.5_f32;
        let y = 1.25_f32;
        let rim = disc_rim(centre_x, centre_z, radius, y, WATER_DISC_SEGMENTS);
        assert_eq!(rim.len(), WATER_DISC_SEGMENTS + 1);
        assert_eq!(
            rim.first(),
            rim.last(),
            "the fan closes back onto its first rim point"
        );
        for point in &rim {
            assert_eq!(point[1], y, "the surface is flat at surface_y");
            let dx = point[0] - centre_x;
            let dz = point[2] - centre_z;
            assert!(
                (dx.hypot(dz) - radius).abs() < 1.0e-4,
                "every rim point sits on the radius: {point:?}"
            );
        }
        let centre = [centre_x, y, centre_z];
        for pair in rim.windows(2) {
            let [current, next] = pair else {
                continue;
            };
            let normal = cross(sub(*next, centre), sub(*current, centre));
            assert!(
                normal[1] > 0.0,
                "fan triangle faces up like a floor: {normal:?}"
            );
        }
    }

    /// A fan triangle is written in the shared six-vertex run shape and its
    /// alpha is the volume's opacity, exactly like the rectangle's quad.
    #[test]
    fn a_fan_triangle_preserves_the_volume_opacity() {
        let mut vertices = Vec::new();
        add_water_triangle(
            &mut vertices,
            [0.0, 1.0, 0.0],
            [0.5, 0.5, 0.5],
            [0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.5, 0.5, 0.5],
            [1.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.5, 0.5, 0.5],
            [0.0, 1.0],
            0.37,
        );
        assert_eq!(vertices.len(), 6);
        for vertex in &vertices {
            assert_eq!(vertex.color[3], 0.37);
        }
        let mut quads = Vec::new();
        add_water_quad(
            &mut quads,
            [
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            [[0.5; 3]; 4],
            [[0.0; 2]; 4],
            0.37,
        );
        assert_eq!(quads.len(), 6);
        assert_eq!(quads[5].color[3], 0.37);
    }
}
