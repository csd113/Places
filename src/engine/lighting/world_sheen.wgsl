
// The reference's view-dependent sheen, in linear light.
fn surface_sheen(in: VsOut, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, water_reflectance: f32) -> vec3<f32> {
    if ((material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) == 0u || all(material.specular == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    if (water_blend_pass && in.water_surface > 0.5) { return material.specular * water_reflectance * light * 0.12; }
    let facing = clamp(abs(dot(normal, view)), 0.0, 1.0);
    let gloss = 1.0 - material.roughness;
    let grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    let ahead = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    return material.specular * (grazing * 0.55 + ahead * 0.45) * light;
}
