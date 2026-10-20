// Shared by world and decal receivers. Radiance is assembled exclusively from
// eligible direct sources, using the existing compiled visibility evaluator.
struct CausticNode {
    min: vec4<f32>,
    max: vec4<f32>,
    links: vec4<u32>,
    grid: vec4<u32>,
    tint_speed: vec4<f32>,
    params: vec4<f32>,
    motion: vec4<f32>,
};
struct CausticData {
    seconds: f32,
    count: u32,
    mean: f32,
    water_count: u32,
    nodes: array<CausticNode>,
};
@group(3) @binding(7) var<storage, read> caustics: CausticData;
@group(3) @binding(8) var caustic_texture: texture_2d<f32>;
@group(3) @binding(9) var caustic_sampler: sampler;
@group(3) @binding(10) var<storage, read> caustic_samples: array<vec4<f32>>;

fn caustic_contains(node: CausticNode, point: vec2<f32>) -> bool {
    if (any(point < node.min.xz) || any(point > node.max.xz)) { return false; }
    let centre = (node.min.xz + node.max.xz) * 0.5;
    return node.max.w <= 0.0 || distance(point, centre) < node.max.w;
}

// Last-authored overlap precedence also applies to dark water bodies.
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

fn caustic_field(node: CausticNode, position: vec3<f32>) -> vec3<f32> {
    if (node.links.w == 0u || any(node.grid.xyz < vec3<u32>(2u))) { return vec3<f32>(0.0); }
    let dimensions = node.grid.xyz;
    let coordinate = clamp((position - node.min.xyz) / max(node.max.xyz - node.min.xyz, vec3<f32>(1.0e-6)), vec3<f32>(0.0), vec3<f32>(1.0)) * vec3<f32>(dimensions - vec3<u32>(1u));
    let lower = min(vec3<u32>(coordinate), dimensions - vec3<u32>(2u));
    let fraction = coordinate - vec3<f32>(lower);
    var sample = vec4<f32>(0.0);
    for (var corner = 0u; corner < 8u; corner += 1u) {
        let offset = vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
        let cells = dimensions - vec3<u32>(1u);
        let cell = lower.x + cells.x * (lower.y + cells.y * lower.z);
        let index = node.grid.w + cell * 8u + corner;
        let weight = select(vec3<f32>(1.0) - fraction, fraction, offset != vec3<u32>(0u));
        sample += caustic_samples[index] * weight.x * weight.y * weight.z;
    }
    // A shelf's sampled light cannot reach a receiver buried beneath its floor.
    if (position.y < sample.w - 0.02) { return vec3<f32>(0.0); }
    return sample.rgb;
}

// Numeric PNG imagery supplies the bands; source radiance supplies every RGB
// channel. Mean subtraction redistributes only that source energy. Authored
// tint multiplies it, so a red source cannot acquire blue from a blue tint.
fn caustic_modulation(position: vec3<f32>, selected: u32, dx: vec2<f32>, dy: vec2<f32>, beam: vec3<f32>, receiver_light: vec4<f32>) -> vec3<f32> {
    if (selected == 0u) { return vec3<f32>(0.0); }
    let node = caustics.nodes[selected - 1u];
    var supported = receiver_light.rgb;
    if (receiver_light.w <= 0.0) { supported = caustic_field(node, position); }
    let depth = node.min.w - position.y;
    let source = dynamic_lights.flashlight.position_range.xyz;
    if (source.y > node.min.w && any(beam > vec3<f32>(0.0))) {
        let ray = source - position;
        let entry = position.xz + ray.xz * (depth / max(ray.y, 1.0e-6));
        if (caustic_contains(node, entry)) { supported += beam; }
    }
    if (all(supported <= vec3<f32>(0.0)) || node.params.x <= 0.0) { return vec3<f32>(0.0); }
    let edge_box = min(position.xz - node.min.xz, node.max.xz - position.xz);
    var edge = min(edge_box.x, edge_box.y);
    if (node.max.w > 0.0) { edge = node.max.w - distance(position.xz, (node.min.xz + node.max.xz) * 0.5); }
    let fade = smoothstep(0.01, 0.12, depth) * exp(-0.22 * depth) * smoothstep(0.0, 0.06, edge);
    // Surface and receiver patterns share fluid advection, retaining their
    // independent light eligibility, tint, speed and strength.
    let uv = (position.xz - node.motion.zw * caustics.seconds) * 0.45;
    let t = caustics.seconds * node.tint_speed.w + node.params.y;
    let rotate = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let a = textureSampleGrad(caustic_texture, caustic_sampler, uv + vec2<f32>(0.013, 0.009) * t, dx, dy).r;
    let b = textureSampleGrad(caustic_texture, caustic_sampler, rotate * uv * 1.13 + vec2<f32>(-0.008, 0.011) * t, rotate * dx * 1.13, rotate * dy * 1.13).r;
    let modulation = (0.5 * (a + b) / max(caustics.mean, 0.001) - 1.0) * node.params.x * fade;
    return supported * node.tint_speed.rgb * modulation;
}

// Static water sheets alone ask for presentation controls. The same bounded
// tree is shared, with an independent count so unlit water still moves.
fn water_surface_node(position: vec3<f32>) -> u32 {
    var cursor = 0u;
    var priority = 0u;
    var selected = 0u;
    while (cursor < caustics.water_count) {
        let node = caustics.nodes[cursor];
        if (node.links.y <= priority || any(position.xz < node.min.xz) || any(position.xz > node.max.xz)) {
            cursor = node.links.x;
        } else if (node.links.z == 0u) {
            cursor += 1u;
        } else {
            if (abs(position.y - node.min.w) < 0.001 && caustic_contains(node, position.xz)) {
                priority = node.links.z; selected = cursor + 1u;
            }
            cursor = node.links.x;
        }
    }
    return selected;
}
