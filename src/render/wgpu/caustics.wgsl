struct CausticNode {
    min: vec4<f32>,
    max: vec4<f32>,
    links: vec4<u32>,
};
struct CausticData {
    seconds: f32,
    count: u32,
    mean: f32,
    padding: u32,
    nodes: array<CausticNode>,
};
@group(3) @binding(7) var<storage, read> caustics: CausticData;
@group(3) @binding(8) var caustic_texture: texture_2d<f32>;
@group(3) @binding(9) var caustic_sampler: sampler;

fn caustic_contains(node: CausticNode, point: vec2<f32>) -> bool {
    if (any(point < node.min.xz) || any(point > node.max.xz)) { return false; }
    let centre = (node.min.xz + node.max.xz) * 0.5;
    return node.max.w <= 0.0 || distance(point, centre) < node.max.w;
}

// Threaded AABB traversal, including last-authored overlap precedence. A dark
// winning volume still wins: never fall through to an older illuminated body.
fn caustic_volume(position: vec3<f32>, geometric_normal: vec3<f32>) -> u32 {
    if (caustics.count == 0u || geometric_normal.y <= 0.5) { return 0u; }
    var cursor = 0u;
    var priority = 0u;
    var selected = 0u;
    while (cursor < caustics.count) {
        let node = caustics.nodes[cursor];
        if (node.links.y <= priority || any(position < node.min.xyz) || any(position > node.max.xyz)) {
            cursor = node.links.x;
        } else if (node.links.z == 0u) {
            cursor += 1u;
        } else {
            if (caustic_contains(node, position.xz)) { priority = node.links.z; selected = cursor + 1u; }
            cursor = node.links.x;
        }
    }
    return selected;
}

// Image samples carry all pattern imagery. Two slowly advected, differently
// oriented copies vary the bands' intersections and brightness continuously.
// Subtracting their measured mean redistributes existing energy; it cannot
// create emission or change alpha, reflections, or bloom's emission buffer.
fn caustic_modulation(in: VsOut, selected: u32, dx: vec2<f32>, dy: vec2<f32>, beam_lit: bool) -> vec2<f32> {
    if (selected == 0u) { return vec2<f32>(0.0); }
    let node = caustics.nodes[selected - 1u];
    if (node.links.w == 0u && dynamic_lights.flashlight.outer_enabled.y <= 0.5) { return vec2<f32>(0.0); }
    let depth = node.min.w - in.world_position.y;
    let edge_box = min(in.world_position.xz - node.min.xz, node.max.xz - in.world_position.xz);
    var edge = min(edge_box.x, edge_box.y);
    if (node.max.w > 0.0) { edge = node.max.w - distance(in.world_position.xz, (node.min.xz + node.max.xz) * 0.5); }
    let fade = smoothstep(0.01, 0.12, depth) * exp(-0.22 * depth)
        * smoothstep(0.0, 0.06, edge);
    var beam_support = 0.0;
    let source = dynamic_lights.flashlight.position_range.xyz;
    if (source.y > node.min.w && beam_lit) {
        let ray = source - in.world_position;
        let entry = in.world_position.xz + ray.xz * (depth / max(ray.y, 1.0e-6));
        beam_support = select(0.0, 1.0, caustic_contains(node, entry));
    }
    if (node.links.w == 0u && beam_support == 0.0) { return vec2<f32>(0.0); }
    let uv = in.world_position.xz * 0.45;
    let t = caustics.seconds;
    let rotate = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let a = textureSampleGrad(caustic_texture, caustic_sampler, uv + vec2<f32>(0.013, 0.009) * t, dx, dy).r;
    let b = textureSampleGrad(caustic_texture, caustic_sampler, rotate * uv * 1.13 + vec2<f32>(-0.008, 0.011) * t,
        rotate * dx * 1.13, rotate * dy * 1.13).r;
    let modulation = (0.5 * (a + b) / max(caustics.mean, 0.001) - 1.0) * 0.24 * fade;
    return modulation * vec2<f32>(f32(node.links.w), beam_support);
}
