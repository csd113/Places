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
use super::{
    DEFAULT_EMISSION_INTENSITY, DEFAULT_REFLECTION_STRENGTH, DEFAULT_TINT, MISSING_TEXTURE_KEY,
    MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, ReflectionMode,
};

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
    /// Lightweight surface response: optional normal map, sheen and roughness.
    ///
    /// [`MaterialResponse::normal`] is an index into [`MaterialTable::textures`]
    /// and is only filled in by [`resolve_materials`], which interns the normal
    /// map. A logical-only table always leaves it `None`.
    pub response: MaterialResponse,
    /// Alpha mode, opacity multiplier and cut-out threshold.
    pub alpha: MaterialAlpha,
    /// Emission of the surface: colour, intensity and optional mask texture.
    ///
    /// [`MaterialEmission::mask`] is an index into [`MaterialTable::textures`]
    /// and is only filled in by [`resolve_materials`], which interns the mask.
    /// A logical-only table always leaves it `None`.
    pub emission: MaterialEmission,
    /// The decoded image; `None` only for catalog-only logical tables used by
    /// geometry tests that do not render.
    pub image: Option<Rc<RawImage>>,
    /// Where the surface's reflection image comes from, and how strong it is.
    ///
    /// [`MaterialReflection::NONE`] for every material that does not author a
    /// mode.
    pub reflection: MaterialReflection,
    /// The resolution problem that forced the diagnostic fallback, if any.
    pub error: Option<String>,
}

/// One distinct texture a [`MaterialTable`] needs; the unit of GPU upload.
#[derive(Clone, Debug)]
pub struct ResolvedTexture {
    pub key: String,
    pub origin: TextureOrigin,
    /// Which runtime quality budget applies to this image.
    ///
    /// An albedo sheet is sized for a tiling surface; an emissive mask is a
    /// separate, usually smaller image, and Low may scale it harder.
    pub class: crate::quality::TextureClass,
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
    /// Builds the logical table for a level: ids, tiling, tint and emission
    /// colour/intensity resolved through the catalog and an optional pack, with
    /// no image decoding.
    ///
    /// Emission's `mask` is always `None` here: a mask is a texture-table
    /// index, and no table exists until images are decoded. Only
    /// [`resolve_materials`] fills it in.
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
    class: crate::quality::TextureClass,
    image: Rc<RawImage>,
) -> u16 {
    if let Some(index) = textures.iter().position(|texture| texture.key == key) {
        return u16::try_from(index).unwrap_or(u16::MAX);
    }
    // The new entry's index is the length before the push, which is also
    // `len - 1` afterwards — computed without an off-by-one subtraction.
    let index = u16::try_from(textures.len()).unwrap_or(u16::MAX);
    textures.push(ResolvedTexture {
        key,
        origin,
        class,
        image,
    });
    index
}

/// Every material id a level references, in first-reference order.
///
/// The scan covers defaults, rooms, walls (including per-face overrides),
/// floor patches and regions, and every generic architectural piece (ramps,
/// staircases, half walls, columns, archways, guardrails, thresholds and
/// baseboards). It is deterministic even though `WallDef::faces` is a map, so
/// the material index of an id never depends on hash order.
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
        for opening in &wall.openings {
            if let Some(glass) = opening.glass_material() {
                push(glass);
            }
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
    push_architecture_materials(level, &mut push);
    ids
}

/// Pushes every material id the level's generic architectural pieces name.
///
/// Split out of [`referenced_material_ids`] so the scan stays one readable
/// list per asset class; the order is the authored order of the arrays.
fn push_architecture_materials(level: &LevelDef, push: &mut impl FnMut(&str)) {
    let mut push_all = |materials: [Option<&String>; 3]| {
        for material in materials.into_iter().flatten() {
            push(material);
        }
    };
    for ramp in &level.ramps {
        push_all([ramp.material.as_ref(), ramp.edge_material.as_ref(), None]);
    }
    for stair in &level.stairs {
        push_all([
            stair.material.as_ref(),
            stair.riser_material.as_ref(),
            stair.side_material.as_ref(),
        ]);
    }
    for piece in &level.half_walls {
        push_all([
            piece.material.as_ref(),
            piece.end_material.as_ref(),
            piece.cap_material.as_ref(),
        ]);
    }
    for piece in &level.columns {
        push_all([piece.material.as_ref(), piece.cap_material.as_ref(), None]);
    }
    for piece in &level.archways {
        push_all([
            piece.material.as_ref(),
            piece.reveal_material.as_ref(),
            None,
        ]);
    }
    for rail in &level.guardrails {
        push_all([rail.material.as_ref(), rail.post_material.as_ref(), None]);
    }
    for strip in &level.thresholds {
        push_all([strip.material.as_ref(), None, None]);
    }
    for board in &level.baseboards {
        push_all([board.material.as_ref(), None, None]);
    }
}

/// Builds a material description before its image is resolved.
fn material_base(
    id: &str,
    texture_key: String,
    origin: TextureOrigin,
    tile_metres: f32,
    tint: [f32; 3],
    emission: MaterialEmission,
    error: Option<String>,
) -> ResolvedMaterial {
    ResolvedMaterial {
        id: id.to_string(),
        texture_key,
        texture_index: 0,
        origin,
        tile_metres,
        tint,
        response: MaterialResponse::NONE,
        alpha: MaterialAlpha::OPAQUE,
        reflection: MaterialReflection::NONE,
        emission,
        image: None,
        error,
    }
}

impl ResolvedMaterial {
    /// Attaches a resolved surface response and alpha contract.
    #[must_use]
    const fn with_surface(mut self, response: MaterialResponse, alpha: MaterialAlpha) -> Self {
        self.response = response;
        self.alpha = alpha;
        self
    }

    /// Attaches a resolved reflection contract.
    #[must_use]
    const fn with_reflection(mut self, reflection: MaterialReflection) -> Self {
        self.reflection = reflection;
        self
    }
}

/// The surface response a catalog material describes.
///
/// The normal map is left `None` here: it is a texture-table index, and no
/// table exists until [`resolve_materials`] interns the image. The sheen, shine
/// and the internal roughness are already final, because they are plain
/// numbers: an authored `shine` becomes `roughness = 1 - shine`, and a catalog
/// that only authors the legacy `roughness` keeps it verbatim.
fn catalog_response(entry: &crate::assets::AssetEntry) -> MaterialResponse {
    let sheen = entry.specular.unwrap_or(0.0).clamp(0.0, 1.0);
    let specular = entry.specular_color.map_or([sheen; 3], |color| {
        color.map(|channel| (channel * sheen).clamp(0.0, 1.0))
    });
    let roughness = entry.shine.map_or_else(
        || entry.roughness.unwrap_or(super::DEFAULT_ROUGHNESS),
        super::roughness_from_shine,
    );
    MaterialResponse {
        normal: None,
        normal_strength: entry
            .normal_strength
            .unwrap_or(super::DEFAULT_NORMAL_STRENGTH),
        specular,
        roughness,
    }
    .sanitized()
}

/// The alpha contract a catalog material describes.
fn catalog_alpha(entry: &crate::assets::AssetEntry) -> MaterialAlpha {
    let mode = entry
        .alpha_mode
        .as_deref()
        .and_then(super::AlphaMode::parse)
        .unwrap_or_default();
    MaterialAlpha {
        mode,
        opacity: entry.opacity.unwrap_or(1.0),
        cutoff: entry.alpha_cutoff.unwrap_or(super::DEFAULT_ALPHA_CUTOFF),
    }
    .sanitized()
}

/// The reflection a catalog material describes.
///
/// The mode is validated when the catalog is parsed, so an unknown name has
/// already been reported; here it only has to become a
/// [`MaterialReflection`]. A material that names no mode reflects nothing.
fn catalog_reflection(entry: &crate::assets::AssetEntry) -> MaterialReflection {
    let Some(mode) = entry
        .reflection_mode
        .as_deref()
        .and_then(ReflectionMode::parse)
    else {
        return MaterialReflection::NONE;
    };
    MaterialReflection::new(
        mode,
        entry
            .reflection_strength
            .unwrap_or(DEFAULT_REFLECTION_STRENGTH),
    )
    .sanitized()
}

/// The emission a catalog material describes.
///
/// An authored colour without an intensity keeps
/// [`DEFAULT_EMISSION_INTENSITY`]; the mask index is filled in later by
/// [`resolve_materials`], once the mask texture is interned.
fn catalog_emission(entry: &crate::assets::AssetEntry) -> MaterialEmission {
    let Some(color) = entry.emissive else {
        return MaterialEmission::NONE;
    };
    MaterialEmission::new(
        color,
        entry
            .emissive_intensity
            .unwrap_or(DEFAULT_EMISSION_INTENSITY),
    )
    .sanitized()
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
            MaterialEmission::NONE,
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
        MaterialEmission::NONE,
        Some(format!(
            "unknown material `{id}`; add it to the asset catalog or use the diagnostic material"
        )),
    )
}

/// Describes a material the catalog declares as a surface material, naming the
/// problem when its texture cannot be resolved.
///
/// Emission colour and intensity come straight from the catalog entry; the
/// mask index is left `None` because it refers to the resolved texture table,
/// which does not exist yet.
fn describe_catalog_material(
    id: &str,
    entry: &crate::assets::AssetEntry,
    catalog: &AssetCatalog,
) -> ResolvedMaterial {
    let emission = catalog_emission(entry);
    let texture_id = entry.texture.clone().unwrap_or_default();
    if texture_id.is_empty() {
        return material_base(
            id,
            MISSING_TEXTURE_KEY.to_string(),
            TextureOrigin::Missing,
            DEFAULT_TILE_METRES,
            DEFAULT_TINT,
            emission,
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
            emission,
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
        emission,
        None,
    )
    .with_surface(catalog_response(entry), catalog_alpha(entry))
    .with_reflection(catalog_reflection(entry))
}

/// Describes a `pack:` material through the pack's own definitions.
fn describe_pack_material(
    id: &str,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) -> ResolvedMaterial {
    let definition = pack.and_then(|pack| pack.definition(id));
    let (response, alpha) = definition.map_or(
        (MaterialResponse::NONE, MaterialAlpha::OPAQUE),
        |definition| (definition.response(), definition.alpha()),
    );
    if let Some(definition) = definition {
        // A pack may reuse a catalog texture by logical id, or ship its own
        // PNG. A declared-but-missing pack file is a named error, not a
        // silent fall-through to a same-named file.
        let emission = definition.emission();
        if definition.texture.contains(':') && catalog.texture_path(&definition.texture).is_some() {
            return material_base(
                id,
                definition.texture.clone(),
                TextureOrigin::Catalog,
                definition.tile_metres(),
                definition.tint(),
                emission,
                None,
            )
            .with_surface(response, alpha)
            .with_reflection(definition.reflection());
        }
        if pack.is_some_and(|pack| pack.lookup(&definition.texture).is_some()) {
            return material_base(
                id,
                definition.texture.clone(),
                TextureOrigin::Pack,
                definition.tile_metres(),
                definition.tint(),
                emission,
                None,
            )
            .with_surface(response, alpha);
        }
        return material_base(
            id,
            format!("pack:unresolved:{id}"),
            TextureOrigin::Pack,
            definition.tile_metres(),
            definition.tint(),
            MaterialEmission::NONE,
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
            MaterialEmission::NONE,
            None,
        );
    }
    material_base(
        id,
        format!("pack:unresolved:{id}"),
        TextureOrigin::Pack,
        DEFAULT_TILE_METRES,
        DEFAULT_TINT,
        MaterialEmission::NONE,
        Some(format!(
            "pack material `{id}` has no `materials.json` entry and no matching PNG in the pack"
        )),
    )
}

/// One additional texture a material authors beside its albedo (an emissive
/// mask or a normal map), before it is decoded.
enum AuthoredTexture {
    /// A catalog texture asset; the dedupe key is its logical id.
    Catalog { texture_id: String, path: String },
    /// A PNG inside the loaded pack; the dedupe key is the pack cache key.
    Pack { path: String },
    /// The texture cannot resolve; the error is already phrased for the entry.
    Unresolved { error: String },
}

/// Where a material's authored `field` texture resolves to, before decoding.
///
/// Catalog materials name a catalog texture id; `pack:` materials may reuse a
/// catalog texture or ship their own PNG, exactly like their albedo `texture`.
/// `None` means the material declares no such texture at all.
fn authored_texture(
    material_id: &str,
    field: &str,
    texture_id: Option<&str>,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) -> Option<AuthoredTexture> {
    if let Some(entry) = catalog.material(material_id) {
        let id = match field {
            "emissive_mask" => entry.emissive_mask.as_deref(),
            _ => entry.normal_texture.as_deref(),
        }?;
        return Some(catalog.texture_path(id).map_or_else(
            || AuthoredTexture::Unresolved {
                error: format!(
                    "material `{material_id}` {field} `{id}` has no PNG file in the catalog"
                ),
            },
            |path| AuthoredTexture::Catalog {
                texture_id: id.to_string(),
                path: path.to_string(),
            },
        ));
    }
    let authored = texture_id?;
    if authored.contains(':')
        && let Some(path) = catalog.texture_path(authored)
    {
        return Some(AuthoredTexture::Catalog {
            texture_id: authored.to_string(),
            path: path.to_string(),
        });
    }
    if pack.is_some_and(|pack| pack.lookup(authored).is_some()) {
        return Some(AuthoredTexture::Pack {
            path: authored.to_string(),
        });
    }
    Some(AuthoredTexture::Unresolved {
        error: format!(
            "pack material `{material_id}`: `materials.json` names {field} `{authored}`, which is not present in the pack"
        ),
    })
}

/// Decodes one authored texture into its dedupe key, origin and image.
fn resolve_authored_image(
    material_id: &str,
    field: &str,
    texture: AuthoredTexture,
    cache: &mut TextureCache,
    pack: Option<&PackMaterials>,
    asset_root: Option<&Path>,
) -> Result<(String, TextureOrigin, Rc<RawImage>), String> {
    match texture {
        AuthoredTexture::Catalog { texture_id, path } => {
            if let Some(image) = cache.get(&texture_id) {
                return Ok((texture_id, TextureOrigin::Catalog, image));
            }
            match asset_root {
                Some(root) => load_png_relative(root, &path)
                    .map_err(|error| {
                        format!("material `{material_id}` {field} `{texture_id}`: {error}")
                    })
                    .map(|image| {
                        let image = cache.insert(texture_id.clone(), image);
                        (texture_id, TextureOrigin::Catalog, image)
                    }),
                None => Err(format!(
                    "material `{material_id}` {field} `{texture_id}`: the asset root is missing"
                )),
            }
        }
        AuthoredTexture::Pack { path } => pack.map_or_else(
            || {
                Err(format!(
                    "material `{material_id}` {field} `{path}`: the pack is not loaded"
                ))
            },
            |pack| {
                pack.decode_cached(cache, &path)
                    .map(|(image, key)| (key, TextureOrigin::Pack, image))
                    .map_err(|error| format!("material `{material_id}` {field}: {error}"))
            },
        ),
        AuthoredTexture::Unresolved { error } => Err(error),
    }
}

/// Degrades one entry to the shared diagnostic texture.
///
/// A material that cannot bind every authored texture loses everything that
/// texture was supposed to carry: it must not glow, must not sheen and must not
/// be see-through, because those are all properties of artwork nobody could
/// load. What is left is the diagnostic pattern with the material's plain lit
/// look, plus a context-rich error on the entry.
fn fall_back_to_missing(
    entry: &mut ResolvedMaterial,
    textures: &mut Vec<ResolvedTexture>,
    missing: &Rc<RawImage>,
    error: String,
) {
    let texture_index = intern_texture(
        textures,
        MISSING_TEXTURE_KEY.to_string(),
        TextureOrigin::Missing,
        crate::quality::TextureClass::Surface,
        Rc::clone(missing),
    );
    entry.texture_key = MISSING_TEXTURE_KEY.to_string();
    entry.origin = TextureOrigin::Missing;
    entry.image = Some(Rc::clone(missing));
    entry.texture_index = texture_index;
    entry.error = Some(error);
    entry.emission = MaterialEmission::NONE;
    entry.response = MaterialResponse::NONE;
    entry.alpha = MaterialAlpha::OPAQUE;
    // A material whose artwork is missing must not claim a reflection source:
    // the diagnostic checkerboard is not a wet floor.
    entry.reflection = MaterialReflection::NONE;
}

/// Resolves every material a level references into decoded images.
///
/// Built-in materials resolve through the catalog and are decoded once into
/// `cache`; pack materials decode from the pack's own bytes with the pack's
/// namespace folded into the cache key. An authored emissive mask is decoded
/// through the same cache and interned into the texture table, so two
/// materials that share a mask upload one GPU texture. Unresolvable materials
/// keep a context-rich error on their entry and share one diagnostic texture.
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
    let context = ResolveContext {
        catalog,
        pack,
        asset_root,
    };

    for entry in entries.iter_mut() {
        resolve_entry(entry, textures, &missing, &context, cache);
    }

    table
}

/// The immutable inputs every per-material resolution step shares.
#[derive(Clone, Copy)]
struct ResolveContext<'a> {
    catalog: &'a AssetCatalog,
    pack: Option<&'a PackMaterials>,
    asset_root: Option<&'a Path>,
}

/// Resolves one material entry in place, degrading it to the diagnostic
/// texture (and clearing its emission, response and alpha) when any authored
/// texture is missing.
fn resolve_entry(
    entry: &mut ResolvedMaterial,
    textures: &mut Vec<ResolvedTexture>,
    missing: &Rc<RawImage>,
    context: &ResolveContext<'_>,
    cache: &mut TextureCache,
) {
    let ResolveContext {
        catalog,
        pack,
        asset_root,
    } = *context;
    let origin = entry.origin;
    let key = entry.texture_key.clone();

    let image = match decode_albedo(entry, origin, &key, context, cache) {
        Ok(image) => image,
        Err(error) => {
            fall_back_to_missing(entry, textures, missing, error);
            return;
        }
    };

    // The mask is part of the material: a mask that cannot resolve
    // degrades the whole material exactly like a broken albedo, rather
    // than emitting through a texture nobody authored.
    let pack_definition = pack.and_then(|pack| pack.definition(&entry.id));
    let mask_image = authored_texture(
        &entry.id,
        "emissive_mask",
        pack_definition.and_then(|definition| definition.emissive_mask.as_deref()),
        catalog,
        pack,
    )
    .map(|mask| resolve_authored_image(&entry.id, "emissive mask", mask, cache, pack, asset_root));
    if let Some(Err(error)) = mask_image {
        fall_back_to_missing(entry, textures, missing, error);
        return;
    }

    // The normal map follows the same rule: a normal map that cannot
    // resolve degrades the material rather than silently flat-shading it.
    let normal_image = authored_texture(
        &entry.id,
        "normal_texture",
        pack_definition.and_then(|definition| definition.normal_texture.as_deref()),
        catalog,
        pack,
    )
    .map(|normal| resolve_authored_image(&entry.id, "normal map", normal, cache, pack, asset_root));
    if let Some(Err(error)) = normal_image {
        fall_back_to_missing(entry, textures, missing, error);
        return;
    }

    let texture_index = intern_texture(
        textures,
        key,
        origin,
        crate::quality::TextureClass::Surface,
        image.clone(),
    );
    entry.image = Some(image);
    entry.texture_index = texture_index;
    if let Some(Ok((mask_key, mask_origin, mask_image))) = mask_image {
        let mask_index = intern_texture(
            textures,
            mask_key,
            mask_origin,
            crate::quality::TextureClass::EmissionMask,
            mask_image,
        );
        entry.emission.mask = Some(mask_index);
    }
    if let Some(Ok((normal_key, normal_origin, normal_image))) = normal_image {
        let normal_index = intern_texture(
            textures,
            normal_key,
            normal_origin,
            crate::quality::TextureClass::Surface,
            normal_image,
        );
        entry.response.normal = Some(normal_index);
    }
}

/// Decodes one material's albedo image through the catalog or the pack.
///
/// Returns the shared diagnostic failure message when the entry is already
/// known to be unresolvable, so the caller degrades it exactly like a decode
/// error.
fn decode_albedo(
    entry: &ResolvedMaterial,
    origin: TextureOrigin,
    key: &str,
    context: &ResolveContext<'_>,
    cache: &mut TextureCache,
) -> Result<Rc<RawImage>, String> {
    let ResolveContext {
        catalog,
        pack,
        asset_root,
    } = *context;
    match origin {
        TextureOrigin::Catalog => cache.get(key).map_or_else(
            || match (asset_root, catalog.texture_path(key)) {
                (Some(root), Some(path)) => match load_png_relative(root, path) {
                    Ok(image) => Ok(cache.insert(key.to_string(), image)),
                    Err(error) => Err(format!("material `{}` texture `{key}`: {error}", entry.id)),
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
                    .decode_cached(cache, key)
                    .map(|(image, _key)| image)
                    .map_err(|error| format!("material `{}`: {error}", entry.id)),
                _ => Err(entry.error.clone().unwrap_or_else(|| {
                    format!("material `{}` has no resolvable pack texture", entry.id)
                })),
            }
        }
        TextureOrigin::Missing => return_error(entry.error.as_deref(), &entry.id),
    }
}

/// Builds the error of an entry that was already known to be missing.
fn return_error(error: Option<&str>, id: &str) -> Result<Rc<RawImage>, String> {
    Err(error.map_or_else(
        || format!("material `{id}` could not be resolved"),
        str::to_string,
    ))
}
