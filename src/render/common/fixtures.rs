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
//!   fixture swaps the aperture's world X/Z extents but leaves these UV axes
//!   fixed, so the sheet is not rotated with it (see the asset specification's
//!   rotated-panel note).
//! * **Round diffuser.** Planar, in the fixture's own plane: the sheet centre
//!   is the fixture centre and the sheet's inscribed circle is the diffuser's
//!   outer radius. A 128x128 sheet therefore shows 3.9 mm per texel.
//! * **Wall luminaire.** `u` is the 0.4 m face width and `v` its 0.2 m height,
//!   so the sheet's aspect matches the face exactly.
//!
//! The *housing* — the round can and bezel ring, the wall fixture's top,
//! bottom and ends, and the office panel's frame and body — is genuine body
//! geometry and stays untextured: it draws its flat authored metal colour
//! through the shared white sheet. The office panel is a real recessed
//! troffer: four housing side walls drop from the ceiling plane, a bottom
//! frame borders the aperture, a top flange closes the housing and the fitted
//! sheet is recessed a lip above the frame's bottom, so nothing but the
//! diffuser glows and the fixture reads with a frame and real depth.
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

/// How far the office panel's housing drops below the ceiling plane, in metres.
pub const PANEL_BODY_DROP_M: f32 = 0.045;
/// How far the panel's diffuser sits recessed above its frame's bottom, in m.
pub const PANEL_LIP_M: f32 = 0.012;
/// Bottom-frame border along the panel's 1.2 m width axis, in metres.
pub const PANEL_BORDER_WIDTH_M: f32 = 0.04;
/// Bottom-frame border along the panel's 0.6 m depth axis, in metres.
///
/// The border widths are chosen so the aperture keeps the fixture sheet's 2:1
/// aspect exactly: `1.2 - 2*0.04` by `0.6 - 2*0.02` is `1.12 x 0.56`.
pub const PANEL_BORDER_DEPTH_M: f32 = 0.02;
/// Paint of the panel's untextured housing; a mid grey, never black.
const PANEL_HOUSING_COLOR: [f32; 3] = [0.62, 0.62, 0.60];

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

/// Emits the office fluorescent panel's troffer: its luminous diffuser into
/// `lit`, and its housing into `housing`.
///
/// The outer footprint is the family's 1.2 x 0.6 m rectangle, hanging just
/// below the ceiling plane at `y`. The housing is a real shallow body: four
/// side walls drop to [`PANEL_BODY_DROP_M`], a bottom frame borders the
/// aperture, and a top flange closes the body at the ceiling plane. The
/// diffuser samples the whole fitted sheet ([`SHEET_UV`]) once, recessed
/// [`PANEL_LIP_M`] above the frame's bottom; its aperture is inset by
/// [`PANEL_BORDER_WIDTH_M`] across the width axis and
/// [`PANEL_BORDER_DEPTH_M`] across the depth one, so the 2:1 sheet fits the
/// aperture exactly. `emission` is the diffuser's neutral emission strength;
/// the housing is always its fixed mid grey.
// The emitter takes the panel's two world extents, the housing sinks and the
// emission separately; bundling them would add a type used by one call site.
#[allow(clippy::too_many_arguments)]
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
    let dx0 = x0 + PANEL_BORDER_WIDTH_M;
    let dx1 = x1 - PANEL_BORDER_WIDTH_M;
    let dz0 = z0 + PANEL_BORDER_DEPTH_M;
    let dz1 = z1 - PANEL_BORDER_DEPTH_M;
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
    // The top flange: the same ring at the ceiling plane, reaching in to the
    // aperture edge so the body is closed and the recess has a ceiling.
    panel_down_quad(housing, x0, x1, z0, dz0, y);
    panel_down_quad(housing, x0, x1, dz1, z1, y);
    panel_down_quad(housing, x0, dx0, dz0, dz1, y);
    panel_down_quad(housing, dx1, x1, dz0, dz1, y);
    // The diffuser: the fitted sheet, once, recessed above the frame's bottom.
    let y_diffuser = y_bottom + PANEL_LIP_M;
    add_quad_flat(
        lit,
        [dx0, y_diffuser, dz0],
        [dx1, y_diffuser, dz0],
        [dx1, y_diffuser, dz1],
        [dx0, y_diffuser, dz1],
        emission,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
}

/// One flat ring quad of a round fixture, facing down.
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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

/// Emits the residential flush-mount ceiling lamp: a shallow white drum with a
/// glowing diffuser disc, centred on `(cx, cz)` with its mounting plane at `y`.
///
/// The visible appearance is the family's sheet. The diffuser's lit ring samples
/// the sheet's inscribed circle (its planar UVs make the sheet's centre the
/// fixture's centre, so the artwork's concentric tone and rim land where the
/// geometry expects them), and the drum wall, its bottom rim and the centre boss
/// behind the diffuser's small centre hole are genuine untextured body geometry.
/// `emission` is the face's neutral emission strength, so the lit surface keeps
/// the sheet's own colour.
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
    /// How far the drum hangs below the ceiling plane, in metres.
    const BODY_DROP: f32 = 0.07;
    /// How far the diffuser sits inset behind the drum's outer edge, in metres.
    const DIFFUSER_INSET: f32 = 0.012;
    /// Painted white body, drawn through the shared untextured white sheet.
    const BODY_COLOR: [f32; 3] = [0.86, 0.86, 0.84];
    /// The body's shadowed bottom rim, a touch brighter than the wall it caps.
    const RIM_COLOR: [f32; 3] = [0.92, 0.92, 0.90];
    /// Radius of the diffuser's centre screw hole, as a fraction of `radius`.
    const CENTRE_HOLE: f32 = 0.08;
    /// How far the centre boss sits behind the diffuser plane, in metres.
    const BOSS_INSET: f32 = 0.004;
    let y_bottom = y - BODY_DROP;
    let diffuser_outer = (radius - DIFFUSER_INSET).max(radius * 0.5);
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
        let segment = u8::try_from(segment).unwrap_or(0);
        let a0 = f32::from(segment) / f32::from(segments) * std::f32::consts::TAU;
        let a1 = f32::from(segment.saturating_add(1)) / f32::from(segments) * std::f32::consts::TAU;
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
        // The diffuser ring: the sheet's own artwork, lit by its neutral
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
    // The centre boss: a small untextured plate just behind the diffuser's
    // centre hole, so the hole never shows the room through the fitting.
    let boss = radius * 0.13;
    add_quad_flat(
        housing,
        [cx - boss, y_bottom - BOSS_INSET, cz - boss],
        [cx + boss, y_bottom - BOSS_INSET, cz - boss],
        [cx + boss, y_bottom - BOSS_INSET, cz + boss],
        [cx - boss, y_bottom - BOSS_INSET, cz + boss],
        BODY_COLOR,
        SHEET_UV[0],
        SHEET_UV[1],
        SHEET_UV[2],
        SHEET_UV[3],
    );
}

/// Emits a wall-mounted luminaire at `(x, y, z)` facing `yaw_degrees`: its
/// emissive front face into `lit` and its shallow housing into `housing`.
/// `emission` is the face's neutral emission strength, so the lens keeps its
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
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect, indexing, float comparison and permissive
    // arithmetic are idiomatic in tests; the production lints stay enforced
    // everywhere else in the crate.
    #![allow(
        clippy::cast_precision_loss,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::suboptimal_flops
    )]

    use super::*;
    use crate::level::LevelDef;
    use crate::lighting::LevelLighting;

    /// One room with one fixture of every built-in family.
    fn fixture_family_level() -> LevelDef {
        LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "fixture_alignment",
                "name": "Fixture Alignment",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 6.0, "height": 3.0 }],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 3.0 },
                    { "fixture": "core:pool_light_round", "x": 6.0, "z": 3.0 },
                    { "fixture": "core:pool_light_wall", "x": 10.0, "z": 3.0,
                      "mount": "wall", "y": 1.7 },
                    { "fixture": "home:ceiling_light_round", "x": 4.0, "z": 3.0 }
                ]
            }"#,
        )
        .expect("valid fixture alignment level")
    }

    /// Geometric normal of the first triangle of a quad run (the renderer's
    /// front face is the winding's counter-clockwise side).
    fn quad_normal(vertices: &[Vertex]) -> [f32; 3] {
        let a = vertices[0].pos;
        let b = vertices[1].pos;
        let c = vertices[2].pos;
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let length = normal[0].hypot(normal[1]).hypot(normal[2]);
        [normal[0] / length, normal[1] / length, normal[2] / length]
    }

    /// Centre of a vertex run.
    fn quad_centre(vertices: &[Vertex]) -> [f32; 3] {
        let count = vertices.len() as f32;
        let sum = vertices.iter().fold([0.0_f32; 3], |sum, vertex| {
            [
                sum[0] + vertex.pos[0],
                sum[1] + vertex.pos[1],
                sum[2] + vertex.pos[2],
            ]
        });
        [sum[0] / count, sum[1] / count, sum[2] / count]
    }

    /// The luminous faces of every ceiling family face down; the wall lens
    /// faces its authored yaw. A mis-wound or flipped emitter would light the
    /// back of the room while reading black from the front.
    #[test]
    fn every_luminous_face_points_into_its_room() {
        const EMISSION: [f32; 3] = [0.8, 0.8, 0.8];
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_panel_fixture(&mut lit, &mut housing, 0.0, 1.2, 0.0, 0.6, 2.0, EMISSION);
        assert_eq!(quad_normal(&lit), [0.0, -1.0, 0.0], "the panel faces down");

        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_round_fixture(&mut lit, &mut housing, 0.0, 0.0, 2.0, 0.22, EMISSION);
        for quad in lit.as_chunks::<6>().0 {
            assert_eq!(
                quad_normal(quad),
                [0.0, -1.0, 0.0],
                "the round ring faces down"
            );
        }

        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_flush_mount_fixture(&mut lit, &mut housing, 0.0, 0.0, 2.9, 0.16, EMISSION);
        for quad in lit.as_chunks::<6>().0 {
            assert_eq!(
                quad_normal(quad),
                [0.0, -1.0, 0.0],
                "the drum diffuser faces down"
            );
        }

        for yaw in [0.0_f32, 90.0, 180.0, 270.0] {
            let mut lit = Vec::new();
            let mut housing = Vec::new();
            add_wall_fixture(&mut lit, &mut housing, 10.0, 1.7, 3.0, yaw, EMISSION);
            let radians = yaw.to_radians();
            let expected = [radians.sin(), 0.0, radians.cos()];
            let normal = quad_normal(&lit);
            assert!(
                (normal[0] - expected[0]).abs() < 1e-5
                    && normal[1].abs() < 1e-5
                    && (normal[2] - expected[2]).abs() < 1e-5,
                "the wall lens must face its authored yaw {yaw}: {normal:?} vs {expected:?}"
            );
        }
    }

    /// Only the luminous face is emissive; every other vertex is genuine
    /// housing and never carries the neutral emission value.
    #[test]
    fn only_the_luminous_face_is_emissive() {
        const EMISSION: [f32; 3] = [0.8, 0.8, 0.8];
        let is_emission = |color: [f32; 4]| color[0] == EMISSION[0];
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_panel_fixture(&mut lit, &mut housing, 0.0, 1.2, 0.0, 0.6, 2.0, EMISSION);
        add_round_fixture(&mut lit, &mut housing, 3.0, 0.0, 2.0, 0.22, EMISSION);
        add_flush_mount_fixture(&mut lit, &mut housing, 6.0, 0.0, 2.9, 0.16, EMISSION);
        add_wall_fixture(&mut lit, &mut housing, 10.0, 1.7, 3.0, 90.0, EMISSION);
        assert!(!lit.is_empty() && !housing.is_empty());
        assert!(lit.iter().all(|vertex| is_emission(vertex.color)));
        assert!(housing.iter().all(|vertex| !is_emission(vertex.color)));
    }

    /// The wall lens is the front face of the luminaire: its centre sits one
    /// body depth in front of the authored point, on the wall plane. The baked
    /// emitter that must light the room is the same face, so the two may only
    /// differ by that body depth (the bake currently emits from the authored
    /// point; the proposal in `bake_pool_proposal.md` moves it onto the face).
    #[test]
    fn the_wall_lens_sits_on_the_front_face_at_the_authored_point() {
        const EMISSION: [f32; 3] = [0.8, 0.8, 0.8];
        let (x, y, z, yaw) = (10.0_f32, 1.7_f32, 3.0_f32, 90.0_f32);
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_wall_fixture(&mut lit, &mut housing, x, y, z, yaw, EMISSION);
        let centre = quad_centre(&lit);
        let radians = yaw.to_radians();
        let forward = [radians.sin(), 0.0, radians.cos()];
        // The body is 0.11 m deep; the lens is its front face.
        assert!((centre[0] - forward[0].mul_add(0.11, x)).abs() < 1e-5);
        assert!((centre[1] - y).abs() < 1e-5);
        assert!((centre[2] - forward[2].mul_add(0.11, z)).abs() < 1e-5);
        let normal = quad_normal(&lit);
        assert!(
            (normal[0] - forward[0]).abs() < 1e-5
                && normal[1].abs() < 1e-5
                && (normal[2] - forward[2]).abs() < 1e-5,
            "the wall lens faces the authored yaw: {normal:?} vs {forward:?}"
        );

        // Whatever the bake emits from, it is within one body depth of the
        // drawn lens: a fixture cannot light the room from inside the wall.
        let level = fixture_family_level();
        let lighting = LevelLighting::bake(&level);
        let wall = lighting
            .lights()
            .iter()
            .find(|light| light.x() == x && light.z() == z)
            .expect("the wall fixture baked");
        let offset = (wall.x() - centre[0]).hypot(wall.z() - centre[2]);
        assert!(
            offset <= 0.12,
            "the wall emitter is {offset} m from its drawn lens"
        );
    }

    /// The bake's emitter plane matches the drawn luminous plane per family,
    /// in the same horizontal footprint and with the same centre: the pool can
    /// never sit visibly to one side of the fixture that casts it.
    #[test]
    fn emitter_planes_and_drawn_faces_share_their_centre_and_extents() {
        let level = fixture_family_level();
        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.lights().len(), 4);

        // Panel: 1.2 x 0.6 m housing, diffuser inside it.
        let panel = &lighting.lights()[0];
        assert_eq!(panel.half_w(), 0.6);
        assert_eq!(panel.half_d(), 0.3);
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_panel_fixture(
            &mut lit,
            &mut housing,
            2.0 - 0.6,
            2.0 + 0.6,
            3.0 - 0.3,
            3.0 + 0.3,
            panel.y(),
            [0.8; 3],
        );
        let centre = quad_centre(&lit);
        assert!((centre[0] - panel.x()).abs() < 1e-5 && (centre[2] - panel.z()).abs() < 1e-5);
        assert!(
            (panel.y() - centre[1] - (PANEL_BODY_DROP_M - PANEL_LIP_M)).abs() < 1e-5,
            "the panel emitter sits one recess (frame bottom minus lip) above its diffuser"
        );

        // Round downlight: the diffuser is the emitter plane exactly.
        let round = &lighting.lights()[1];
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_round_fixture(
            &mut lit,
            &mut housing,
            round.x(),
            round.z(),
            round.y(),
            round.half_w(),
            [0.8; 3],
        );
        assert_eq!(round.half_w(), round.half_d());
        let centre = quad_centre(&lit);
        assert!((centre[1] - round.y()).abs() < 1e-5);

        // Flush mount: the drum hangs below the emitter plane by its body drop.
        let flush = &lighting.lights()[3];
        let mut lit = Vec::new();
        let mut housing = Vec::new();
        add_flush_mount_fixture(
            &mut lit,
            &mut housing,
            flush.x(),
            flush.z(),
            flush.y(),
            flush.half_w(),
            [0.8; 3],
        );
        let centre = quad_centre(&lit);
        let drop = flush.y() - centre[1];
        assert!(
            (0.05..=0.08).contains(&drop),
            "the flush-mount diffuser hangs its drum depth below the emitter plane: {drop}"
        );
    }

    /// One ceiling fixture of a single family in a 12 x 6 x 3 m room.
    fn single_fixture_level(entry: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "fixture_falloff",
                "name": "Fixture Falloff",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 12.0, "depth": 6.0, "height": 3.0 }}],
                "ceiling_lights": [{entry}]
            }}"#
        );
        LevelDef::from_json(&json).expect("valid single-fixture level")
    }

    /// A fixture's pool is strongest directly under its drawn luminous face and
    /// falls off monotonically away from it: the emitter the bake uses shares
    /// the centre of the face the mesh draws.
    #[test]
    fn a_fixtures_pool_falls_off_monotonically_from_its_drawn_centre() {
        for (entry, centre) in [
            (
                r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 3.0, "brightness": 0.45 }"#,
                (2.0_f32, 3.0_f32),
            ),
            (
                r#"{ "fixture": "core:pool_light_round", "x": 6.0, "z": 3.0, "brightness": 0.45 }"#,
                (6.0, 3.0),
            ),
            (
                r#"{ "fixture": "home:ceiling_light_round", "x": 4.0, "z": 3.0, "brightness": 0.45 }"#,
                (4.0, 3.0),
            ),
        ] {
            let lighting = LevelLighting::bake(&single_fixture_level(entry));
            let lum = |x: f32| lighting.sample_luminance(x, 0.02, centre.1);
            let (under, one, two, three) = (
                lum(centre.0),
                lum(centre.0 + 1.0),
                lum(centre.0 + 2.0),
                lum(centre.0 + 3.0),
            );
            assert!(
                under > one && one > two && two > three,
                "{entry}: expected a monotone pool, got {under} {one} {two} {three}"
            );
        }
    }
}
