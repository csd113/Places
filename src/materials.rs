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

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;

use crate::assets::{AssetCatalog, DEFAULT_TILE_METRES, MAX_TEXTURE_DIMENSION};
use crate::level::LevelDef;

/// Cache/dedupe key of the one diagnostic texture every resolution failure
/// shares, so a broken level produces one GPU texture and one obvious pattern.
pub const MISSING_TEXTURE_KEY: &str = "core:tex_missing";

/// Default tint of a material that does not author one: no multiply.
pub const DEFAULT_TINT: [f32; 3] = [1.0, 1.0, 1.0];

/// Edge length of the generated missing-texture pattern, in texels.
const MISSING_TEXTURE_SIZE: u32 = 64;

/// Decoded 8-bit RGBA image buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RawImage {
    #[must_use]
    pub const fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }
}

/// Encodes an 8-bit RGBA image as PNG bytes.
///
/// Mirror of [`decode_png`], used by the `LIMINAL_CAPTURE` developer path so a
/// rendered frame can be inspected on hardware without a screenshot tool.
/// # Errors
///
/// Returns a message when the image has a zero dimension or the PNG encoder
/// rejects the buffer.
pub fn encode_png(image: &RawImage) -> Result<Vec<u8>, String> {
    if image.width == 0 || image.height == 0 {
        return Err("cannot encode a zero-sized image".into());
    }
    if image.rgba.len() != (image.width * image.height * 4) as usize {
        return Err("image buffer length does not match its dimensions".into());
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("PNG header error: {error}"))?;
        writer
            .write_image_data(&image.rgba)
            .map_err(|error| format!("PNG encode error: {error}"))?;
    }
    Ok(out)
}

/// Decodes PNG bytes into an 8-bit RGBA raw image, validating the dimensions.
///
/// Any colour type the PNG specification allows is accepted: RGB, RGBA,
/// grayscale, grayscale+alpha and palette images (with or without `tRNS`) are
/// normalised to RGBA8, and 16-bit samples are stripped to 8 bits. Missing or
/// malformed data is an error, never a panic.
/// # Errors
///
/// Returns a message when the bytes are not a PNG, the image is empty, larger
/// than [`MAX_TEXTURE_DIMENSION`] on either edge, or the decoded buffer does
/// not match its declared size.
pub fn decode_png(bytes: &[u8]) -> Result<RawImage, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("not a PNG file (missing signature)".into());
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("invalid PNG: {e}"))?;
    let info = reader.info();
    let width = info.width;
    let height = info.height;

    if width == 0 || height == 0 {
        return Err("texture dimensions cannot be zero".into());
    }
    if width > MAX_TEXTURE_DIMENSION || height > MAX_TEXTURE_DIMENSION {
        return Err(format!(
            "texture dimensions {width}x{height} exceed the {MAX_TEXTURE_DIMENSION}x{MAX_TEXTURE_DIMENSION} limit"
        ));
    }

    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| "failed to size the PNG output buffer".to_string())?;
    let mut buf = vec![0; buf_size];
    let output_info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG decode error: {e}"))?;
    buf.truncate(output_info.buffer_size());

    let rgba = match output_info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in buf.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for &g in &buf {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in buf.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err("PNG palette was not expanded by the decoder".into());
        }
    };

    if rgba.len() != (width * height * 4) as usize {
        return Err("decoded image buffer length does not match width * height * 4".into());
    }

    Ok(RawImage::new(width, height, rgba))
}

/// Reads and decodes a PNG below `root`, naming the file in every error.
/// # Errors
///
/// Returns a message when the file cannot be read or [`decode_png`] rejects it.
pub fn load_png_relative(root: &Path, relative: &str) -> Result<RawImage, String> {
    let path = root.join(relative);
    let bytes =
        fs::read(&path).map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    decode_png(&bytes).map_err(|error| format!("`{}`: {error}", path.display()))
}

/// The one conspicuous pattern a missing or corrupt texture resolves to.
///
/// A magenta/black checker is the classic "texture is broken" signal: it can
/// never be confused with authored content, so an authoring mistake is visible
/// in a capture instead of hidden behind a plausible-looking substitute.
#[must_use]
pub fn missing_texture() -> RawImage {
    let size = MISSING_TEXTURE_SIZE;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let checker = ((x / 8) + (y / 8)).is_multiple_of(2);
            let colour: [u8; 3] = if checker { [255, 0, 255] } else { [24, 24, 24] };
            let index = ((y * size + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    RawImage::new(size, size, rgba)
}

/// Session cache of decoded images, keyed by logical texture id.
///
/// The cache is what guarantees "one decode per texture per session": a level
/// that uses a texture in twenty rooms decodes it once, and switching back to a
/// level never touches the disk again. GPU textures are owned separately by the
/// renderer, which uploads each distinct entry once per level.
#[derive(Default, Debug)]
pub struct TextureCache {
    images: HashMap<String, Rc<RawImage>>,
    decodes: usize,
}

impl TextureCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a freshly decoded image, returning the shared handle.
    pub fn insert(&mut self, key: impl Into<String>, image: RawImage) -> Rc<RawImage> {
        let image = Rc::new(image);
        self.images.insert(key.into(), Rc::clone(&image));
        self.decodes += 1;
        image
    }

    /// The cached image for a key, if it was decoded before.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Rc<RawImage>> {
        self.images.get(key).map(Rc::clone)
    }

    /// Number of successful decodes this session (tests and diagnostics).
    #[must_use]
    pub fn decoded_count(&self) -> usize {
        self.decodes
    }

    /// Number of cached images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// True when nothing has been decoded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Drops every cached image (developer tooling and tests).
    pub fn clear(&mut self) {
        self.images.clear();
        self.decodes = 0;
    }
}

/// One material a pack's `materials.json` declares.
///
/// Both the historical string form (`"pack:wall": "textures/wall.png"`) and the
/// object form (`{"texture": ..., "tile_metres": ..., "tint": [...]}`) parse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackMaterialDef {
    /// Path inside the pack, or a logical catalog texture id.
    pub texture: String,
    /// World metres per repeat; `None` keeps [`DEFAULT_TILE_METRES`].
    pub tile_metres: Option<f32>,
    pub tint: Option<[f32; 3]>,
}

impl PackMaterialDef {
    #[must_use]
    pub fn tile_metres(&self) -> f32 {
        self.tile_metres
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_TILE_METRES)
    }

    #[must_use]
    pub fn tint(&self) -> [f32; 3] {
        self.tint.unwrap_or(DEFAULT_TINT)
    }
}

/// The material definitions and raw textures extracted from one level pack.
///
/// `namespace` keeps two packs that both ship `textures/wall.png` from sharing
/// a cache entry in one session.
#[derive(Clone, Debug, Default)]
pub struct PackMaterials {
    namespace: String,
    definitions: HashMap<String, PackMaterialDef>,
    /// Raw PNG bytes keyed by the alias paths the pack extractor registered.
    textures: HashMap<String, Rc<[u8]>>,
}

impl PackMaterials {
    /// Builds the pack material view from `materials.json` and the extracted
    /// texture blobs. Malformed JSON yields no definitions (the pack's own
    /// `pack:` ids then fall back to the direct `textures/<name>.png` lookup,
    /// exactly as before).
    #[must_use]
    pub fn new(
        namespace: impl Into<String>,
        materials_json: Option<&str>,
        textures: HashMap<String, Rc<[u8]>>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            definitions: parse_materials_json(materials_json),
            textures,
        }
    }

    /// True when the pack carries no material definitions and no textures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.textures.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> &HashMap<String, PackMaterialDef> {
        &self.definitions
    }

    /// The definition for a `pack:` material id, if the pack declares one.
    #[must_use]
    pub fn definition(&self, material_id: &str) -> Option<&PackMaterialDef> {
        self.definitions.get(material_id)
    }

    /// The texture path a `pack:` material id resolves to.
    ///
    /// A declared definition wins; otherwise the historical direct-name
    /// candidates apply. A match is only returned when the pack actually
    /// carries bytes for the path.
    #[must_use]
    pub fn texture_for(&self, material_id: &str) -> Option<String> {
        if let Some(definition) = self.definition(material_id)
            && !definition.texture.is_empty()
            && self.lookup(&definition.texture).is_some()
        {
            return Some(definition.texture.clone());
        }
        let name = material_id.strip_prefix("pack:").unwrap_or(material_id);
        [
            format!("textures/{name}.png"),
            format!("{name}.png"),
            format!("textures/{name}"),
            name.to_string(),
        ]
        .into_iter()
        .find(|candidate| self.lookup(candidate).is_some())
    }

    /// The raw PNG bytes behind a pack-relative path (or a catalog texture id
    /// the pack reuses), with the pack's own alias rules.
    #[must_use]
    pub fn lookup(&self, path: &str) -> Option<Rc<[u8]>> {
        let normalized = path.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
        self.textures
            .get(&normalized)
            .or_else(|| self.textures.get(file_name))
            .map(Rc::clone)
    }

    /// Decodes one pack texture through the session cache.
    fn decode_cached(
        &self,
        cache: &mut TextureCache,
        path: &str,
    ) -> Result<(Rc<RawImage>, String), String> {
        let key = self.cache_key(path);
        if let Some(image) = cache.get(&key) {
            return Ok((image, key));
        }
        let bytes = self
            .lookup(path)
            .ok_or_else(|| format!("`{path}` is not present in the pack"))?;
        let image = decode_png(&bytes).map_err(|error| format!("`{path}`: {error}"))?;
        let image = cache.insert(key.clone(), image);
        Ok((image, key))
    }

    /// Decodes one pack texture into the session cache.
    /// # Errors
    ///
    /// Returns a message when the path is absent from the pack or the bytes are
    /// not a valid PNG.
    pub fn decode_texture(
        &self,
        cache: &mut TextureCache,
        path: &str,
    ) -> Result<Rc<RawImage>, String> {
        self.decode_cached(cache, path).map(|(image, _key)| image)
    }

    /// The session-unique cache/dedupe key of one pack texture.
    #[must_use]
    pub fn cache_key(&self, path: &str) -> String {
        format!("pack:{}:{}", self.namespace, path.replace('\\', "/"))
    }

    /// Maps a session key back to the pack-relative path, when it is one of
    /// this pack's keys.
    #[must_use]
    pub fn path_of_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        key.strip_prefix(&format!("pack:{}:", self.namespace))
    }
}

/// Parses `materials.json` into material definitions.
///
/// Accepts `{"materials": {...}}` and a flat object, with string values
/// (`"pack:wall": "textures/wall.png"`) or objects carrying `texture`/`file`/
/// `diffuse`, plus the optional `tile_metres` and `tint` fields. Unknown fields
/// are ignored and malformed values fall back to the defaults, so an older or
/// newer pack keeps loading.
#[must_use]
pub fn parse_materials_json(json_str: Option<&str>) -> HashMap<String, PackMaterialDef> {
    let mut result = HashMap::new();
    let Some(json_str) = json_str else {
        return result;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return result;
    };
    let table = value
        .get("materials")
        .and_then(|materials| materials.as_object())
        .or_else(|| value.as_object());
    let Some(table) = table else {
        return result;
    };
    for (key, value) in table {
        if key == "materials" {
            continue;
        }
        let definition = if let Some(path) = value.as_str() {
            PackMaterialDef {
                texture: path.to_string(),
                ..PackMaterialDef::default()
            }
        } else {
            let Some(path) = value
                .get("texture")
                .or_else(|| value.get("file"))
                .or_else(|| value.get("diffuse"))
                .and_then(|texture| texture.as_str())
            else {
                continue;
            };
            PackMaterialDef {
                texture: path.to_string(),
                tile_metres: value
                    .get("tile_metres")
                    .and_then(serde_json::Value::as_f64)
                    .map(|value| value as f32),
                tint: value.get("tint").and_then(parse_tint_value),
            }
        };
        if !definition.texture.is_empty() {
            result.insert(key.clone(), definition);
        }
    }
    result
}

/// Parses a `[r, g, b]` tint array from JSON.
fn parse_tint_value(value: &serde_json::Value) -> Option<[f32; 3]> {
    let array = value.as_array()?;
    if array.len() != 3 {
        return None;
    }
    let mut tint = [0.0f32; 3];
    for (index, channel) in array.iter().enumerate() {
        let value = channel.as_f64()? as f32;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return None;
        }
        tint[index] = value;
    }
    Some(tint)
}

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
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the level references no materials at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
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

    fn intern_texture(&mut self, key: String, origin: TextureOrigin, image: Rc<RawImage>) -> u16 {
        if let Some(index) = self.textures.iter().position(|texture| texture.key == key) {
            return u16::try_from(index).unwrap_or(u16::MAX);
        }
        self.textures.push(ResolvedTexture { key, origin, image });
        u16::try_from(self.textures.len() - 1).unwrap_or(u16::MAX)
    }
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

/// Logical description of one material before its image is resolved.
fn describe_material(
    id: &str,
    catalog: &AssetCatalog,
    pack: Option<&PackMaterials>,
) -> ResolvedMaterial {
    let base = |texture_key: String,
                origin: TextureOrigin,
                tile_metres: f32,
                tint: [f32; 3],
                error: Option<String>| ResolvedMaterial {
        id: id.to_string(),
        texture_key,
        texture_index: 0,
        origin,
        tile_metres,
        tint,
        image: None,
        error,
    };

    if let Some(entry) = catalog.material(id) {
        let texture_id = entry.texture.clone().unwrap_or_default();
        if texture_id.is_empty() {
            return base(
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
            return base(
                MISSING_TEXTURE_KEY.to_string(),
                TextureOrigin::Missing,
                tile_metres,
                tint,
                Some(format!(
                    "material `{id}` references texture `{texture_id}`, which has no PNG file in the catalog"
                )),
            );
        }
        return base(texture_id, TextureOrigin::Catalog, tile_metres, tint, None);
    }

    if let Some(entry) = catalog.get(id) {
        return base(
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
        if let Some(definition) = pack.and_then(|pack| pack.definition(id)) {
            // A pack may reuse a catalog texture by logical id, or ship its own
            // PNG. A declared-but-missing pack file is a named error, not a
            // silent fall-through to a same-named file.
            if definition.texture.contains(':')
                && catalog.texture_path(&definition.texture).is_some()
            {
                return base(
                    definition.texture.clone(),
                    TextureOrigin::Catalog,
                    definition.tile_metres(),
                    definition.tint(),
                    None,
                );
            }
            if pack.is_some_and(|pack| pack.lookup(&definition.texture).is_some()) {
                return base(
                    definition.texture.clone(),
                    TextureOrigin::Pack,
                    definition.tile_metres(),
                    definition.tint(),
                    None,
                );
            }
            return base(
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
            return base(
                path,
                TextureOrigin::Pack,
                DEFAULT_TILE_METRES,
                DEFAULT_TINT,
                None,
            );
        }
        return base(
            format!("pack:unresolved:{id}"),
            TextureOrigin::Pack,
            DEFAULT_TILE_METRES,
            DEFAULT_TINT,
            Some(format!(
                "pack material `{id}` has no `materials.json` entry and no matching PNG in the pack"
            )),
        );
    }

    base(
        MISSING_TEXTURE_KEY.to_string(),
        TextureOrigin::Missing,
        DEFAULT_TILE_METRES,
        DEFAULT_TINT,
        Some(format!(
            "unknown material `{id}`; add it to the asset catalog or use the diagnostic material"
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

    for index in 0..table.entries.len() {
        let (origin, key, image_result) = {
            let entry = &table.entries[index];
            let origin = entry.origin;
            let key = entry.texture_key.clone();
            let result: Result<Rc<RawImage>, String> = match origin {
                TextureOrigin::Catalog => {
                    if let Some(cached) = cache.get(&key) {
                        Ok(cached)
                    } else {
                        match (asset_root, catalog.texture_path(&key)) {
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
                        }
                    }
                }
                TextureOrigin::Pack => {
                    let unresolved = entry.texture_key.starts_with("pack:unresolved:");
                    match pack {
                        Some(pack) if !unresolved => {
                            let path = entry.texture_key.clone();
                            pack.decode_cached(cache, &path)
                                .map(|(image, _key)| image)
                                .map_err(|error| format!("material `{}`: {error}", entry.id))
                        }
                        _ => Err(entry.error.clone().unwrap_or_else(|| {
                            format!("material `{}` has no resolvable pack texture", entry.id)
                        })),
                    }
                }
                TextureOrigin::Missing => return_error(&entry.error, &entry.id),
            };
            (origin, key, result)
        };

        match image_result {
            Ok(image) => {
                let texture_index = table.intern_texture(key, origin, image.clone());
                let entry = &mut table.entries[index];
                entry.image = Some(image);
                entry.texture_index = texture_index;
            }
            Err(error) => {
                let texture_index = table.intern_texture(
                    MISSING_TEXTURE_KEY.to_string(),
                    TextureOrigin::Missing,
                    Rc::clone(&missing),
                );
                let entry = &mut table.entries[index];
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
fn return_error(error: &Option<String>, id: &str) -> Result<Rc<RawImage>, String> {
    Err(error
        .clone()
        .unwrap_or_else(|| format!("material `{id}` could not be resolved")))
}

/// One external (PNG-backed) decal sheet a level places.
///
/// The four built-in decal sheets are generated by the renderer; a decal asset
/// with `source: "file"` and a `.png` model is ordinary external artwork like a
/// surface texture, resolved through the same catalog and decoded once per
/// session into the same [`TextureCache`].
#[derive(Clone, Debug)]
pub struct ResolvedDecalSheet {
    /// Logical decal asset id, exactly as the level writes it.
    pub material: String,
    /// Texture cache key (the PNG path relative to the asset root).
    pub key: String,
    /// Decoded pixels, shared with the session cache.
    pub image: Rc<RawImage>,
}

/// Resolves one decal sheet asset to its decoded PNG.
///
/// # Errors
///
/// Returns a message naming the decal and the exact catalog/PNG problem when
/// the asset is unknown, is not a `file` decal with a `.png` model, the asset
/// root is missing, or the PNG cannot be decoded.
pub fn resolve_decal_sheet(
    catalog: &AssetCatalog,
    asset_root: Option<&Path>,
    cache: &mut TextureCache,
    material_id: &str,
) -> Result<ResolvedDecalSheet, String> {
    let Some(entry) = catalog.get(material_id) else {
        return Err(format!(
            "decal `{material_id}` is not declared in the asset catalog"
        ));
    };
    if !matches!(entry.source, crate::assets::AssetSource::File) {
        return Err(format!(
            "decal `{material_id}` is not a file asset; a generated decal is drawn from the built-in sheet"
        ));
    }
    let Some(path) = entry.model.as_deref() else {
        return Err(format!(
            "decal `{material_id}` names no PNG; a file-backed decal needs a `model`"
        ));
    };
    if !path.to_ascii_lowercase().ends_with(".png") {
        return Err(format!(
            "decal `{material_id}` must name a `.png` sheet, found `{path}`"
        ));
    }
    let key = path.to_string();
    if let Some(image) = cache.get(&key) {
        return Ok(ResolvedDecalSheet {
            material: material_id.to_string(),
            key,
            image,
        });
    }
    let Some(root) = asset_root else {
        return Err(format!("decal `{material_id}`: the asset root is missing"));
    };
    let image =
        load_png_relative(root, &key).map_err(|error| format!("decal `{material_id}`: {error}"))?;
    let image = cache.insert(key.clone(), image);
    Ok(ResolvedDecalSheet {
        material: material_id.to_string(),
        key,
        image,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetSource;
    use crate::level::LevelDef;

    fn level_from(json: &str) -> LevelDef {
        LevelDef::from_json(json).expect("test level parses")
    }

    fn basic_level(wall: &str, floor: &str, ceiling: &str) -> LevelDef {
        level_from(&format!(
            r##"{{
                "format_version": 1,
                "id": "material_test", "name": "Material Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "defaults": {{ "wall": "{wall}", "floor": "{floor}", "ceiling": "{ceiling}" }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }}]
            }}"##
        ))
    }

    fn shipped_catalog() -> AssetCatalog {
        AssetCatalog::load_default()
    }

    /// Encodes raw samples with an explicit PNG colour type (test helper).
    fn encode_as(
        color: png::ColorType,
        depth: png::BitDepth,
        width: u32,
        height: u32,
        data: &[u8],
        palette: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if let Some(palette) = palette {
                encoder.set_palette(palette);
            }
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(data).expect("data");
        }
        out
    }

    #[test]
    fn decode_png_round_trips_rgba_exactly() {
        let image = RawImage::new(
            3,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 10, 20, 30, 255, 40, 50, 60, 255, 70,
                80, 90, 64,
            ],
        );
        let encoded = encode_png(&image).expect("encode");
        let decoded = decode_png(&encoded).expect("decode");
        assert_eq!(decoded, image);
    }

    #[test]
    fn decode_png_accepts_rgb_grayscale_palette_and_sixteen_bit_images() {
        let rgb = encode_as(
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            2,
            1,
            &[1, 2, 3, 4, 5, 6],
            None,
        );
        assert_eq!(
            decode_png(&rgb).expect("rgb").rgba,
            vec![1, 2, 3, 255, 4, 5, 6, 255]
        );

        let gray = encode_as(
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            2,
            1,
            &[7, 9],
            None,
        );
        assert_eq!(
            decode_png(&gray).expect("gray").rgba,
            vec![7, 7, 7, 255, 9, 9, 9, 255]
        );

        let gray_alpha = encode_as(
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
            2,
            1,
            &[7, 128, 9, 0],
            None,
        );
        assert_eq!(
            decode_png(&gray_alpha).expect("gray alpha").rgba,
            vec![7, 7, 7, 128, 9, 9, 9, 0]
        );

        let palette = encode_as(
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            2,
            1,
            &[0, 1],
            Some(vec![10, 20, 30, 40, 50, 60]),
        );
        assert_eq!(
            decode_png(&palette).expect("palette").rgba,
            vec![10, 20, 30, 255, 40, 50, 60, 255]
        );

        // 16-bit samples are stripped to their high byte, not rejected.
        let sixteen = encode_as(
            png::ColorType::Rgb,
            png::BitDepth::Sixteen,
            1,
            1,
            &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC],
            None,
        );
        let decoded = decode_png(&sixteen).expect("16-bit");
        assert_eq!(decoded.rgba, vec![0x12, 0x56, 0x9A, 255]);
    }

    #[test]
    fn malformed_and_truncated_pngs_are_errors_not_panics() {
        assert!(decode_png(b"").is_err());
        assert!(decode_png(b"not a png at all").is_err());
        assert!(decode_png(b"\x89PNG\r\n\x1a\n").is_err());

        let image = RawImage::new(4, 4, vec![200; 4 * 4 * 4]);
        let bytes = encode_png(&image).expect("encode");
        let truncated = &bytes[..bytes.len() / 2];
        assert!(decode_png(truncated).is_err(), "truncated PNG must fail");
        let mut corrupted = bytes.clone();
        let mid = corrupted.len() / 2;
        corrupted[mid] ^= 0xFF;
        assert!(decode_png(&corrupted).is_err(), "corrupt PNG must fail");
    }

    #[test]
    fn oversized_pngs_are_rejected_with_a_clear_message() {
        let wide = RawImage::new(
            MAX_TEXTURE_DIMENSION + 1,
            1,
            vec![0; 4 * (MAX_TEXTURE_DIMENSION as usize + 1)],
        );
        let bytes = encode_png(&wide).expect("encode");
        let error = decode_png(&bytes).expect_err("oversized must fail");
        assert!(error.contains("exceed"), "unexpected error: {error}");
    }

    #[test]
    fn cache_decodes_each_key_once_and_reuses_the_buffer() {
        let mut cache = TextureCache::new();
        assert!(cache.get("core:tex_a").is_none());
        let first = cache.insert("core:tex_a", RawImage::new(1, 1, vec![1, 2, 3, 4]));
        let second = cache.get("core:tex_a").expect("cached");
        assert!(Rc::ptr_eq(&first, &second), "the same buffer is shared");
        assert_eq!(cache.decoded_count(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn shipped_materials_resolve_through_the_catalog() {
        let catalog = shipped_catalog();
        let level = basic_level(
            "core:wallpaper_yellow_01",
            "core:carpet_damp_01",
            "core:ceiling_panel_01",
        );
        let mut cache = TextureCache::new();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

        assert_eq!(table.len(), 3);
        assert!(table.errors().is_empty(), "errors: {:?}", table.errors());
        let wall = table.entry_of("core:wallpaper_yellow_01").expect("wall");
        assert_eq!(wall.origin, TextureOrigin::Catalog);
        assert_eq!(wall.texture_key, "core:tex_wallpaper_yellow_01");
        assert_eq!(wall.tile_metres, 2.0);
        assert_eq!(wall.tint, [0.85, 0.80, 0.42]);
        let image = wall.image.as_ref().expect("decoded image");
        assert_eq!(image.width, 128);
        assert_eq!(image.height, 128);

        let floor = table.entry_of("core:carpet_damp_01").expect("floor");
        assert_eq!(floor.tint, DEFAULT_TINT);
        assert_eq!(table.textures().len(), 3);
        assert_eq!(cache.decoded_count(), 3);
    }

    #[test]
    fn two_materials_sharing_a_texture_share_one_resolved_texture() {
        let catalog = shipped_catalog();
        // A synthetic catalog where two materials point at one texture.
        let json = r##"{
            "themes": [{ "id": "office" }],
            "assets": [
                { "id": "core:tex_shared", "asset_class": "environment", "asset_type": "texture",
                  "source": "file", "model": "environment/office/textures/ceilings/ceiling_panel_01.png" },
                { "id": "core:mat_a", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_shared" },
                { "id": "core:mat_b", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_shared", "tile_metres": 4.0 }
            ]
        }"##;
        let catalog2 = AssetCatalog::from_json_str(json).expect("synthetic catalog");
        let level = basic_level("core:mat_a", "core:mat_b", "core:mat_a");
        let mut cache = TextureCache::new();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let table = resolve_materials(&level, &catalog2, None, Some(&root), &mut cache);

        assert_eq!(table.len(), 2);
        assert_eq!(table.textures().len(), 1, "one shared texture");
        assert_eq!(cache.decoded_count(), 1, "decoded once");
        assert_eq!(table.entry_of("core:mat_b").expect("b").tile_metres, 4.0);
        let a = table.entry_of("core:mat_a").expect("a");
        let b = table.entry_of("core:mat_b").expect("b");
        assert_eq!(a.texture_index, b.texture_index, "one GPU upload slot");
        assert_eq!(a.texture_index, 0);
        let _ = catalog;
    }

    #[test]
    fn unknown_material_uses_the_diagnostic_texture_with_a_useful_error() {
        let catalog = shipped_catalog();
        let level = basic_level(
            "core:not_a_material",
            "core:carpet_beige_01",
            "core:ceiling_panel_01",
        );
        let mut cache = TextureCache::new();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);

        let entry = table.entry_of("core:not_a_material").expect("entry");
        assert_eq!(entry.origin, TextureOrigin::Missing);
        assert_eq!(entry.texture_key, MISSING_TEXTURE_KEY);
        let error = entry.error.as_deref().expect("error");
        assert!(error.contains("core:not_a_material"), "error: {error}");
        assert!(error.contains("catalog"), "error: {error}");
        assert!(table.first_missing().is_some());
    }

    #[test]
    fn missing_png_falls_back_to_the_diagnostic_and_names_both_ids() {
        let catalog = shipped_catalog();
        let level = basic_level(
            "core:wallpaper_yellow_01",
            "core:carpet_beige_01",
            "core:ceiling_panel_01",
        );
        let mut cache = TextureCache::new();
        let empty =
            std::env::temp_dir().join(format!("places_materials_missing_{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty);
        fs::create_dir_all(&empty).expect("temp dir");
        let table = resolve_materials(&level, &catalog, None, Some(&empty), &mut cache);

        let wall = table.entry_of("core:wallpaper_yellow_01").expect("wall");
        let error = wall.error.as_deref().expect("error");
        assert!(error.contains("core:wallpaper_yellow_01"), "error: {error}");
        assert!(
            error.contains("core:tex_wallpaper_yellow_01"),
            "error: {error}"
        );
        assert_eq!(wall.origin, TextureOrigin::Missing);
        let _ = fs::remove_dir_all(&empty);
    }

    #[test]
    fn material_id_in_the_wrong_type_is_reported() {
        let catalog = shipped_catalog();
        let level = basic_level("core:desk", "core:carpet_beige_01", "core:ceiling_panel_01");
        let mut cache = TextureCache::new();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let table = resolve_materials(&level, &catalog, None, Some(&root), &mut cache);
        let error = table
            .entry_of("core:desk")
            .and_then(|entry| entry.error.clone())
            .expect("error");
        assert!(error.contains("`prop` asset"), "error: {error}");
    }

    #[test]
    fn referenced_ids_are_deterministic_and_cover_faces_patches_and_regions() {
        let level = level_from(
            r##"{
                "format_version": 1,
                "id": "scan_test", "name": "Scan Test",
                "spawn": { "x": 0.0, "z": 0.0 },
                "defaults": { "wall": "w", "floor": "f", "ceiling": "c" },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0,
                            "material": "room-floor", "ceiling_material": "room-ceiling" }],
                "walls": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.2,
                            "material": "wall-body", "faces": { "south": "face-s", "north": "face-n" } }],
                "floor_patches": [{ "x": 1.0, "z": 1.0, "width": 1.0, "depth": 1.0, "material": "patch" }],
                "floor_regions": [{ "x": 2.0, "z": 2.0, "width": 1.0, "depth": 1.0,
                                    "offset_y": -0.5, "material": "region-floor", "edge_material": "region-edge" }]
            }"##,
        );
        let ids = referenced_material_ids(&level);
        assert_eq!(
            ids,
            vec![
                "w",
                "f",
                "c",
                "room-floor",
                "room-ceiling",
                "wall-body",
                "face-n",
                "face-s",
                "patch",
                "region-floor",
                "region-edge",
            ]
        );
        let again = referenced_material_ids(&level);
        assert_eq!(ids, again, "the scan must be deterministic");
    }

    #[test]
    fn pack_materials_parse_both_shapes_and_decode_from_pack_bytes() {
        let png = encode_png(&RawImage::new(
            2,
            2,
            vec![9, 8, 7, 255, 6, 5, 4, 255, 3, 2, 1, 255, 0, 0, 0, 255],
        ))
        .expect("encode");
        let mut textures = HashMap::new();
        textures.insert("textures/wall.png".to_string(), Rc::from(png.clone()));
        textures.insert("wall.png".to_string(), Rc::<[u8]>::from(png.clone()));
        let json = r#"{
            "materials": {
                "pack:wall": { "texture": "textures/wall.png", "tile_metres": 3.0,
                               "tint": [0.5, 0.5, 0.5] },
                "pack:legacy": "textures/wall.png"
            }
        }"#;
        let pack = PackMaterials::new("unit_pack", Some(json), textures);
        let level = basic_level("pack:wall", "pack:legacy", "pack:wall");
        let catalog = AssetCatalog::builtin();
        let mut cache = TextureCache::new();
        let table = resolve_materials(&level, &catalog, Some(&pack), None, &mut cache);

        assert!(table.errors().is_empty(), "errors: {:?}", table.errors());
        let wall = table.entry_of("pack:wall").expect("wall");
        assert_eq!(wall.origin, TextureOrigin::Pack);
        assert_eq!(wall.tile_metres, 3.0);
        assert_eq!(wall.tint, [0.5, 0.5, 0.5]);
        assert_eq!(wall.image.as_ref().expect("image").width, 2);
        assert_eq!(cache.decoded_count(), 1, "shared key decodes once");
        assert_eq!(
            table.textures().len(),
            1,
            "both materials share one texture"
        );
    }

    #[test]
    fn pack_material_without_a_texture_is_a_context_rich_error() {
        let pack = PackMaterials::new("unit_pack", None, HashMap::new());
        let level = basic_level("pack:missing_wall", "f", "c");
        let catalog = AssetCatalog::builtin();
        let mut cache = TextureCache::new();
        let table = resolve_materials(&level, &catalog, Some(&pack), None, &mut cache);
        let error = table
            .entry_of("pack:missing_wall")
            .and_then(|entry| entry.error.clone())
            .expect("error");
        assert!(error.contains("pack:missing_wall"), "error: {error}");
        assert!(
            error.contains("materials.json") && error.contains("PNG"),
            "error should say what to add: {error}"
        );
    }

    #[test]
    fn pack_materials_may_reuse_a_catalog_texture_or_name_a_missing_file() {
        // `materials.json` naming a catalog texture id resolves it through the
        // catalog; naming an absent pack file is a named error.
        let json = r#"{
            "materials": {
                "pack:builtin": { "texture": "core:tex_ceiling_panel_01" },
                "pack:typo": { "texture": "textures/typo_wall.png" }
            }
        }"#;
        let pack = PackMaterials::new("unit_pack", Some(json), HashMap::new());
        let level = basic_level("pack:builtin", "pack:typo", "core:ceiling_panel_01");
        let catalog = AssetCatalog::load_default();
        let root = crate::assets::resolve_asset_root().expect("assets/ is discoverable");
        let mut cache = TextureCache::new();
        let table = resolve_materials(&level, &catalog, Some(&pack), Some(&root), &mut cache);

        let builtin = table.entry_of("pack:builtin").expect("builtin entry");
        assert_eq!(builtin.origin, TextureOrigin::Catalog);
        assert_eq!(builtin.texture_key, "core:tex_ceiling_panel_01");
        assert!(builtin.image.is_some());

        let typo = table.entry_of("pack:typo").expect("typo entry");
        assert_eq!(typo.origin, TextureOrigin::Missing);
        let error = typo.error.as_deref().expect("error");
        assert!(error.contains("textures/typo_wall.png"), "error: {error}");
        assert!(error.contains("materials.json"), "error: {error}");
    }

    #[test]
    fn catalog_rejects_materials_with_dangling_or_non_png_textures() {
        let dangling = r##"{
            "assets": [
                { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_nope" }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(dangling).expect_err("dangling texture");
        assert!(error.contains("core:tex_nope"), "error: {error}");

        let not_a_texture = r##"{
            "assets": [
                { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:prop" },
                { "id": "core:prop", "asset_class": "environment", "asset_type": "prop",
                  "source": "file", "model": "core/props/models/couch.glb" }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(not_a_texture).expect_err("wrong type");
        assert!(error.contains("not a texture"), "error: {error}");

        let not_png = r##"{
            "assets": [
                { "id": "core:tex_bad", "asset_class": "environment", "asset_type": "texture",
                  "source": "file", "model": "core/textures/bad.jpg" }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(not_png).expect_err("not a png");
        assert!(error.contains(".png"), "error: {error}");
    }

    #[test]
    fn catalog_rejects_materials_without_a_texture_and_bad_material_metadata() {
        let no_texture = r##"{
            "assets": [
                { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition" }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(no_texture).expect_err("no texture");
        assert!(error.contains("texture"), "error: {error}");

        let bad_tile = r##"{
            "assets": [
                { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
                  "source": "file", "model": "core/textures/a.png" },
                { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_a", "tile_metres": 0.0 }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(bad_tile).expect_err("bad tile");
        assert!(error.contains("tile_metres"), "error: {error}");

        let bad_tint = r##"{
            "assets": [
                { "id": "core:tex_a", "asset_class": "environment", "asset_type": "texture",
                  "source": "file", "model": "core/textures/a.png" },
                { "id": "core:mat", "asset_class": "environment", "asset_type": "material",
                  "source": "definition", "texture": "core:tex_a", "tint": [1.5, 0.0, 0.0] }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(bad_tint).expect_err("bad tint");
        assert!(error.contains("tint"), "error: {error}");

        let texture_on_prop = r##"{
            "assets": [
                { "id": "core:p", "asset_class": "environment", "asset_type": "prop",
                  "source": "file", "model": "core/props/models/couch.glb",
                  "texture": "core:tex_a" }
            ]
        }"##;
        let error = AssetCatalog::from_json_str(texture_on_prop).expect_err("texture on prop");
        assert!(error.contains("material"), "error: {error}");
    }

    #[test]
    fn every_shipped_material_resolves_to_a_png_texture() {
        let catalog = shipped_catalog();
        for material in catalog.materials() {
            let texture_id = material
                .texture
                .as_deref()
                .unwrap_or_else(|| panic!("{}: materials must declare a texture", material.id));
            let path = catalog
                .texture_path(texture_id)
                .unwrap_or_else(|| panic!("{}: texture {texture_id} has no PNG", material.id));
            assert!(path.ends_with(".png"), "{}: {path}", material.id);
            assert_eq!(material.source, AssetSource::Definition);
        }
    }
}
