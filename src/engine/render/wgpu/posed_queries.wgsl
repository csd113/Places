// Preparation and fragment traversal have different register/branch costs.
// Keep the fragment's original early parallel rejection; the ambient compute
// program specializes this constant to defer edge lengths until a candidate hit.
const POSED_DEFER_PARALLEL_TEST:bool = false;

const POSED_DIRECTIONS = array<vec3<f32>, 64>(
        vec3<f32>(0.9998779222, 0.0000000000, 0.0156250000),
        vec3<f32>(-0.9998779222, -0.0000000000, -0.0156250000),
        vec3<f32>(-0.7365583349, 0.6747477704, 0.0468750000),
        vec3<f32>(0.7365583349, -0.6747477704, -0.0468750000),
        vec3<f32>(0.0871585142, -0.9931263151, 0.0781250000),
        vec3<f32>(-0.0871585142, 0.9931263151, -0.0781250000),
        vec3<f32>(0.6047885673, 0.7888395896, 0.1093750000),
        vec3<f32>(-0.6047885673, -0.7888395896, -0.1093750000),
        vec3<f32>(-0.9749283205, -0.1724510925, 0.1406250000),
        vec3<f32>(0.9749283205, 0.1724510925, -0.1406250000),
        vec3<f32>(0.8311991754, -0.5287408772, 0.1718750000),
        vec3<f32>(-0.8311991754, 0.5287408772, -0.1718750000),
        vec3<f32>(-0.2541922858, 0.9455826332, 0.2031250000),
        vec3<f32>(0.2541922858, -0.9455826332, -0.2031250000),
        vec3<f32>(-0.4480690423, -0.8627296753, 0.2343750000),
        vec3<f32>(0.4480690423, 0.8627296753, -0.2343750000),
        vec3<f32>(0.9055775203, 0.3307154580, 0.2656250000),
        vec3<f32>(-0.9055775203, -0.3307154580, -0.2656250000),
        vec3<f32>(-0.8826726841, 0.3643544527, 0.2968750000),
        vec3<f32>(0.8826726841, -0.3643544527, -0.2968750000),
        vec3<f32>(0.4003794701, -0.8555876719, 0.3281250000),
        vec3<f32>(-0.4003794701, 0.8555876719, -0.3281250000),
        vec3<f32>(0.2792896769, 0.8904194999, 0.3593750000),
        vec3<f32>(-0.2792896769, -0.8904194999, -0.3593750000),
        vec3<f32>(-0.7964700884, -0.4615706963, 0.3906250000),
        vec3<f32>(0.7964700884, 0.4615706963, -0.3906250000),
        vec3<f32>(0.8855070015, -0.1946762302, 0.4218750000),
        vec3<f32>(-0.8855070015, 0.1946762302, -0.4218750000),
        vec3<f32>(-0.5126973979, 0.7292592903, 0.4531250000),
        vec3<f32>(0.5126973979, -0.7292592903, -0.4531250000),
        vec3<f32>(-0.1124289238, -0.8676062451, 0.4843750000),
        vec3<f32>(0.1124289238, 0.8676062451, -0.4843750000),
        vec3<f32>(0.6551622450, 0.5521714337, 0.5156250000),
        vec3<f32>(-0.6551622450, -0.5521714337, -0.5156250000),
        vec3<f32>(-0.8364993345, 0.0345918737, 0.5468750000),
        vec3<f32>(0.8364993345, -0.0345918737, -0.5468750000),
        vec3<f32>(0.5783680931, -0.5755535017, 0.5781250000),
        vec3<f32>(-0.5783680931, 0.5755535017, -0.5781250000),
        vec3<f32>(-0.0366243731, 0.7920358355, 0.6093750000),
        vec3<f32>(0.0366243731, -0.7920358355, -0.6093750000),
        vec3<f32>(-0.4919710147, -0.5895456980, 0.6406250000),
        vec3<f32>(0.4919710147, 0.5895456980, -0.6406250000),
        vec3<f32>(0.7340499865, 0.0987653875, 0.6718750000),
        vec3<f32>(-0.7340499865, -0.0987653875, -0.6718750000),
        vec3<f32>(-0.5836846724, 0.4061125922, 0.7031250000),
        vec3<f32>(0.5836846724, -0.4061125922, -0.7031250000),
        vec3<f32>(0.1489716531, -0.6621939338, 0.7343750000),
        vec3<f32>(-0.1489716531, 0.6621939338, -0.7343750000),
        vec3<f32>(0.3198300689, 0.5581461157, 0.7656250000),
        vec3<f32>(-0.3198300689, -0.5581461157, -0.7656250000),
        vec3<f32>(-0.5755638315, -0.1836205606, 0.7968750000),
        vec3<f32>(0.5755638315, 0.1836205606, -0.7968750000),
        vec3<f32>(0.5088563942, -0.2351045607, 0.8281250000),
        vec3<f32>(-0.5088563942, 0.2351045607, -0.8281250000),
        vec3<f32>(-0.1974107440, 0.4717028806, 0.8593750000),
        vec3<f32>(0.1974107440, -0.4717028806, -0.8593750000),
        vec3<f32>(-0.1539072841, -0.4279014574, 0.8906250000),
        vec3<f32>(0.1539072841, 0.4279014574, -0.8906250000),
        vec3<f32>(0.3429997527, 0.1802710571, 0.9218750000),
        vec3<f32>(-0.3429997527, -0.1802710571, -0.9218750000),
        vec3<f32>(-0.2925827130, 0.0771238642, 0.9531250000),
        vec3<f32>(0.2925827130, -0.0771238642, -0.9531250000),
        vec3<f32>(0.0952333090, -0.1481096764, 0.9843750000),
        vec3<f32>(-0.0952333090, 0.1481096764, -0.9843750000));

// The same immutable opaque world in all quality modes and material passes.
struct VisibilityNode {
    minimum: vec3<f32>, first: u32,
    maximum: vec3<f32>, count: u32,
    escape: u32, ready: u32, _pad0: u32, _pad1: u32,
};
struct VisibilityTriangle { a_solid: vec4<f32>, edge0: vec4<f32>, edge1: vec4<f32> };
// One reciprocal per ray, reused by every visited node. Parallel axes use
// explicit slab membership rather than infinities (0 * infinity yields NaN).
fn visibility_box(node: VisibilityNode, origin: vec3<f32>, inverse: vec3<f32>, parallel: vec3<bool>) -> bool {
    if (any(parallel & ((origin < node.minimum) | (origin > node.maximum)))) { return false; }
    let a = (node.minimum - origin) * inverse;
    let b = (node.maximum - origin) * inverse;
    let near = select(min(a, b), vec3<f32>(0.0), parallel);
    let far = select(max(a, b), vec3<f32>(1.0), parallel);
    return max(max(near.x, near.y), max(near.z, 0.0)) <= min(min(far.x, far.y), min(far.z, 1.0));
}

// Query only the receiving character's current opaque posed range. Existing
// static/rigid visibility and residual irradiance retain their own paths.
fn posed_character_visible(point: vec3<f32>, normal: vec3<f32>, source: vec3<f32>, root_value: f32, scale: f32) -> bool {
    if (root_value == 0.0) { return true; }
    if (root_value < 0.0) { return false; }
    let root = u32(root_value - 1.0);
    if (root >= arrayLength(&posed_visibility_nodes)) { return false; }
    let end = posed_visibility_nodes[root].escape;
    if (end <= root || end > arrayLength(&posed_visibility_nodes)) { return false; }
    let origin = point + normal * (0.0002 * scale);
    let ray = source - origin;
    let ray_length = length(ray);
    if (ray_length <= 1.0e-6 * scale) { return true; }
    let parallel = abs(ray) < vec3<f32>(1.0e-12);
    let inverse = 1.0 / select(ray, vec3<f32>(1.0), parallel);
    var index = root;
    for (var visit = 0u; visit < 512u; visit += 1u) {
        if (index >= end) { return true; }
        let node = posed_visibility_nodes[index];
        if (!visibility_box(node, origin, inverse, parallel)) {
            index = node.escape;
            continue;
        }
        if (node.count == 0u) { index = node.first; continue; }
        if (node.count > 64u) { return false; }
        for (var slot = 0u; slot < node.count; slot += 1u) {
            let triangle = posed_visibility_triangles[node.first + slot];
            if (triangle.a_solid.w == 0.0) { continue; }
            let edge0 = triangle.edge0.xyz;
            let edge1 = triangle.edge1.xyz;
            let p = cross(ray, edge1);
            let determinant = dot(edge0, p);
            // A zero determinant cannot intersect. Reject barycentric/segment
            // misses before evaluating the scale-aware parallel tolerance;
            // only a candidate hit needs the two edge lengths.
            if (POSED_DEFER_PARALLEL_TEST) {
                if (determinant == 0.0) { continue; }
            } else {
                let tolerance = 1.0e-7 * length(edge0) * length(edge1) * ray_length;
                if (abs(determinant) <= tolerance) { continue; }
            }
            let offset = origin - triangle.a_solid.xyz;
            let u = dot(offset, p) / determinant;
            let q = cross(offset, edge0);
            let v = dot(ray, q) / determinant;
            let t = dot(edge1, q) / determinant;
            if (u >= -1.0e-6 && v >= -1.0e-6 && u + v <= 1.000001 &&
                t > 0.00005 * scale / ray_length && t < 1.0 - 0.00001 * scale / ray_length) {
                if (POSED_DEFER_PARALLEL_TEST) {
                    let tolerance = 1.0e-7 * length(edge0) * length(edge1) * ray_length;
                    if (abs(determinant) <= tolerance) { continue; }
                }
                return false;
            }
        }
        index = node.escape;
    }
    return false;
}

// Deterministic antipodal quadrature rotates with the posed receiver, avoiding
// camera/frame randomness and retaining rigid-transform invariance.
// Normalize its cosine weights so an unobstructed receiver retains exactly
// the existing isotropic residual. Only this receiver's posed mesh is tested.
fn posed_hemisphere_visibility_basis(point: vec3<f32>, normal: vec3<f32>, root: f32, scale: f32, reach: f32, basis: mat3x3<f32>) -> f32 {
    var total = 0.0;
    var visible = 0.0;
    for (var i = 0u; i < 64u; i += 1u) {
        let direction = basis * POSED_DIRECTIONS[i];
        let cosine = max(dot(normal, direction), 0.0);
        if (cosine > 0.0) {
            total += cosine;
            if (posed_character_visible(point, normal, point + direction * reach, root, scale)) {
                visible += cosine;
            }
        }
    }
    return visible / max(total, 1.0e-6);
}


fn posed_rotate(rotation:vec4<f32>,value:vec3<f32>) -> vec3<f32> {
    return value+2.0*cross(rotation.xyz,cross(rotation.xyz,value)+rotation.w*value);
}
fn posed_hemisphere_visibility_oriented(point:vec3<f32>,normal:vec3<f32>,root:f32,scale:f32,reach:f32,rotation:vec4<f32>) -> f32 {
    let basis=mat3x3<f32>(posed_rotate(rotation,vec3<f32>(1.0,0.0,0.0)),posed_rotate(rotation,vec3<f32>(0.0,1.0,0.0)),posed_rotate(rotation,vec3<f32>(0.0,0.0,1.0)));
    return posed_hemisphere_visibility_basis(point,normal,root,scale,reach,basis);
}
fn posed_hemisphere_visibility(point: vec3<f32>, normal: vec3<f32>, root: f32, scale: f32, reach: f32) -> f32 {
    return posed_hemisphere_visibility_oriented(point, normal, root, scale, reach, vec4<f32>(0.0, 0.0, 0.0, 1.0));
}
