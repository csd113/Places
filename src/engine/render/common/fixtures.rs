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
//! * **Panel.** `u` runs along the panel's 1.12 m diffuser width axis and `v`
//!   across its 0.56 m depth axis, so a 2:1 sheet shows 1.09 mm per texel in
//!   both directions at 1024x512. The border between the diffuser and the
//!   outer 1.2 x 0.6 m footprint is the housing frame, not artwork. A rotated
//!   fixture rotates both the aperture and fitted sheet: local X becomes world
//!   +Z and local Z becomes world -X, preserving orientation and texel density.
//! * **Round diffuser.** Planar, in the fixture's own plane: the sheet centre
//!   is the fixture centre and the sheet's inscribed circle is the diffuser's
//!   outer radius. A 128x128 sheet therefore shows 3.9 mm per texel.
//! * **Wall luminaire.** `u` is the 0.4 m face width and `v` its 0.2 m height,
//!   so the sheet's aspect matches the face exactly.
//!
//! The *housing* is genuine body geometry. Fluorescent panels bind the ordinary
//! brushed-metal material, with no emission; other families keep their flat
//! authored body colours through the shared white sheet. The office panel is
//! a real recessed troffer: four housing side walls drop from the ceiling
//! plane, a bottom frame borders the aperture, an opaque backplate closes the housing
//! and the fitted sheet is recessed a lip above the frame's bottom, so nothing but the
//! diffuser glows and the fixture reads with a frame and real depth.
//!
//! A luminous face's vertex colour carries the source tint and emission
//! strength. The fitted sheet keeps its artwork and housing stays separate.

use super::{Vertex, add_quad_flat};
pub use crate::engine::lighting::{
    BLACKLIGHT_LENS_DROP_M, FLUSH_MOUNT_BODY_DROP_M, FLUSH_MOUNT_DIFFUSER_INSET_M,
    PANEL_BODY_DROP_M, PANEL_BORDER_DEPTH_M, PANEL_BORDER_WIDTH_M, PANEL_LIP_M, ROUND_LENS_DROP_M,
};

/// Texture rectangle of a whole fitted fixture sheet.
///
/// Fixture sheets are not tiles: each face samples the sheet once, so the
/// rectangle is the complete image. Corner order matches the face windings
/// below (`[low-v, low-u]` first), and `v = 0` is the image's top row.
const SHEET_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Neutral base for the panel housing: its ordinary metal material and baked
/// receiver illumination supply the final colour in the level emitter.
const PANEL_HOUSING_COLOR: [f32; 3] = [1.0; 3];

/// One down-facing housing quad at a constant `y`, in the same winding as the
/// luminous panel face.
fn panel_down_quad(housing: &mut Vec<Vertex>, x0: f32, x1: f32, z0: f32, z1: f32, y: f32) {
    add_quad_flat(
        housing,
        [x0, y, z0],
        [x1, y, z0],
        [x1, y, z1],
        [x0, y, z1],
        PANEL_HOUSING_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
}

/// A fitted 8:1 lens and an enclosed, non-emissive dark batten body.
pub fn add_blacklight_fixture(
    lit: &mut Vec<Vertex>,
    housing: &mut Vec<Vertex>,
    center: [f32; 3],
    rotation_degrees: f32,
    emission: [f32; 3],
) {
    let [world_x, world_y, world_z] = center;
    let turned = crate::engine::lighting::fixture_is_turned(rotation_degrees);
    let point = |dx: f32, dy: f32, dz: f32| {
        if turned {
            [world_x - dz, world_y + dy, world_z + dx]
        } else {
            [world_x + dx, world_y + dy, world_z + dz]
        }
    };
    let mut quad = |corners: [[f32; 3]; 4]| {
        let [corner_a, corner_b, corner_c, corner_d] = corners;
        add_quad_flat(
            housing,
            corner_a,
            corner_b,
            corner_c,
            corner_d,
            [0.06, 0.065, 0.08],
            SHEET_UV[0],
            SHEET_UV[1],
            SHEET_UV[2],
            SHEET_UV[3],
        );
    };
    // Four sides, the closed top, and the dark underside beneath the lens.
    quad([
        point(-0.6, -0.045, -0.075),
        point(0.6, -0.045, -0.075),
        point(0.6, 0.0, -0.075),
        point(-0.6, 0.0, -0.075),
    ]);
    quad([
        point(0.6, -0.045, 0.075),
        point(-0.6, -0.045, 0.075),
        point(-0.6, 0.0, 0.075),
        point(0.6, 0.0, 0.075),
    ]);
    quad([
        point(-0.6, -0.045, 0.075),
        point(-0.6, -0.045, -0.075),
        point(-0.6, 0.0, -0.075),
        point(-0.6, 0.0, 0.075),
    ]);
    quad([
        point(0.6, -0.045, -0.075),
        point(0.6, -0.045, 0.075),
        point(0.6, 0.0, 0.075),
        point(0.6, 0.0, -0.075),
    ]);
    quad([
        point(-0.6, 0.0, -0.075),
        point(-0.6, 0.0, 0.075),
        point(0.6, 0.0, 0.075),
        point(0.6, 0.0, -0.075),
    ]);
    quad([
        point(-0.6, -0.045, -0.075),
        point(0.6, -0.045, -0.075),
        point(0.6, -0.045, 0.075),
        point(-0.6, -0.045, 0.075),
    ]);
    add_quad_flat(
        lit,
        point(-0.56, -BLACKLIGHT_LENS_DROP_M, -0.07),
        point(0.56, -BLACKLIGHT_LENS_DROP_M, -0.07),
        point(0.56, -BLACKLIGHT_LENS_DROP_M, 0.07),
        point(-0.56, -BLACKLIGHT_LENS_DROP_M, 0.07),
        emission,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
}

/// Emits the office fluorescent panel's troffer: its luminous diffuser into
/// `lit`, and its housing into `housing`.
///
/// The outer footprint is the family's 1.2 x 0.6 m rectangle, hanging just
/// below the ceiling plane at `y`. The housing is a real shallow body: four
/// side walls drop to [`PANEL_BODY_DROP_M`], a bottom frame borders the
/// aperture, and a full opaque backplate closes the body at the mounting plane. The
/// diffuser samples the whole fitted sheet ([`SHEET_UV`]) once, recessed
/// [`PANEL_LIP_M`] above the frame's bottom; its aperture is inset by
/// [`PANEL_BORDER_WIDTH_M`] across the width axis and
/// [`PANEL_BORDER_DEPTH_M`] across the depth one, so the 2:1 sheet fits the
/// aperture exactly. `emission` is the diffuser's source-coloured emission strength;
/// the housing receives its ordinary metal material in the level emitter.
// The emitter takes the panel's two world extents, the housing sinks and the
// emission separately; bundling them would add a type used by one call site.
#[expect(
    clippy::too_many_arguments,
    reason = "The emitter takes the panel's two world extents, the housing sinks and the emission separately; bundling them would add a type used by one call site."
)]
pub fn add_panel_fixture(
    lit: &mut Vec<Vertex>,
    housing: &mut Vec<Vertex>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    emission: [f32; 3],
) {
    let y_bottom = y - PANEL_BODY_DROP_M;
    let turned = x1 - x0 < z1 - z0;
    let (border_x, border_z) = if turned {
        (PANEL_BORDER_DEPTH_M, PANEL_BORDER_WIDTH_M)
    } else {
        (PANEL_BORDER_WIDTH_M, PANEL_BORDER_DEPTH_M)
    };
    let dx0 = x0 + border_x;
    let dx1 = x1 - border_x;
    let dz0 = z0 + border_z;
    let dz1 = z1 - border_z;
    // The body's four outer side walls, drawn outward from the ceiling plane
    // down to the frame.
    add_quad_flat(
        housing,
        [x1, y_bottom, z0],
        [x0, y_bottom, z0],
        [x0, y, z0],
        [x1, y, z0],
        PANEL_HOUSING_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
    add_quad_flat(
        housing,
        [x0, y_bottom, z1],
        [x1, y_bottom, z1],
        [x1, y, z1],
        [x0, y, z1],
        PANEL_HOUSING_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
    add_quad_flat(
        housing,
        [x0, y_bottom, z0],
        [x0, y_bottom, z1],
        [x0, y, z1],
        [x0, y, z0],
        PANEL_HOUSING_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
    add_quad_flat(
        housing,
        [x1, y_bottom, z1],
        [x1, y_bottom, z0],
        [x1, y, z0],
        [x1, y, z1],
        PANEL_HOUSING_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
    // The bottom frame: four full-width/full-depth border strips that tile the
    // ring around the diffuser aperture without overlapping each other.
    panel_down_quad(housing, x0, x1, z0, dz0, y_bottom);
    panel_down_quad(housing, x0, x1, dz1, z1, y_bottom);
    panel_down_quad(housing, x0, dx0, dz0, dz1, y_bottom);
    panel_down_quad(housing, dx1, x1, dz0, dz1, y_bottom);
    // The opaque backplate seals the entire footprint. A ring leaves the back
    // open above the diffuser: ceiling receivers can then see its analytic
    // source and acquire direct light plus desaturated room recovery fill. The source
    // stays below this plate, so legitimate room-facing illumination survives.
    panel_down_quad(housing, x0, x1, z0, z1, y);
    panel_diffuser(
        lit,
        [dx0, dx1, dz0, dz1],
        y_bottom + PANEL_LIP_M,
        turned,
        emission,
    );
}

/// Fits the complete sheet to the luminous aperture in its local UV frame.
fn panel_diffuser(
    lit: &mut Vec<Vertex>,
    [x0, x1, z0, z1]: [f32; 4],
    y: f32,
    turned: bool,
    emission: [f32; 3],
) {
    let uv = if turned {
        // The same quarter-turn as the batten: local +X -> world +Z,
        // local +Z -> world -X. This rotates the artwork without mirroring it.
        [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
    } else {
        SHEET_UV
    };
    add_quad_flat(
        lit,
        [x0, y, z0],
        [x1, y, z0],
        [x1, y, z1],
        [x0, y, z1],
        emission,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
}

/// One flat ring quad of a round fixture, facing down.
#[expect(
    clippy::too_many_arguments,
    reason = "One fixture quad keeps its coordinates, angular endpoints, color and UV inputs explicit without allocating an intermediate object."
)]
pub fn add_ring_quad(
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
#[expect(
    clippy::too_many_arguments,
    reason = "One fixture quad keeps its coordinates, angular endpoints, color and UV inputs explicit without allocating an intermediate object."
)]
pub fn add_can_quad(
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

/// Close a ten-sided can at its mounting plane with four nondegenerate quads.
/// The fan follows the existing can vertices, preserving its exact silhouette.
fn add_round_backplate(housing: &mut Vec<Vertex>, center: [f32; 3], radius: f32, color: [f32; 3]) {
    let [cx, y, cz] = center;
    let point = |segment: u8| {
        let angle = f32::from(segment) / 10.0 * std::f32::consts::TAU;
        let (sin, cos) = angle.sin_cos();
        [radius.mul_add(cos, cx), y, radius.mul_add(sin, cz)]
    };
    for segment in (1_u8..9).step_by(2) {
        add_quad_flat(
            housing,
            point(0),
            point(segment.saturating_add(2)),
            point(segment.saturating_add(1)),
            point(segment),
            color,
            SHEET_UV[0],
            SHEET_UV[1],
            SHEET_UV[2],
            SHEET_UV[3],
        );
    }
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
/// face's source-coloured emission strength, so the diffuser retains its fitted artwork.
pub fn add_round_fixture(
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
    let diffuser_y = y - ROUND_LENS_DROP_M;
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
        let segment_index = u8::try_from(segment).unwrap_or(0);
        let a0 = f32::from(segment_index) / f32::from(segments) * std::f32::consts::TAU;
        let a1 = f32::from(segment_index.saturating_add(1)) / f32::from(segments)
            * std::f32::consts::TAU;
        let (sin0, cos0) = a0.sin_cos();
        let (sin1, cos1) = a1.sin_cos();
        add_ring_quad(
            lit,
            cx,
            cz,
            diffuser_y,
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
            diffuser_y,
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
    add_round_backplate(housing, [cx, y, cz], bezel_outer, BEZEL_COLOR);
}

/// Emits the residential flush-mount ceiling lamp: a shallow white drum with a
/// glowing diffuser disc, centred on `(cx, cz)` with its mounting plane at `y`.
///
/// The visible appearance is the family's sheet. The diffuser's lit ring samples
/// the sheet's inscribed circle (its planar UVs make the sheet's centre the
/// fixture's centre, so the artwork's concentric tone and rim land where the
/// geometry expects them), and the drum wall, its bottom rim and the centre boss
/// behind the diffuser's small centre hole are genuine untextured body geometry.
/// `emission` is the face's source-coloured emission strength, so the lit surface keeps
/// the source-tinted sheet colour.
pub fn add_flush_mount_fixture(
    lit: &mut Vec<Vertex>,
    housing: &mut Vec<Vertex>,
    cx: f32,
    cz: f32,
    y: f32,
    radius: f32,
    emission: [f32; 3],
) {
    const SEGMENTS: usize = 10;
    /// Painted white body, drawn through the shared untextured white sheet.
    const BODY_COLOR: [f32; 3] = [0.86, 0.86, 0.84];
    /// The body's shadowed bottom rim, a touch brighter than the wall it caps.
    const RIM_COLOR: [f32; 3] = [0.92, 0.92, 0.90];
    /// Radius of the diffuser's centre screw hole, as a fraction of `radius`.
    const CENTRE_HOLE: f32 = 0.08;
    /// How far the centre boss sits behind the diffuser plane, in metres.
    const BOSS_INSET: f32 = 0.004;
    let y_bottom = y - FLUSH_MOUNT_BODY_DROP_M;
    let diffuser_outer = (radius - FLUSH_MOUNT_DIFFUSER_INSET_M).max(radius * 0.5);
    let inner = radius * CENTRE_HOLE;
    // Planar UVs: the sheet's inscribed circle is the fixture's outer radius, so
    // one texel covers the same distance along both in-plane axes.
    let planar_uv = |r: f32, cos: f32, sin: f32| -> [f32; 2] {
        let unit = r / radius;
        [unit.mul_add(cos, 1.0) * 0.5, unit.mul_add(sin, 1.0) * 0.5]
    };
    // `SEGMENTS` is 10, so every segment index fits `u8` and converts to `f32`
    // exactly.
    let segments = u8::try_from(SEGMENTS).unwrap_or(0);
    for segment in 0..SEGMENTS {
        let segment_index = u8::try_from(segment).unwrap_or(0);
        let a0 = f32::from(segment_index) / f32::from(segments) * std::f32::consts::TAU;
        let a1 = f32::from(segment_index.saturating_add(1)) / f32::from(segments)
            * std::f32::consts::TAU;
        let (sin0, cos0) = a0.sin_cos();
        let (sin1, cos1) = a1.sin_cos();
        // The drum's outer wall, from the ceiling down to the diffuser plane.
        add_can_quad(
            housing, cx, cz, y, y_bottom, radius, cos0, sin0, cos1, sin1, BODY_COLOR,
        );
        // The body's bottom rim, between the drum wall and the diffuser.
        add_ring_quad(
            housing,
            cx,
            cz,
            y_bottom,
            diffuser_outer,
            radius,
            cos0,
            sin0,
            cos1,
            sin1,
            RIM_COLOR,
            SHEET_UV,
        );
        // The diffuser ring: the sheet's own artwork, lit by its source-coloured
        // emission. It is a ring rather than a filled disc so the fixture stays
        // quad-only; the small centre it leaves is covered by the boss below.
        add_ring_quad(
            lit,
            cx,
            cz,
            y_bottom,
            inner,
            diffuser_outer,
            cos0,
            sin0,
            cos1,
            sin1,
            emission,
            [
                planar_uv(diffuser_outer, cos0, sin0),
                planar_uv(diffuser_outer, cos1, sin1),
                planar_uv(inner, cos1, sin1),
                planar_uv(inner, cos0, sin0),
            ],
        );
    }
    add_round_backplate(housing, [cx, y, cz], radius, BODY_COLOR);
    // The centre boss: a small untextured plate just behind the diffuser's
    // centre hole, so the hole never shows the room through the fitting.
    let boss = radius * 0.13;
    add_quad_flat(
        housing,
        [cx - boss, y_bottom + BOSS_INSET, cz - boss],
        [cx + boss, y_bottom + BOSS_INSET, cz - boss],
        [cx + boss, y_bottom + BOSS_INSET, cz + boss],
        [cx - boss, y_bottom + BOSS_INSET, cz + boss],
        BODY_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
}

/// Emits a wall-mounted luminaire at `(x, y, z)` facing `yaw_degrees`: its
/// emissive front face into `lit` and its shallow housing into `housing`.
/// `emission` is the face's source-coloured emission strength, so the lens keeps its
/// sheet colour.
pub fn add_wall_fixture(
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
    // The mounting back is ordinary metal, including when a model falls back
    // to this procedural shell. Keep the front sheet as the only luminous face.
    add_quad_flat(
        housing,
        point(-HALF_WIDTH, -HALF_HEIGHT, 0.0),
        point(-HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, HALF_HEIGHT, 0.0),
        point(HALF_WIDTH, -HALF_HEIGHT, 0.0),
        BEZEL_COLOR,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
    );
}
