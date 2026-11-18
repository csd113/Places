// Prepared linear receiver light for a raster surface mark.
// Caller carries unchanged per-corner light and the same-frame
// visibility/beam/caustics bindings. The inputs are independent of
// the package reader and of the world vertex layout.
fn prepared_decal_light(
    receiver_position: vec3<f32>,
    receiver_normal: vec3<f32>,
    prepared_rgb: vec3<f32>,
    caustics_albedo: vec3<f32>,
    eye: vec3<f32>,
) -> vec3<f32> {
    let position_dy = dpdy(receiver_position);
    let position_dx = dpdx(receiver_position);
    var normal = receiver_normal;
    if (dot(normal, normal) < 1.0e-10) {
        normal = cross(position_dy, position_dx);
    }
    normal = normal / max(length(normal), 1.0e-6);
    if (dot(normal, eye - receiver_position) < 0.0) { normal = -normal; }
    let beam = flashlight_term(normal, receiver_position);
    let selected = caustic_volume(receiver_position, normal);
    let modulation = caustic_modulation(receiver_position, selected, dpdx(receiver_position.xz * 0.45), dpdy(receiver_position.xz * 0.45), beam, vec4<f32>(0.0));
    return max(prepared_rgb + modulation * caustics_albedo, vec3<f32>(0.0)) + dynamic_light_term(normal, receiver_position) + beam;
}
