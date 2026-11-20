
fn movable_contact_visibility(in: VsOut, normal: vec3<f32>) -> f32 {
    // Entity bodies cannot receive their own projected contact footprint.
    if (environment.entity_irradiance.w != 0.0 || normal.y <= 0.5) { return 1.0; }
    var loss = 0.0;
    for (var index = 0u; index < min(environment.contact_shadow_count.x, 8u); index = index + 1u) {
        let shadow = environment.contact_shadows[index];
        let plane = shadow.receiver_plane;
        if (abs(dot(plane.xyz, in.world_position) + plane.w) > 0.03 || dot(normal, plane.xyz) < 0.95) { continue; }
        let height = max(shadow.position_floor.y - shadow.position_floor.w, 0.0);
        let projection = -shadow.incoming.xz * height / max(shadow.incoming.y, 0.35);
        let offset = in.world_position.xz - shadow.position_floor.xz - projection;
        let radius = max(shadow.half_height.xy, vec2<f32>(0.05)) + vec2<f32>(0.08 + height * 0.15);
        let elliptical = offset / radius;
        let footprint = exp(-2.0 * dot(elliptical, elliptical));
        // Bound the formerly infinite Gaussian before doing any ray work.
        if (footprint < 0.002) { continue; }
        let support = vec3<f32>(shadow.position_floor.x, shadow.position_floor.w, shadow.position_floor.z);
        let receiver = in.world_position + plane.xyz * 0.01;
        let origin = support + plane.xyz * 0.01;
        if (!static_world_visible(origin, receiver)) { continue; }
        // A segment over a gap must not connect separate coplanar decks.
        let middle = mix(support, in.world_position, 0.5);
        if (static_world_visible(middle + plane.xyz * 0.02, middle - plane.xyz * 0.02)) { continue; }
        loss += 0.22 * footprint * shadow.incoming.w;
    }
    return 1.0 - min(loss, 0.22);
}
