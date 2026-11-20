
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

// A flat static hint avoids tree traversal inside an unambiguous authored
// surface. Boundary pixels and all ambiguous/uncached geometry use the complete
// original priority query. The cache adds no vertex bytes or per-frame uploads.
fn water_surface_node_hinted(position: vec3<f32>, hint: u32) -> u32 {
    if (hint > 0u && hint <= caustics.water_count) {
        let node = caustics.nodes[hint - 1u];
        let inside = all(position.xz > node.min.xz + vec2<f32>(0.001))
            && all(position.xz < node.max.xz - vec2<f32>(0.001));
        let circle = node.max.w <= 0.0 || distance(position.xz, (node.min.xz + node.max.xz) * 0.5) < node.max.w - 0.001;
        if (node.links.z > 0u && abs(position.y - node.min.w) < 0.0005 && inside && circle) { return hint; }
    }
    return water_surface_node(position);
}
