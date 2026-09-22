//! Ceiling and wall fixture geometry.
//!
//! The drawn fixture and the baked light pool come from one profile table
//! (`lighting::fixture_profile`), so what a fixture looks like and where it
//! pools light cannot drift apart.

use super::{Vertex, add_quad_flat};

/// Emits the office fluorescent panel: a luminous panel with two bezel strips,
/// all facing down into the room.
pub(super) fn add_panel_fixture(
    scratch: &mut Vec<Vertex>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    glow: [f32; 3],
) {
    add_quad_flat(
        scratch,
        [x0, y, z0],
        [x1, y, z0],
        [x1, y, z1],
        [x0, y, z1],
        glow,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );

    let bezel_color = [0.40, 0.40, 0.40];
    let b = 0.05;
    add_quad_flat(
        scratch,
        [x0 - b, y, z0 - b],
        [x1 + b, y, z0 - b],
        [x1 + b, y, z0],
        [x0 - b, y, z0],
        bezel_color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
    add_quad_flat(
        scratch,
        [x0 - b, y, z1],
        [x1 + b, y, z1],
        [x1 + b, y, z1 + b],
        [x0 - b, y, z1 + b],
        bezel_color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// One flat ring quad of a round fixture, facing down.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_ring_quad(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    r_in: f32,
    r_out: f32,
    cos0: f32,
    sin0: f32,
    cos1: f32,
    sin1: f32,
    color: [f32; 3],
) {
    let outer0 = [r_out.mul_add(cos0, cx), y, r_out.mul_add(sin0, cz)];
    let outer1 = [r_out.mul_add(cos1, cx), y, r_out.mul_add(sin1, cz)];
    let inner1 = [r_in.mul_add(cos1, cx), y, r_in.mul_add(sin1, cz)];
    let inner0 = [r_in.mul_add(cos0, cx), y, r_in.mul_add(sin0, cz)];
    add_quad_flat(
        scratch,
        outer0,
        outer1,
        inner1,
        inner0,
        color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// One outward-facing side quad of a round fixture's shallow can.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_can_quad(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y_top: f32,
    y_bottom: f32,
    radius: f32,
    cos0: f32,
    sin0: f32,
    cos1: f32,
    sin1: f32,
    color: [f32; 3],
) {
    let top0 = [radius.mul_add(cos0, cx), y_top, radius.mul_add(sin0, cz)];
    let top1 = [radius.mul_add(cos1, cx), y_top, radius.mul_add(sin1, cz)];
    let bottom1 = [radius.mul_add(cos1, cx), y_bottom, radius.mul_add(sin1, cz)];
    let bottom0 = [radius.mul_add(cos0, cx), y_bottom, radius.mul_add(sin0, cz)];
    add_quad_flat(
        scratch,
        top0,
        top1,
        bottom1,
        bottom0,
        color,
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    );
}

/// Emits a round recessed ceiling downlight: a shallow can, a flat bezel ring
/// and an emissive diffuser ring, all facing down into the room.
///
/// The diffuser is a ring rather than a filled disc so the fixture stays
/// quad-only; the small centre it leaves reads as the lamp recess behind a
/// nearly-closed diffuser.
pub(super) fn add_round_fixture(
    scratch: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    radius: f32,
    glow: [f32; 3],
) {
    const SEGMENTS: usize = 10;
    const BEZEL_COLOR: [f32; 3] = [0.40, 0.40, 0.40];
    /// Depth of the visible can below the ceiling plane, in metres.
    const CAN_DEPTH: f32 = 0.03;
    let inner = radius * 0.12;
    let bezel_outer = radius + 0.03;
    // `SEGMENTS` is 10, so every segment index fits `u8` and converts to `f32`
    // exactly.
    let segments = u8::try_from(SEGMENTS).unwrap_or(0);
    for segment in 0..SEGMENTS {
        let segment = u8::try_from(segment).unwrap_or(0);
        let a0 = f32::from(segment) / f32::from(segments) * std::f32::consts::TAU;
        let a1 = f32::from(segment.saturating_add(1)) / f32::from(segments) * std::f32::consts::TAU;
        let (sin0, cos0) = a0.sin_cos();
        let (sin1, cos1) = a1.sin_cos();
        add_ring_quad(
            scratch, cx, cz, y, inner, radius, cos0, sin0, cos1, sin1, glow,
        );
        add_ring_quad(
            scratch,
            cx,
            cz,
            y,
            radius,
            bezel_outer,
            cos0,
            sin0,
            cos1,
            sin1,
            BEZEL_COLOR,
        );
        add_can_quad(
            scratch,
            cx,
            cz,
            y,
            y - CAN_DEPTH,
            bezel_outer,
            cos0,
            sin0,
            cos1,
            sin1,
            BEZEL_COLOR,
        );
    }
}

/// Emits a wall-mounted luminaire at `(x, y, z)` facing `yaw_degrees`:
/// a shallow housing with one emissive outward face.
pub(super) fn add_wall_fixture(
    scratch: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    z: f32,
    yaw_degrees: f32,
    glow: [f32; 3],
) {
    const HALF_WIDTH: f32 = 0.20;
    const HALF_HEIGHT: f32 = 0.10;
    const DEPTH: f32 = 0.11;
    const BEZEL_COLOR: [f32; 3] = [0.40, 0.40, 0.40];
    let yaw = yaw_degrees.to_radians();
    let (sin, cos) = yaw.sin_cos();
    // `+Z` is the fixture's front, exactly like a prop at rotation 0.
    let forward = [sin, 0.0, cos];
    let right = [cos, 0.0, -sin];
    let point = |u: f32, v: f32, d: f32| {
        [
            forward[0].mul_add(d, right[0].mul_add(u, x)),
            y + v,
            forward[2].mul_add(d, right[2].mul_add(u, z)),
        ]
    };
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    // Emissive front face.
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        glow,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    // Top face (up), bottom face (down) and the two ends.
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    add_quad_flat(
        scratch,
        point(-HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
}
