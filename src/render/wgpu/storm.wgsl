// Shared storm optics. No image/noise synthesis: only sightline extinction.
struct StormShelter { min: vec4<f32>, max: vec4<f32>, roof: vec4<f32> };
struct Storm { color_density: vec4<f32>, count: vec4<u32>, shelters: array<StormShelter, 32> };

// Clips a segment against dot(normal, point) <= offset. Parallel rays are
// handled explicitly so walls/roof planes cannot introduce NaNs.
fn storm_clip(interval: vec2<f32>, origin: vec3<f32>, ray: vec3<f32>, normal: vec3<f32>, offset: f32) -> vec2<f32> {
    let start = offset - dot(normal, origin);
    let delta = dot(normal, ray);
    if (abs(delta) < 1.0e-6) {
        if (start < -1.0e-5) { return vec2<f32>(1.0, 0.0); }
        return interval;
    }
    let t = start / delta;
    if (delta > 0.0) { return vec2<f32>(interval.x, min(interval.y, t)); }
    return vec2<f32>(max(interval.x, t), interval.y);
}
fn storm_interval(shelter: StormShelter, origin: vec3<f32>, ray: vec3<f32>) -> vec2<f32> {
    var span = vec2<f32>(0.0, 1.0);
    for (var axis = 0u; axis < 3u; axis += 1u) {
        var normal = vec3<f32>(0.0); normal[axis] = 1.0;
        span = storm_clip(span, origin, ray, normal, shelter.max[axis]);
        span = storm_clip(span, origin, ray, -normal, -shelter.min[axis]);
    }
    let slope = vec3<f32>(shelter.roof.x, 0.0, shelter.roof.y);
    let gradient = shelter.roof.x + shelter.roof.y;
    span = storm_clip(span, origin, ray, slope + vec3<f32>(0.0,1.0,0.0), shelter.roof.z + gradient * shelter.roof.w);
    span = storm_clip(span, origin, ray, -slope + vec3<f32>(0.0,1.0,0.0), shelter.roof.z - gradient * shelter.roof.w);
    return span;
}
fn storm_transmission(eye: vec3<f32>, point: vec3<f32>) -> f32 {
    if (storm_density() == 0.0) { return 1.0; }
    let ray = point - eye;
    // Sweep the union without thread-private arrays. Usually this is one
    // bounds-only scan outdoors, or an immediate return for an indoor ray.
    var covered = 0.0;
    var cursor = 0.0;
    let count = min(storm_count(), 32u);
    for (var step = 0u; step < count; step += 1u) {
        var next_start = 1.0;
        var next_end = cursor;
        for (var i = 0u; i < count; i += 1u) {
            let shelter = storm_shelter(i);
            if (any(max(eye, point) < shelter.min.xyz) || any(min(eye, point) > shelter.max.xyz)) { continue; }
            let span = storm_interval(shelter, eye, ray);
            if (span.x <= 0.0 && span.y >= 1.0) { return 1.0; }
            if (span.y <= max(span.x, cursor)) { continue; }
            let start = max(span.x, cursor);
            if (start < next_start) {
                next_start = start; next_end = span.y;
            } else if (start == next_start) {
                next_end = max(next_end, span.y);
            }
        }
        if (next_end <= next_start) { break; }
        covered += next_end - next_start;
        cursor = next_end;
        if (cursor >= 1.0) { break; }
    }
    let optical = storm_density() * length(ray) * clamp(1.0-covered, 0.0, 1.0);
    return exp(-optical * optical);
}
fn storm_fog(eye: vec3<f32>, point: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    if (storm_density() == 0.0) { return color; }
    return mix(storm_color(), color, storm_transmission(eye, point));
}
