fn caustic_receiver_light(in: VsOut, selected: u32) -> vec4<f32> {
    let minimum = environment.caustics_receiver.bounds_min;
    let extent = environment.caustics_receiver.bounds_extent;
    if (selected == 0u || minimum.w <= 0.0) { return vec4<f32>(0.0); }
    if (extent.w != f32(caustics.nodes[selected - 1u].links.z)) { return vec4<f32>(0.0); }
    let coordinate = clamp((in.local_position - minimum.xyz) / max(extent.xyz, vec3<f32>(1.0e-6)), vec3<f32>(0.0), vec3<f32>(1.0));
    var radiance = vec4<f32>(0.0);
    for (var corner = 0u; corner < 8u; corner += 1u) {
        let offset = vec3<u32>(corner & 1u, (corner >> 1u) & 1u, (corner >> 2u) & 1u);
        let weight = select(vec3<f32>(1.0) - coordinate, coordinate, offset != vec3<u32>(0u));
        radiance += environment.caustics_receiver.radiance[corner] * weight.x * weight.y * weight.z;
    }
    // Anchors may sit above submerged static supports. A buried fragment must
    // keep an active zero override, rather than borrow either raised light or
    // the global field that includes this object's own dynamic shadow.
    if (in.world_position.y < radiance.w - 0.02) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    return vec4<f32>(radiance.rgb, 1.0);
}
