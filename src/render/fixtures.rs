//! Ceiling and wall fixture geometry.
//!
//! The drawn fixture and the baked light pool come from one profile table
//! (`lighting::fixture_profile`), so what a fixture looks like and where it
//! pools light cannot drift apart.
//!
//! Fixture sheets
//! --------------
//! A family's *luminous* faces — the office panel's panel face, the round
//! downlight's diffuser ring, the wall luminaire's front face — draw the
//! fixture's own PNG (`assets/catalog.json` names it on the `light` asset). The
//! sheet is fitted once across the face it belongs to; nothing here tiles, and
//! the renderer uploads the sheets clamped for exactly that reason.
//!
//! * **Panel.** `u` runs along the panel's 1.2 m width axis and `v` across its
//!   0.6 m depth axis, so a 2:1 sheet shows 1.17 mm per texel in both
//!   directions at 1024x512. A rotated fixture rotates the sheet with it.
//! * **Round diffuser.** Planar, in the fixture's own plane: the sheet centre
//!   is the fixture centre and the sheet's inscribed circle is the diffuser's
//!   outer radius. A 128x128 sheet therefore shows 3.9 mm per texel.
//! * **Wall luminaire.** `u` is the 0.4 m face width and `v` its 0.2 m height,
//!   so the sheet's aspect matches the face exactly.
//!
//! The *housing* — the round can and bezel ring, the wall fixture's top,
//! bottom and ends — is genuine body geometry and stays untextured: it draws
//! its flat authored metal colour through the shared white sheet. The office
//! panel has no housing at all: its whole visible fixture is the fitted sheet,
//! which is why nothing is drawn beside the panel artwork.
//!
//! A luminous face's vertex colour is a *neutral* emission strength, never the
//! placed light's colour: the sheet defines the fixture's visible appearance,
//! and the authored light colour belongs to the illumination the bake resolves.

use super::{Vertex, add_quad_flat};

/// Texture rectangle of a whole fitted fixture sheet.
///
/// Fixture sheets are not tiles: each face samples the sheet once, so the
/// rectangle is the complete image. Corner order matches the face windings
/// below (`[low-v, low-u]` first), and `v = 0` is the image's top row.
const SHEET_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Emits the office fluorescent panel's luminous face into `lit`, facing down
/// into the room.
///
/// The fitted sheet is the whole fixture: no generated strip, frame or bezel
/// is drawn beside it, so the panel's visible appearance is exactly its PNG.
pub(super) fn add_panel_fixture(
    lit: &mut Vec<Vertex>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    emission: [f32; 3],
) {
    add_quad_flat(
        lit,
        [x0, y, z0],
        [x1, y, z0],
        [x1, y, z1],
        [x0, y, z1],
        emission,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
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
    uv: [[f32; 2]; 4],
) {
    let outer0 = [r_out.mul_add(cos0, cx), y, r_out.mul_add(sin0, cz)];
    let outer1 = [r_out.mul_add(cos1, cx), y, r_out.mul_add(sin1, cz)];
    let inner1 = [r_in.mul_add(cos1, cx), y, r_in.mul_add(sin1, cz)];
    let inner0 = [r_in.mul_add(cos0, cx), y, r_in.mul_add(sin0, cz)];
    add_quad_flat(
        scratch, outer0, outer1, inner1, inner0, color, uv[0], uv[1], uv[2], uv[3],
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

/// Emits a round recessed ceiling downlight: the emissive diffuser ring into
/// `lit`, and the flat bezel ring plus shallow can into `housing`, all facing
/// down into the room.
///
/// The diffuser is a ring rather than a filled disc so the fixture stays
/// quad-only; the small centre it leaves reads as the lamp recess behind a
/// nearly-closed diffuser. Its planar UVs make the sheet's centre the fixture
/// centre, so the artwork's concentric rings and lamp core land where the
/// geometry expects them whatever the fixture's radius. `emission` is the
/// face's neutral emission strength, so the diffuser keeps its sheet colour.
pub(super) fn add_round_fixture(
    lit: &mut Vec<Vertex>,
    housing: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    radius: f32,
    emission: [f32; 3],
) {
    const SEGMENTS: usize = 10;
    const BEZEL_COLOR: [f32; 3] = [0.40, 0.40, 0.40];
    /// Depth of the visible can below the ceiling plane, in metres.
    const CAN_DEPTH: f32 = 0.03;
    let inner = radius * 0.12;
    let bezel_outer = radius + 0.03;
    // Planar UVs: the sheet's inscribed circle is the diffuser's outer edge, so
    // one texel covers the same distance along both in-plane axes.
    let planar_uv = |r: f32, cos: f32, sin: f32| -> [f32; 2] {
        let unit = r / radius;
        [unit.mul_add(cos, 1.0) * 0.5, unit.mul_add(sin, 1.0) * 0.5]
    };
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
            lit,
            cx,
            cz,
            y,
            inner,
            radius,
            cos0,
            sin0,
            cos1,
            sin1,
            emission,
            [
                planar_uv(radius, cos0, sin0),
                planar_uv(radius, cos1, sin1),
                planar_uv(inner, cos1, sin1),
                planar_uv(inner, cos0, sin0),
            ],
        );
        add_ring_quad(
            housing,
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
            SHEET_UV,
        );
        add_can_quad(
            housing,
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

/// Emits a wall-mounted luminaire at `(x, y, z)` facing `yaw_degrees`: its
/// emissive front face into `lit` and its shallow housing into `housing`.
/// `emission` is the face's neutral emission strength, so the lens keeps its
/// sheet colour.
pub(super) fn add_wall_fixture(
    lit: &mut Vec<Vertex>,
    housing: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    z: f32,
    yaw_degrees: f32,
    emission: [f32; 3],
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
    // Emissive front face: the whole luminaire face is the sheet.
    add_quad_flat(
        lit,
        point(-HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, -HALF_HEIGHT, DEPTH),
        point(HALF_WIDTH, HALF_HEIGHT, DEPTH),
        point(-HALF_WIDTH, HALF_HEIGHT, DEPTH),
        emission,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
    // Top face (up), bottom face (down) and the two ends.
    add_quad_flat(
        housing,
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
        housing,
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
        housing,
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
        housing,
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
