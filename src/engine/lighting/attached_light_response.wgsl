

// The attached lights' diffuse term at one fragment. Each light contributes a
// cosine lobe windowed by a quadratic range falloff:
//
//     d = length(light.position - world_position)
//     ndl = max(dot(normal, normalize(light.position - world_position)), 0)
//     window = clamp(1 - d / light.radius, 0, 1)
//     sum += color * intensity * ndl * window * window
//
// The term is exactly zero when no light is attached, so the baked path (and
// the shader's unit-light bypass) is unchanged at zero lights. Zero distance
// is guarded before the normalise; a zero-radius light is refused by the
// neutral set, so `light.radius` is always positive here.
fn dynamic_light_term(normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = min(dynamic_lights.count, 8u);
    for (var slot = 0u; slot < count; slot = slot + 1u) {
        let light = dynamic_lights.lights[slot];
        let to_light = light.position - world_position;
        let distance = length(to_light);
        if (distance >= light.radius) {
            continue;
        }
        let direction = to_light / max(distance, 1.0e-6);
        let ndl = max(dot(normal, direction), 0.0);
        // A back-facing receiver contributes exactly zero. Avoid tracing the
        // cabin stock for that lobe, especially the exterior landing wall.
        if (ndl <= 0.0 || !cabin_light_visible(light, world_position)) {
            continue;
        }
        // Enclosure clipping still owns the shell, door leaves and closed
        // seal. Static geometry only adds blockers beyond that exact aperture.
        if (light.enclosure_right_enabled.z != 0.0 &&
            !static_world_visible(world_position + normal * 0.001, light.position)) {
            continue;
        }
        let window = clamp(1.0 - distance / light.radius, 0.0, 1.0);
        sum += light.color * light.intensity * ndl * window * window;
    }
    return sum;
}

fn flashlight_term(normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let beam = dynamic_lights.flashlight;
    if (beam.outer_enabled.y > 0.5) {
        let to_light = beam.position_range.xyz - world_position;
        let distance = length(to_light);
        if (distance > 1.0e-6 && distance < beam.position_range.w) {
            let direction = to_light / distance;
            let angular = dot(beam.direction_inner.xyz, -direction);
            let cone = smoothstep(beam.outer_enabled.x, beam.direction_inner.w, angular);
            let ndl = max(dot(normal, direction), 0.0);
            if (cone > 0.0 && ndl > 0.0) {
                let relative = distance / beam.position_range.w;
                let window = max(1.0 - relative * relative * relative * relative, 0.0);
                let falloff = window * window / (1.0 + distance * distance);
                let visibility = flashlight_visibility(world_position, normal, direction);
                sum += beam.color_strength.rgb * beam.color_strength.w * cone * falloff * ndl * visibility;
            }
        }
    }
    return sum;
}
