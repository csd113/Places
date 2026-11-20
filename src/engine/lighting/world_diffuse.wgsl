
// The reference's lit term, in linear light:
// `tex_color.rgb * v_color.rgb * light * (1.0 - u_emission_vertex)`.
fn lit_linear(base_linear: vec3<f32>, vertex_color: vec3<f32>, light: vec3<f32>, emission_vertex: f32) -> vec3<f32> {
    return base_linear * vertex_color * light * (1.0 - emission_vertex);
}
