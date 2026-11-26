
fn flashlight_visibility(world_position: vec3<f32>, normal: vec3<f32>, direction: vec3<f32>) -> f32 {
    let light = dynamic_lights.flashlight;
    // A small geometric normal bias removes self acne without changing the
    // surrounding darkness. The comparison footprint is bounded in texels.
    let receiver = world_position + normal * (0.002 + 0.004 * (1.0 - max(dot(normal, direction), 0.0)));
    let clip = light.view_projection * vec4<f32>(receiver, 1.0);
    if (clip.w <= 0.0) { return 0.0; }
    let projected = clip.xyz / clip.w;
    let uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || projected.z < 0.0 || projected.z > 1.0) { return 0.0; }
    let texel = 1.0 / vec2<f32>(textureDimensions(flashlight_depth));
    var visible = 0.0;
    for (var y = 0; y < 2; y = y + 1) {
        for (var x = 0; x < 2; x = x + 1) {
            let offset = (vec2<f32>(f32(x), f32(y)) - vec2<f32>(0.5)) * texel;
            visible += textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv + offset, projected.z);
        }
    }
    return visible * 0.25;
}
