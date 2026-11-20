//! Static prop receiver selection and prepared irradiance for uncharted props.

use super::{
    LevelLighting,
    probes::{ProbeField, probe_light_enabled},
};
use crate::engine::gltf::PropModel;
use crate::engine::render::{LIGHTMAP_NONE, PropMeshBatch};
use crate::engine::spatial::Aabb;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StaticPropReceiver {
    Vertex,
    Chart,
    Probe,
}

impl StaticPropReceiver {
    pub fn select(model: &PropModel, bounds: &Aabb, lightmapped: bool) -> Self {
        if !lightmapped || model.is_animatable() {
            Self::Vertex
        } else if model.triangles > crate::engine::level::PROP_TRIANGLE_BUDGET && large_prop(bounds)
        {
            // Oversized reviewed scenery retains its indexed draw geometry
            // and uses prepared irradiance instead of padding every tiny face.
            Self::Probe
        } else {
            Self::Chart
        }
    }
}

pub fn large_prop(bounds: &Aabb) -> bool {
    bounds
        .max
        .iter()
        .zip(bounds.min)
        .any(|(high, low)| high - low > 3.0)
}

/// Light the deferred, uncharted static scenery after transport has consumed
/// its original albedo. Valid dark probes stay dark; missing fields use the
/// established environment lighting, never an unlit/full-bright colour.
/// Switched probe bounce uses the authored source state at preparation time.
/// Dense colours are stored in the package, while charted vertices sample
/// the renderer's live selection of atlas layers.
pub fn relight_probe_props(
    batches: &mut [PropMeshBatch],
    lighting: &LevelLighting,
    field: Option<&ProbeField>,
) {
    for batch in batches
        .iter_mut()
        .filter(|batch| batch.pending_probe_lighting)
    {
        for vertex in batch
            .vertices
            .make_mut()
            .iter_mut()
            .filter(|vertex| vertex.lightmap_page == LIGHTMAP_NONE)
        {
            let position = glam::Vec3::from_array(vertex.pos);
            let normal = glam::Vec3::from_array(vertex.normal).normalize_or_zero();
            let sample = normal
                .mul_add(glam::Vec3::splat(0.025), position)
                .to_array();
            let display = field
                .and_then(|prepared| {
                    prepared.sample_with_switchable_indirect(
                        sample,
                        None,
                        |index| probe_light_enabled(lighting, index),
                        lighting.labelled_probe_visibility(sample),
                    )
                })
                .map_or_else(
                    || {
                        let light = lighting
                            .sample(position.x, position.y, position.z)
                            .plus(lighting.global_surface_light(vertex.pos, normal.to_array()));
                        [light.r, light.g, light.b]
                    },
                    |texel| texel.light_at(normal.to_array()),
                );
            for (channel, light) in vertex.color.iter_mut().take(3).zip(display) {
                *channel *= light;
            }
        }
        batch.pending_probe_lighting = false;
    }
}
