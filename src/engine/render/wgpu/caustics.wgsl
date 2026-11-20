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
