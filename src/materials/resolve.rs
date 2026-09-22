//! Resolving a level's material ids into the table the renderer binds.
//!
//! The chain is: material id -> catalog entry -> logical texture -> PNG bytes
//! -> decoded image, with the pack's own definitions taking precedence over the
//! catalog. Failures never panic: they resolve to the diagnostic texture with
//! the offending ids in the error.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use crate::assets::{AssetCatalog, DEFAULT_TILE_METRES};
use crate::level::LevelDef;

use super::image::{RawImage, TextureCache, load_png_relative, missing_texture};
use super::pack::PackMaterials;
use super::{DEFAULT_TINT, MISSING_TEXTURE_KEY};

/// Where a resolved texture came from; also its GPU lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureOrigin {
    /// A catalog texture asset below the asset root. Decoded into the session
    /// cache and uploaded once per session.
    Catalog,
    /// A texture carried inside one loaded level pack. Decoded per level load
    /// and owned by that level.
    Pack,
    /// Nothing resolved; the diagnostic missing-texture pattern is showing.
    Missing,
}

/// One material as a level uses it: its texture, tiling and tint.
#[derive(Clone, Debug)]
pub struct ResolvedMaterial {
    /// The material id exactly as the level wrote it.
    pub id: String,
    /// Session-unique texture key (logical texture id, or `pack:<ns>:<path>`).
    pub texture_key: String,
    /// Index into [`MaterialTable::textures`].
    pub texture_index: u16,
    pub origin: TextureOrigin,
    /// World metres covered by one texture repeat.
    pub tile_metres: f32,
    /// Static multiply tint applied to the sampled texture.
    pub tint: [f32; 3],
    /// The decoded image; `None` only for catalog-only logical tables used by
    /// geometry tests that do not render.
    pub image: Option<Rc<RawImage>>,
    /// The resolution problem that forced the diagnostic fallback, if any.
    pub error: Option<String>,
}

/// One distinct texture a [`MaterialTable`] needs; the unit of GPU upload.
#[derive(Clone, Debug)]
pub struct ResolvedTexture {
    pub key: String,
    pub origin: TextureOrigin,
    pub image: Rc<RawImage>,
}

/// Every material a level references, resolved to images and render parameters.
///
/// Entries are ordered by first reference (defaults, then rooms, walls, faces,
/// patches, regions), which is deterministic and gives the renderer a stable
/// material index per id. Two materials that share a texture share one entry in
/// [`MaterialTable::textures`], so the renderer uploads one GPU texture.
#[derive(Clone, Debug, Default)]
pub struct MaterialTable {
    entries: Vec<ResolvedMaterial>,
    by_id: HashMap<String, u16>,
    textures: Vec<ResolvedTexture>,
}

impl MaterialTable {
    /// Builds the logical table for a level: ids, tiling and tint resolved
    /// through the catalog and an optional pack, with no image decoding.
    ///
    /// This is what geometry-only tests and the lighting audit use; the
    /// renderer always uses [`resolve_materials`] so every entry has an image.
    #[must_use]
    pub fn logical(level: &LevelDef, catalog: &AssetCatalog, pack: Option<&PackMaterials>) -> Self {
        let mut entries: Vec<ResolvedMaterial> = Vec::new();
        for id in referenced_material_ids(level) {
            entries.push(describe_material(&id, catalog, pack));
        }
        let mut by_id = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            by_id.insert(entry.id.clone(), u16::try_from(index).unwrap_or(u16::MAX));
        }
        Self {
            entries,
            by_id,
            textures: Vec::new(),
        }
    }

    /// Number of materials.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the level references no materials at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every material, in first-reference order.
    #[must_use]
    pub fn entries(&self) -> &[ResolvedMaterial] {
        &self.entries
    }

    /// Every distinct texture, in first-reference order.
    #[must_use]
    pub fn textures(&self) -> &[ResolvedTexture] {
        &self.textures
    }

    /// The material index for a level-authored id.
    #[must_use]
    pub fn index_of(&self, material_id: &str) -> Option<u16> {
        self.by_id.get(material_id).copied()
    }

    /// The entry for a level-authored id.
    #[must_use]
    pub fn entry_of(&self, material_id: &str) -> Option<&ResolvedMaterial> {
        self.index_of(material_id)
            .and_then(|index| self.entries.get(index as usize))
    }

    /// The entry for a material index.
    #[must_use]
    pub fn entry(&self, index: u16) -> Option<&ResolvedMaterial> {
        self.entries.get(index as usize)
    }

    /// The texture index a material index uploads/binds.
    #[must_use]
    pub fn texture_index(&self, material_index: u16) -> Option<u16> {
        self.entries
            .get(material_index as usize)
            .map(|entry| entry.texture_index)
    }

    /// Every resolution error, with the material that caused it.
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| entry.error.clone())
            .collect()
    }

    /// The diagnostic fallback entry (the first missing one), if any.
    #[must_use]
    pub fn first_missing(&self) -> Option<&ResolvedMaterial> {
        self.entries
            .iter()
            .find(|entry| entry.origin == TextureOrigin::Missing)
    }
}

/// Adds a texture to a table's texture list, reusing an existing entry with the
/// same key, and returns its index.
fn intern_texture(
    textures: &mut Vec<ResolvedTexture>,
    key: String,
    origin: TextureOrigin,
    image: Rc<RawImage>,
) -> u16 {
    if let Some(index) = textures.iter().position(|texture| texture.key == key) {
        return u16::try_from(index).unwrap_or(u16::MAX);
    }
    // The new entry's index is the length before the push, which is also
    // `len - 1` afterwards — computed without an off-by-one subtraction.
    let index = u16::try_from(textures.len()).unwrap_or(u16::MAX);
    textures.push(ResolvedTexture { key, origin, image });
    index
}

/// Every material id a level references, in first-reference order.
///
/// The scan covers defaults, rooms, walls (including per-face overrides),
/// floor patches and floor regions (floor and transition-edge materials). It is
/// deterministic even though `WallDef::faces` is a map, so the material index
/// of an id never depends on hash order.
#[must_use]
pub fn referenced_material_ids(level: &LevelDef) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |id: &str| {
        let id = id.trim();
        if !id.is_empty() && seen.insert(id.to_string()) {
            ids.push(id.to_string());
        }
    };
    push(&level.defaults.wall);
    push(&level.defaults.floor);
    push(&level.defaults.ceiling);
    for room in level.room_iter() {
        if let Some(material) = &room.material {
            push(material);
        }
        if let Some(material) = &room.ceiling_material {
            push(material);
        }
    }
    for wall in &level.walls {
        if let Some(material) = &wall.material {
            push(material);
        }
        let mut faces: Vec<&String> = wall.faces.values().collect();
        faces.sort();
        for material in faces {
            push(material);
        }
    }
    for patch in &level.floor_patches {
        push(&patch.material);
    }
    for region in &level.floor_regions {
        if let Some(material) = &region.material {
            push(material);
        }
        if let Some(material) = &region.edge_material {
            push(material);
        }
    }
    ids
}

/// Builds a material description before its image is resolved.
fn material_base(
    id: &str,
    texture_key: String,
    origin: TextureOrigin,
    tile_metres: f32,
    tint: [f32; 3],
    error: Option<String>,
) -> ResolvedMaterial {
    ResolvedMaterial {
        id: id.to_string(),
        texture_key,
        texture_index: 0,
        origin,
        tile_metres,
        tint,
        image: None,
        error,
    }
}

/// Logical description of one material before its image is resolved.
fn describe_material(
    id: &str,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) -> ResolvedMaterial {
    if let Some(entry) = catalog.material(id) {
        return describe_catalog_material(id, entry, catalog);
    }

    if let Some(entry) = catalog.get(id) {
        return material_base(
            id,
            MISSING_TEXTURE_KEY.to_string(),
            TextureOrigin::Missing,
            DEFAULT_TILE_METRES,
            DEFAULT_TINT,
            Some(format!(
                "`{id}` is a `{}` asset, not a surface material; using the diagnostic texture",
                entry.asset_type.as_str()
            )),
        );
    }

    if id.starts_with("pack:") {
        return describe_pack_material(id, catalog, pack);
    }

    material_base(
        id,
        MISSING_TEXTURE_KEY.to_string(),
        TextureOrigin::Missing,
        DEFAULT_TILE_METRES,
        DEFAULT_TINT,
        Some(format!(
            "unknown material `{id}`; add it to the asset catalog or use the diagnostic material"
        )),
    )
}

/// Describes a material the catalog declares as a surface material, naming the
/// problem when its texture cannot be resolved.
fn describe_catalog_material(
    id: &str,
    entry: &crate::assets::AssetEntry,
    catalog: &AssetCatalog,
) -> ResolvedMaterial {
    let texture_id = entry.texture.clone().unwrap_or_default();
    if texture_id.is_empty() {
        return material_base(
            id,
            MISSING_TEXTURE_KEY.to_string(),
            TextureOrigin::Missing,
            DEFAULT_TILE_METRES,
            DEFAULT_TINT,
            Some(format!(
                "material `{id}` declares no `texture`; using the diagnostic texture"
            )),
        );
    }
    let tile_metres = entry.tile_metres.unwrap_or(DEFAULT_TILE_METRES);
    let tint = entry.tint.unwrap_or(DEFAULT_TINT);
    if catalog.texture_path(&texture_id).is_none() {
        return material_base(
            id,
            MISSING_TEXTURE_KEY.to_string(),
            TextureOrigin::Missing,
            tile_metres,
            tint,
            Some(format!(
                "material `{id}` references texture `{texture_id}`, which has no PNG file in the catalog"
            )),
        );
    }
    material_base(
        id,
        texture_id,
        TextureOrigin::Catalog,
        tile_metres,
        tint,
        None,
    )
}

/// Describes a `pack:` material through the pack's own definitions.
fn describe_pack_material(
    id: &str,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) -> ResolvedMaterial {
    if let Some(definition) = pack.and_then(|pack| pack.definition(id)) {
        // A pack may reuse a catalog texture by logical id, or ship its own
        // PNG. A declared-but-missing pack file is a named error, not a
        // silent fall-through to a same-named file.
        if definition.texture.contains(':') && catalog.texture_path(&definition.texture).is_some() {
            return material_base(
                id,
                definition.texture.clone(),
                TextureOrigin::Catalog,
                definition.tile_metres(),
                definition.tint(),
                None,
            );
        }
        if pack.is_some_and(|pack| pack.lookup(&definition.texture).is_some()) {
            return material_base(
                id,
                definition.texture.clone(),
                TextureOrigin::Pack,
                definition.tile_metres(),
                definition.tint(),
                None,
            );
        }
        return material_base(
            id,
            format!("pack:unresolved:{id}"),
            TextureOrigin::Pack,
            definition.tile_metres(),
            definition.tint(),
            Some(format!(
                "pack material `{id}`: `materials.json` names `{}`, which is not present in the pack",
                definition.texture
            )),
        );
    }
    if let Some(path) = pack.and_then(|pack| pack.texture_for(id)) {
        return material_base(
            id,
            path,
            TextureOrigin::Pack,
            DEFAULT_TILE_METRES,
            DEFAULT_TINT,
            None,
        );
    }
    material_base(
        id,
        format!("pack:unresolved:{id}"),
        TextureOrigin::Pack,
        DEFAULT_TILE_METRES,
        DEFAULT_TINT,
        Some(format!(
            "pack material `{id}` has no `materials.json` entry and no matching PNG in the pack"
        )),
    )
}

/// Resolves every material a level references into decoded images.
///
/// Built-in materials resolve through the catalog and are decoded once into
/// `cache`; pack materials decode from the pack's own bytes with the pack's
/// namespace folded into the cache key. Unresolvable materials keep a
/// context-rich error on their entry and share one diagnostic texture.
#[must_use]
pub fn resolve_materials(
    level: &LevelDef,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
    asset_root: Option<&Path>,
    cache: &mut TextureCache,
) -> MaterialTable {
    let mut table = MaterialTable::logical(level, catalog, pack);
    let missing = Rc::new(missing_texture());
    // Split the borrow so an entry can be updated while the shared texture list
    // is interned into.
    let MaterialTable {
        entries, textures, ..
    } = &mut table;

    for entry in entries.iter_mut() {
        let origin = entry.origin;
        let key = entry.texture_key.clone();
        let image_result: Result<Rc<RawImage>, String> = match origin {
            TextureOrigin::Catalog => cache.get(&key).map_or_else(
                || match (asset_root, catalog.texture_path(&key)) {
                    (Some(root), Some(path)) => match load_png_relative(root, path) {
                        Ok(image) => Ok(cache.insert(key.clone(), image)),
                        Err(error) => {
                            Err(format!("material `{}` texture `{key}`: {error}", entry.id))
                        }
                    },
                    (None, _) => Err(format!(
                        "material `{}` texture `{key}`: the asset root is missing",
                        entry.id
                    )),
                    (_, None) => Err(format!(
                        "material `{}` texture `{key}`: no PNG path in the catalog",
                        entry.id
                    )),
                },
                Ok,
            ),
            TextureOrigin::Pack => {
                let unresolved = key.starts_with("pack:unresolved:");
                match pack {
                    Some(pack) if !unresolved => pack
                        .decode_cached(cache, &key)
                        .map(|(image, _key)| image)
                        .map_err(|error| format!("material `{}`: {error}", entry.id)),
                    _ => Err(entry.error.clone().unwrap_or_else(|| {
                        format!("material `{}` has no resolvable pack texture", entry.id)
                    })),
                }
            }
            TextureOrigin::Missing => return_error(entry.error.as_deref(), &entry.id),
        };

        match image_result {
            Ok(image) => {
                let texture_index = intern_texture(textures, key, origin, image.clone());
                entry.image = Some(image);
                entry.texture_index = texture_index;
            }
            Err(error) => {
                let texture_index = intern_texture(
                    textures,
                    MISSING_TEXTURE_KEY.to_string(),
                    TextureOrigin::Missing,
                    Rc::clone(&missing),
                );
                entry.texture_key = MISSING_TEXTURE_KEY.to_string();
                entry.origin = TextureOrigin::Missing;
                entry.image = Some(Rc::clone(&missing));
                entry.texture_index = texture_index;
                entry.error = Some(error);
            }
        }
    }

    table
}

/// Builds the error of an entry that was already known to be missing.
fn return_error(error: Option<&str>, id: &str) -> Result<Rc<RawImage>, String> {
    Err(error.map_or_else(
        || format!("material `{id}` could not be resolved"),
        str::to_string,
    ))
}
