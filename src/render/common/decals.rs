//! Decals: the fixed atlas, external PNG sheets and the emitted quads.
//!
//! Decals are small local surface markings. They all share one committed RGBA
//! sheet so the whole level draws them with a single texture bind, and they are
//! authored without a plate: the background is alpha 0 and the decal pass
//! discards it, which is what lets a cut-out silhouette (a sign, a floor arrow)
//! sit on a surface instead of a floating rectangle. A decal whose asset is an
//! external PNG binds its own sheet instead.

use super::LevelDef;

// ------------------------------------------------------------- decal sheets
//
// Decals are small local surface markings. They all share one committed RGBA
// sheet so the whole level draws them with a single texture bind, and they are
// authored without a plate: the background is alpha 0 and the decal pass
// discards it, which is what lets a future NO DIVING sign or floor arrow have
// a cut-out silhouette instead of a floating rectangle.

/// Physical distance a decal is displaced along its surface normal, in metres.
///
/// This is the *geometry* half of the decal depth solution: it turns the
/// exactly-coplanar tie between a decal and the surface it is printed on into a
/// real, rasteriser-independent depth ordering, so the base texture can never
/// win a pixel. 0.2 mm is chosen to be invisible from every practical distance
/// (sub-pixel parallax even at the near plane) while still exceeding the
/// depth-buffer resolution over the whole interior range: at 10 m a 24-bit
/// buffer resolves about 60 µm, so this is several steps of separation there.
///
/// The offset is always along the *surface* normal reported by
/// [`crate::level::DecalSurface::normal`], so it lifts floor and ceiling decals
/// vertically and wall decals out of the wall, never into their surface. It is
/// applied in one place, `render::common::add_decal_quad`, so every decal a
/// level authors — current or future — inherits it without any level-side
/// epsilon.
pub const DECAL_SURFACE_OFFSET_M: f32 = 2.0e-4;

/// Depth bias the decal pass applies, as `(factor, units)`.
///
/// This is the *depth-buffer* half of the decal depth solution; the geometry
/// half is [`DECAL_SURFACE_OFFSET_M`]. The bias is negative on both terms so it
/// pulls a decal towards the camera:
///
/// * `units = -4` moves a decal four depth-buffer resolution steps towards the
///   viewer. A coplanar decal needs only a couple of steps in the ideal case,
///   but the depth a rasteriser interpolates for two different tessellations of
///   the *same* plane routinely disagrees by more than that: the plane
///   coefficients are fitted from different triangles, so the error grows with
///   the depth slope and the triangle size. Four units keeps the marking in
///   front of its parent surface in the near and mid field.
/// * `factor = -1.0` adds one depth-slope of bias, which is what keeps a decal
///   winning at grazing angles and at long range, where the constant term is
///   below the buffer's resolution. A slope-scaled term is exactly what a
///   coplanar decoration needs: the interpolation disagreement is proportional
///   to the depth slope too, so the bias tracks it instead of being outrun by
///   it as the camera changes distance and angle.
///
/// Both are window-depth offsets, not a physical separation, so they cannot
/// make a decal hang in the air. [`DECAL_SURFACE_OFFSET_M`] checks the rendered
/// result's near-field ordering; this bias carries the far field and grazing
/// angles, where no sub-millimetre physical offset is resolvable. The historical
/// OpenGL pass fed the pair to `glPolygonOffset`; the wgpu decal pass maps it to
/// its fixed-point depth-bias state.
pub const DECAL_POLYGON_OFFSET: (f32, f32) = (-1.0, -4.0);

/// Alpha below which the decal pass discards a decal texel.
pub const DECAL_ALPHA_CUTOFF: f32 = 0.5;

/// Validation decal sheet id for the internal validation marking.
///
/// This fixed PNG sheet exercises the atlas machinery with a filled frame,
/// font text and unused spare cells. The floor arrow, hazard stripes and Pool
/// safety sign use separate PNG sheets under `assets/`.
pub const DECAL_TEST_MATERIAL: &str = "core:decal_test_01";

/// Edge length of the validation decal sheet.
pub const DECAL_ATLAS_SIZE: i32 = 256;
/// One decal pattern's cell size inside the sheet.
pub const DECAL_SLOT_SIZE: i32 = 128;
/// Transparent gutter between cells, so mip-mapping never bleeds one pattern
/// into its neighbour.
const DECAL_SLOT_GUTTER: i32 = 8;
/// Every fixed decal sheet id the renderer can draw, in slot order.
///
/// The floor arrow, the hazard stripes and the Pool safety sign are not among
/// these: they are external PNG artwork (`source: "file"` catalog decals)
/// drawn from their own sheets. Three of the atlas cells are therefore unused
/// and stay transparent.
pub const DECAL_MATERIALS: [&str; 1] = [DECAL_TEST_MATERIAL];

/// Resolves a decal material id to its slot in the fixed sheet.
///
/// Unknown ids are not an error: a level may reference a decal sheet a future
/// build knows about, and simply drawing nothing is the graceful degradation
/// the loader wants for unsupported content.
#[must_use]
pub fn decal_material_slot(material: &str) -> Option<u32> {
    DECAL_MATERIALS
        .iter()
        .position(|id| *id == material)
        .and_then(|slot| u32::try_from(slot).ok())
}

/// Sheet index of the first external (PNG-backed) decal sheet.
///
/// `DECAL_MATERIALS` is a fixed one-element table, so its length is 1 and the
/// `u32` conversion is exact; `as` is used because `TryFrom` is not const.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "The fixed array type contains one material; its length fits u32 and TryFrom is unavailable in a const initializer."
)]
pub const DECAL_EXTERNAL_BASE: u32 = DECAL_MATERIALS.len() as u32;

/// True when the catalog declares `material` as a file-backed decal sheet.
///
/// Only these resolve to external PNG artwork; the fixed patterns and
/// unknown ids are handled by [`decal_material_slot`].
fn catalog_decal_sheet<'a>(
    catalog: &'a crate::assets::AssetCatalog,
    material: &str,
) -> Option<&'a str> {
    let entry = catalog.get(material)?;
    if entry.asset_type.as_str() != crate::assets::AssetType::DECAL {
        return None;
    }
    if !matches!(entry.source, crate::assets::AssetSource::File) {
        return None;
    }
    entry
        .model
        .as_deref()
        .filter(|model| model.to_ascii_lowercase().ends_with(".png"))
}

/// External decal sheets a level places, in first-use order.
///
/// A decal asset that is not one of the fixed patterns and is declared in
/// the catalog as a file-backed PNG resolves as external artwork, exactly like
/// a surface texture. Both the mesh builder and the GPU uploader derive the
/// mapping from the level and the catalog alone, so a decal's sheet index never
/// needs extra renderer state: `0..DECAL_EXTERNAL_BASE` are the fixed atlas slots, then
/// one index per external sheet in the order the level first places it. The
/// mapping is stable and independent of whether a sheet's PNG could actually be
/// decoded; the renderer draws the diagnostic sheet for a broken file.
///
/// An id that is neither a fixed atlas slot nor a catalogued file sheet is skipped, the
/// same graceful degradation unknown materials use.
#[must_use]
pub fn decal_external_sheet_ids(
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for decal in &level.decals {
        if decal_material_slot(&decal.material).is_some() {
            continue;
        }
        if catalog_decal_sheet(catalog, &decal.material).is_none() {
            continue;
        }
        if !ids.contains(&decal.material) {
            ids.push(decal.material.clone());
        }
    }
    ids
}

/// Sheet index a decal material draws from, or `None` when a level references
/// an unknown decal (no geometry is emitted for it, as before).
#[must_use]
pub fn decal_sheet_index(
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
    material: &str,
) -> Option<u32> {
    if let Some(slot) = decal_material_slot(material) {
        return Some(slot);
    }
    decal_external_sheet_ids(level, catalog)
        .iter()
        .position(|id| id == material)
        .and_then(|position| u32::try_from(position).ok())
        .and_then(|position| DECAL_EXTERNAL_BASE.checked_add(position))
}

/// UV rectangle of a whole external decal sheet.
///
/// An external sheet is uploaded as one image, so its decal quad samples the
/// full texture. The decal quad's corners arrive as
/// `[bottom-left, bottom-right, top-right, top-left]` of the decal's own
/// in-plane frame, and the uploaded image's row order runs opposite to that
/// frame's V axis, so both in-plane axes are swapped here. The same rect serves
/// floors, ceilings and walls: each family's frame is built from its own
/// out-of-plane axis, but the correction is the same. A marking then reads
/// upright and unmirrored in the world exactly as it does in an image viewer,
/// with the authored `rotation_degrees` applied as a real in-plane rotation.
///
/// Verified by scoring ink masks of the Pool showcase's external sign on the
/// deck and on a wall against the PNG under all four square symmetries (both
/// matched `identity`), and pinned by
/// `external_decal_sheets_pin_their_world_orientation`.
#[must_use]
pub const fn decal_uv_rect_full() -> [[f32; 2]; 4] {
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
}

/// Per-external-sheet blend flags, in [`decal_external_sheet_ids`] order.
///
/// A catalog decal entry that authors `alpha_mode: "blend"` draws through the
/// soft-edge pipeline; the fixed atlas and every other external sheet stay
/// on the reference's hard cut-out.
#[must_use]
pub fn decal_blend_sheets(level: &LevelDef, catalog: &crate::assets::AssetCatalog) -> Vec<bool> {
    decal_external_sheet_ids(level, catalog)
        .into_iter()
        .map(|id| {
            catalog
                .get(&id)
                .and_then(|entry| entry.alpha_mode.as_deref())
                .is_some_and(|mode| mode.eq_ignore_ascii_case("blend"))
        })
        .collect()
}

/// Whether one decal range's sheet index is a soft-edged (blended) sheet.
///
/// The fixed atlas (indices below [`DECAL_EXTERNAL_BASE`]) is never
/// blended; an
/// external sheet follows the flag order of [`decal_blend_sheets`].
#[must_use]
pub fn decal_sheet_is_blend(
    material: crate::render::common::mesh::MaterialIndex,
    blend_sheets: &[bool],
) -> bool {
    let index = material;
    if index < DECAL_EXTERNAL_BASE {
        return false;
    }
    usize::try_from(index.saturating_sub(DECAL_EXTERNAL_BASE))
        .ok()
        .and_then(|offset| blend_sheets.get(offset))
        .copied()
        .unwrap_or(false)
}

/// Replaces the vertex-lit approximation baked into blended decals with the
/// prepared probe field's display light, in place.
///
/// Every decal vertex colour is `surface tint x vertex-lit sample`, because
/// that is the light model the decal pass renders with in the vertex-lit
/// variant. In a lightmapped build the prepared solve lights the surrounding
/// surfaces, and its unlit floor is deliberately darker than the vertex model's
/// ambient floor; a feather decal left on the vertex model therefore glows over
/// its dark surface. This pass rewrites each *blended* decal's colour as
/// `surface tint x prepared display light` sampled from the same probe lattice
/// moving objects use, so a feather follows the baked scene. Cut-out decals are
/// untouched: existing signage keeps its authored look, and a vertex-lit build
/// (no probe field) is untouched too.
pub fn relight_blend_decals(
    mesh: &mut super::mesh::LevelMesh,
    level: &LevelDef,
    catalog: &crate::assets::AssetCatalog,
    lighting: &crate::lighting::LevelLighting,
    field: &crate::lighting::probes::ProbeField,
) {
    let flags = decal_blend_sheets(level, catalog);
    if !flags.iter().any(|blend| *blend) {
        return;
    }
    for range in &mut mesh.ranges {
        if range.key.kind != super::mesh::SurfaceKind::Decal
            || !decal_sheet_is_blend(range.key.material, &flags)
        {
            continue;
        }
        for vertex in &mut range.vertices {
            let normal = glam::Vec3::from(vertex.normal);
            if normal.length_squared() <= 1.0e-12 {
                continue;
            }
            let unit_normal = normal.normalize();
            let probe = if unit_normal.y.abs() > 0.5 {
                super::DECAL_HORIZONTAL_LIGHT_PROBE_M
            } else {
                super::DECAL_WALL_LIGHT_PROBE_M
            };
            let sample =
                unit_normal.mul_add(glam::Vec3::splat(probe), glam::Vec3::from(vertex.pos));
            let room = lighting.room_index_at_height(sample.x, sample.y, sample.z);
            let Some(display) = field.sample_display(sample.to_array(), room) else {
                continue;
            };
            let tint = super::decal_tint_for_normal(unit_normal.to_array());
            vertex.color = [
                tint[0].mul_add(display[0], 0.0).clamp(0.0, 1.0),
                tint[1].mul_add(display[1], 0.0).clamp(0.0, 1.0),
                tint[2].mul_add(display[2], 0.0).clamp(0.0, 1.0),
                vertex.color[3],
            ];
        }
    }
}

/// Loads the committed validation atlas, retaining its bottom-up row order,
/// existing slot-zero marking and three transparent cells.
///
/// The historical accessor name remains for renderer callers.
pub fn generate_decal_atlas() -> Vec<u8> {
    crate::materials::BuiltinImage::DecalAtlas.decode().rgba
}

/// Texture-coordinate rectangle of one decal slot, as
/// `[bottom-left, bottom-right, top-right, top-left]` matching the decal quad
/// winding (`add_decal_quad`).
#[must_use]
pub fn decal_uv_rect(slot: u32) -> [[f32; 2]; 4] {
    let cell = i32::try_from(slot).unwrap_or(0_i32).clamp(0_i32, 3_i32);
    let (col, row) = (cell % 2_i32, cell / 2_i32);
    let inset = DECAL_SLOT_GUTTER;
    let x0 = atlas_pixels_f32(col.saturating_mul(DECAL_SLOT_SIZE).saturating_add(inset));
    let x1 = atlas_pixels_f32(
        col.saturating_mul(DECAL_SLOT_SIZE)
            .saturating_add(DECAL_SLOT_SIZE)
            .saturating_sub(inset),
    );
    let y0 = atlas_pixels_f32(row.saturating_mul(DECAL_SLOT_SIZE).saturating_add(inset));
    let y1 = atlas_pixels_f32(
        row.saturating_mul(DECAL_SLOT_SIZE)
            .saturating_add(DECAL_SLOT_SIZE)
            .saturating_sub(inset),
    );
    let size = atlas_pixels_f32(DECAL_ATLAS_SIZE);
    // The sheet is stored bottom-up, so the visual top row maps to the higher
    // texture coordinate.
    let u0 = x0 / size;
    let u1 = x1 / size;
    let v_top = (size - y0) / size;
    let v_bottom = (size - y1) / size;
    [[u0, v_bottom], [u1, v_bottom], [u1, v_top], [u0, v_top]]
}

/// Exact `f32` value of a non-negative atlas pixel coordinate.
///
/// The sheet is 256 px, so every coordinate here fits `u16` and converts to
/// `f32` without loss.
fn atlas_pixels_f32(pixels: i32) -> f32 {
    f32::from(u16::try_from(pixels).unwrap_or(0))
}
