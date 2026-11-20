// The emissive term, in linear light: `mix(u_emission_color, v_color.rgb,
// u_emission_vertex) * mask * tex_color.rgb * u_emission_scale`.
fn surface_emission(in: VsOut, base_linear: vec3<f32>) -> vec3<f32> {
    var mask = vec3<f32>(1.0);
    if (material.emission_mask_enabled > 0.5) {
        mask = textureSample(emission_texture, emission_sampler, in.uv).rgb;
    }
    let color = mix(material.emission_color, in.color.rgb, material.emission_vertex);
    return color * mask * base_linear * material.emission_scale;
}
