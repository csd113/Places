//! The per-texel light evaluation of one lightmap chart.
//!
//! A chart is a rectangle of atlas texels covering one planar patch of static
//! geometry. Filling it is the whole lightmap pass: for every texel centre,
//! evaluate the baked lighting at the corresponding world position and write
//! the result as a linear RGB triple. The bake itself is
//! [`crate::lighting::LevelLighting`]; this module owns only the texel walk,
//! the patch-to-world mapping and the output ordering the atlas builder and the
//! shader both rely on.
//!
//! Contract (frozen between the mesh emitter, this pass and the renderer):
//!
//! * `chart.height` rows of `chart.width` RGB triples, row-major.
//! * Row `j` runs along the patch's `v` axis, column `i` along `u`.
//! * Texel `(i, j)` samples the patch at `u = (i + 0.5) / width`,
//!   `v = (j + 0.5) / height` — texel *centres*, never corners, so a chart's
//!   edge texels sample a half texel inside the geometry they cover.
//! * Values are linear display-space light, the same units as
//!   [`crate::lighting::LevelLighting::sample_in_room`], clamped per channel to
//!   `[AMBIENT_LEVEL, MAX_BRIGHTNESS]` exactly like the vertex bake.
//!
//! The evaluation goes through
//! [`crate::lighting::LevelLighting::lightmap_texel`], which is
//! `sample_in_room` without the wall-clearing walk: a texel centre is generated
//! on a surface plane, and the fast path returns exactly the same value as the
//! vertex bake for every texel that is not buried in a wall.
//!
//! Two details keep a *vertical* face honest, because its texels are generated
//! exactly on a solid boundary:
//!
//! * every wall/skirt texel is nudged one
//!   [`LIGHTMAP_FACE_NORMAL_BIAS_M`](super::super::tuning::LIGHTMAP_FACE_NORMAL_BIAS_M)
//!   along its patch normal, the same nudge the vertex bake applies to a face
//!   sample, so the texel is evaluated in the air the face opens into;
//! * a junction texel that is still inside a crossing wall after the nudge
//!   takes the walked
//!   [`crate::lighting::LevelLighting::sample_in_room`] path instead.
//!
//! With both, `fill_chart` reproduces the vertex bake's wall values exactly on
//! the shipped demo (verified over every wall texel during development).

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
    let width_f = f32::from(u16::try_from(chart.width).unwrap_or(u16::MAX)).max(1.0);
    let height_f = f32::from(u16::try_from(chart.height).unwrap_or(u16::MAX)).max(1.0);
    // A vertical face texel sits exactly on the face's solid boundary, which
    // point containment counts as buried; the vertex bake nudges such a sample
    // into the room before it measures light, so the texel grid does the same
    // along the patch normal. See `LIGHTMAP_FACE_NORMAL_BIAS_M`.
    let vertical_face = matches!(patch.kind, PatchKind::Wall | PatchKind::Skirt);
    let bias = face_normal_bias(patch);
    for j in 0..height {
        let v = (texel_centre(j) / height_f).clamp(0.0, 1.0);
        for i in 0..width {
            let u = (texel_centre(i) / width_f).clamp(0.0, 1.0);
            let point = patch.point_at(u, v);
            let point = [point[0] + bias[0], point[1] + bias[1], point[2] + bias[2]];
            // At a wall junction the nudged texel can still be inside another
            // wall solid; only that rare case pays for the walked path, so the
            // texel matches the vertex bake there too.
            let light = if vertical_face && lighting.wall_contains_point(point[0], point[2]) {
                patch.room.map_or_else(
                    || lighting.sample(point[0], point[1], point[2]),
                    |room| lighting.sample_in_room(room, point[0], point[1], point[2]),
                )
            } else {
                lighting.lightmap_texel(patch.room, point[0], point[1], point[2])
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

/// Index of a texel's centre: `i + 0.5` for a non-negative `usize` index.
///
/// A `usize` above `2^24` cannot be represented exactly as an `f32` index, but
/// a chart needing one would need a page edge that no [`super::LightmapConfig`]
/// allows; the conversion is still saturating instead of wrapping.
fn texel_centre(index: usize) -> f32 {
    let index = u32::try_from(index).unwrap_or(u32::MAX);
    f32::from(u16::try_from(index).unwrap_or(u16::MAX)) + 0.5
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
    fn fill_chart_writes_texel_centres_row_major_along_v() {
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
                let u = (i as f32 + 0.5) / 4.0;
                let v = (j as f32 + 0.5) / 3.0;
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
        let point = patch.point_at(0.75, 0.5);
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
                let u = (i as f32 + 0.5) / 5.0;
                let v = texel_centre(j) / 2.0;
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
                let u = (i as f32 + 0.5) / 4.0;
                let v = texel_centre(j) / 2.0;
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
