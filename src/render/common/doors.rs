//! Door geometry: the white interior leaf and the sauna leaf, built in code.
//!
//! A door is placed by its hinge; this module authors the two models the
//! renderer draws per door, both in **hinge space** (`+X` from the hinge to the
//! latch, `+Y` up, `+Z` the closed leaf's normal):
//!
//! * the **frame**, a static three-piece surround that never moves;
//! * the **leaf**, the moving slab with its round handle (and, for a sauna
//!   leaf, a wooden surround with a glass panel).
//!
//! Both models are ordinary [`PropModel`]s, so they travel the dynamic-object
//! path unchanged: model-space vertices, a per-object transform, and light from
//! the baked-light probe. The leaf's transform and the physical collider are
//! both derived from the same [`DoorRuntime`](crate::door::DoorRuntime) angle,
//! so what the player sees and what stops them can never disagree.
//!
//! Textures come from the level's resolved [`MaterialTable`] by
//! [`DoorDef`](crate::level::DoorDef) material id, so a door's surfaces are
//! catalog materials like every other surface, and replacing a door's PNG needs
//! no Rust change.

use std::sync::Arc;

use glam::Vec3;

use crate::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::level::{DoorDef, DoorKind, door_materials};
use crate::loader::RawImage;
use crate::materials::{MaterialAlpha, MaterialEmission, MaterialTable};

/// Frame width, in metres: how far the surround overlaps the wall opening on
/// each side.
const FRAME_WIDTH_M: f32 = 0.028;
/// How far the frame stands proud of the leaf on each face.
const FRAME_PROUD_M: f32 = 0.018;
/// Frame head height above the opening.
const FRAME_HEAD_M: f32 = 0.055;

/// Knob radius of the interior leaf's round brass handle, in metres.
const KNOB_RADIUS_M: f32 = 0.030;
/// Knob centre height above the leaf's bottom, in metres.
const KNOB_HEIGHT_M: f32 = 1.00;
/// Knob centre distance from the latch edge, in metres.
const KNOB_EDGE_M: f32 = 0.085;
/// Radius of the handle's back plate (the rose), in metres.
const ROSE_RADIUS_M: f32 = 0.042;

/// Sauna leaf: width of the side stiles, in metres.
const SAUNA_STILE_M: f32 = 0.10;
/// Sauna leaf: height of the top and bottom rails, in metres.
const SAUNA_RAIL_M: f32 = 0.13;
/// Sauna leaf: glass panel thickness, in metres.
const SAUNA_GLASS_M: f32 = 0.012;
/// Sauna handle radius, in metres.
const SAUNA_KNOB_RADIUS_M: f32 = 0.026;

/// Segment count of the round handle parts.
const KNOB_SEGMENTS: usize = 12;

/// The models one door draws, plus the textures and alpha contracts their
/// submeshes reference.
pub struct DoorModels {
    /// The static frame: jambs and head, opaque.
    pub frame: PropModel,
    /// The moving leaf: slab (or sauna stiles) plus handle and glass.
    pub leaf: PropModel,
    /// Decoded images, indexed by each submesh's `texture`.
    pub textures: Vec<Arc<RawImage>>,
    /// Alpha contract per submesh, parallel to each model's `submeshes`.
    pub alphas: Vec<MaterialAlpha>,
}

/// Builds the frame and leaf models for one door definition.
///
/// Returns `None` when a material cannot resolve to an image: a door without
/// its painted surfaces is not a door, and the caller logs the material table's
/// own error. `alphas` is shared by both models (the frame uses only opaque
/// slots, the leaf may add the glass slot).
#[must_use]
pub fn build_door_models(def: &DoorDef, materials: &MaterialTable) -> Option<DoorModels> {
    let defaults = door_materials(def.kind);
    let slab_id = def.material.as_deref().unwrap_or(defaults.slab);
    let frame_id = def.frame_material.as_deref().unwrap_or(defaults.frame);
    let handle_id = def.handle_material.as_deref().unwrap_or(defaults.handle);

    let slab = material_image(materials, slab_id)?;
    let frame = material_image(materials, frame_id)?;
    let handle = material_image(materials, handle_id)?;

    let mut textures = vec![slab.image.clone(), frame.image, handle.image];
    let mut alphas = vec![
        MaterialAlpha::OPAQUE,
        MaterialAlpha::OPAQUE,
        MaterialAlpha::OPAQUE,
    ];
    let glass_slot = match def.kind {
        DoorKind::Sauna => {
            let glass = material_image(materials, crate::level::SAUNA_DOOR_GLASS_MATERIAL)?;
            textures.push(glass.image.clone());
            alphas.push(glass.alpha);
            Some(3_u16)
        }
        DoorKind::Interior => None,
    };

    let mut frame_mesh = MeshBuilder::default();
    push_frame(&mut frame_mesh, def, frame.tile);
    let mut leaf_mesh = MeshBuilder::default();
    match def.kind {
        DoorKind::Interior => push_interior_leaf(&mut leaf_mesh, def, slab.tile, handle.tile),
        DoorKind::Sauna => {
            push_sauna_leaf(&mut leaf_mesh, def, slab.tile, handle.tile, glass_slot);
        }
    }

    Some(DoorModels {
        frame: frame_mesh.into_model(1)?,
        leaf: leaf_mesh.into_model(4)?,
        textures,
        alphas,
    })
}

/// One resolved door material: its image, tiling period and alpha contract.
struct DoorMaterial {
    image: Arc<RawImage>,
    tile: f32,
    alpha: MaterialAlpha,
}

/// Resolves a door material id through the table, or `None` when it has no
/// image or the id does not resolve.
fn material_image(materials: &MaterialTable, id: &str) -> Option<DoorMaterial> {
    let entry = materials.entry_of(id)?;
    let image = entry.image.clone()?;
    let tile = if entry.tile_metres.is_finite() && entry.tile_metres > 0.0 {
        entry.tile_metres
    } else {
        1.0
    };
    Some(DoorMaterial {
        image,
        tile,
        alpha: entry.alpha,
    })
}

/// An accumulating vertex/index pair, split into material slots.
#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<PropVertex>,
    indices: Vec<u16>,
    /// `(slot, first index, index count)` for every appended run, in order.
    runs: Vec<(u16, u32, u32)>,
}

impl MeshBuilder {
    /// Records one contiguous index run in `slot`.
    fn run(&mut self, slot: u16, first_index: u32, index_count: u32) {
        self.runs.push((slot, first_index, index_count));
    }

    /// Appends one quad with `p0 -> p1` as `u` and `p0 -> p3` as `v`.
    ///
    /// The corner order is the caller's winding; the UVs are the quad's own
    /// metric size divided by the material's tiling period, so a door face
    /// tiles at the same world scale as a wall built from the same material.
    #[allow(clippy::arithmetic_side_effects)] // bounded world dimensions and a positive tiling period
    fn quad(&mut self, corners: [[f32; 3]; 4], slot: u16, tile: f32) {
        let p0 = Vec3::from(corners[0]);
        let p1 = Vec3::from(corners[1]);
        let p3 = Vec3::from(corners[3]);
        let u = (p1 - p0).length() / tile;
        let v = (p3 - p0).length() / tile;
        let first = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
        let base = u16::try_from(self.vertices.len()).unwrap_or(u16::MAX);
        let uvs = [[0.0, 0.0], [u, 0.0], [u, v], [0.0, v]];
        for (corner, uv) in corners.iter().zip(uvs) {
            self.vertices.push(PropVertex {
                pos: *corner,
                color: [1.0, 1.0, 1.0, 1.0],
                uv,
            });
        }
        for offset in [0_u16, 1, 2, 0, 2, 3] {
            self.indices.push(base.saturating_add(offset));
        }
        self.run(slot, first, 6);
    }

    /// Appends an axis-aligned box as six quads in slot `slot`.
    #[allow(clippy::arithmetic_side_effects)] // fixed, bounded dimensions
    fn push_box(&mut self, min: [f32; 3], max: [f32; 3], slot: u16, tile: f32) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        if x1 <= x0 || y1 <= y0 || z1 <= z0 {
            return;
        }
        let faces: [[[f32; 3]; 4]; 6] = [
            // +X
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            // -X
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            // +Y
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            // -Y
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            // +Z
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            // -Z
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
        ];
        for corners in faces {
            self.quad(corners, slot, tile);
        }
    }

    /// Appends a capped cylinder whose axis runs along Z.
    #[allow(clippy::arithmetic_side_effects)] // bounded segment count and radius
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // bounded segment count
    fn push_cylinder_z(
        &mut self,
        centre: (f32, f32),
        span: (f32, f32),
        radius: f32,
        segments: usize,
        slot: u16,
        tile: f32,
    ) {
        let (z0, z1) = span;
        if segments < 3 || radius <= 0.0 || z1 <= z0 {
            return;
        }
        let (cx, cy) = centre;
        let count = u32::try_from(segments).unwrap_or(u32::MAX);
        let step = std::f32::consts::TAU / count as f32;
        let mut ring: Vec<(f32, f32)> = Vec::with_capacity(segments);
        for index in 0..count {
            let angle = step * index as f32;
            ring.push((
                radius.mul_add(angle.cos(), cx),
                radius.mul_add(angle.sin(), cy),
            ));
        }
        for index in 0..segments {
            let Some(&(ax, ay)) = ring.get(index) else {
                return;
            };
            let Some(&(bx, by)) = ring.get((index + 1) % segments) else {
                return;
            };
            self.quad(
                [[ax, ay, z0], [bx, by, z0], [bx, by, z1], [ax, ay, z1]],
                slot,
                tile,
            );
        }
        for (z, reverse) in [(z0, true), (z1, false)] {
            let first = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
            let base = u16::try_from(self.vertices.len()).unwrap_or(u16::MAX);
            let axis = [cx, cy, z];
            self.vertices.push(PropVertex {
                pos: axis,
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.5, 0.5],
            });
            for (x, y) in &ring {
                self.vertices.push(PropVertex {
                    pos: [*x, *y, z],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [*x / tile, *y / tile],
                });
            }
            for index in 0..count {
                let a = base
                    .saturating_add(1)
                    .saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
                let b = base
                    .saturating_add(1)
                    .saturating_add(u16::try_from((index + 1) % count).unwrap_or(u16::MAX));
                let (second, third) = if reverse { (b, a) } else { (a, b) };
                self.indices.extend_from_slice(&[base, second, third]);
            }
            self.run(slot, first, count.saturating_mul(3));
        }
    }

    /// Finishes the builder into a model with `slots` material slots.
    ///
    /// Adjacent runs in the same slot merge into one submesh, so a slab's six
    /// faces are one draw and the handle chain is one more. Each submesh's
    /// `texture` is its slot index into the shared texture list.
    #[allow(clippy::arithmetic_side_effects)] // bounded index counts
    fn into_model(self, slots: u16) -> Option<PropModel> {
        if self.indices.is_empty() || self.vertices.is_empty() {
            return None;
        }
        let mut submeshes: Vec<PropSubmesh> = Vec::new();
        for (slot, first, count) in &self.runs {
            if *count == 0 {
                continue;
            }
            if let Some(last) = submeshes.last_mut()
                && last.material == *slot
                && last.first_index.saturating_add(last.index_count) == *first
            {
                last.index_count = last.index_count.saturating_add(*count);
                continue;
            }
            submeshes.push(PropSubmesh {
                material: *slot,
                texture: Some(*slot),
                emission: MaterialEmission::NONE,
                first_index: *first,
                index_count: *count,
            });
        }
        if submeshes.is_empty() {
            return None;
        }
        let triangles = self.indices.len() / 3;
        Some(PropModel {
            vertices: self.vertices,
            indices: self.indices,
            submeshes,
            triangles,
            materials: usize::from(slots.max(1)),
            ..PropModel::default()
        })
    }
}

/// The static three-piece frame around a doorway.
#[allow(clippy::arithmetic_side_effects)] // bounded door dimensions
fn push_frame(builder: &mut MeshBuilder, def: &DoorDef, tile: f32) {
    let half = def.thickness.mul_add(0.5, FRAME_PROUD_M);
    let height = def.height + FRAME_HEAD_M;
    let width = def.width;
    // Hinge jamb.
    builder.push_box(
        [-FRAME_WIDTH_M, 0.0, -half],
        [FRAME_WIDTH_M, height, half],
        0,
        tile,
    );
    // Latch jamb.
    builder.push_box(
        [width - FRAME_WIDTH_M, 0.0, -half],
        [width + FRAME_WIDTH_M, height, half],
        0,
        tile,
    );
    // Head, between the jambs so no face is coplanar with one.
    builder.push_box(
        [FRAME_WIDTH_M, def.height - FRAME_WIDTH_M, -half],
        [width - FRAME_WIDTH_M, height, half],
        0,
        tile,
    );
}

/// The interior leaf: a slab with a round brass handle on both faces.
#[allow(clippy::arithmetic_side_effects)] // bounded door dimensions
fn push_interior_leaf(builder: &mut MeshBuilder, def: &DoorDef, slab_tile: f32, handle_tile: f32) {
    let half = def.thickness * 0.5;
    builder.push_box(
        [0.0, 0.0, -half],
        [def.width, def.height, half],
        0,
        slab_tile,
    );
    // Two shallow raised panels per face: the classic four-panel door read,
    // built from geometry so the painted sheet stays a plain surface.
    let margin_x = 0.14_f32.min(def.width * 0.18);
    let margin_y = 0.20_f32.min(def.height * 0.12);
    let gap = 0.05_f32.min(def.height * 0.04);
    let middle = def.height * 0.52;
    for side in [1.0_f32, -1.0] {
        let z0 = half * side;
        let z1 = (half + 0.008) * side;
        let (z_low, z_high) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };
        builder.push_box(
            [margin_x, middle + gap, z_low],
            [def.width - margin_x, def.height - margin_y, z_high],
            0,
            slab_tile,
        );
        builder.push_box(
            [margin_x, margin_y, z_low],
            [def.width - margin_x, middle - gap, z_high],
            0,
            slab_tile,
        );
    }
    push_knob(
        builder,
        def,
        def.width - KNOB_EDGE_M,
        KNOB_HEIGHT_M,
        1.0,
        KNOB_RADIUS_M,
        handle_tile,
    );
    push_knob(
        builder,
        def,
        def.width - KNOB_EDGE_M,
        KNOB_HEIGHT_M,
        -1.0,
        KNOB_RADIUS_M,
        handle_tile,
    );
}

/// A round handle on one face: rose, stem and knob along the leaf normal.
#[allow(clippy::arithmetic_side_effects)] // bounded door dimensions
fn push_knob(
    builder: &mut MeshBuilder,
    def: &DoorDef,
    x: f32,
    y: f32,
    side: f32,
    radius: f32,
    tile: f32,
) {
    let surface = side * (def.thickness * 0.5);
    let rose_depth = 0.010_f32;
    let stem_depth = 0.020_f32;
    let knob_depth = 0.024_f32;
    let rose_end = side.mul_add(rose_depth, surface);
    let stem_end = side.mul_add(stem_depth, rose_end);
    let knob_end = side.mul_add(knob_depth, stem_end);
    builder.push_cylinder_z(
        (x, y),
        (surface.min(rose_end), surface.max(rose_end)),
        ROSE_RADIUS_M,
        KNOB_SEGMENTS,
        1,
        tile,
    );
    builder.push_cylinder_z(
        (x, y),
        (rose_end.min(stem_end), rose_end.max(stem_end)),
        radius * 0.55,
        KNOB_SEGMENTS,
        1,
        tile,
    );
    builder.push_cylinder_z(
        (x, y),
        (stem_end.min(knob_end), stem_end.max(knob_end)),
        radius,
        KNOB_SEGMENTS,
        1,
        tile,
    );
}

/// The sauna leaf: wooden stiles and rails around a glass panel, with a wooden
/// round handle on both faces.
#[allow(clippy::arithmetic_side_effects)] // bounded door dimensions
fn push_sauna_leaf(
    builder: &mut MeshBuilder,
    def: &DoorDef,
    wood_tile: f32,
    handle_tile: f32,
    glass_slot: Option<u16>,
) {
    let half = def.thickness * 0.5;
    let stile = SAUNA_STILE_M.min(def.width * 0.4);
    let rail = SAUNA_RAIL_M.min(def.height * 0.4);
    // Side stiles.
    builder.push_box([0.0, 0.0, -half], [stile, def.height, half], 0, wood_tile);
    builder.push_box(
        [def.width - stile, 0.0, -half],
        [def.width, def.height, half],
        0,
        wood_tile,
    );
    // Top and bottom rails between the stiles.
    builder.push_box(
        [stile, def.height - rail, -half],
        [def.width - stile, def.height, half],
        0,
        wood_tile,
    );
    builder.push_box(
        [stile, 0.0, -half],
        [def.width - stile, rail, half],
        0,
        wood_tile,
    );
    // The glass panel filling the opening.
    if let Some(slot) = glass_slot {
        let glass_half = SAUNA_GLASS_M * 0.5;
        builder.push_box(
            [stile, rail, -glass_half],
            [def.width - stile, def.height - rail, glass_half],
            slot,
            wood_tile,
        );
    }
    let knob_x = def.width - stile * 0.5;
    let knob_y = def.height * 0.5;
    push_knob(
        builder,
        def,
        knob_x,
        knob_y,
        1.0,
        SAUNA_KNOB_RADIUS_M,
        handle_tile,
    );
    push_knob(
        builder,
        def,
        knob_x,
        knob_y,
        -1.0,
        SAUNA_KNOB_RADIUS_M,
        handle_tile,
    );
}

/// The metric bounds of a door model, for tests and diagnostics.
#[cfg(test)]
#[must_use]
pub fn door_model_bounds(model: &PropModel) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for vertex in &model.vertices {
        let point = Vec3::from(vertex.pos);
        if !point.is_finite() {
            return None;
        }
        min = min.min(point);
        max = max.max(point);
    }
    (min.x <= max.x).then_some((min, max))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing, clippy::float_cmp)]

    use super::*;
    use crate::assets::AssetCatalog;

    fn materials() -> MaterialTable {
        let level_fixture = r#"{
            "format_version": 2,
            "id": "door_geo",
            "name": "Door Geometry",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 } ],
            "doors": [
                { "id": "d1", "x": 1.0, "z": 3.0, "rotation_degrees": 0.0,
                  "width": 0.9, "height": 2.1 },
                { "id": "d2", "x": 4.0, "z": 3.0, "rotation_degrees": 0.0,
                  "width": 0.8, "height": 2.0, "kind": "sauna" }
            ]
        }"#;
        let level = crate::level::LevelDef::from_json(level_fixture).expect("level parses");
        let catalog = AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let mut cache = crate::materials::TextureCache::new();
        crate::materials::resolve_materials(&level, &catalog, None, Some(&root), &mut cache)
    }

    fn door(kind: DoorKind, width: f32, height: f32) -> DoorDef {
        DoorDef {
            id: "test".into(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rotation_degrees: 0.0,
            width,
            height,
            thickness: 0.045,
            kind,
            ..DoorDef::default()
        }
    }

    #[test]
    fn an_interior_door_builds_a_frame_and_a_leaf() {
        let materials = materials();
        let models = build_door_models(&door(DoorKind::Interior, 0.9, 2.1), &materials)
            .expect("door models build");
        assert!(!models.frame.vertices.is_empty());
        assert!(!models.leaf.vertices.is_empty());
        let (min, max) = door_model_bounds(&models.leaf).expect("bounds");
        assert!(min.x.abs() < 1e-3, "the leaf starts at its hinge: {min:?}");
        assert!(max.x >= 0.9 - 1e-3, "the leaf reaches its latch edge");
        assert!(max.y >= 2.1 - 1e-3);
        assert!(models.textures.len() >= 3);
    }

    #[test]
    fn a_sauna_door_adds_a_blended_glass_slot() {
        let materials = materials();
        let models = build_door_models(&door(DoorKind::Sauna, 0.8, 2.0), &materials)
            .expect("sauna models build");
        assert_eq!(models.textures.len(), 4, "wood, frame, handle, glass");
        assert!(
            models
                .alphas
                .iter()
                .any(|alpha| alpha.mode == crate::materials::AlphaMode::Blend),
            "the sauna glass must declare the blended pass"
        );
    }
}
