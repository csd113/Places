
// The reference's reflection term, in linear light. A planar surface projects
// the reflected frame through the mirrored camera; a probe surface reads the
// static cubemap baked at load. Both are weighted by the authored strength (the
// material's `specular Ã— strength`), a Fresnel term and the gloss.
//
// Reflection targets store unexposed linear HDR radiance; sampling never
// re-applies tone mapping, exposure or display conversion.
fn surface_reflection(in: VsOut, normal: vec3<f32>, view: vec3<f32>, water_reflectance: f32) -> vec3<f32> {
    if (material.reflection_mode == 0u || all(material.reflection_strength == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
