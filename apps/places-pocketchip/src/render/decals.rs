//! Decals: the generated atlas, external PNG sheets and the emitted quads.
//!
//! Decals are small local surface markings. They all share one generated RGBA
//! sheet so the whole level draws them with a single texture bind, and they are
//! authored without a plate: the background is alpha 0 and the decal pass
//! discards it, which is what lets a cut-out silhouette (a sign, a floor arrow)
//! sit on a surface instead of a floating rectangle. A decal whose asset is an
//! external PNG binds its own sheet instead.

use super::LevelDef;

// ------------------------------------------------------------- decal sheets
//
// Decals are small local surface markings. They all share one generated RGBA
// sheet so the whole level draws them with a single texture bind, and they are
// authored without a plate: the background is alpha 0 and the decal pass
// discards it, which is what lets a future NO DIVING sign or floor arrow have
// a cut-out silhouette instead of a floating rectangle.

/// Generated decal sheet id for the internal validation marking.
///
/// This is the only pattern the renderer still draws: it exists to exercise the
/// atlas machinery (a filled frame, embedded-font text, an unused spare cell),
/// not to be edited. The floor arrow, the hazard stripes and the Pool safety
/// sign are all external PNG sheets under `assets/`, so a creator can replace
/// them without a Rust change.
pub const DECAL_TEST_MATERIAL: &str = "core:decal_test_01";

/// Edge length of the generated decal sheet.
pub const DECAL_ATLAS_SIZE: i32 = 256;
/// One decal pattern's cell size inside the sheet.
pub(super) const DECAL_SLOT_SIZE: i32 = 128;
/// Transparent gutter between cells, so mip-mapping never bleeds one pattern
/// into its neighbour.
const DECAL_SLOT_GUTTER: i32 = 8;
/// Every generated decal sheet id the renderer can draw, in slot order.
///
/// The floor arrow, the hazard stripes and the Pool safety sign are no longer
/// among these: they are external PNG artwork (`source: "file"` catalog decals)
/// drawn from their own sheets. Three of the atlas cells are therefore unused
/// and stay transparent.
pub const DECAL_MATERIALS: [&str; 1] = [DECAL_TEST_MATERIAL];

/// Resolves a decal material id to its slot in the generated sheet.
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
#[allow(clippy::cast_possible_truncation)]
pub const DECAL_EXTERNAL_BASE: u32 = DECAL_MATERIALS.len() as u32;

/// True when the catalog declares `material` as a file-backed decal sheet.
///
/// Only these resolve to external PNG artwork; the generated patterns and
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
/// A decal asset that is not one of the generated patterns and is declared in
/// the catalog as a file-backed PNG resolves as external artwork, exactly like
/// a surface texture. Both the mesh builder and the GPU uploader derive the
/// mapping from the level and the catalog alone, so a decal's sheet index never
/// needs extra renderer state: `0..4` are the generated atlas patterns, then
/// one index per external sheet in the order the level first places it. The
/// mapping is stable and independent of whether a sheet's PNG could actually be
/// decoded; the renderer draws the diagnostic sheet for a broken file.
///
/// An id that is neither generated nor a catalogued file sheet is skipped, the
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

/// Writes one texel into the decal sheet, in visual (top-down) coordinates.
///
/// The sheet is stored bottom-up so the generated text reads upright under the
/// game's `v` convention (v = 0 is the bottom of the image as displayed); every
/// other generated sheet is vertically symmetric, so this is the first texture
/// where the distinction is visible.
fn decal_atlas_put(pixels: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
        return;
    };
    let Ok(size) = usize::try_from(DECAL_ATLAS_SIZE) else {
        return;
    };
    if x >= size || y >= size {
        return;
    }
    // The sheet is stored bottom-up, so the visual top row is the last one.
    let Some(row) = size.checked_sub(1).and_then(|top| top.checked_sub(y)) else {
        return;
    };
    let Some(index) = row
        .checked_mul(size)
        .and_then(|offset| offset.checked_add(x))
        .and_then(|offset| offset.checked_mul(4))
    else {
        return;
    };
    let Some(texel) = pixels.get_mut(index..index.saturating_add(4)) else {
        return;
    };
    texel.copy_from_slice(&color);
}

/// Plain rectangle fill in visual sheet coordinates.
fn decal_atlas_rect(pixels: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            decal_atlas_put(pixels, x, y, color);
        }
    }
}

/// Rectangle outline in visual sheet coordinates.
fn decal_atlas_frame(
    pixels: &mut [u8],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    // A frame's bars are `thickness` texels wide, so the inner edge sits
    // `thickness - 1` texels inside the outer one.
    let inset = thickness.saturating_sub(1);
    decal_atlas_rect(pixels, x0, y0, x1, y0.saturating_add(inset), color);
    decal_atlas_rect(pixels, x0, y1.saturating_sub(inset), x1, y1, color);
    decal_atlas_rect(pixels, x0, y0, x0.saturating_add(inset), y1, color);
    decal_atlas_rect(pixels, x1.saturating_sub(inset), y0, x1, y1, color);
}

/// Stamps one line of the embedded 8x8 font into the sheet at `scale`.
///
/// The font table is already the project's own bitmap resource (the HUD uses
/// it), so diagnostic decal text stays project-created data with no new asset
/// pipeline.
fn decal_atlas_text(
    pixels: &mut [u8],
    origin_x: i32,
    origin_y: i32,
    text: &str,
    scale: i32,
    color: [u8; 4],
) {
    let mut cursor_x = origin_x;
    for character in text.bytes() {
        if character < crate::font::FONT_FIRST_CHAR {
            continue;
        }
        let glyph_index = usize::from(character.saturating_sub(crate::font::FONT_FIRST_CHAR));
        if let Some(glyph) = crate::font::FONT_DATA.get(glyph_index) {
            for (row, bits) in glyph.iter().enumerate() {
                let Ok(row) = i32::try_from(row) else {
                    continue;
                };
                for column in 0..8i32 {
                    if bits & (0x80 >> column) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = cursor_x
                                .saturating_add(column.saturating_mul(scale))
                                .saturating_add(dx);
                            let y = origin_y
                                .saturating_add(row.saturating_mul(scale))
                                .saturating_add(dy);
                            decal_atlas_put(pixels, x, y, color);
                        }
                    }
                }
            }
        }
        cursor_x = cursor_x.saturating_add(8_i32.saturating_mul(scale));
    }
}

/// Draws one line of text horizontally centred in a decal cell.
fn decal_atlas_text_centered(
    pixels: &mut [u8],
    slot: i32,
    text: &str,
    y: i32,
    scale: i32,
    color: [u8; 4],
) {
    let (col, row) = (slot % 2, slot / 2);
    let cell_x = col.saturating_mul(DECAL_SLOT_SIZE);
    let cell_y = row.saturating_mul(DECAL_SLOT_SIZE);
    let width = i32::try_from(text.len())
        .unwrap_or(0)
        .saturating_mul(8)
        .saturating_mul(scale);
    decal_atlas_text(
        pixels,
        cell_x.saturating_add(DECAL_SLOT_SIZE.saturating_sub(width) / 2),
        cell_y.saturating_add(y),
        text,
        scale,
        color,
    );
}

/// Generates the shared decal sheet: the internal validation marking, with the
/// three other cells left transparent.
pub fn generate_decal_atlas() -> Vec<u8> {
    let size = usize::try_from(DECAL_ATLAS_SIZE).unwrap_or(0);
    let mut pixels = vec![0u8; size.saturating_mul(size).saturating_mul(4)];
    let white = [245, 245, 240, 255];

    // Slot 0: the validation marking, "DECAL TEST" in a frame on transparency.
    // The other three cells stay empty: the arrow, the stripes and the sign are
    // external PNG sheets now, so nothing else is drawn here.
    decal_atlas_frame(&mut pixels, 14, 14, 113, 113, 4, white);
    decal_atlas_text_centered(&mut pixels, 0, "DECAL", 40, 2, white);
    decal_atlas_text_centered(&mut pixels, 0, "TEST", 72, 2, white);

    pixels
}

/// Texture-coordinate rectangle of one decal slot, as
/// `[bottom-left, bottom-right, top-right, top-left]` matching the decal quad
/// winding (`add_decal_quad`).
#[must_use]
pub fn decal_uv_rect(slot: u32) -> [[f32; 2]; 4] {
    let cell = i32::try_from(slot).unwrap_or(0).clamp(0, 3);
    let (col, row) = (cell % 2, cell / 2);
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
