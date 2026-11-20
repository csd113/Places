
// Preserve direction before length/cross-product arithmetic. Screen-space
// derivatives shrink as the camera approaches or render resolution increases;
// authored normals change magnitude under the inverse-transpose model transform.
fn rescale_direction(direction: vec3<f32>) -> vec3<f32> {
    let scale = max(max(abs(direction.x), abs(direction.y)), abs(direction.z));
    return direction / select(1.0, scale, scale > 0.0);
}

fn unit_direction_or_zero(direction: vec3<f32>) -> vec3<f32> {
    let scaled = rescale_direction(direction);
    // Nonzero scaled vectors have a component of magnitude one. The zero
    // vector therefore stays finite without normalizing it or using an epsilon.
    return scaled * inverseSqrt(max(dot(scaled, scaled), 1.0));
}

struct SurfaceNormals {
    material: vec3<f32>,
    geometric: vec3<f32>,
};

// Prepare the normal frame once. Material shading orients the visible side
// and applies the normal map; caustic receiver selection keeps the original
// geometric orientation and its independent front-facing gate.
fn surface_normals(in: VsOut, front_facing: bool, selected_water: u32) -> SurfaceNormals {
    // Models may omit authored normals. Derive their diffuse normal from
    // actual posed world geometry, oriented toward the visible side.
    // Derivatives remain outside conditional control flow. Scale each vector
    // before crossing so a valid tiny/large surface cannot underflow/overflow.
    let position_dx = dpdx(in.world_position);
    let position_dy = dpdy(in.world_position);
    let authored_normal = unit_direction_or_zero(in.world_normal);
    let has_authored_normal = any(in.world_normal != vec3<f32>(0.0));
    var geometric = authored_normal;
    if (!has_authored_normal) {
        geometric = unit_direction_or_zero(cross(rescale_direction(position_dy), rescale_direction(position_dx)));
    }
    var normal = select(vec3<f32>(0.0, 1.0, 0.0), authored_normal, has_authored_normal);
    if (!front_facing) { normal = -normal; }
    if (!has_authored_normal) {
        if (any(geometric != vec3<f32>(0.0))) {
            normal = geometric;
            if (dot(normal, camera.position - in.world_position) < 0.0) { normal = -normal; }
        }
    }
    let normal_on = (material.flags & MATERIAL_FLAG_RESPONSE_ENABLED) != 0u
        && (material.flags & MATERIAL_FLAG_NORMAL_ENABLED) != 0u;
    if (normal_on) {
        let sampled = textureSample(normal_texture, normal_sampler, in.uv).xyz * 2.0 - 1.0;
        let scaled = vec3<f32>(
            sampled.x * material.normal_strength,
            sampled.y * material.normal_strength,
            sampled.z,
        );
        let tangent = normalize(in.world_tangent - normal * dot(normal, in.world_tangent));
        let bitangent = cross(normal, tangent) * in.handedness;
        normal = normalize(tangent * scaled.x + bitangent * scaled.y + normal * scaled.z);
    }
    var out: SurfaceNormals;
    if (water_blend_pass && in.water_surface > 0.5) {
        let selected = selected_water;
        if (selected > 0u) {
            let motion = caustics.nodes[selected - 1u].motion;
            let slope = water_wave_slope(in.world_position.xz, motion, caustics.seconds)
                + water_local_slope(in.world_position, caustics.nodes[selected - 1u].links.z);
            let facing = select(-1.0, 1.0, front_facing);
            normal = normalize(normal + vec3<f32>(-slope.x, 0.0, -slope.y) * facing);
        }
    }
    out.material = normal;
    out.geometric = geometric;
    return out;
}
