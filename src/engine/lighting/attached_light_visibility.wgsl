
fn visibility_triangle(triangle: VisibilityTriangle, origin: vec3<f32>, ray: vec3<f32>) -> bool {
    if (triangle.a_solid.w == 0.0) { return false; }
    let edge0 = triangle.edge0.xyz;
    let edge1 = triangle.edge1.xyz;
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
    let parallel = abs(ray) < vec3<f32>(1.0e-12);
    let inverse = 1.0 / select(ray, vec3<f32>(1.0), parallel);
    var index = 0u;
    let count = arrayLength(&visibility_nodes);
    for (var visit = 0u; visit < 512u; visit += 1u) {
        if (index >= count) { return true; }
        let node = visibility_nodes[index];
        if (!visibility_box(node, origin, inverse, parallel)) {
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
