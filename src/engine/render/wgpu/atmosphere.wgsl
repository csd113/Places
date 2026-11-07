// Shared ordinary fog integration; callers read their own view uniforms.
struct FogRegion {
    min_falloff: vec4<f32>,
    max_top: vec4<f32>,
    color_density: vec4<f32>,
    ground_pad: vec4<f32>,
};

fn fog_interval(eye: vec3<f32>, delta: vec3<f32>, lower: vec3<f32>, upper: vec3<f32>) -> vec2<f32> {
    var near = 0.0;
    var far = 1.0;
    for (var axis = 0u; axis < 3u; axis = axis + 1u) {
        if (abs(delta[axis]) <= 1.0e-6) {
            if (eye[axis] < lower[axis] || eye[axis] > upper[axis]) {
                return vec2<f32>(1.0, 0.0);
            }
        } else {
            let first = (lower[axis] - eye[axis]) / delta[axis];
            let last = (upper[axis] - eye[axis]) / delta[axis];
            near = max(near, min(first, last));
            far = min(far, max(first, last));
            if (far <= near) {
                return vec2<f32>(1.0, 0.0);
            }
        }
    }
    return vec2<f32>(near, far);
}

fn region_density(region: FogRegion, point: vec3<f32>) -> f32 {
    let lower = region.min_falloff.xyz;
    let upper = region.max_top.xyz;
    if (point.y < lower.y || point.y > upper.y) {
        return 0.0;
    }
    let horizontal = min(min(point.x - lower.x, upper.x - point.x), min(point.z - lower.z, upper.z - point.z));
    var edge = 0.0;
    if (horizontal > 0.0) {
        edge = clamp(horizontal / max(region.min_falloff.w, 1.0e-6), 0.0, 1.0);
    }
    let ground_y = region.ground_pad.x;
    let top_y = region.max_top.w;
    var vertical = 0.0;
    if (point.y <= ground_y) {
        vertical = 1.0;
    } else if (top_y > ground_y && point.y < top_y) {
        vertical = clamp((top_y - point.y) / (top_y - ground_y), 0.0, 1.0);
    }
    return region.color_density.a * edge * vertical;
}

fn atmosphere(world_position: vec3<f32>) -> vec4<f32> {
    if (ordinary_fog_density() == 0.0 && ordinary_fog_region_count() == 0u) {
        return vec4<f32>(0.0);
    }
    let delta = world_position - camera.position;
    let distance = length(delta);
    let below = max(0.0, ordinary_fog_reference_y() - world_position.y);
    let global_density = ordinary_fog_density() * (1.0 + ordinary_fog_height_gain() * min(below, 12.0));
    let global_depth = global_density * distance;
    var layer_depth = 0.0;
    var layer_color = ordinary_fog_color();
    let quadrature = array<vec2<f32>, 4u>(
        vec2<f32>(0.06943184, 0.17392743), vec2<f32>(0.3300095, 0.32607257),
        vec2<f32>(0.66999054, 0.32607257), vec2<f32>(0.93056816, 0.17392743)
    );
    let region_count = min(ordinary_fog_region_count(), 16u);
    for (var index = 0u; index < region_count; index = index + 1u) {
        let region = ordinary_fog_region(index);
        if (region.color_density.a <= 0.0) {
            continue;
        }
        let interval = fog_interval(camera.position, delta, region.min_falloff.xyz, region.max_top.xyz);
        if (interval.y <= interval.x) {
            continue;
        }
        var average = 0.0;
        for (var sample = 0u; sample < 4u; sample = sample + 1u) {
            let point = camera.position + delta * mix(interval.x, interval.y, quadrature[sample].x);
            average = average + region_density(region, point) * quadrature[sample].y;
        }
        let depth = average * distance * (interval.y - interval.x);
        // Preserve the strongest-volume overlap policy and stable tie order.
        if (depth > layer_depth) {
            layer_depth = depth;
            layer_color = region.color_density.rgb;
        }
    }
    let depth = global_depth + layer_depth;
    let color = mix(srgb_to_linear(ordinary_fog_color()), srgb_to_linear(layer_color), layer_depth / max(depth, 1.0e-6));
    let amount = 1.0 - exp(-depth * depth);
    return vec4<f32>(color, clamp(amount, 0.0, 1.0));
}

fn fogged(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let fog = atmosphere(world_position);
    return mix(color, fog.rgb, fog.a);
}

