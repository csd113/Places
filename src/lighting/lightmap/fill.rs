//! The per-texel light evaluation of one lightmap chart.
//!
//! A chart is a rectangle of atlas texels covering one planar patch of static
//! geometry. Filling it is the whole lightmap pass: for every texel, evaluate
//! the baked lighting at the corresponding world position and write the result
//! as a linear RGB triple. The bake itself is
//! [`crate::lighting::LevelLighting`]; this module owns only the texel walk,
//! the patch-to-world mapping and the output ordering the atlas builder and the
//! shader both rely on.
//!
//! Contract (frozen between the mesh emitter, this pass and the renderer):
//!
//! * `chart.height` rows of `chart.width` RGB triples, row-major.
//! * Row `j` runs along the patch's `v` axis, column `i` along `u`.
//! * The texels *span* the patch: texel `i` of `width` samples
//!   `u = i / (width - 1)`, so the first and last texel sit exactly on the
//!   patch's geometric edges (a one-texel axis samples the middle). The same
//!   holds along `v`.
//! * Values are linear display-space light, the same units as
//!   [`crate::lighting::LevelLighting::sample_in_room`], clamped per channel to
//!   `[AMBIENT_LEVEL, MAX_BRIGHTNESS]` exactly like the vertex bake.
//!
//! Why the edges are exact
//! -----------------------
//! The atlas is sampled bilinearly, and a chart's gutter is a copy of its own
//! border texel, so a fragment on a patch's geometric edge reconstructs exactly
//! that border texel's value. Sampling texel *centres* therefore made every
//! edge reconstruct the light half a texel *inside* its own patch. Two
//! coplanar patches sharing an edge (two albedo materials on one floor, two
//! length runs of one wall, two charts of one surface split at the chart-span
//! cap) then each reconstructed their own inward-shifted value, and since the
//! shifts point in opposite directions the pair formed a first-order step of
//! `g * (tA + tB) / 2` — a visible lighting seam at every material boundary,
//! even though the lighting itself is continuous there.
//!
//! Spanning the patch inclusively removes it at the source: both charts
//! evaluate the *same world point* on the shared edge and store it in the texel
//! that edge reconstructs. Nothing has to be matched, stitched or averaged, and
//! a genuine discontinuity — a 90-degree corner, a wall, another room — is
//! untouched because those samples are different world points to begin with.
//!
//! The evaluation goes through
//! [`crate::lighting::LevelLighting::lightmap_texel`], which is
//! `sample_in_room` without the wall-clearing walk: a texel is generated on a
//! surface plane, and the fast path returns exactly the same value as the
//! vertex bake for every texel that is not buried in a wall.
//!
//! Two details keep a *vertical* face honest, because its texels are generated
//! exactly on a solid boundary:
//!
//! * every wall/skirt texel is nudged one
//!   [`LIGHTMAP_FACE_NORMAL_BIAS_M`](super::super::tuning::LIGHTMAP_FACE_NORMAL_BIAS_M)
//!   along its patch normal, the same nudge the vertex bake applies to a face
//!   sample, so the texel is evaluated in the air the face opens into;
//! * a texel that is still inside a wall solid — a junction with a crossing
//!   wall, or the outermost floor/ceiling row where a wall is authored across
//!   the room boundary — takes the walked
//!   [`crate::lighting::LevelLighting::sample_in_room`] path instead, exactly
//!   like the vertex bake.
//!
//! A wall patch's room is resolved **per texel**, not from the patch's stored
//! hint. Abutting wall pieces coalesce into emission units, and one unit's
//! length run can span a room boundary (two rooms sharing a corridor wall, a
//! wall run through a doorway); a single per-run hint would make the baked
//! light switch room at the arbitrary run seam instead of at the room
//! boundary, and the seam would read as a lighting step in the middle of a
//! continuous face. The per-texel containment at the bias-shifted sample point
//! is the same resolution the vertex bake uses, so the light follows the world
//! continuously. Floors and ceilings are emitted per room already, so their
//! hint is exact and is kept.
//!
//! With both, `fill_chart` reproduces the vertex bake's values on the shipped
//! demo where the two paths are meant to agree.

use super::super::tuning::LIGHTMAP_FACE_NORMAL_BIAS_M;
use super::{Chart, LightmapPatch, PatchKind};
use crate::lighting::{AMBIENT_LEVEL, LevelLighting, MAX_BRIGHTNESS};

/// Fills one chart's texels.
///
/// Returns `chart.height` rows of `chart.width` RGB triples in `0..=1`,
/// row-major, `j` along `v`. The caller (the atlas builder) validates the
/// length and finiteness and turns any deviation into a named build failure,
/// so a malformed patch degrades to vertex lighting instead of a black level.
#[must_use]
pub fn fill_chart(lighting: &LevelLighting, patch: &LightmapPatch, chart: &Chart) -> Vec<[f32; 3]> {
    let width = usize::try_from(chart.width).unwrap_or(0);
    let height = usize::try_from(chart.height).unwrap_or(0);
    let mut texels: Vec<[f32; 3]> = Vec::with_capacity(width.saturating_mul(height));
    if width == 0 || height == 0 {
        return texels;
    }
    // A vertical face texel sits exactly on the face's solid boundary, which
    // point containment counts as buried; the vertex bake nudges such a sample
    // into the room before it measures light, so the texel grid does the same
    // along the patch normal. See `LIGHTMAP_FACE_NORMAL_BIAS_M`.
    let bias = face_normal_bias(patch);
    // A wall face can span several coalesced wall pieces, and a coalesced unit
    // can itself span a room boundary (two rooms sharing one corridor wall, a
    // wall run through a doorway). The patch's room is a per-emission-strip
    // hint: carrying it across the whole strip would resolve one room's
    // baseline, fixture set and doorway blends for a face that really stands in
    // two of them, and the lighting would step at the strip boundary — an
    // arbitrary geometry seam, not a room boundary. Wall texels therefore
    // resolve their room per texel from the (bias-shifted) sample point, the
    // same containment the vertex bake uses, so the light follows the world
    // continuously along the face. Floors and ceilings are emitted per room
    // already, so their hint is exact and stays.
    let room = match patch.kind {
        PatchKind::Wall => None,
        PatchKind::Floor | PatchKind::Ceiling | PatchKind::Skirt => patch.room,
    };
    for j in 0..height {
        let v = texel_axis(j, height);
        for i in 0..width {
            let u = texel_axis(i, width);
            let point = patch.point_at(u, v);
            let point = [point[0] + bias[0], point[1] + bias[1], point[2] + bias[2]];
            // A texel can lie inside a wall solid: at a junction with a
            // crossing wall, and along the outermost floor/ceiling row where a
            // wall was authored across the room boundary. Only those texels pay
            // for the walked path, which is what the vertex bake applies to
            // every sample; the fast path is exactly equivalent everywhere
            // else.
            let light = if lighting.wall_contains_point(point[0], point[2]) {
                room.map_or_else(
                    || lighting.sample(point[0], point[1], point[2]),
                    |room| lighting.sample_in_room(room, point[0], point[1], point[2]),
                )
            } else {
                lighting.lightmap_texel(room, point[0], point[1], point[2])
            };
            texels.push([
                light.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                light.g.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                light.b.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
            ]);
        }
    }
    texels
}

/// Local coordinate of texel `index` along one axis of `count` texels.
///
/// The texels span the patch inclusively: the first sits on the patch's `0`
/// edge and the last on its `1` edge, so a chart's border texels hold the light
/// exactly where the geometry ends. That is what makes two coplanar charts
/// agree on a shared edge (see the module docs). An axis of a single texel has
/// no span to divide, so it samples the middle.
fn texel_axis(index: usize, count: usize) -> f32 {
    let count = u16::try_from(count).unwrap_or(u16::MAX);
    if count <= 1 {
        return 0.5;
    }
    let index = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
    let last = f32::from(count.saturating_sub(1));
    (index / last).clamp(0.0, 1.0)
}

/// The world-space nudge a patch's texels get before evaluation.
///
/// Zero for floors and ceilings: their texel planes are interfaces or bodies a
/// segment endpoint on them is never blocked by. Walls and skirts sit on a
/// solid boundary, so they get [`LIGHTMAP_FACE_NORMAL_BIAS_M`] along their
/// winding normal — the same normal the mesh emits, since the patch is built
/// from the quad's own corners (`u = p0 -> p1`, `v = p0 -> p3`) and the quad
/// always winds toward the face the room sees.
fn face_normal_bias(patch: &LightmapPatch) -> [f32; 3] {
    if !matches!(patch.kind, PatchKind::Wall | PatchKind::Skirt) {
        return [0.0; 3];
    }
    let normal = cross(patch.u_axis, patch.v_axis);
    let length = dot(normal, normal).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return [0.0; 3];
    }
    let unit = [normal[0] / length, normal[1] / length, normal[2] / length];
    [
        unit[0] * LIGHTMAP_FACE_NORMAL_BIAS_M,
        unit[1] * LIGHTMAP_FACE_NORMAL_BIAS_M,
        unit[2] * LIGHTMAP_FACE_NORMAL_BIAS_M,
    ]
}

/// The cross product of two world vectors.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

/// The dot product of two world vectors.
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

#[cfg(test)]
// Test code: unwrap/expect, indexing and permissive float comparison are
// idiomatic here; the production lints stay enforced everywhere else.
#[allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::level::LevelDef;
    use crate::lighting::LevelLighting;
    use crate::lighting::lightmap::PatchKind;

    fn level(json: &str) -> LevelDef {
        LevelDef::from_json(json).unwrap_or_else(|error| panic!("test level must parse: {error}"))
    }

    #[test]
    fn fill_chart_spans_the_patch_edge_to_edge_row_major_along_v() {
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill",
                "name": "Fill",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }],
                "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01",
                                     "x": 4.0, "z": 4.0, "intensity": 0.4 }]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        // A 2 x 2 m floor patch at y = 0, u along +X and v along +Z.
        let patch = LightmapPatch {
            origin: [1.0, 0.0, 1.0],
            u_axis: [2.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 2.0],
            room: Some(0),
            kind: PatchKind::Floor,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 3,
        };
        let texels = fill_chart(&lighting, &patch, &chart);
        assert_eq!(texels.len(), 12);
        for j in 0..3usize {
            for i in 0..4usize {
                // Spanning, not centred: 0, 1/3, 2/3, 1 along u and 0, 1/2, 1
                // along v, so the border texels hold the patch's edge light.
                let u = i as f32 / 3.0;
                let v = j as f32 / 2.0;
                let point = patch.point_at(u, v);
                let expected = lighting.lightmap_texel(Some(0), point[0], point[1], point[2]);
                let expected = [
                    expected.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.g.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.b.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                ];
                let actual = texels[j * 4 + i];
                for (channel, value) in actual.iter().enumerate() {
                    assert!(
                        (value - expected[channel]).abs() < 1e-6,
                        "texel ({i}, {j}) channel {channel}: {value} vs {}",
                        expected[channel]
                    );
                }
            }
        }
    }

    #[test]
    fn a_single_texel_axis_samples_the_middle() {
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill_thin",
                "name": "Fill Thin",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        let patch = LightmapPatch {
            origin: [2.0, 0.5, 3.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            room: Some(0),
            kind: PatchKind::Floor,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let texels = fill_chart(&lighting, &patch, &chart);
        assert_eq!(texels.len(), 1);
        let middle = patch.point_at(0.5, 0.5);
        let expected = lighting.lightmap_texel(Some(0), middle[0], middle[1], middle[2]);
        assert_eq!(
            texels[0][0],
            expected.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS)
        );
    }

    #[test]
    fn fill_chart_samples_a_patch_outside_every_room_through_sample() {
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill_outside",
                "name": "Fill Outside",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        let patch = LightmapPatch {
            origin: [20.0, 0.0, 20.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            room: None,
            kind: PatchKind::Floor,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        let texels = fill_chart(&lighting, &patch, &chart);
        assert_eq!(texels.len(), 2);
        // Outside every room there is no pool: the ambient fill, and the same
        // value the whole-position sample returns with no room hint.
        assert_eq!(texels[0], [AMBIENT_LEVEL; 3]);
        let point = patch.point_at(1.0, 0.5);
        assert_eq!(texels[1], {
            let value = lighting.sample(point[0], point[1], point[2]);
            [
                value.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                value.g.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                value.b.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
            ]
        });
    }

    #[test]
    fn a_wall_chart_samples_just_off_its_own_face() {
        // A full-height partition at x = 4.9..5.1 splits the room into two
        // baseline areas (the bake's connectivity probe catches this wall).
        // The patch covers the partition's west face; its texels are pushed
        // one face-bias west, into the area the face opens into, exactly where
        // the vertex bake evaluates a wall sample. The unbiased face point
        // counts as buried and is walked east through the wall, which is what
        // the bias replaces.
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill_wall",
                "name": "Fill Wall",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
                "walls": [{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.0 }],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 5.0, "intensity": 0.6 },
                    { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 5.0, "intensity": 0.15 }
                ]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        assert!(
            lighting.is_partitioned(0),
            "the wall must partition the room: zones={}",
            lighting.zone_count()
        );
        // West face of the partition: u runs +Z and v runs +Y, so u x v = -X
        // is the face normal and the bias must point west.
        let patch = LightmapPatch {
            origin: [4.9, 0.0, 0.0],
            u_axis: [0.0, 0.0, 10.0],
            v_axis: [0.0, 3.0, 0.0],
            room: Some(0),
            kind: PatchKind::Wall,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 5,
            height: 2,
        };
        let texels = fill_chart(&lighting, &patch, &chart);
        assert_eq!(texels.len(), 10);
        let bias = face_normal_bias(&patch);
        assert!(
            bias[0] < -1e-6,
            "the bias must follow the face normal: {bias:?}"
        );
        let mut walked_differently = false;
        for j in 0..2usize {
            for i in 0..5usize {
                let u = texel_axis(i, 5);
                let v = texel_axis(j, 2);
                let point = patch.point_at(u, v);
                let shifted = [point[0] + bias[0], point[1] + bias[1], point[2] + bias[2]];
                let expected = lighting.sample_in_room(0, shifted[0], shifted[1], shifted[2]);
                let expected = [
                    expected.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.g.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.b.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                ];
                let actual = texels[j * 5 + i];
                for (channel, value) in actual.iter().enumerate() {
                    assert!(
                        (value - expected[channel]).abs() < 1e-6,
                        "texel ({i}, {j}) channel {channel}: {value} vs {}",
                        expected[channel]
                    );
                }
                // The unbiased face point is walked through the wall into the
                // east area, so its vertex-path value must differ: that is the
                // mismatch the bias exists to avoid.
                let walked = lighting.sample_in_room(0, point[0], point[1], point[2]);
                let biased = lighting.sample_in_room(0, shifted[0], shifted[1], shifted[2]);
                if (walked.r - biased.r).abs() > 1e-3
                    || (walked.g - biased.g).abs() > 1e-3
                    || (walked.b - biased.b).abs() > 1e-3
                {
                    walked_differently = true;
                }
            }
        }
        assert!(
            walked_differently,
            "the unbiased face point must be walked into the other baseline area"
        );
    }

    #[test]
    fn a_wall_junction_texel_falls_back_to_the_walked_path() {
        // Two crossing walls. The patch covers the west face of the X-running
        // wall across the junction with the Z-running one: there the nudged
        // texel is still inside the crossing wall, so it must equal the walked
        // `sample_in_room` path, not the fast texel path.
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill_junction",
                "name": "Fill Junction",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
                "walls": [
                    { "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.0 },
                    { "x": 0.0, "z": 4.9, "width": 10.0, "depth": 0.2, "height": 3.0 }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "intensity": 0.6 }
                ]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        let patch = LightmapPatch {
            origin: [4.9, 0.0, 4.8],
            u_axis: [0.0, 0.0, 0.4],
            v_axis: [0.0, 3.0, 0.0],
            room: Some(0),
            kind: PatchKind::Wall,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 2,
        };
        let texels = fill_chart(&lighting, &patch, &chart);
        assert_eq!(texels.len(), 8);
        let bias = face_normal_bias(&patch);
        let mut junction_texels = 0usize;
        for j in 0..2usize {
            for i in 0..4usize {
                let u = texel_axis(i, 4);
                let v = texel_axis(j, 2);
                let point = patch.point_at(u, v);
                let shifted = [point[0] + bias[0], point[1] + bias[1], point[2] + bias[2]];
                if lighting.wall_contains_point(shifted[0], shifted[2]) {
                    junction_texels += 1;
                }
                let expected = lighting.sample_in_room(0, shifted[0], shifted[1], shifted[2]);
                let expected = [
                    expected.r.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.g.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                    expected.b.clamp(AMBIENT_LEVEL, MAX_BRIGHTNESS),
                ];
                let actual = texels[j * 4 + i];
                for (channel, value) in actual.iter().enumerate() {
                    assert!(
                        (value - expected[channel]).abs() < 1e-6,
                        "texel ({i}, {j}) channel {channel}: {value} vs {}",
                        expected[channel]
                    );
                }
            }
        }
        assert!(
            junction_texels > 0,
            "the patch must cover the junction to exercise the fallback"
        );
    }

    #[test]
    fn fill_chart_of_an_empty_chart_is_empty() {
        let level = level(
            r#"{
                "format_version": 1,
                "id": "fill_empty",
                "name": "Fill Empty",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }]
            }"#,
        );
        let lighting = LevelLighting::bake(&level);
        let patch = LightmapPatch {
            origin: [0.0, 0.0, 0.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 0.0, 1.0],
            room: Some(0),
            kind: PatchKind::Floor,
        };
        let chart = Chart {
            page: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert!(fill_chart(&lighting, &patch, &chart).is_empty());
    }
}
