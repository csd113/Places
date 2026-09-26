//! Static water surfaces for authored `water` volumes.
//!
//! A level's water bodies are described by [`crate::level::WaterVolumeDef`]
//! entries and resolved once by [`crate::level::WaterVolumes`]. The renderer
//! draws the *surface* only: one flat quad per volume at its `surface_y`,
//! covering the footprint, into the static mesh's floor bucket. The basin's own
//! tile geometry already owns the walls and the bottom, so no side or bottom
//! face is emitted here.
//!
//! The quad is deliberately vertex-lit even on a lightmapped build: it carries
//! the baked light of its corners in the vertex colour exactly like a prop or a
//! fixture face (`LIGHTMAP_NONE`), which keeps it out of the lightmap atlas —
//! the surface belongs to the volume, not to a room's chart. Its vertex alpha
//! carries the volume's opacity while the catalog material stays `opacity:
//! 1.0`, so one catalog material serves every level's water and the authored
//! per-volume opacity is what blends. The blend classification is the existing
//! material `alpha_mode: "blend"` path: a floor key lands in the sorted
//! back-to-front translucent pass with depth writes off, and the renderer draws
//! the pass two-sided, so the surface is visible from above and below the
//! waterline without any new pipeline state.

use crate::level::{MaterialRef, WaterVolumes};
use crate::spatial::SpatialBuckets;

use super::geometry::EmitContext;
use super::{MaterialSlot, SurfaceKey, Vertex, lit_corners, tiled_uv};

/// Emits one translucent surface quad for every water volume of the level.
///
/// Volumes are resolved through [`WaterVolumes::from_level`] — the same
/// resolution the player controller samples — so the drawn footprint, surface
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
        // Floor winding (`p0 -> p1 -> p2` gives `+Y`), so the surface's front
        // faces up and its underside is the same quad seen from below.
        let corners = [
            [volume.x0, y, volume.z1],
            [volume.x1, y, volume.z1],
            [volume.x1, y, volume.z0],
            [volume.x0, y, volume.z0],
        ];
        // The surface is never lightmapped, so the baked light rides in the
        // vertex colour: tint × the light sampled at each corner, exactly like
        // the vertex-lit floor path.
        let colours = lit_corners(tint, corners, context.lighting);
        // World-space UVs at the material's tiling, like every floor sheet: a
        // quad's UVs continue the pool deck's metre grid instead of restarting
        // at each volume.
        let uv = corners.map(|[x, _, z]| tiled_uv(x, z, tile));
        scratch.clear();
        add_water_quad(scratch, corners, colours, uv, volume.opacity);
        buckets.add_quads(key, scratch);
    }
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
