//! Door geometry: the interior leaf and its frame, the sauna leaf, built in code.
//!
//! A door is placed by its hinge; this module authors the two models the
//! renderer draws per door, both in **hinge space** (`+X` from the hinge to the
//! latch, `+Y` up, `+Z` the closed leaf's normal):
//!
//! * the **frame**, a static surround that never moves: a reveal liner through
//!   the wall the leaf sits in, a stop lip into the opening, a casing on each
//!   end face and the hinge furniture;
//! * the **leaf**, the moving slab with its round handle, its hinge leaves and
//!   (for a sauna leaf, a wooden surround with a glass panel).
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
//! no Rust change. Each model's submesh samples the slot its own material was
//! resolved into (leaf, frame, handle, and the sauna glass), which is what makes
//! a per-door `frame_material` / `handle_material` override visible and keeps
//! the sauna glass in the blended pass.
//!
//! The dynamic path carries one flat probe shade per object and no per-vertex
//! normal, so a code-built model supplies its own face response the way the
//! imported props do: every emitted quad multiplies its face shade by a
//! constant chosen from its geometric normal. That is what makes the recessed
//! panels, the casing edges, the stop reveal and the rounded handles read at
//! walking distance instead of resolving into one flat silhouette.

use std::sync::Arc;

use glam::Vec3;

use crate::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::level::{DoorDef, DoorFrame, DoorKind, door_materials};
use crate::loader::RawImage;
use crate::materials::{MaterialAlpha, MaterialEmission, MaterialTable};

/// Frame casing width on the wall face, in metres.
const FRAME_CASING_M: f32 = 0.07;
/// How far the casing stands proud of the wall face, in metres.
const FRAME_CASING_PROUD_M: f32 = 0.012;
/// How far the casing and the liner bury behind the face they meet, in metres,
/// so no face of the frame is coplanar with a wall face or a wall reveal.
const FRAME_EMBED_M: f32 = 0.002;
/// How far the liner's stop lip reaches into the opening, in metres. The leaf
/// sits behind it, exactly like a real door behind a rebate, and the two
/// surfaces are 4 mm apart rather than coplanar.
const FRAME_STOP_M: f32 = 0.004;
/// How far the liner overlaps the wall beyond an opening edge, in metres.
const FRAME_OVERLAP_M: f32 = 0.03;
/// Smallest liner depth the build emits, in metres.
const FRAME_MIN_DEPTH_M: f32 = 0.05;
/// How far the casing feet sink into the floor, in metres, so no two feet (or
/// a foot and the liner's own base) share the floor's plane.
const FLOOR_EMBED_M: f32 = 0.004;

/// Interior leaf: width of the hinge and latch stiles, in metres.
const INTERIOR_STILE_M: f32 = 0.11;
/// Interior leaf: height of the top rail, in metres.
const INTERIOR_TOP_RAIL_M: f32 = 0.12;
/// Interior leaf: height of the bottom rail, in metres.
const INTERIOR_BOTTOM_RAIL_M: f32 = 0.22;
/// Interior leaf: height of the middle rail, in metres.
const INTERIOR_MIDDLE_RAIL_M: f32 = 0.14;
/// Interior leaf: how far each recessed panel sits back from the leaf face, in
/// metres, i.e. the depth of the shadow line around it.
const INTERIOR_PANEL_RECESS_M: f32 = 0.012;
/// How far a jointing box overlaps its neighbour, in metres: adjacent boxes
/// interpenetrate instead of abutting, so no two faces are ever coplanar.
const JOINT_OVERLAP_M: f32 = 0.005;

/// Knob radius of the interior leaf's round brass handle, in metres.
const KNOB_RADIUS_M: f32 = 0.028;
/// Knob centre height above the leaf's bottom, in metres.
const KNOB_HEIGHT_M: f32 = 1.00;
/// Knob centre distance from the latch edge, in metres.
const KNOB_EDGE_M: f32 = 0.055;
/// Radius of the handle's back plate (the rose), in metres.
const ROSE_RADIUS_M: f32 = 0.032;

/// Hinge leaf: width of the plate on the door's face, in metres.
const HINGE_PLATE_WIDTH_M: f32 = 0.075;
/// Hinge leaf: height of the plate, in metres.
const HINGE_PLATE_HEIGHT_M: f32 = 0.065;
/// Hinge leaf: how far the plate stands proud of the leaf's face, in metres.
const HINGE_PLATE_PROUD_M: f32 = 0.0025;
/// Hinge leaf: distance from the leaf's hinge edge to the plate's near side.
const HINGE_PLATE_INSET_M: f32 = 0.014;
/// Hinge knuckle radius, in metres. It sits on the hinge axis, so it is
/// rotation-invariant and can live in the moving leaf's model.
const HINGE_KNUCKLE_RADIUS_M: f32 = 0.012;
/// Hinge knuckle height, in metres.
const HINGE_KNUCKLE_HEIGHT_M: f32 = 0.10;
/// Hinge centre heights above the leaf's bottom, in metres. Each is clamped
/// into a short leaf.
const HINGE_HEIGHTS_M: [f32; 2] = [0.25, 1.85];

/// Sauna leaf: width of the side stiles, in metres.
const SAUNA_STILE_M: f32 = 0.10;
/// Sauna leaf: height of the top and bottom rails, in metres.
const SAUNA_RAIL_M: f32 = 0.13;
/// Sauna leaf: glass panel thickness, in metres.
const SAUNA_GLASS_M: f32 = 0.012;
/// Sauna handle radius, in metres.
const SAUNA_KNOB_RADIUS_M: f32 = 0.026;

/// Segment count of the round handle and hinge parts.
const KNOB_SEGMENTS: usize = 12;

/// The models one door draws, plus the textures and alpha contracts their
/// submeshes reference.
pub struct DoorModels {
    /// The static frame: the reveal liner, its stop lip, both casings and the
    /// hinge furniture.
    pub frame: PropModel,
    /// The moving leaf: slab (or sauna stiles) plus handle, hinge leaves and
    /// glass.
    pub leaf: PropModel,
    /// Decoded images, indexed by each submesh's `texture` slot.
    pub textures: Vec<Arc<RawImage>>,
    /// Alpha contract per frame submesh, parallel to `frame.submeshes`.
    pub frame_alphas: Vec<MaterialAlpha>,
    /// Alpha contract per leaf submesh, parallel to `leaf.submeshes`. The
    /// sauna glass entry is the blended one.
    pub leaf_alphas: Vec<MaterialAlpha>,
}

/// Slot of the leaf's own surface (slab, sauna wood) and of the frame's.
const SLOT_LEAF: u16 = 0;
const SLOT_FRAME: u16 = 1;
const SLOT_HANDLE: u16 = 2;
const SLOT_GLASS: u16 = 3;

/// Builds the frame and leaf models for one door definition.
///
/// `frame` is the wall tunnel the leaf is installed in (see
/// [`LevelDef::door_frame`](crate::level::LevelDef::door_frame)); it decides
/// how deep the liner runs and where the two casings land.
///
/// Returns `None` when a material cannot resolve to an image: a door without
/// its painted surfaces is not a door, and the caller logs the material table's
/// own error.
#[must_use]
pub fn build_door_models(
    def: &DoorDef,
    materials: &MaterialTable,
    frame: DoorFrame,
) -> Option<DoorModels> {
    let defaults = door_materials(def.kind);
    let slab_id = def.material.as_deref().unwrap_or(defaults.slab);
    let frame_id = def.frame_material.as_deref().unwrap_or(defaults.frame);
    let handle_id = def.handle_material.as_deref().unwrap_or(defaults.handle);

    let slab = material_image(materials, slab_id)?;
    let frame_image = material_image(materials, frame_id)?;
    let handle = material_image(materials, handle_id)?;

    let mut textures = vec![slab.image.clone(), frame_image.image, handle.image];
    let mut slot_alphas = vec![slab.alpha, frame_image.alpha, handle.alpha];
    let glass_slot = match def.kind {
        DoorKind::Sauna => {
            let glass = material_image(materials, crate::level::SAUNA_DOOR_GLASS_MATERIAL)?;
            textures.push(glass.image.clone());
            slot_alphas.push(glass.alpha);
            Some(SLOT_GLASS)
        }
        DoorKind::Interior => None,
    };
    let glass_tile = match def.kind {
        DoorKind::Sauna => material_image(materials, crate::level::SAUNA_DOOR_GLASS_MATERIAL)
            .map_or(slab.tile, |glass| glass.tile),
        DoorKind::Interior => slab.tile,
    };

    let mut frame_mesh = MeshBuilder::default();
    push_frame(&mut frame_mesh, def, frame, frame_image.tile);
    let mut leaf_mesh = MeshBuilder::default();
    match def.kind {
        DoorKind::Interior => push_interior_leaf(&mut leaf_mesh, def, slab.tile, handle.tile),
        DoorKind::Sauna => {
            push_sauna_leaf(
                &mut leaf_mesh,
                def,
                slab.tile,
                handle.tile,
                glass_tile,
                glass_slot,
            );
        }
    }

    let frame_model = frame_mesh.into_model()?;
    let leaf = leaf_mesh.into_model()?;
    let frame_alphas = submesh_alphas(&frame_model, &slot_alphas);
    let leaf_alphas = submesh_alphas(&leaf, &slot_alphas);
    Some(DoorModels {
        frame: frame_model,
        leaf,
        textures,
        frame_alphas,
        leaf_alphas,
    })
}

/// One alpha contract per submesh, taken from the slot that submesh samples.
///
/// The dynamic draw path reads this table by *submesh index*, not by material
/// slot, so it has to be derived from the built model: that is what keeps the
/// sauna glass blended when it is not the model's last submesh.
#[must_use]
fn submesh_alphas(model: &PropModel, slots: &[MaterialAlpha]) -> Vec<MaterialAlpha> {
    model
        .submeshes
        .iter()
        .map(|submesh| {
            submesh
                .texture
                .and_then(|slot| slots.get(usize::from(slot)).copied())
                .unwrap_or(MaterialAlpha::OPAQUE)
        })
        .collect()
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

/// The baked face shade of one emitted quad, from its hinge-space normal.
///
/// The dynamic path has no per-vertex normal: this constant stands in for the
/// room's key light, the way the prop toolkit bakes face shades into an
/// imported model's vertex colours. Faces that lie in the leaf's own plane keep
/// full brightness, top edges keep most of it, and bottom edges and the reveal
/// walls inside the frame are visibly darker, so a recess and a casing edge
/// read as geometry.
#[must_use]
fn face_shade(normal: Vec3) -> f32 {
    if normal.z.abs() >= 0.9 {
        1.0
    } else if normal.y >= 0.5 {
        0.94
    } else if normal.y <= -0.5 {
        0.82
    } else {
        0.86
    }
}

/// How one appended run is painted: the material slot it samples, that
/// material's tiling period and the surface's own shade multiplier.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Paint {
    slot: u16,
    tile: f32,
    shade: f32,
}

impl Paint {
    /// The paint of one part at `shade` times the face's orientation term.
    const fn new(slot: u16, tile: f32, shade: f32) -> Self {
        Self { slot, tile, shade }
    }
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
    /// The vertex colour is the quad's baked face shade (`shade` scales the
    /// orientation term), which is how a dynamic model with no per-vertex
    /// normals still shows its relief.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "bounded world dimensions and a positive tiling period"
    )] // bounded world dimensions and a positive tiling period
    fn quad(&mut self, corners: [[f32; 3]; 4], paint: Paint) {
        let p0 = Vec3::from(corners[0]);
        let p1 = Vec3::from(corners[1]);
        let p3 = Vec3::from(corners[3]);
        let u = (p1 - p0).length() / paint.tile;
        let v = (p3 - p0).length() / paint.tile;
        let normal = (p1 - p0).cross(p3 - p0).normalize_or_zero();
        let level = (face_shade(normal) * paint.shade).clamp(0.0, 1.0);
        let first = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
        let base = u16::try_from(self.vertices.len()).unwrap_or(u16::MAX);
        let uvs = [[0.0, 0.0], [u, 0.0], [u, v], [0.0, v]];
        for (corner, uv) in corners.iter().zip(uvs) {
            self.vertices.push(PropVertex {
                normal: None,
                pos: *corner,
                color: [level, level, level, 1.0],
                uv,
            });
        }
        for offset in [0_u16, 1, 2, 0, 2, 3] {
            self.indices.push(base.saturating_add(offset));
        }
        self.run(paint.slot, first, 6);
    }

    /// Appends an axis-aligned box as six quads in slot `slot`.
    // fixed, bounded dimensions
    fn push_box(&mut self, min: [f32; 3], max: [f32; 3], paint: Paint) {
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
            self.quad(corners, paint);
        }
    }

    /// Appends a capped cylinder whose axis runs along Z.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "bounded segment count and radius"
    )] // bounded segment count and radius
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "The private cylinder builders receive the fixed small KNOB_SEGMENTS count; count and loop indices fit f32 exactly"
    )] // bounded segment count
    fn push_cylinder_z(
        &mut self,
        centre: (f32, f32),
        span: (f32, f32),
        radius: f32,
        segments: usize,
        paint: Paint,
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
                paint,
            );
        }
        for (z, reverse) in [(z0, true), (z1, false)] {
            let first = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
            let base = u16::try_from(self.vertices.len()).unwrap_or(u16::MAX);
            let cap_level =
                face_shade(Vec3::new(0.0, 0.0, if reverse { -1.0 } else { 1.0 })) * paint.shade;
            self.vertices.push(PropVertex {
                normal: None,
                pos: [cx, cy, z],
                color: [cap_level, cap_level, cap_level, 1.0],
                uv: [0.5, 0.5],
            });
            for (x, y) in &ring {
                self.vertices.push(PropVertex {
                    normal: None,
                    pos: [*x, *y, z],
                    color: [cap_level, cap_level, cap_level, 1.0],
                    uv: [*x / paint.tile, *y / paint.tile],
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
            self.run(paint.slot, first, count.saturating_mul(3));
        }
    }

    /// Appends a capped cylinder whose axis runs along Y, for hinge knuckles.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "bounded segment count and radius"
    )] // bounded segment count and radius
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "The private cylinder builders receive the fixed small KNOB_SEGMENTS count; count and loop indices fit f32 exactly"
    )] // bounded segment count
    fn push_cylinder_y(
        &mut self,
        centre: (f32, f32),
        span: (f32, f32),
        radius: f32,
        segments: usize,
        paint: Paint,
    ) {
        let (y0, y1) = span;
        if segments < 3 || radius <= 0.0 || y1 <= y0 {
            return;
        }
        let (cx, cz) = centre;
        let count = u32::try_from(segments).unwrap_or(u32::MAX);
        let step = std::f32::consts::TAU / count as f32;
        let mut ring: Vec<(f32, f32)> = Vec::with_capacity(segments);
        for index in 0..count {
            let angle = step * index as f32;
            ring.push((
                radius.mul_add(angle.cos(), cx),
                radius.mul_add(angle.sin(), cz),
            ));
        }
        for index in 0..segments {
            let Some(&(ax, az)) = ring.get(index) else {
                return;
            };
            let Some(&(bx, bz)) = ring.get((index + 1) % segments) else {
                return;
            };
            self.quad(
                [[ax, y0, az], [bx, y0, bz], [bx, y1, bz], [ax, y1, az]],
                paint,
            );
        }
        for (y, reverse) in [(y0, true), (y1, false)] {
            let first = u32::try_from(self.indices.len()).unwrap_or(u32::MAX);
            let base = u16::try_from(self.vertices.len()).unwrap_or(u16::MAX);
            let cap_level =
                face_shade(Vec3::new(0.0, if reverse { -1.0 } else { 1.0 }, 0.0)) * paint.shade;
            self.vertices.push(PropVertex {
                normal: None,
                pos: [cx, y, cz],
                color: [cap_level, cap_level, cap_level, 1.0],
                uv: [0.5, 0.5],
            });
            for (x, z) in &ring {
                self.vertices.push(PropVertex {
                    normal: None,
                    pos: [*x, y, *z],
                    color: [cap_level, cap_level, cap_level, 1.0],
                    uv: [*x / paint.tile, *z / paint.tile],
                });
            }
            for index in 0..count {
                let a = base
                    .saturating_add(1)
                    .saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
                let b = base
                    .saturating_add(1)
                    .saturating_add(u16::try_from((index + 1) % count).unwrap_or(u16::MAX));
                let (second, third) = if reverse { (a, b) } else { (b, a) };
                self.indices.extend_from_slice(&[base, second, third]);
            }
            self.run(paint.slot, first, count.saturating_mul(3));
        }
    }

    /// Finishes the builder into a model.
    ///
    /// Adjacent runs in the same slot merge into one submesh, so a slab's six
    /// faces are one draw and the handle chain is one more. Each submesh's
    /// `texture` is its slot index into the shared texture list.
    // bounded index counts
    fn into_model(self) -> Option<PropModel> {
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
                response: crate::materials::MaterialResponse::NONE,
                material: *slot,
                texture: Some(*slot),
                emission: MaterialEmission::NONE,
                // The model's alpha contract lives in `DoorModels`' per-submesh
                // table, which the dynamic draw path reads.
                alpha: MaterialAlpha::OPAQUE,
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
            materials: 1,
            ..PropModel::default()
        })
    }
}

/// The static surround around a doorway: reveal liner, stop lip, casings and
/// hinges.
///
/// `frame` gives the tunnel's two end planes in hinge space, which is where the
/// casings land; the liner runs the whole tunnel and laps 30 mm into the wall
/// on every side so no wall reveal shows through a gap. Every box either laps
/// its neighbour or ends inside it, so no two frame faces share a plane.
// bounded door dimensions
fn push_frame(builder: &mut MeshBuilder, def: &DoorDef, frame: DoorFrame, tile: f32) {
    let (near, far) = frame.span();
    let (z0, z1) = (near.min(far), near.max(far));
    if z1 - z0 < FRAME_MIN_DEPTH_M {
        return;
    }
    // The liner laps this far past each face so a casing always covers its end.
    let z_lo = z0 - FRAME_EMBED_M;
    let z_hi = z1 + FRAME_EMBED_M;
    let stop = FRAME_STOP_M.min(def.width * 0.1);
    let head_stop = FRAME_STOP_M.min(def.height * 0.05);
    let overlap = FRAME_OVERLAP_M;
    // Reveal liner: the two jambs and the head, each reaching into the opening
    // by `stop` and back into the wall by `overlap`. The jambs stop where the
    // head begins, so the two never share a face plane.
    let jamb_top = (def.height - head_stop).max(0.0);
    let liner = Paint::new(SLOT_FRAME, tile, 0.9);
    let casing_paint = Paint::new(SLOT_FRAME, tile, 1.0);
    let furniture = Paint::new(SLOT_FRAME, tile, 0.88);
    builder.push_box([-overlap, 0.0, z_lo], [stop, jamb_top, z_hi], liner);
    builder.push_box(
        [def.width - stop, 0.0, z_lo],
        [def.width + overlap, jamb_top, z_hi],
        liner,
    );
    builder.push_box(
        [-overlap, def.height - head_stop, z_lo],
        [def.width + overlap, def.height + overlap, z_hi],
        liner,
    );

    // Casing on both end faces: two legs and a head, mitred square at the
    // corners so the head sits between the legs instead of over them. Each leg
    // ends inside the liner, so the stop lip remains the only face at the
    // opening edge.
    let casing = FRAME_CASING_M.min(def.width * 0.35);
    let leg_inner = stop - JOINT_OVERLAP_M;
    for (face_z, outward) in [(z1, 1.0_f32), (z0, -1.0)] {
        let back = outward.mul_add(-FRAME_EMBED_M, face_z);
        let front = outward.mul_add(FRAME_CASING_PROUD_M, face_z);
        let (c0, c1) = if back <= front {
            (back, front)
        } else {
            (front, back)
        };
        let top = def.height + casing;
        // The legs run a few millimetres into the floor: a casing foot must
        // never share a plane with the liner's own foot, and the sliver is
        // inside the floor surface.
        let foot = -FLOOR_EMBED_M;
        builder.push_box([-casing, foot, c0], [leg_inner, top, c1], casing_paint);
        builder.push_box(
            [def.width - leg_inner, foot, c0],
            [def.width + casing, top, c1],
            casing_paint,
        );
        builder.push_box(
            [stop, def.height, c0],
            [def.width - stop, top, c1],
            casing_paint,
        );
    }

    // Hinge furniture: a knuckle on the hinge axis plus its plate on the jamb
    // face, at each hinge height. Both sit inside the liner when the leaf is
    // closed and are exposed as it opens.
    for height in hinge_heights(def.height) {
        let half = HINGE_KNUCKLE_HEIGHT_M * 0.5;
        builder.push_cylinder_y(
            (0.0, 0.0),
            (height - half, height + half),
            HINGE_KNUCKLE_RADIUS_M,
            KNOB_SEGMENTS,
            furniture,
        );
        builder.push_box(
            [
                stop,
                HINGE_PLATE_HEIGHT_M.mul_add(-0.5, height),
                -FRAME_EMBED_M,
            ],
            [
                stop + HINGE_PLATE_PROUD_M,
                HINGE_PLATE_HEIGHT_M.mul_add(0.5, height),
                FRAME_EMBED_M,
            ],
            Paint::new(SLOT_FRAME, tile, 0.92),
        );
    }
}

/// The two hinge heights of a leaf, clamped so a short leaf still gets two.
#[must_use]
fn hinge_heights(height: f32) -> [f32; 2] {
    let margin = 0.15_f32.min(height * 0.2);
    let first = HINGE_HEIGHTS_M[0].clamp(margin, (height - margin).max(margin));
    let second = HINGE_HEIGHTS_M[1].clamp(first, (height - margin).max(first));
    [first, second]
}

/// The interior leaf: a panelled slab with a round brass handle and hinges.
///
/// The leaf is a stile-and-rail frame with two recessed panels rather than a
/// flat face with raised strips: the recesses give the silhouette of a real
/// door, and their side walls and floors carry the darker baked shades that
/// make the relief read under the dynamic probe light.
// bounded door dimensions
fn push_interior_leaf(builder: &mut MeshBuilder, def: &DoorDef, slab_tile: f32, handle_tile: f32) {
    let half = def.thickness * 0.5;
    let stile = INTERIOR_STILE_M.min(def.width * 0.22);
    let top_rail = INTERIOR_TOP_RAIL_M.min(def.height * 0.2);
    let bottom_rail = INTERIOR_BOTTOM_RAIL_M.min(def.height * 0.25);
    let middle_rail = INTERIOR_MIDDLE_RAIL_M.min(def.height * 0.15);
    let panel_half = (half - INTERIOR_PANEL_RECESS_M).max(half * 0.4);
    let middle_lo = middle_rail.mul_add(-0.5, def.height * 0.5);
    let middle_hi = middle_rail.mul_add(0.5, def.height * 0.5);
    let panel_lo = bottom_rail + JOINT_OVERLAP_M;
    let panel_hi = (def.height - top_rail - JOINT_OVERLAP_M).max(panel_lo);
    let panel_x0 = stile - JOINT_OVERLAP_M;
    let panel_x1 = def.width - stile + JOINT_OVERLAP_M;

    // Stiles and rails: the leaf's full thickness, butted at every joint and
    // overlapping only where the parts differ in thickness (the panels).
    let wood = Paint::new(SLOT_LEAF, slab_tile, 1.0);
    builder.push_box([0.0, 0.0, -half], [stile, def.height, half], wood);
    builder.push_box(
        [def.width - stile, 0.0, -half],
        [def.width, def.height, half],
        wood,
    );
    builder.push_box(
        [stile, def.height - top_rail, -half],
        [def.width - stile, def.height, half],
        wood,
    );
    builder.push_box(
        [stile, 0.0, -half],
        [def.width - stile, bottom_rail, half],
        wood,
    );
    // The recessed panels. Each is thinner than the frame around it, so both
    // faces show a real recess; `0.93` stands in for the reduced light a
    // recessed surface receives.
    let upper_lo = (middle_hi - JOINT_OVERLAP_M).max(panel_lo);
    let panel_spans = [
        (panel_lo, middle_lo - JOINT_OVERLAP_M),
        (upper_lo, panel_hi),
    ];
    let mut panelled = false;
    for (lo, hi) in panel_spans {
        if hi - lo < 0.08 || panel_x1 - panel_x0 < 0.08 {
            continue;
        }
        builder.push_box(
            [panel_x0, lo, -panel_half],
            [panel_x1, hi, panel_half],
            Paint::new(SLOT_LEAF, slab_tile, 0.93),
        );
        panelled = true;
    }
    if !panelled {
        // A leaf too small to hold a panel keeps a solid slab instead of an
        // empty gap between its stiles.
        builder.push_box(
            [stile, bottom_rail, -half],
            [def.width - stile, def.height - top_rail, half],
            wood,
        );
    }
    if middle_rail > JOINT_OVERLAP_M * 2.0 {
        builder.push_box(
            [stile, middle_lo, -half],
            [def.width - stile, middle_hi, half],
            wood,
        );
    }

    let knob_x = def.width - KNOB_EDGE_M.min(stile * 0.6);
    for side in [1.0_f32, -1.0] {
        push_knob(
            builder,
            def,
            knob_x,
            KNOB_HEIGHT_M.min(def.height * 0.6),
            side,
            KNOB_RADIUS_M,
            handle_tile,
        );
        push_hinge_plates(
            builder,
            def,
            side,
            HINGE_PLATE_INSET_M.min(stile * 0.25),
            handle_tile,
        );
    }
}

/// The sauna leaf: wooden stiles and rails around a glass panel, with a wooden
/// round handle on both faces and hinge furniture to match.
// bounded door dimensions
fn push_sauna_leaf(
    builder: &mut MeshBuilder,
    def: &DoorDef,
    wood_tile: f32,
    handle_tile: f32,
    glass_tile: f32,
    glass_slot: Option<u16>,
) {
    let half = def.thickness * 0.5;
    let stile = SAUNA_STILE_M.min(def.width * 0.4);
    let rail = SAUNA_RAIL_M.min(def.height * 0.4);
    let joint = JOINT_OVERLAP_M;
    // Side stiles.
    let wood = Paint::new(SLOT_LEAF, wood_tile, 1.0);
    builder.push_box([0.0, 0.0, -half], [stile, def.height, half], wood);
    builder.push_box(
        [def.width - stile, 0.0, -half],
        [def.width, def.height, half],
        wood,
    );
    // Top and bottom rails between the stiles.
    builder.push_box(
        [stile, def.height - rail, -half],
        [def.width - stile, def.height, half],
        wood,
    );
    builder.push_box([stile, 0.0, -half], [def.width - stile, rail, half], wood);
    // The glass panel filling the opening, inset in the wooden frame and drawn
    // in the blended pass at the glass material's own tiling.
    if let Some(slot) = glass_slot {
        let glass_half = SAUNA_GLASS_M * 0.5;
        builder.push_box(
            [stile - joint, rail - joint, -glass_half],
            [
                def.width - stile + joint,
                def.height - rail + joint,
                glass_half,
            ],
            Paint::new(slot, glass_tile, 1.0),
        );
    }
    let knob_x = stile.mul_add(-0.5, def.width);
    let knob_y = def.height * 0.5;
    for side in [1.0_f32, -1.0] {
        push_knob(
            builder,
            def,
            knob_x,
            knob_y,
            side,
            SAUNA_KNOB_RADIUS_M,
            handle_tile,
        );
        push_hinge_plates(
            builder,
            def,
            side,
            HINGE_PLATE_INSET_M.min(stile * 0.2),
            handle_tile,
        );
    }
}

/// A round handle on one face: rose, stem and knob along the leaf normal.
// bounded door dimensions
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
        ROSE_RADIUS_M.min(def.width * 0.25),
        KNOB_SEGMENTS,
        Paint::new(SLOT_HANDLE, tile, 0.96),
    );
    builder.push_cylinder_z(
        (x, y),
        (rose_end.min(stem_end), rose_end.max(stem_end)),
        radius * 0.55,
        KNOB_SEGMENTS,
        Paint::new(SLOT_HANDLE, tile, 1.0),
    );
    builder.push_cylinder_z(
        (x, y),
        (stem_end.min(knob_end), stem_end.max(knob_end)),
        radius,
        KNOB_SEGMENTS,
        Paint::new(SLOT_HANDLE, tile, 1.0),
    );
}

/// The door's own hinge leaves: a plate on each face at every hinge height.
// bounded door dimensions
fn push_hinge_plates(builder: &mut MeshBuilder, def: &DoorDef, side: f32, inset: f32, tile: f32) {
    let half = def.thickness * 0.5;
    let z0 = half * side;
    let z1 = (half + HINGE_PLATE_PROUD_M) * side;
    let (zl, zh) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };
    let x0 = inset.max(JOINT_OVERLAP_M);
    let x1 = (x0 + HINGE_PLATE_WIDTH_M).min(def.width * 0.35);
    if x1 <= x0 {
        return;
    }
    for height in hinge_heights(def.height) {
        let y0 = HINGE_PLATE_HEIGHT_M.mul_add(-0.5, height).max(0.0);
        let y1 = HINGE_PLATE_HEIGHT_M.mul_add(0.5, height).min(def.height);
        if y1 <= y0 {
            continue;
        }
        builder.push_box(
            [x0, y0, zl],
            [x1, y1, zh],
            Paint::new(SLOT_HANDLE, tile, 0.9),
        );
    }
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
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::assets::AssetCatalog;

    fn materials() -> MaterialTable {
        let level_fixture = r#"{
            "format_version": 3,
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
        let models = build_door_models(
            &door(DoorKind::Interior, 0.9, 2.1),
            &materials,
            DoorFrame {
                depth: 0.3,
                center: 0.0,
            },
        )
        .expect("door models build");
        assert!(
            !models.frame.vertices.is_empty(),
            "models.frame.vertices must contain entries"
        );
        assert!(
            !models.leaf.vertices.is_empty(),
            "models.leaf.vertices must contain entries"
        );
        let (min, max) = door_model_bounds(&models.leaf).expect("bounds");
        assert!(min.x.abs() < 1e-3, "the leaf starts at its hinge: {min:?}");
        assert!(max.x >= 0.9 - 1e-3, "the leaf reaches its latch edge");
        assert!(max.y >= 2.1 - 1e-3);
        assert!(models.textures.len() >= 3);
    }

    #[test]
    fn a_sauna_door_adds_a_blended_glass_slot() {
        let materials = materials();
        let models = build_door_models(
            &door(DoorKind::Sauna, 0.8, 2.0),
            &materials,
            DoorFrame {
                depth: 0.3,
                center: 0.0,
            },
        )
        .expect("sauna models build");
        assert_eq!(models.textures.len(), 4, "wood, frame, handle, glass");
        assert_eq!(
            models.leaf_alphas.len(),
            models.leaf.submeshes.len(),
            "one alpha contract per leaf submesh"
        );
        let blended: Vec<usize> = models
            .leaf_alphas
            .iter()
            .enumerate()
            .filter(|(_, alpha)| alpha.mode == crate::materials::AlphaMode::Blend)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            blended.len(),
            1,
            "exactly the glass submesh is blended: {:?}",
            models.leaf_alphas
        );
        let glass_slot = models.leaf.submeshes[blended[0]]
            .texture
            .expect("the glass submesh samples a texture");
        assert_eq!(glass_slot, SLOT_GLASS, "the blended submesh is the glass");
        let _ = (&models.frame_alphas, models.frame.submeshes.len());
    }

    #[test]
    fn every_submesh_samples_its_own_material_slot() {
        let materials = materials();
        let models = build_door_models(
            &door(DoorKind::Interior, 0.9, 2.1),
            &materials,
            DoorFrame {
                depth: 0.3,
                center: 0.0,
            },
        )
        .expect("door models build");
        // The frame samples the frame material, never the leaf's sheet; that is
        // what makes `frame_material` and `handle_material` visible.
        assert!(
            models
                .frame
                .submeshes
                .iter()
                .all(|submesh| submesh.texture == Some(SLOT_FRAME)),
            "the frame must sample the frame material: {:?}",
            models.frame.submeshes
        );
        assert!(
            models
                .leaf
                .submeshes
                .iter()
                .any(|submesh| submesh.texture == Some(SLOT_LEAF)),
            "the leaf keeps its own slab material"
        );
        assert!(
            models
                .leaf
                .submeshes
                .iter()
                .any(|submesh| submesh.texture == Some(SLOT_HANDLE)),
            "the handle and hinges sample the furniture material"
        );
    }

    #[test]
    fn a_wall_depth_frame_puts_a_casing_on_both_faces() {
        let materials = materials();
        let def = door(DoorKind::Interior, 0.9, 2.1);
        let frame = DoorFrame {
            depth: 0.3,
            center: 0.0,
        };
        let models = build_door_models(&def, &materials, frame).expect("door models build");
        let (min, max) = door_model_bounds(&models.frame).expect("bounds");
        let proud = FRAME_CASING_PROUD_M.mul_add(1.0, frame.depth * 0.5);
        assert!(
            (min.z + proud).abs() < 1e-4 && (max.z - proud).abs() < 1e-4,
            "the casings stand proud of both wall faces: {min:?}..{max:?}"
        );
        assert!(
            max.x > def.width && min.x < 0.0,
            "the casings lap past both jambs: {min:?}..{max:?}"
        );
    }

    #[test]
    fn no_two_frame_faces_share_a_plane() {
        // A coplanar pair is a z-fight waiting to happen, and the frame is the
        // one door model that never moves, so a bad plane is permanent.
        let materials = materials();
        let def = door(DoorKind::Interior, 1.4, 2.1);
        let models = build_door_models(
            &def,
            &materials,
            DoorFrame {
                depth: 0.7,
                center: -0.2,
            },
        )
        .expect("door models build");
        assert_no_coplanar_faces(&models.frame);
        assert_no_coplanar_faces(&models.leaf);
    }

    /// Vector difference, kept out of the operator form so the test body
    /// itself stays lint-clean. Test-only, bounded model coordinates.
    fn sub(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    }

    /// Fails when two faces with the same facing sit in one plane and overlap.
    ///
    /// Triangles that share a vertex are skipped: the two halves of one quad
    /// and the fan around a cylinder cap are coplanar by construction and never
    /// overlap a *different* face. The overlap is measured in the plane's own
    /// two axes, not in world X/Y, so two radial strips of one cylinder at
    /// different depths are not mistaken for a stack.
    fn assert_no_coplanar_faces(model: &PropModel) {
        let mut faces: Vec<(Vec3, f32, [Vec3; 3], [u16; 3])> = Vec::new();
        let (triangles, _) = model.indices.as_chunks::<3>();
        for triangle in triangles {
            let ids = [triangle[0], triangle[1], triangle[2]];
            let corner =
                |index: usize| Vec3::from(model.vertices[usize::from(triangle[index])].pos);
            let (a, b, c) = (corner(0), corner(1), corner(2));
            let normal_raw = sub(b, a).cross(sub(c, a));
            if normal_raw.length_squared() <= 1e-12 {
                continue;
            }
            let normal = normal_raw.normalize();
            faces.push((normal, normal.dot(a), [a, b, c], ids));
        }
        for (index, (normal, offset, points, ids)) in faces.iter().enumerate() {
            for (other_normal, other_offset, other_points, other_ids) in
                faces.iter().skip(index.saturating_add(1))
            {
                if ids.iter().any(|id| other_ids.contains(id)) {
                    continue;
                }
                if normal.dot(*other_normal) < 0.999 {
                    continue;
                }
                if (offset - other_offset).abs() > 1e-4 {
                    continue;
                }
                // A shared in-plane frame, so both faces project the same way.
                let seed = if normal.x.abs() < 0.9 {
                    Vec3::X.cross(*normal)
                } else {
                    Vec3::Y.cross(*normal)
                };
                let t1 = seed.normalize_or_zero();
                if t1.length_squared() <= 0.5 {
                    continue;
                }
                let t2 = normal.cross(t1);
                let rect = |triangle: &[Vec3; 3]| {
                    let mut lo = Vec3::splat(f32::INFINITY);
                    let mut hi = Vec3::splat(f32::NEG_INFINITY);
                    for point in triangle {
                        let u = t1.dot(*point);
                        let v = t2.dot(*point);
                        lo = lo.min(Vec3::new(u, v, 0.0));
                        hi = hi.max(Vec3::new(u, v, 0.0));
                    }
                    [lo.x, hi.x, lo.y, hi.y]
                };
                let (a, b) = (rect(points), rect(other_points));
                let overlap_u = a[1].min(b[1]) - a[0].max(b[0]);
                let overlap_v = a[3].min(b[3]) - a[2].max(b[2]);
                assert!(
                    overlap_u <= 1e-4 || overlap_v <= 1e-4,
                    "coplanar same-facing faces at {offset:.4} normal {normal:?} overlap by \
                     {overlap_u:.4} x {overlap_v:.4}"
                );
            }
        }
    }

    #[test]
    fn the_leaf_bakes_face_shades_into_its_vertices() {
        let materials = materials();
        let models = build_door_models(
            &door(DoorKind::Interior, 0.9, 2.1),
            &materials,
            DoorFrame {
                depth: 0.3,
                center: 0.0,
            },
        )
        .expect("door models build");
        let levels: Vec<f32> = models
            .leaf
            .vertices
            .iter()
            .map(|vertex| vertex.color[0])
            .collect();
        let darkest = levels.iter().copied().fold(f32::INFINITY, f32::min);
        let brightest = levels.iter().copied().fold(0.0_f32, f32::max);
        assert!(brightest <= 1.0 && brightest > 0.9, "{brightest}");
        assert!(
            darkest < 0.95 && darkest > 0.5,
            "recesses and edges bake a visibly darker shade: {darkest}"
        );
        // The leaf's two faces stay symmetric: a door must read the same from
        // either side, whatever the room's key light does.
        let (min, max) = door_model_bounds(&models.leaf).expect("bounds");
        assert!(max.z > 0.0 && min.z < 0.0);
        assert!(
            (min.z + max.z).abs() < 1e-6,
            "the leaf is centred on its own plane: {min:?}..{max:?}"
        );
    }

    #[test]
    fn a_small_leaf_still_builds_a_closed_slab() {
        let materials = materials();
        for (width, height) in [(0.4_f32, 0.5_f32), (0.35, 0.45), (2.5, 2.4)] {
            let built = build_door_models(
                &door(DoorKind::Interior, width, height),
                &materials,
                DoorFrame {
                    depth: 0.3,
                    center: 0.0,
                },
            );
            assert!(built.is_some(), "{width}x{height} door models build");
            let models = built.expect("door models build");
            let (min, max) = door_model_bounds(&models.leaf).expect("bounds");
            assert!(max.y >= height - 1e-3, "{width}x{height}: {max:?}");
            assert!(max.x >= width - 1e-3, "{width}x{height}: {max:?}");
            assert!(min.x.abs() < 1e-3, "{width}x{height}: {min:?}");
        }
    }
}
