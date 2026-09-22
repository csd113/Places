//! Surface materials and their texture images.
//!
//! A level never stores a physical image path. It references a **material id**
//! (`core:carpet_beige_01`), the asset catalog maps that material to a logical
//! **texture id** (`core:tex_carpet_beige_01`), the texture asset names the PNG
//! below the asset root, and this module resolves the whole chain into a
//! [`MaterialTable`] whose entries carry the decoded image, the world tiling
//! period and the static tint the renderer multiplies in.
//!
//! ```
//! material id            core:carpet_beige_01        (levels store this)
//!     ↓                  assets/catalog.json
//! logical texture id     core:tex_carpet_beige_01
//!     ↓                  catalog `model` path, relative to the asset root
//! external PNG           environment/office/textures/floors/carpet_beige_01.png
//!     ↓                  decoded once per session ([`TextureCache`])
//! decoded image          Rc<RawImage>, uploaded to one GPU texture per level
//! ```
//!
//! Ids are stable and physical paths are not: moving a PNG between directories
//! only requires updating the catalog, and replacing its pixels requires no
//! Rust change at all. Duplicate ids and dangling references are catalog
//! errors ([`crate::assets::AssetCatalog`]); a missing or corrupt PNG at
//! runtime resolves to one conspicuous diagnostic texture plus a logged,
//! context-rich error — never a panic.
//!
//! Built-in materials and level-pack materials resolve through the same table:
//! a `pack:` material uses the pack's own PNG bytes, and everything else uses
//! the catalog. Geometry only ever sees table indices.
//!
//! Module layout
//! -------------
//! ```text
//! image.rs    PNG decode/encode, the diagnostic pattern and the session cache
//! pack.rs     material definitions carried inside a level pack
//! resolve.rs  material id -> catalog -> texture -> decoded image
//! decal.rs    external PNG decal sheets
//! tests.rs    unit tests for the whole pipeline
//! ```

mod decal;
mod image;
mod pack;
mod resolve;

#[cfg(test)]
mod tests;

pub use decal::{ResolvedDecalSheet, resolve_decal_sheet};
pub use image::{
    RawImage, TextureCache, decode_png, encode_png, load_png_relative, missing_texture,
};
pub use pack::{PackMaterialDef, PackMaterials, parse_materials_json};
pub use resolve::{
    MaterialTable, ResolvedMaterial, ResolvedTexture, TextureOrigin, referenced_material_ids,
    resolve_materials,
};

/// Cache/dedupe key of the one diagnostic texture every resolution failure
/// shares, so a broken level produces one GPU texture and one obvious pattern.
pub const MISSING_TEXTURE_KEY: &str = "core:tex_missing";

/// Default tint of a material that does not author one: no multiply.
pub const DEFAULT_TINT: [f32; 3] = [1.0, 1.0, 1.0];
