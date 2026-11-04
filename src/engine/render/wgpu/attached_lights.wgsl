// Group 4 is the frame's attached lights: a small, fixed-capacity array of
// character glows the renderer resolves from the entity frames each frame.
// It is the one realtime light term in this shader, and it is deliberately
// bounded and additive on top of the baked `surface_light`; the spotlight
// additionally samples the current meshes' visibility target. The
// emissive pass never reads it, so bloom cannot double the pool.
struct DynamicLight {
    // World-space position, in metres.
    position: vec3<f32>,
    // The falloff window's radius, in metres, strictly positive.
    radius: f32,
    // Linear light colour, each channel `0..=1`.
    color: vec3<f32>,
    // Additive intensity.
    intensity: f32,
    enclosure_origin_gap: vec4<f32>,
    enclosure_right_enabled: vec4<f32>,
    enclosure_bounds: vec4<f32>,
    enclosure_portal: vec4<f32>,
    enclosure_doors: vec4<f32>,
};

struct Flashlight {
    view_projection: mat4x4<f32>,
    position_range: vec4<f32>,
    color_strength: vec4<f32>,
    direction_inner: vec4<f32>,
    outer_enabled: vec4<f32>,
};

struct DynamicLights {
    // Number of valid entries in `lights`, 0 when nothing is attached.
    count: u32,
    // Explicit scalar padding so the array starts at offset 16: three separate
    // `u32`s, because a `vec3<u32>` would itself align to 16.
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    lights: array<DynamicLight, 8>,
    flashlight: Flashlight,
};

@group(4) @binding(0)
var<uniform> dynamic_lights: DynamicLights;
@group(4) @binding(1)
var flashlight_depth: texture_depth_2d;
@group(4) @binding(2)
var flashlight_sampler: sampler_comparison;

// The same immutable opaque world in all quality modes and material passes.
struct VisibilityNode {
    minimum: vec3<f32>, first: u32,
    maximum: vec3<f32>, count: u32,
    escape: u32, ready: u32, _pad0: u32, _pad1: u32,
};
struct VisibilityTriangle { a_solid: vec4<f32>, b: vec4<f32>, c: vec4<f32> };
@group(4) @binding(3) var<storage, read> visibility_nodes: array<VisibilityNode>;
@group(4) @binding(4) var<storage, read> visibility_triangles: array<VisibilityTriangle>;

fn visibility_box(node: VisibilityNode, origin: vec3<f32>, ray: vec3<f32>) -> bool {
    var near = 0.0;
    var far = 1.0;
    for (var axis = 0u; axis < 3u; axis += 1u) {
        if (abs(ray[axis]) < 1.0e-12) {
            if (origin[axis] < node.minimum[axis] || origin[axis] > node.maximum[axis]) { return false; }
        } else {
            let a = (node.minimum[axis] - origin[axis]) / ray[axis];
            let b = (node.maximum[axis] - origin[axis]) / ray[axis];
            near = max(near, min(a, b));
            far = min(far, max(a, b));
            if (near > far) { return false; }
        }
    }
    return true;
}

fn visibility_triangle(triangle: VisibilityTriangle, origin: vec3<f32>, ray: vec3<f32>) -> bool {
    if (triangle.a_solid.w == 0.0) { return false; }
    let edge0 = triangle.b.xyz - triangle.a_solid.xyz;
    let edge1 = triangle.c.xyz - triangle.a_solid.xyz;
    let p = cross(ray, edge1);
    let determinant = dot(edge0, p);
    if (abs(determinant) < 1.0e-10) { return false; }
    let offset = origin - triangle.a_solid.xyz;
    let u = dot(offset, p) / determinant;
    let q = cross(offset, edge0);
    let v = dot(ray, q) / determinant;
    let t = dot(edge1, q) / determinant;
    return u >= -1.0e-6 && v >= -1.0e-6 && u + v <= 1.000001 && t > 1.0e-5 && t < 0.99999;
}

// Stackless preorder traversal has fixed work limits. Exhaustion or unavailable
// geometry rejects the ray rather than admitting light through untested walls.
fn static_world_visible(origin: vec3<f32>, receiver: vec3<f32>) -> bool {
    if (visibility_nodes[0u].ready == 0u) { return false; }
    let ray = receiver - origin;
    var index = 0u;
    let count = arrayLength(&visibility_nodes);
    for (var visit = 0u; visit < 512u; visit += 1u) {
        if (index >= count) { return true; }
        let node = visibility_nodes[index];
        if (!visibility_box(node, origin, ray)) {
            index = node.escape;
            continue;
        }
        if (node.count == 0u) {
            index = node.first;
            continue;
        }
        if (node.count > 64u) { return false; }
        for (var slot = 0u; slot < node.count; slot += 1u) {
            let triangle_index = node.first + slot;
            if (triangle_index >= arrayLength(&visibility_triangles)) { return false; }
            if (visibility_triangle(visibility_triangles[triangle_index], origin, ray)) { return false; }
        }
        index = node.escape;
    }
    return index >= count;
}

fn cabin_local(light: DynamicLight, position: vec3<f32>) -> vec3<f32> {
    let relative = position - light.enclosure_origin_gap.xyz;
    let right = light.enclosure_right_enabled.xy;
    return vec3<f32>(dot(relative.xz, right), relative.y,
                     dot(relative.xz, vec2<f32>(-right.y, right.x)));
}

fn cabin_section_visible(source: vec3<f32>, point: vec3<f32>, section: vec4<f32>, floor_y: f32) -> bool {
    if (point.z >= section.x - 0.0001) { return true; }
    let ray = point - source;
    // Clip to the receiver when it lies inside the stock's depth, so an
    // oblique ray cannot enter the side of a leaf/pocket between its faces.
    let planes = vec2<f32>(section.x, max(point.z, section.y));
    for (var face = 0u; face < 2u; face = face + 1u) {
        let crossing = source + ray * ((planes[face] - source.z) / ray.z);
        // Keep boundary rays on the opaque side under rotated f32 transforms.
        if (abs(crossing.x) >= section.z - 0.0001 || crossing.y < floor_y || crossing.y > section.w) {
            return false;
        }
    }
    return true;
}

// Trace the finite source/receiver segment through the opaque convex cabin and
// both faces of both sliding door pairs. The aperture follows exactly the same
// eased pose and centre seal as render/collision; a closed gap admits no ray.
// This runs in the shared world/decal/reflection shader, in every quality mode.
fn cabin_light_visible(light: DynamicLight, world_position: vec3<f32>) -> bool {
    if (light.enclosure_right_enabled.z == 0.0) { return true; }
    let point = cabin_local(light, world_position);
    let bounds = light.enclosure_bounds;
    if (point.z >= bounds.w - 0.0001) {
        return abs(point.x) <= bounds.x + 0.0001 && point.y >= -0.0001 &&
               point.y <= bounds.y + 0.0001 && point.z <= bounds.z + 0.0001;
    }
    let panel_back = max(light.enclosure_doors.x, light.enclosure_doors.z);
    if (light.enclosure_origin_gap.w <= 0.0 && point.z < panel_back - 0.0001) {
        return false;
    }
    let source = cabin_local(light, light.position);
    let ray = point - source;
    let aperture = light.enclosure_portal;
    // Opaque sill stock spans both tracks and extends into the hall. Reject
    // rays that enter its top before reaching a lower receiver.
    if (point.y < aperture.y - 0.0001) {
        let sill = source + ray * ((aperture.y - source.y) / ray.y);
        if (abs(sill.x) <= 0.86 && sill.z >= -0.32 && sill.z <= 0.42) {
            return false;
        }
    }
    let hall_front = light.enclosure_right_enabled.w;
    if (point.z < hall_front) {
        // Beyond the hall mouth, a centred source's complete aperture stack
        // reduces to one angular interval. The hall mouth bounds the fixed
        // stock, the furthest door face bounds the moving gap, the nearest
        // header bounds the top, and the hall mouth bounds the bottom. Keep
        // the full finite sections below for receivers within the tracks.
        let depth = source.z - point.z;
        let door_front = min(light.enclosure_doors.y, light.enclosure_doors.w);
        let centre_margin = abs(source.x) + 0.0001;
        let horizontal = min((aperture.w - centre_margin) / (source.z - hall_front),
                             (light.enclosure_origin_gap.w - centre_margin) / (source.z - door_front));
        let down = -ray.y / depth;
        let minimum_down = (source.y - aperture.z) / (source.z - bounds.w);
        let maximum_down = (source.y - aperture.y) / (source.z - hall_front);
        return abs(ray.x) < horizontal * depth && down >= minimum_down && down <= maximum_down;
    }
    // Manufactured jamb, pocket liners and hall mouth from the rigid GLB
    // contract: back/front Z, clear half-width, clear height. Trace complete
    // depth intervals, including receivers within the recessed channels.
    let sections = array<vec4<f32>, 4>(
        vec4<f32>(bounds.w, 0.312, aperture.x, aperture.z),
        vec4<f32>(0.260, 0.235, 0.679, aperture.z),
        vec4<f32>(0.225, 0.195, 0.675, aperture.z),
        vec4<f32>(0.138, light.enclosure_right_enabled.w, aperture.w, aperture.z));
    for (var section = 0u; section < 4u; section = section + 1u) {
        if (!cabin_section_visible(source, point, sections[section], aperture.y)) {
            return false;
        }
    }
    for (var track = 0u; track < 2u; track = track + 1u) {
        // Trace the main rendered skin, not the wider conservative collider or
        // the concealed centre seal. The clear half-gap includes that seal.
        let back = light.enclosure_doors[track * 2u];
        let front = light.enclosure_doors[track * 2u + 1u];
        let section = vec4<f32>(back, front, light.enclosure_origin_gap.w, aperture.z);
        if (!cabin_section_visible(source, point, section, aperture.y)) {
            return false;
        }
    }
    return true;
}

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
            if (cone > 0.0) {
                let relative = distance / beam.position_range.w;
                let window = max(1.0 - relative * relative * relative * relative, 0.0);
                let falloff = window * window / (1.0 + distance * distance);
                let ndl = max(dot(normal, direction), 0.0);
                let visibility = flashlight_visibility(world_position, normal, direction);
                sum += beam.color_strength.rgb * beam.color_strength.w * cone * falloff * ndl * visibility;
            }
        }
    }
    return sum;
}
