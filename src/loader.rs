use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use zip::ZipArchive;

use crate::level::{LevelDef, MAX_LEVEL_FLOOR_AREA_M2, MAX_LEVEL_VERTICES};
use crate::render::{
    generate_carpet_texture, generate_ceiling_texture, generate_damp_carpet_texture,
    generate_stained_ceiling_texture, generate_stained_wall_texture, generate_wall_texture,
    generate_white_texture,
};

const FALLBACK_LEVEL1_JSON: &str = include_str!("../assets/levels/level1.json");

const MAX_ZIP_ENTRIES: usize = 500;
const MAX_ZIP_ENTRY_SIZE: u64 = 10 * 1024 * 1024; // 10 MB per file
const MAX_ZIP_TOTAL_SIZE: u64 = 50 * 1024 * 1024; // 50 MB total uncompressed

/// Decoded 8-bit RGBA image buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RawImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }
}

/// Resolved level textures ready for OpenGL upload.
#[derive(Clone, Debug)]
pub struct LoadedTextures {
    pub wall: RawImage,
    pub floor: RawImage,
    pub ceiling: RawImage,
    pub fixture: RawImage,
}

impl Default for LoadedTextures {
    fn default() -> Self {
        Self {
            wall: RawImage::new(128, 128, generate_wall_texture().to_vec()),
            floor: RawImage::new(64, 64, generate_carpet_texture().to_vec()),
            ceiling: RawImage::new(128, 128, generate_ceiling_texture().to_vec()),
            fixture: RawImage::new(2, 2, generate_white_texture().to_vec()),
        }
    }
}

/// Source type of an installed level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelSourceType {
    Official,
    CustomJson,
    PackZip,
}

/// Discovered level entry for level selection menu.
#[derive(Clone, Debug)]
pub struct LevelEntry {
    pub id: String,
    pub name: String,
    pub author: String,
    pub source_type: LevelSourceType,
    pub path: PathBuf,
}

/// A validated, fully loaded level ready for gameplay.
#[derive(Clone, Debug)]
pub struct LoadedLevel {
    pub level: LevelDef,
    pub textures: LoadedTextures,
    pub entry: LevelEntry,
}

/// Encodes an 8-bit RGBA image as PNG bytes.
///
/// Mirror of [`decode_png`], used by the `LIMINAL_CAPTURE` developer path so a
/// rendered frame can be inspected on hardware without a screenshot tool.
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

/// Decodes PNG bytes into 8-bit RGBA raw image buffer with dimensions validation.
pub fn decode_png(bytes: &[u8]) -> Result<RawImage, String> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("Invalid PNG format: {e}"))?;
    let info = reader.info();
    let width = info.width;
    let height = info.height;

    // Security & sanity checks on dimensions
    if width == 0 || height == 0 {
        return Err("Texture dimensions cannot be zero".into());
    }
    if width > 1024 || height > 1024 {
        return Err(format!(
            "Texture dimensions {width}x{height} exceed limit of 1024x1024"
        ));
    }

    let buf_size = reader
        .output_buffer_size()
        .ok_or_else(|| "Failed to get PNG output buffer size".to_string())?;
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
        other => return Err(format!("Unsupported PNG color type: {:?}", other)),
    };

    if rgba.len() != (width * height * 4) as usize {
        return Err("Decoded image buffer length does not match width * height * 4".into());
    }

    Ok(RawImage::new(width, height, rgba))
}

/// Raw contents extracted safely from a ZIP level pack.
///
/// Texture bytes are reference-counted so that the several alias keys a pack
/// may use (`textures/x.png`, `x.png`, ...) share a single physical buffer
/// instead of duplicating it.
#[derive(Default, Debug)]
pub struct RawPackContents {
    pub level_json: String,
    pub materials_json: Option<String>,
    pub textures: HashMap<String, Rc<[u8]>>,
}

/// Reads and parses only the `level.json` entry from a ZIP pack without
/// decompressing any textures or other assets.
///
/// Used for cheap level discovery: probing a pack must not extract its full
/// contents just to learn its id/name/author.
pub fn read_zip_level_json<R: Read + Seek>(reader: R) -> Result<String, String> {
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive exceeds maximum entry count of {MAX_ZIP_ENTRIES}"
        ));
    }

    // Match `extract_zip` semantics: normalize separators and let the last
    // level.json entry win if a pack contains more than one.
    let mut target: Option<usize> = None;
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("Corrupt ZIP entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let file_name = name.rsplit('/').next().unwrap_or(&name);
        if file_name.eq_ignore_ascii_case("level.json") {
            target = Some(i);
        }
    }

    let index = target.ok_or_else(|| "ZIP level pack missing required 'level.json'".to_string())?;
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("Corrupt ZIP entry {index}: {e}"))?;
    if entry.size() > MAX_ZIP_ENTRY_SIZE {
        return Err("level.json exceeds the maximum decompression limit".into());
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read level.json: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("level.json is not valid UTF-8: {e}"))
}

/// Safely extracts a ZIP level pack with path traversal and size limits enforcement.
pub fn extract_zip<R: Read + Seek>(reader: R) -> Result<RawPackContents, String> {
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP archive exceeds maximum entry count of {MAX_ZIP_ENTRIES}"
        ));
    }

    let mut pack = RawPackContents::default();
    let mut total_uncompressed: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Corrupt ZIP entry {i}: {e}"))?;
        if file.is_dir() {
            continue;
        }

        // Security: Path traversal validation
        let raw_name = file.name().to_string();
        if raw_name.contains("..") || raw_name.starts_with('/') || raw_name.starts_with('\\') {
            return Err(format!(
                "Unsafe path traversal detected in ZIP entry: {raw_name}"
            ));
        }
        if file.enclosed_name().is_none() {
            return Err(format!(
                "Path in ZIP is outside extraction boundary: {raw_name}"
            ));
        }

        // Security: Exclude executable or script extensions
        let lower = raw_name.to_lowercase();
        if lower.ends_with(".exe")
            || lower.ends_with(".sh")
            || lower.ends_with(".bat")
            || lower.ends_with(".so")
            || lower.ends_with(".dylib")
            || lower.ends_with(".dll")
            || lower.ends_with(".bin")
            || lower.ends_with(".wasm")
        {
            continue;
        }

        let size = file.size();
        if size > MAX_ZIP_ENTRY_SIZE {
            return Err(format!(
                "ZIP entry {raw_name} exceeds 10MB decompression limit"
            ));
        }
        total_uncompressed = total_uncompressed.saturating_add(size);
        if total_uncompressed > MAX_ZIP_TOTAL_SIZE {
            return Err("Total uncompressed size of ZIP exceeds 50MB limit".into());
        }

        let mut bytes = Vec::with_capacity(size as usize);
        file.read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read {raw_name}: {e}"))?;

        let normalized = raw_name.replace('\\', "/");
        let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);

        if file_name.eq_ignore_ascii_case("level.json") {
            pack.level_json = String::from_utf8(bytes)
                .map_err(|e| format!("level.json is not valid UTF-8: {e}"))?;
        } else if file_name.eq_ignore_ascii_case("materials.json") {
            pack.materials_json = Some(
                String::from_utf8(bytes)
                    .map_err(|e| format!("materials.json is not valid UTF-8: {e}"))?,
            );
        } else if normalized.contains("textures/") || normalized.ends_with(".png") {
            // Share one physical buffer across every alias key.
            let blob: Rc<[u8]> = Rc::from(bytes);
            pack.textures.insert(normalized.clone(), Rc::clone(&blob));
            if let Some(tex_sub) = normalized.split("textures/").nth(1) {
                pack.textures
                    .insert(format!("textures/{tex_sub}"), Rc::clone(&blob));
                pack.textures.insert(tex_sub.to_string(), Rc::clone(&blob));
            }
            pack.textures.insert(file_name.to_string(), blob);
        }
    }

    if pack.level_json.is_empty() {
        return Err("ZIP level pack missing required 'level.json'".into());
    }

    Ok(pack)
}

/// Parses optional materials.json mapping into material_id -> texture_path.
pub fn parse_materials_json(json_str: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(obj) = val.get("materials").and_then(|m| m.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    result.insert(k.clone(), s.to_string());
                } else if let Some(tex) = v
                    .get("texture")
                    .or_else(|| v.get("file"))
                    .or_else(|| v.get("diffuse"))
                    .and_then(|t| t.as_str())
                {
                    result.insert(k.clone(), tex.to_string());
                }
            }
        } else if let Some(obj) = val.as_object() {
            for (k, v) in obj {
                if k == "materials" {
                    continue;
                }
                if let Some(s) = v.as_str() {
                    result.insert(k.clone(), s.to_string());
                } else if let Some(tex) = v
                    .get("texture")
                    .or_else(|| v.get("file"))
                    .or_else(|| v.get("diffuse"))
                    .and_then(|t| t.as_str())
                {
                    result.insert(k.clone(), tex.to_string());
                }
            }
        }
    }
    result
}

/// Neutral fallback colour used for unknown prop models, `#8a8a8a`.
const PROP_FALLBACK_COLOR_HEX: &str = "#8a8a8a";

/// Neutral grey used for unknown prop models.
fn prop_fallback_color() -> [f32; 3] {
    parse_hex_color(PROP_FALLBACK_COLOR_HEX).unwrap_or([0.541, 0.541, 0.541])
}

/// Parses `#rrggbb` (or bare `rrggbb`) into 0..1 RGB components.
pub fn parse_hex_color(value: &str) -> Option<[f32; 3]> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let component = |start: usize| -> f32 {
        u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or(0) as f32 / 255.0
    };
    Some([component(0), component(2), component(4)])
}

/// One catalog entry describing a placeable prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropCatalogEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub model: Option<String>,
    pub solid: bool,
}

#[derive(serde::Deserialize)]
struct PropCatalogFile {
    /// Part of the stable `props.json` shape, reserved for future revisions.
    #[serde(default)]
    #[allow(dead_code)]
    format_version: u32,
    #[serde(default)]
    props: Vec<PropCatalogFileEntry>,
}

#[derive(serde::Deserialize)]
struct PropCatalogFileEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    size: Option<[f32; 3]>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    solid: bool,
}

/// Catalog of prop models available to levels.
///
/// Lookups always succeed: unknown models resolve to a generated fallback entry
/// using [`crate::level::PROP_FALLBACK_SIZE`] and a neutral colour, so a level
/// referencing a missing prop still loads with an obvious placeholder.
#[derive(Debug, Clone, Default)]
pub struct PropCatalog {
    entries: HashMap<String, PropCatalogEntry>,
}

impl PropCatalog {
    /// Empty catalog; every lookup falls back.
    pub fn builtin() -> Self {
        Self::default()
    }

    /// Parses a `props.json` document into a catalog.
    ///
    /// Entries without an `id` are skipped; missing fields default to the
    /// neutral fallback values.
    pub fn from_json_str(json: &str) -> Result<Self, String> {
        let file: PropCatalogFile =
            serde_json::from_str(json).map_err(|e| format!("Invalid prop catalog JSON: {e}"))?;

        let mut catalog = Self::default();
        for entry in file.props {
            if entry.id.trim().is_empty() {
                continue;
            }
            let size = entry
                .size
                .filter(|s| s.iter().all(|v| v.is_finite() && *v > 0.0))
                .unwrap_or(crate::level::PROP_FALLBACK_SIZE);
            let color = entry
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .unwrap_or_else(prop_fallback_color);
            let name = if entry.name.trim().is_empty() {
                entry.id.clone()
            } else {
                entry.name
            };
            catalog.entries.insert(
                entry.id.clone(),
                PropCatalogEntry {
                    id: entry.id,
                    name,
                    category: entry.category.unwrap_or_else(|| "Other".into()),
                    size,
                    color,
                    model: entry.model.filter(|m| !m.trim().is_empty()),
                    solid: entry.solid,
                },
            );
        }
        Ok(catalog)
    }

    /// Loads a catalog from `path`, returning `None` when the file is missing
    /// or invalid. Never panics.
    pub fn load_from_path(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        Self::from_json_str(&content).ok()
    }

    /// Loads the shipped prop catalog, falling back to an empty catalog when
    /// no `props.json` can be found.
    pub fn load_default() -> Self {
        for candidate in [
            Path::new("assets/props/props.json"),
            Path::new("./assets/props/props.json"),
        ] {
            if let Some(catalog) = Self::load_from_path(candidate) {
                return catalog;
            }
        }
        Self::builtin()
    }

    /// Resolves a model id, or a generated fallback entry for unknown models.
    pub fn get(&self, model: &str) -> PropCatalogEntry {
        if let Some(entry) = self.entries.get(model) {
            return entry.clone();
        }
        PropCatalogEntry {
            id: model.to_string(),
            name: model.to_string(),
            category: "Other".into(),
            size: crate::level::PROP_FALLBACK_SIZE,
            color: prop_fallback_color(),
            model: None,
            solid: false,
        }
    }

    /// Number of catalog entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every catalogue entry, ordered by id so validation and reports are stable.
    pub fn entries(&self) -> Vec<PropCatalogEntry> {
        let mut entries: Vec<PropCatalogEntry> = self.entries.values().cloned().collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    /// True when the catalog defines this exact model id.
    pub fn contains(&self, model: &str) -> bool {
        self.entries.contains_key(model)
    }

    /// True when the catalog has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Validates level schema, format version, and physical dimensions.
/// Preserves intentional overlapping/intersecting geometry without snapping or rejecting.
pub fn validate_level(level: &LevelDef) -> Result<(), String> {
    // 1. Format version
    if level.format_version != 1 {
        return Err(format!(
            "Unsupported level format_version: {} (expected 1)",
            level.format_version
        ));
    }

    // 2. Identity
    if level.id.trim().is_empty() {
        return Err("Level 'id' cannot be empty".into());
    }
    if level.name.trim().is_empty() {
        return Err("Level 'name' cannot be empty".into());
    }

    // 3. Spawn point sanity
    if !level.spawn.x.is_finite()
        || !level.spawn.z.is_finite()
        || !level.spawn.yaw_degrees.is_finite()
    {
        return Err("Player spawn coordinates or orientation contain non-finite numbers".into());
    }

    // 4. Geometry limits
    let room_count = level.room_iter().count();
    if room_count > 500 {
        return Err(format!(
            "Level contains too many rooms: {room_count} (limit: 500)"
        ));
    }
    if level.walls.len() > 5000 {
        return Err(format!(
            "Level contains too many walls: {} (limit: 5000)",
            level.walls.len()
        ));
    }
    if level.ceiling_lights.len() > 5000 {
        return Err(format!(
            "Level contains too many ceiling lights: {} (limit: 5000)",
            level.ceiling_lights.len()
        ));
    }
    if level.props.len() > 5000 {
        return Err(format!(
            "Level contains too many props: {} (limit: 5000)",
            level.props.len()
        ));
    }

    for (i, r) in level.room_iter().enumerate() {
        if !r.width.is_finite() || !r.depth.is_finite() || !r.height.is_finite() {
            return Err(format!("Room {i} dimensions must be finite numbers"));
        }
        if r.width <= 0.0 || r.depth <= 0.0 || r.height <= 0.0 {
            return Err(format!(
                "Room {i} width, depth, and height must be positive"
            ));
        }
        if r.width > 2000.0 || r.depth > 2000.0 || r.height > 50.0 {
            return Err(format!(
                "Room {i} dimensions exceed maximum limits (max 2000x2000x50m)"
            ));
        }
    }

    for (i, w) in level.walls.iter().enumerate() {
        if !w.x.is_finite()
            || !w.y.is_finite()
            || !w.z.is_finite()
            || !w.width.is_finite()
            || !w.depth.is_finite()
        {
            return Err(format!("Wall {i} dimensions must be finite numbers"));
        }
        if let Some(h) = w.height {
            if !h.is_finite() {
                return Err(format!("Wall {i} dimensions must be finite numbers"));
            }
            if h <= 0.0 {
                return Err(format!(
                    "Wall {i} width, depth, and height must be positive"
                ));
            }
        }
        if w.width <= 0.0 || w.depth <= 0.0 {
            return Err(format!(
                "Wall {i} width, depth, and height must be positive"
            ));
        }

        // Openings are optional cutouts; openings that do not overlap the
        // wall's vertical range are allowed (they simply produce no cut).
        for (j, opening) in w.openings.iter().enumerate() {
            if !opening.offset.is_finite()
                || !opening.width.is_finite()
                || !opening.height.is_finite()
                || !opening.sill.is_finite()
            {
                return Err(format!("Wall {i} opening {j} contains non-finite numbers"));
            }
            if opening.width <= 0.0 || opening.height <= 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} must have a positive width and height"
                ));
            }
            if opening.sill < 0.0 {
                return Err(format!(
                    "Wall {i} opening {j} cannot have a negative sill height"
                ));
            }
            if opening.offset < 0.0 {
                return Err(format!("Wall {i} opening {j} starts before the wall"));
            }
            if opening.end() > w.length() + 1e-3 {
                let prefix = if opening.is_door() {
                    "Door opening"
                } else if opening.kind == "window" {
                    "Window opening"
                } else {
                    "Opening"
                };
                return Err(format!(
                    "{prefix} extends beyond this wall (wall {i}: opening ends at {:.2} m, wall is {:.2} m long)",
                    opening.end(),
                    w.length()
                ));
            }
        }
    }

    for (i, light) in level.ceiling_lights.iter().enumerate() {
        if !light.x.is_finite() || !light.z.is_finite() || !light.rotation_degrees.is_finite() {
            return Err(format!(
                "Ceiling light {i} position and rotation must be finite numbers"
            ));
        }
        // The optional fixture intensity (`brightness`, also accepted as
        // `intensity`). Omitted means the standard 1.0 fixture; negative or
        // non-finite values are rejected, high values are clamped while baking.
        if let Some(brightness) = light.brightness {
            if !brightness.is_finite() {
                return Err(format!(
                    "Ceiling light {i} intensity must be a finite number"
                ));
            }
            if brightness < 0.0 {
                return Err(format!("Ceiling light {i} intensity cannot be negative"));
            }
        }
    }

    for (i, prop) in level.props.iter().enumerate() {
        if prop.model.trim().is_empty() {
            return Err(format!("Prop {i} must reference a non-empty model id"));
        }
        if !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.scale.is_finite()
        {
            return Err(format!(
                "Prop {i} position, rotation, and scale must be finite numbers"
            ));
        }
        if prop.scale <= 0.0 {
            return Err(format!("Prop {i} scale must be positive"));
        }
        if let Some(size) = prop.size
            && !size.iter().all(|v| v.is_finite() && *v > 0.0)
        {
            return Err(format!(
                "Prop {i} size must contain positive finite numbers"
            ));
        }
    }

    // 5. Generated-geometry complexity budget, checked after per-element
    //    validation so dimension errors take precedence. This bounds the vertex
    //    buffer built at load time, protecting the ~512 MB PocketCHIP from
    //    levels that would otherwise exhaust memory. Overlapping/intersecting
    //    geometry is explicitly allowed and is not validated here.
    let estimate = level.estimate_geometry();
    if estimate.floor_area_m2 > MAX_LEVEL_FLOOR_AREA_M2 {
        return Err(format!(
            "Level floor area is too large: {} m^2 (limit: {MAX_LEVEL_FLOOR_AREA_M2} m^2). \
             Use smaller or fewer room sections.",
            estimate.floor_area_m2
        ));
    }
    if estimate.total_vertices > MAX_LEVEL_VERTICES {
        return Err(format!(
            "Level geometry is too complex: ~{} vertices (limit: {MAX_LEVEL_VERTICES}). \
             Reduce rooms, walls or ceiling lights.",
            estimate.total_vertices
        ));
    }

    Ok(())
}

/// Resolves materials and textures for a level.
/// Missing custom resources fail gracefully with an obvious fallback rather than crashing.
pub fn resolve_textures(
    level: &LevelDef,
    material_map: &HashMap<String, String>,
    texture_blobs: &HashMap<String, Rc<[u8]>>,
) -> LoadedTextures {
    let mut loaded = LoadedTextures::default();

    let try_decode_material = |mat_id: &str| -> Option<RawImage> {
        if !mat_id.starts_with("pack:") {
            return None;
        }
        // 1. Look up mapped path in material_map
        if let Some(path) = material_map.get(mat_id) {
            let normalized = path.replace('\\', "/");
            let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
            if let Some(bytes) = texture_blobs
                .get(&normalized)
                .or_else(|| texture_blobs.get(file_name))
                && let Ok(img) = decode_png(bytes)
            {
                return Some(img);
            }
        }
        // 2. Direct lookup by material name
        let name = mat_id.strip_prefix("pack:").unwrap_or(mat_id);
        let candidates = [
            format!("textures/{name}.png"),
            format!("{name}.png"),
            format!("textures/{name}"),
            name.to_string(),
        ];
        for cand in &candidates {
            if let Some(bytes) = texture_blobs.get(cand)
                && let Ok(img) = decode_png(bytes)
            {
                return Some(img);
            }
        }
        None
    };

    // Wall texture
    if let Some(img) = try_decode_material(&level.defaults.wall) {
        loaded.wall = img;
    } else if level.defaults.wall == "core:wallpaper_stained_01" {
        loaded.wall = RawImage::new(128, 128, generate_stained_wall_texture().to_vec());
    }

    // Floor texture
    if let Some(img) = try_decode_material(&level.defaults.floor) {
        loaded.floor = img;
    } else if level.defaults.floor == "core:carpet_damp_01" {
        loaded.floor = RawImage::new(64, 64, generate_damp_carpet_texture().to_vec());
    }

    // Ceiling texture
    if let Some(img) = try_decode_material(&level.defaults.ceiling) {
        loaded.ceiling = img;
    } else if level.defaults.ceiling == "core:ceiling_stained_01" {
        loaded.ceiling = RawImage::new(128, 128, generate_stained_ceiling_texture().to_vec());
    }

    // Fixture texture
    for light in &level.ceiling_lights {
        if let Some(img) = try_decode_material(&light.fixture) {
            loaded.fixture = img;
            break;
        }
    }

    loaded
}

/// Unified level loader and package manager.
pub struct LevelManager {
    assets_dir: PathBuf,
    levels_dir: PathBuf,
    import_dir: PathBuf,
    entries: Vec<LevelEntry>,
    prop_catalog: PropCatalog,
}

impl Default for LevelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelManager {
    pub fn new() -> Self {
        let mut manager = Self {
            assets_dir: PathBuf::from("assets/levels"),
            levels_dir: PathBuf::from("levels"),
            import_dir: PathBuf::from("import"),
            entries: Vec::new(),
            prop_catalog: PropCatalog::load_default(),
        };
        manager.refresh();
        manager
    }

    pub fn with_paths(assets_dir: PathBuf, levels_dir: PathBuf, import_dir: PathBuf) -> Self {
        let mut manager = Self {
            assets_dir,
            levels_dir,
            import_dir,
            entries: Vec::new(),
            prop_catalog: PropCatalog::load_default(),
        };
        manager.refresh();
        manager
    }

    pub fn entries(&self) -> &[LevelEntry] {
        &self.entries
    }

    /// Prop catalog used to resolve placed props.
    pub fn prop_catalog(&self) -> &PropCatalog {
        &self.prop_catalog
    }

    pub fn get_entry(&self, idx: usize) -> Option<&LevelEntry> {
        self.entries.get(idx)
    }

    /// Re-scans directories for installed levels.
    pub fn refresh(&mut self) {
        let mut discovered = Vec::new();

        // 1. Official levels in assets_dir
        if let Ok(dir) = fs::read_dir(&self.assets_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json")
                    && let Ok(meta) = self.probe_level_file(&p, LevelSourceType::Official)
                {
                    discovered.push(meta);
                }
            }
        }

        // 2. Installed community / custom levels in levels_dir
        if let Ok(dir) = fs::read_dir(&self.levels_dir) {
            for entry in dir.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(meta) = self.probe_level_file(&p, LevelSourceType::CustomJson) {
                        discovered.push(meta);
                    }
                } else if p.extension().is_some_and(|ext| ext == "zip")
                    && let Ok(meta) = self.probe_zip_file(&p)
                {
                    discovered.push(meta);
                }
            }
        }

        // If level_1 was not found on disk, insert fallback official entry
        if !discovered.iter().any(|e| e.id == "level_1") {
            discovered.insert(
                0,
                LevelEntry {
                    id: "level_1".into(),
                    name: "Level 1".into(),
                    author: "Liminal Team".into(),
                    source_type: LevelSourceType::Official,
                    path: self.assets_dir.join("level1.json"),
                },
            );
        } else {
            // Ensure level_1 is first in the list for immediate access
            if let Some(pos) = discovered.iter().position(|e| e.id == "level_1")
                && pos != 0
            {
                let e = discovered.remove(pos);
                discovered.insert(0, e);
            }
        }

        self.entries = discovered;
    }

    fn probe_level_file(
        &self,
        path: &Path,
        source_type: LevelSourceType,
    ) -> Result<LevelEntry, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let level = LevelDef::from_json(&content).map_err(|e| e.to_string())?;
        validate_level(&level)?;
        Ok(LevelEntry {
            id: level.id,
            name: level.name,
            author: level.author,
            source_type,
            path: path.to_path_buf(),
        })
    }

    fn probe_zip_file(&self, path: &Path) -> Result<LevelEntry, String> {
        // Lightweight probe: read only level.json, no texture extraction.
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let level_json = read_zip_level_json(file)?;
        let level = LevelDef::from_json(&level_json).map_err(|e| e.to_string())?;
        validate_level(&level)?;
        Ok(LevelEntry {
            id: level.id,
            name: level.name,
            author: level.author,
            source_type: LevelSourceType::PackZip,
            path: path.to_path_buf(),
        })
    }

    /// Loads the default or initial Level 1 level through the unified loader.
    pub fn load_default_or_level1(&self) -> Result<LoadedLevel, String> {
        if let Some(entry) = self.entries.iter().find(|e| e.id == "level_1") {
            self.load_level(entry)
        } else {
            // Direct fallback
            let level = LevelDef::from_json(FALLBACK_LEVEL1_JSON)
                .map_err(|e| format!("Failed to parse embedded Level 1: {e}"))?;
            validate_level(&level)?;
            let textures = resolve_textures(&level, &HashMap::new(), &HashMap::new());
            Ok(LoadedLevel {
                level,
                textures,
                entry: LevelEntry {
                    id: "level_1".into(),
                    name: "Level 1".into(),
                    author: "Liminal Team".into(),
                    source_type: LevelSourceType::Official,
                    path: self.assets_dir.join("level1.json"),
                },
            })
        }
    }

    /// Unified level loader loading any standalone JSON or packaged ZIP level.
    pub fn load_level(&self, entry: &LevelEntry) -> Result<LoadedLevel, String> {
        match entry.source_type {
            LevelSourceType::Official | LevelSourceType::CustomJson => {
                let content = if entry.path.exists() {
                    fs::read_to_string(&entry.path)
                        .map_err(|e| format!("Failed to read {}: {e}", entry.path.display()))?
                } else if entry.id == "level_1" {
                    FALLBACK_LEVEL1_JSON.to_string()
                } else {
                    return Err(format!("Level file not found: {}", entry.path.display()));
                };

                let level = LevelDef::from_json(&content)
                    .map_err(|e| format!("JSON parse error in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;
                let textures = resolve_textures(&level, &HashMap::new(), &HashMap::new());

                Ok(LoadedLevel {
                    level,
                    textures,
                    entry: entry.clone(),
                })
            }
            LevelSourceType::PackZip => {
                let file = fs::File::open(&entry.path)
                    .map_err(|e| format!("Failed to open {}: {e}", entry.path.display()))?;
                let pack = extract_zip(file)?;
                let level = LevelDef::from_json(&pack.level_json)
                    .map_err(|e| format!("Invalid level.json in {}: {e}", entry.path.display()))?;
                validate_level(&level)?;

                let material_map = pack
                    .materials_json
                    .as_deref()
                    .map(parse_materials_json)
                    .unwrap_or_default();
                let textures = resolve_textures(&level, &material_map, &pack.textures);

                Ok(LoadedLevel {
                    level,
                    textures,
                    entry: entry.clone(),
                })
            }
        }
    }

    /// Imports an external .json or .zip file into the installed levels directory.
    pub fn import_file(&mut self, source_path: &Path) -> Result<LevelEntry, String> {
        if !source_path.exists() {
            return Err(format!(
                "Source file does not exist: {}",
                source_path.display()
            ));
        }

        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 1. Dry-run validate before copying
        let file_name = source_path
            .file_name()
            .ok_or_else(|| "Invalid file name".to_string())?;

        let (level_id, level_name, author, source_type) = if ext == "json" {
            let content = fs::read_to_string(source_path)
                .map_err(|e| format!("Failed to read {}: {e}", source_path.display()))?;
            let level =
                LevelDef::from_json(&content).map_err(|e| format!("Invalid level JSON: {e}"))?;
            validate_level(&level)?;
            (
                level.id,
                level.name,
                level.author,
                LevelSourceType::CustomJson,
            )
        } else if ext == "zip" {
            // Validate cheaply from level.json only; textures are not needed to
            // decide whether the pack is acceptable.
            let file = fs::File::open(source_path)
                .map_err(|e| format!("Failed to open {}: {e}", source_path.display()))?;
            let level_json = read_zip_level_json(file)?;
            let level = LevelDef::from_json(&level_json)
                .map_err(|e| format!("Invalid level.json in ZIP: {e}"))?;
            validate_level(&level)?;
            (level.id, level.name, level.author, LevelSourceType::PackZip)
        } else {
            return Err(format!(
                "Unsupported file format '.{ext}'. Supported formats: .json, .zip"
            ));
        };

        // 2. Copy file to levels_dir
        fs::create_dir_all(&self.levels_dir)
            .map_err(|e| format!("Failed to create levels directory: {e}"))?;
        let target_path = self.levels_dir.join(file_name);
        if source_path != target_path {
            fs::copy(source_path, &target_path)
                .map_err(|e| format!("Failed to copy file to {}: {e}", target_path.display()))?;
        }

        self.refresh();

        Ok(LevelEntry {
            id: level_id,
            name: level_name,
            author,
            source_type,
            path: target_path,
        })
    }

    /// Scans import_dir and candidate locations for unimported .json or .zip files and imports them.
    pub fn import_available(&mut self) -> Result<usize, String> {
        let _ = fs::create_dir_all(&self.import_dir);
        let _ = fs::create_dir_all(&self.levels_dir);

        let mut imported_count = 0;
        let candidate_dirs = [self.import_dir.clone(), PathBuf::from("levels/import")];

        for dir in &candidate_dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if (ext == "json" || ext == "zip") && self.import_file(&path).is_ok() {
                        imported_count += 1;
                    }
                }
            }
        }

        Ok(imported_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{RoomDef, WallAxis, WallDef};

    #[test]
    fn test_validate_level_success() {
        let level = LevelDef {
            format_version: 1,
            id: "test_level".into(),
            name: "Test Level".into(),
            author: "Author".into(),
            room: None,
            rooms: vec![RoomDef {
                x: 0.0,
                z: 0.0,
                width: 20.0,
                depth: 20.0,
                height: 3.5,
                material: None,
                ceiling_material: None,
            }],
            spawn: crate::level::SpawnDef {
                x: 5.0,
                z: 5.0,
                yaw_degrees: 0.0,
            },
            defaults: Default::default(),
            walls: vec![WallDef {
                x: 10.0,
                y: 0.0,
                z: 10.0,
                width: 2.0,
                depth: 1.0,
                height: Some(3.5),
                faces: HashMap::new(),
                openings: Vec::new(),
                material: None,
            }],
            floor_patches: vec![],
            ceiling_lights: vec![],
            props: vec![],
        };
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_validate_level_invalid_version() {
        let level = LevelDef {
            format_version: 2,
            id: "test".into(),
            name: "Test".into(),
            author: "".into(),
            room: None,
            rooms: vec![],
            spawn: crate::level::SpawnDef {
                x: 0.0,
                z: 0.0,
                yaw_degrees: 0.0,
            },
            defaults: Default::default(),
            walls: vec![],
            floor_patches: vec![],
            ceiling_lights: vec![],
            props: vec![],
        };
        assert!(validate_level(&level).is_err());
    }

    #[test]
    fn test_validate_level_preserves_overlapping_geometry() {
        // Overlapping walls and rooms are explicitly legal
        let level = LevelDef {
            format_version: 1,
            id: "overlap".into(),
            name: "Overlap".into(),
            author: "".into(),
            room: None,
            rooms: vec![
                RoomDef {
                    x: 0.0,
                    z: 0.0,
                    width: 10.0,
                    depth: 10.0,
                    height: 3.5,
                    material: None,
                    ceiling_material: None,
                },
                RoomDef {
                    x: 5.0,
                    z: 5.0,
                    width: 10.0,
                    depth: 10.0,
                    height: 3.5,
                    material: None,
                    ceiling_material: None,
                },
            ],
            spawn: crate::level::SpawnDef {
                x: 1.0,
                z: 1.0,
                yaw_degrees: 0.0,
            },
            defaults: Default::default(),
            walls: vec![
                WallDef {
                    x: 2.0,
                    y: 0.0,
                    z: 2.0,
                    width: 4.0,
                    depth: 4.0,
                    height: Some(3.5),
                    faces: HashMap::new(),
                    openings: Vec::new(),
                    material: None,
                },
                WallDef {
                    x: 3.0,
                    y: 0.0,
                    z: 3.0,
                    width: 4.0,
                    depth: 4.0,
                    height: Some(3.5),
                    faces: HashMap::new(),
                    openings: Vec::new(),
                    material: None,
                },
            ],
            floor_patches: vec![],
            ceiling_lights: vec![],
            props: vec![],
        };
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_parse_materials_json() {
        let json = r#"{
            "materials": {
                "pack:custom_wall": {
                    "texture": "textures/my_wall.png"
                },
                "pack:carpet_gray": "textures/carpet.png"
            }
        }"#;
        let map = parse_materials_json(json);
        assert_eq!(
            map.get("pack:custom_wall"),
            Some(&"textures/my_wall.png".to_string())
        );
        assert_eq!(
            map.get("pack:carpet_gray"),
            Some(&"textures/carpet.png".to_string())
        );
    }

    #[test]
    fn test_zip_extraction_and_path_traversal_rejection() {
        use zip::write::SimpleFileOptions;

        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            writer.start_file("../evil.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"evil").unwrap();
            writer.finish().unwrap();
        }

        let result = extract_zip(Cursor::new(&buffer));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unsafe path traversal"));
    }

    #[test]
    fn test_zip_level_pack_extraction() {
        use zip::write::SimpleFileOptions;

        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            let level_json = r#"{
                "format_version": 1,
                "id": "zip_test",
                "name": "Zip Test Level",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }]
            }"#;

            writer.start_file("level.json", options).unwrap();
            std::io::Write::write_all(&mut writer, level_json.as_bytes()).unwrap();

            let materials_json = r#"{
                "pack:wall": "textures/wall.png"
            }"#;
            writer.start_file("materials.json", options).unwrap();
            std::io::Write::write_all(&mut writer, materials_json.as_bytes()).unwrap();

            writer.start_file("textures/wall.png", options).unwrap();
            std::io::Write::write_all(&mut writer, b"mock_png_bytes").unwrap();

            writer.finish().unwrap();
        }

        let pack = extract_zip(Cursor::new(&buffer)).expect("valid zip");
        assert!(pack.level_json.contains("zip_test"));
        assert!(pack.materials_json.is_some());
        assert!(pack.textures.contains_key("textures/wall.png"));
    }

    #[test]
    fn test_resource_fallback_for_missing_custom_texture() {
        let level = LevelDef {
            format_version: 1,
            id: "fallback_test".into(),
            name: "Fallback Test".into(),
            author: "".into(),
            room: None,
            rooms: vec![],
            spawn: crate::level::SpawnDef {
                x: 0.0,
                z: 0.0,
                yaw_degrees: 0.0,
            },
            defaults: crate::level::LevelDefaults {
                wall: "pack:missing_wall".into(),
                floor: "pack:missing_carpet".into(),
                ceiling: "core:ceiling_panel_01".into(),
            },
            walls: vec![],
            floor_patches: vec![],
            ceiling_lights: vec![],
            props: vec![],
        };

        // No custom textures supplied -> should gracefully fallback to the
        // built-in sheets (wallpaper and ceiling cover two metres per repeat,
        // the carpet one, all at 64 texels per metre).
        let textures = resolve_textures(&level, &HashMap::new(), &HashMap::new());
        assert_eq!(textures.wall.width, 128);
        assert_eq!(textures.floor.width, 64);
        assert_eq!(textures.ceiling.width, 128);
        assert_eq!(
            (textures.ceiling.width, textures.ceiling.height),
            (128, 128)
        );
        assert_eq!(
            textures.ceiling.rgba.len(),
            (textures.ceiling.width * textures.ceiling.height * 4) as usize
        );
    }

    /// The built-in material variants resolve to distinct images, so a level
    /// that asks for the water-damaged surfaces really draws them.
    #[test]
    fn test_damaged_material_variants_resolve() {
        let checksum = |image: &RawImage| -> u64 {
            image
                .rgba
                .iter()
                .enumerate()
                .fold(0x811c_9dc5u64, |hash, (index, byte)| {
                    (hash ^ (*byte as u64 + index as u64)).wrapping_mul(0x0100_0000_01b3)
                })
        };
        // (field, maintained id, damaged id)
        let variants = [
            (
                0usize,
                "core:wallpaper_yellow_01",
                "core:wallpaper_stained_01",
            ),
            (1, "core:carpet_beige_01", "core:carpet_damp_01"),
            (2, "core:ceiling_panel_01", "core:ceiling_stained_01"),
        ];
        for (field, maintained, damaged) in variants {
            let sample = |material: &str| {
                let mut level = LevelDef::from_json(
                    r#"{"format_version": 1, "id": "x", "name": "x", "spawn": {"x": 0.0, "z": 0.0}}"#,
                )
                .expect("minimal level");
                match field {
                    0 => level.defaults.wall = material.into(),
                    1 => level.defaults.floor = material.into(),
                    _ => level.defaults.ceiling = material.into(),
                }
                let textures = resolve_textures(&level, &HashMap::new(), &HashMap::new());
                let image = match field {
                    0 => textures.wall,
                    1 => textures.floor,
                    _ => textures.ceiling,
                };
                (image.width, image.height, checksum(&image))
            };
            let plain = sample(maintained);
            let worn = sample(damaged);
            assert_ne!(
                plain, worn,
                "{damaged} must resolve to its own sheet, not {maintained}'s"
            );
        }
    }

    fn level_from_rooms_json(rooms_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "budget_test",
                "name": "Budget Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": {rooms_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    #[test]
    fn test_validate_accepts_shipped_level1() {
        let level = LevelDef::from_json(include_str!("../assets/levels/level1.json"))
            .expect("valid level1");
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_validate_accepts_moderately_large_level() {
        // 10 rooms of 100x100 m = 100,000 m^2, comfortably under the budget.
        let rooms: Vec<String> = (0..10)
            .map(|i| {
                format!(
                    r#"{{ "x": {}, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }}"#,
                    i as f32 * 100.0
                )
            })
            .collect();
        let level = level_from_rooms_json(&format!("[{}]", rooms.join(",")));
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_validate_rejects_pathological_huge_room() {
        let level = level_from_rooms_json(
            r#"[{ "x": 0.0, "z": 0.0, "width": 2000.0, "depth": 2000.0, "height": 3.5 }]"#,
        );
        let err = validate_level(&level).expect_err("huge room must be rejected");
        assert!(
            err.contains("floor area") || err.contains("complex"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_validate_permits_overlapping_rooms_within_budget() {
        // 50 fully-overlapping 100x100 m rooms = 500,000 m^2: overlapping is
        // intentional and allowed, and the total is within budget.
        let rooms: Vec<String> = (0..50)
            .map(|_| {
                r#"{ "x": 0.0, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }"#
                    .to_string()
            })
            .collect();
        let level = level_from_rooms_json(&format!("[{}]", rooms.join(",")));
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_validate_rejects_huge_geometry_without_overflowing() {
        // Finite but absurd dimensions must be handled by saturating arithmetic
        // (no panic/overflow) and rejected.
        let level = level_from_rooms_json(&format!(
            r#"[{{ "x": 0.0, "z": 0.0, "width": {}, "depth": {}, "height": 3.5 }}]"#,
            f32::MAX,
            f32::MAX
        ));
        assert!(validate_level(&level).is_err());
    }

    #[test]
    fn test_read_zip_level_json_only_reads_level_json() {
        use zip::write::SimpleFileOptions;

        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            let level_json = r#"{ "format_version": 1, "id": "probe", "name": "Probe",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0 }] }"#;
            writer.start_file("level.json", options).unwrap();
            std::io::Write::write_all(&mut writer, level_json.as_bytes()).unwrap();

            // A large texture that probing must not read/decompress.
            writer.start_file("textures/huge.png", options).unwrap();
            std::io::Write::write_all(&mut writer, &vec![0u8; 4096]).unwrap();
            writer.finish().unwrap();
        }

        let json = read_zip_level_json(Cursor::new(&buffer)).expect("reads level.json");
        assert!(json.contains("\"probe\""));
    }

    #[test]
    fn test_read_zip_level_json_missing_entry_errors() {
        use zip::write::SimpleFileOptions;

        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("readme.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"no level here").unwrap();
            writer.finish().unwrap();
        }

        assert!(read_zip_level_json(Cursor::new(&buffer)).is_err());
    }

    #[test]
    fn test_extract_zip_shares_texture_blobs_between_aliases() {
        use zip::write::SimpleFileOptions;

        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("level.json", options).unwrap();
            std::io::Write::write_all(&mut writer, b"{}").unwrap();
            writer.start_file("textures/wall.png", options).unwrap();
            std::io::Write::write_all(&mut writer, b"PAYLOAD").unwrap();
            writer.finish().unwrap();
        }

        let pack = extract_zip(Cursor::new(&buffer)).expect("valid zip");
        let full = pack
            .textures
            .get("textures/wall.png")
            .expect("full path alias");
        let bare = pack.textures.get("wall.png").expect("bare name alias");
        assert_eq!(&**full, b"PAYLOAD");
        // Aliases must reference the same physical allocation.
        assert!(Rc::ptr_eq(full, bare));
    }

    fn level_with_opening_json(opening_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "opening_test",
                "name": "Opening Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
                "walls": [{{
                    "x": 0.0, "z": 4.8, "width": 10.0, "depth": 0.4, "height": 3.5,
                    "openings": [{opening_json}]
                }}]
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    fn level_with_props_json(props_json: &str) -> LevelDef {
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "props_test",
                "name": "Props Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
                "props": {props_json}
            }}"#
        );
        LevelDef::from_json(&json).expect("valid json")
    }

    #[test]
    fn test_validate_accepts_valid_door_and_window() {
        let door = level_with_opening_json(
            r#"{ "kind": "door", "offset": 4.0, "width": 1.0, "height": 2.1 }"#,
        );
        assert!(validate_level(&door).is_ok());

        let window = level_with_opening_json(
            r#"{ "kind": "window", "offset": 2.0, "width": 2.0, "height": 1.0, "sill": 1.2 }"#,
        );
        assert!(validate_level(&window).is_ok());
    }

    #[test]
    fn test_validate_rejects_door_beyond_wall() {
        let level = level_with_opening_json(
            r#"{ "kind": "door", "offset": 9.5, "width": 1.0, "height": 2.1 }"#,
        );
        let err = validate_level(&level).expect_err("door must not extend past the wall");
        assert!(
            err.starts_with("Door opening extends beyond this wall"),
            "unexpected error: {err}"
        );
        assert!(err.contains("wall 0"), "missing wall index: {err}");
    }

    #[test]
    fn test_validate_rejects_window_and_unknown_opening_beyond_wall() {
        let window = level_with_opening_json(
            r#"{ "kind": "window", "offset": 9.0, "width": 1.5, "height": 1.0, "sill": 1.0 }"#,
        );
        let err = validate_level(&window).expect_err("window must not extend past the wall");
        assert!(
            err.starts_with("Window opening extends beyond this wall"),
            "unexpected error: {err}"
        );

        let vent = level_with_opening_json(
            r#"{ "kind": "vent", "offset": 9.0, "width": 2.0, "height": 0.4 }"#,
        );
        let err = validate_level(&vent).expect_err("vent must not extend past the wall");
        assert!(
            err.starts_with("Opening extends beyond this wall"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_validate_rejects_invalid_opening_numbers() {
        let negative_sill = level_with_opening_json(
            r#"{ "kind": "window", "offset": 1.0, "width": 1.0, "height": 1.0, "sill": -0.5 }"#,
        );
        let err = validate_level(&negative_sill).expect_err("negative sill is invalid");
        assert!(
            err.contains("cannot have a negative sill height"),
            "unexpected error: {err}"
        );

        let negative_offset = level_with_opening_json(
            r#"{ "kind": "door", "offset": -1.0, "width": 1.0, "height": 2.1 }"#,
        );
        let err = validate_level(&negative_offset).expect_err("negative offset is invalid");
        assert!(
            err.contains("starts before the wall"),
            "unexpected error: {err}"
        );

        let zero_width = level_with_opening_json(
            r#"{ "kind": "door", "offset": 1.0, "width": 0.0, "height": 2.1 }"#,
        );
        let err = validate_level(&zero_width).expect_err("zero width is invalid");
        assert!(
            err.contains("must have a positive width and height"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_validate_accepts_sunk_and_intersecting_props() {
        // A prop sunk into the floor and a prop embedded in a wall are both
        // intentional and must not be rejected.
        let level = level_with_props_json(
            r#"[
                { "model": "core:rug", "x": 2.0, "y": -0.02, "z": 2.0 },
                { "model": "core:bookshelf", "x": 0.0, "y": 0.0, "z": 4.8, "scale": 1.2, "solid": true }
            ]"#,
        );
        assert!(validate_level(&level).is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_props() {
        let empty_model = level_with_props_json(r#"[{ "model": "  ", "x": 0.0, "z": 0.0 }]"#);
        assert!(
            validate_level(&empty_model)
                .expect_err("empty model id is invalid")
                .contains("model id")
        );

        let non_finite =
            level_with_props_json(r#"[{ "model": "core:crate", "x": 1.0, "z": 0.0 }]"#);
        let mut non_finite = non_finite;
        non_finite.props[0].x = f32::INFINITY;
        assert!(
            validate_level(&non_finite)
                .expect_err("infinite x is invalid")
                .contains("finite")
        );

        let bad_scale = level_with_props_json(
            r#"[{ "model": "core:crate", "scale": 0.0, "x": 0.0, "z": 0.0 }]"#,
        );
        assert!(
            validate_level(&bad_scale)
                .expect_err("zero scale is invalid")
                .contains("scale must be positive")
        );

        let bad_size = level_with_props_json(
            r#"[{ "model": "core:crate", "x": 0.0, "z": 0.0, "size": [1.0, -1.0, 1.0] }]"#,
        );
        assert!(
            validate_level(&bad_size)
                .expect_err("negative size is invalid")
                .contains("size must contain positive finite numbers")
        );
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(
            parse_hex_color("#6b5f4a"),
            Some([
                0x6b as f32 / 255.0,
                0x5f as f32 / 255.0,
                0x4a as f32 / 255.0
            ])
        );
        // The leading '#' is optional and plain greys parse too.
        assert_eq!(parse_hex_color("8a8a8a"), parse_hex_color("#8A8A8A"));
        assert_eq!(parse_hex_color("#fff"), None);
        assert_eq!(parse_hex_color("not-a-colour"), None);
    }

    #[test]
    fn test_prop_catalog_from_json_str() {
        let json = r##"{
            "format_version": 1,
            "props": [
                { "id": "core:couch", "name": "Couch", "category": "Furniture",
                  "size": [2.0, 0.9, 0.9], "color": "#6b5f4a", "model": null, "solid": true },
                { "id": "core:lamp" }
            ]
        }"##;
        let catalog = PropCatalog::from_json_str(json).expect("valid catalog");
        assert_eq!(catalog.len(), 2);
        assert!(!catalog.is_empty());

        let couch = catalog.get("core:couch");
        assert_eq!(couch.name, "Couch");
        assert_eq!(couch.category, "Furniture");
        assert_eq!(couch.size, [2.0, 0.9, 0.9]);
        assert_eq!(couch.color, parse_hex_color("#6b5f4a").unwrap());
        assert_eq!(couch.model, None);
        assert!(couch.solid);

        // Missing fields default to the neutral values.
        let lamp = catalog.get("core:lamp");
        assert_eq!(lamp.name, "core:lamp");
        assert_eq!(lamp.category, "Other");
        assert_eq!(lamp.size, crate::level::PROP_FALLBACK_SIZE);
        assert_eq!(lamp.color, parse_hex_color("#8a8a8a").unwrap());
        assert!(!lamp.solid);

        assert!(PropCatalog::from_json_str("not json").is_err());
    }

    #[test]
    fn test_prop_catalog_falls_back_for_unknown_model() {
        let catalog = PropCatalog::builtin();
        assert!(catalog.is_empty());
        let fallback = catalog.get("core:does_not_exist");
        assert_eq!(fallback.name, "core:does_not_exist");
        assert_eq!(fallback.category, "Other");
        assert_eq!(fallback.size, crate::level::PROP_FALLBACK_SIZE);
        assert_eq!(fallback.color, parse_hex_color("#8a8a8a").unwrap());
        assert!(!fallback.solid);
    }

    #[test]
    fn test_shipped_prop_catalog_parses() {
        let catalog = PropCatalog::from_json_str(include_str!("../assets/props/props.json"))
            .expect("shipped props.json must parse");
        assert!(catalog.len() >= 16, "expected at least 16 props");
        let couch = catalog.get("core:couch");
        assert_eq!(couch.name, "Couch");
        assert!(couch.size.iter().all(|v| *v > 0.0));
        assert!(couch.solid);
    }

    #[test]
    fn test_shipped_test_room_demonstrates_openings_and_props() {
        let json = include_str!("../assets/levels/test_room.json");
        let level = LevelDef::from_json(json).expect("valid test_room json");
        validate_level(&level).expect("the shipped sample level validates");

        let kinds: Vec<&str> = level
            .walls
            .iter()
            .flat_map(|wall| wall.openings.iter().map(|opening| opening.kind.as_str()))
            .collect();
        assert!(kinds.contains(&"door"), "the sample level shows a doorway");
        assert!(kinds.contains(&"window"), "the sample level shows a window");
        assert!(
            !level.props.is_empty(),
            "the sample level shows at least one prop"
        );

        // The sample is also a real geometry exercise: openings and props must
        // produce drawable batches.
        let mesh = crate::render::build_level_geometry(&level);
        assert!(mesh.batches.wall_batch.count > 0);
        assert!(mesh.batches.prop_batch.count > 0);
    }

    /// The playable demo map lives in the custom `levels/` folder, so this also
    /// proves it is discovered and loaded through the ordinary level path.
    #[test]
    fn test_asset_demo_level_loads_and_shows_every_asset() {
        let manager = LevelManager::new();
        let entry = manager
            .entries()
            .iter()
            .find(|entry| entry.id == "asset_demo")
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "levels/asset_demo.json is not discovered; found: {:?}",
                    manager
                        .entries()
                        .iter()
                        .map(|e| e.id.as_str())
                        .collect::<Vec<_>>()
                )
            });
        let loaded = manager
            .load_level(&entry)
            .expect("the asset demo level loads");
        let level = &loaded.level;

        // Every catalogue prop (including spooner-man) appears at least once.
        let catalog = PropCatalog::load_default();
        let placed: std::collections::HashSet<&str> =
            level.props.iter().map(|prop| prop.model.as_str()).collect();
        for asset in catalog.entries() {
            assert!(
                placed.contains(asset.id.as_str()),
                "levels/asset_demo.json must place {}",
                asset.id
            );
        }

        // Every structural feature the level format supports is exercised.
        let kinds: std::collections::HashSet<&str> = level
            .walls
            .iter()
            .flat_map(|wall| wall.openings.iter().map(|opening| opening.kind.as_str()))
            .collect();
        for kind in ["door", "window", "passage", "vent"] {
            assert!(
                kinds.contains(kind),
                "the demo map must include a {kind} opening"
            );
        }
        assert!(
            level.ceiling_lights.len() >= 4,
            "the demo map lights every room"
        );
        assert_eq!(
            level.defaults.wall, "core:wallpaper_stained_01",
            "the demo map shows the stained wallpaper variant"
        );
        assert_eq!(level.defaults.floor, "core:carpet_damp_01");

        // Props keep the placement features the editor and game support: a
        // prop standing on another prop (positive vertical offset) and one
        // placed on furniture.
        assert!(
            level
                .props
                .iter()
                .any(|prop| prop.y > 0.3 && prop.model == "core:plant"),
            "the demo map shows a positive vertical offset"
        );
        assert!(
            level
                .props
                .iter()
                .any(|prop| prop.model == "spooner-man" && prop.y > 0.3),
            "spooner-man is placed on the bed, exercising the vertical offset"
        );

        // The level builds real prop geometry, not placeholder boxes.
        let mut assets = crate::props::PropAssets::load_default();
        let (mesh, batches, lighting) =
            crate::render::build_level_geometry_with_assets_and_lighting(
                level,
                &catalog,
                &mut assets,
            );
        assert_eq!(
            mesh.batches.prop_batch.count, 0,
            "no placeholder boxes expected"
        );
        assert!(
            batches.len() >= 18,
            "the demo map draws most of the pack in one batch per model, got {}",
            batches.len()
        );
        assert_eq!(assets.stats().models_failed, 0);

        // Lighting: every fixture is owned exactly once, every room gets a
        // navigable baseline, and the original corridor beats the large rooms
        // it connects.
        assert_eq!(
            lighting.summary().rooms,
            level.room_iter().count(),
            "every room must bake"
        );
        assert_eq!(
            lighting.summary().lights,
            level.ceiling_lights.len(),
            "every fixture must bake"
        );
        assert_eq!(
            lighting
                .rooms()
                .iter()
                .map(|room| room.fixture_count)
                .sum::<usize>(),
            level.ceiling_lights.len(),
            "every fixture must be owned by exactly one room"
        );
        for room in lighting.rooms() {
            assert!(
                room.baseline >= crate::lighting::MIN_AMBIENT
                    && room.baseline <= crate::lighting::MAX_BRIGHTNESS,
                "baseline {} out of range",
                room.baseline
            );
            assert!(room.baseline.is_finite());
        }
        let corridor = &lighting.rooms()[4];
        for room in &lighting.rooms()[..4] {
            assert!(
                corridor.baseline > room.baseline,
                "the corridor ({}) should read brighter than a room ({})",
                corridor.baseline,
                room.baseline
            );
        }
        // A fixture casts a local pool: directly beneath a corridor panel is
        // brighter than the corridor's baseline.
        let beneath = lighting.sample(-7.0, 0.0, 0.0);
        assert!(
            beneath > corridor.baseline + 0.02,
            "expected a visible pool beneath the panel: {beneath} vs {}",
            corridor.baseline
        );
        // Doorway blending: the two sides of the living-room door (25 cm apart,
        // on opposite sides of the wall) read nearly the same, even though the
        // two rooms' baselines differ by enough to show a seam without blending.
        let living = lighting.rooms()[0].baseline;
        assert!(
            (corridor.baseline - living).abs() > 0.05,
            "the demo's rooms and corridor should differ enough to prove blending"
        );
        let living_side = lighting.sample(-13.4, 0.0, -1.9);
        let corridor_side = lighting.sample(-13.4, 0.0, -1.6);
        assert!(
            (living_side - corridor_side).abs() < 0.05,
            "the doorway seam should be blended, got {living_side} vs {corridor_side}"
        );

        // -------------------------------------------------------------------
        // The lighting demonstration wing: each showcase must actually differ.
        // -------------------------------------------------------------------
        let room_at = |x: f32, z: f32| {
            lighting
                .rooms()
                .iter()
                .position(|room| (room.x0 - x).abs() < 1e-3 && (room.z0 - z).abs() < 1e-3)
                .unwrap_or_else(|| panic!("no room with its minimum corner at ({x}, {z})"))
        };

        // Light density: identical rooms with 0, 1, 2 and 4 fixtures must get
        // strictly brighter in that order.
        let density: Vec<usize> = [6.4_f32, 10.4, 14.4, 18.4]
            .iter()
            .map(|x| room_at(*x, 9.2))
            .collect();
        let counts: Vec<usize> = density
            .iter()
            .map(|index| lighting.rooms()[*index].fixture_count)
            .collect();
        assert_eq!(counts, vec![0, 1, 2, 4], "density row fixture counts");
        for pair in density.windows(2) {
            assert!(
                lighting.rooms()[pair[0]].baseline < lighting.rooms()[pair[1]].baseline,
                "the density row must brighten eastwards: {:?} vs {:?}",
                lighting.rooms()[pair[0]].baseline,
                lighting.rooms()[pair[1]].baseline
            );
        }

        // Fixture intensity: same room, same single fixture, 0.5 vs 1.8.
        let weak = room_at(6.4, 0.8);
        let strong = room_at(10.4, 0.8);
        assert_eq!(lighting.rooms()[weak].fixture_count, 1);
        assert_eq!(lighting.rooms()[strong].fixture_count, 1);
        assert!(
            lighting.rooms()[strong].baseline > lighting.rooms()[weak].baseline + 0.05,
            "the strong fixture room ({}) must clearly beat the weak one ({})",
            lighting.rooms()[strong].baseline,
            lighting.rooms()[weak].baseline
        );

        // Ceiling height: same area and fixtures, 2.6 m vs 4.2 m. The lower
        // room reads brighter and its pool beneath the fixture is stronger.
        let low = room_at(14.4, 0.8);
        let tall = room_at(18.4, 0.8);
        assert_eq!(
            lighting.rooms()[low].fixture_count,
            lighting.rooms()[tall].fixture_count
        );
        assert!(
            lighting.rooms()[low].baseline > lighting.rooms()[tall].baseline,
            "the lower room ({}) must beat the taller one ({})",
            lighting.rooms()[low].baseline,
            lighting.rooms()[tall].baseline
        );
        // Matched corners beside each room's outer wall: the 2.6 m room's pool
        // is strong enough to reach the cap while the 4.2 m room's floor at the
        // same relative point stays visibly below it.
        let low_corner = lighting.sample(14.6, 0.0, 1.2);
        let tall_corner = lighting.sample(22.2, 0.0, 1.2);
        assert!(
            low_corner > tall_corner + 0.05,
            "the 2.6 m room ({low_corner}) must clearly out-light the 4.2 m one ({tall_corner})"
        );

        // Doorway bleed: the bright room (four fixtures) against the dark room
        // (none), joined by a wide doorway. Standing just inside the dark room
        // the doorway must lift the tone; deep in its corner it stays dark.
        let bright_room = room_at(40.8, 4.0);
        let dark_room = room_at(40.8, -2.0);
        assert_eq!(lighting.rooms()[dark_room].fixture_count, 0);
        assert_eq!(
            lighting.rooms()[dark_room].baseline,
            crate::lighting::MIN_AMBIENT
        );
        let at_door = lighting.sample_in_room(dark_room, 43.8, 0.0, 3.6);
        let corner = lighting.sample_in_room(dark_room, 41.4, 0.0, -1.4);
        assert!(
            at_door > lighting.rooms()[dark_room].baseline + 0.02,
            "the doorway must spill light into the dark room: {at_door}"
        );
        assert!(
            (corner - lighting.rooms()[dark_room].baseline).abs() < 0.02,
            "the dark room's far corner must stay dim: {corner}"
        );
        assert!(
            at_door <= crate::lighting::MAX_BRIGHTNESS
                && at_door < lighting.sample_in_room(bright_room, 43.8, 0.0, 7.0) + 0.01,
            "the doorway spill must stay below the bright room itself: {at_door}"
        );

        // Local pools: the wing corridor (room 5) brightens under each of its
        // four spaced fixtures and dips between them.
        let wing_corridor = &lighting.rooms()[5];
        let pool = lighting.sample(25.0, 0.0, 7.0);
        let gap = lighting.sample(31.0, 0.0, 7.0);
        assert!(
            pool > gap + 0.01 && gap > wing_corridor.baseline,
            "spaced fixtures must read as pools: {pool} > {gap} > {}",
            wing_corridor.baseline
        );

        // Prop lighting demonstration: the two-fixture room carries a chair, a
        // crate and a plant, and the bright room one more spooner-man. Every
        // wing prop must sit inside a wing room (x > 6, away from the base map).
        let wing_models: Vec<&str> = level
            .props
            .iter()
            .filter(|prop| prop.x > 6.0)
            .map(|prop| prop.model.as_str())
            .collect();
        assert!(wing_models.contains(&"core:chair"));
        assert!(wing_models.contains(&"core:crate"));
        assert!(wing_models.contains(&"core:plant"));
        assert_eq!(
            wing_models
                .iter()
                .filter(|model| **model == "spooner-man")
                .count(),
            1
        );
        for prop in level.props.iter().filter(|prop| prop.x > 6.0) {
            assert!(
                prop.z > 0.0,
                "wing props stay in the wing's rooms: {prop:?}"
            );
        }

        // The wing is walkable: a player path from the spawn to every
        // comparison room and to both sides of the dark/bright doorway must
        // never intersect a collision box. Sampled at 10 cm steps with the
        // player's radius, this proves the doorways line up with the rooms.
        let walls = level.collision_aabbs();
        let assert_walkable = |points: &[(f32, f32)]| {
            for pair in points.windows(2) {
                let (ax, az) = pair[0];
                let (bx, bz) = pair[1];
                let distance = ((bx - ax).hypot(bz - az) * 10.0).ceil() as i32;
                for step in 0..=distance {
                    let t = step as f32 / distance.max(1) as f32;
                    let x = ax + (bx - ax) * t;
                    let z = az + (bz - az) * t;
                    for wall in &walls {
                        assert!(
                            !wall.intersects_circle(glam::Vec2::new(x, z), 0.3),
                            "player path blocked at ({x:.2}, {z:.2}) by {wall:?}"
                        );
                    }
                }
            }
        };
        // Spawn -> base corridor -> lobby -> wing corridor.
        // Through the corridor doorway (x ~ 0.6) then west of the lobby chairs
        // to the wing door (z ~ 7).
        let mut waypoints = vec![
            (-13.0, 0.0),
            (0.6, 1.0),
            (0.6, 3.0),
            (-1.5, 3.0),
            (-1.5, 7.2),
            (1.0, 7.2),
            (6.2, 7.2),
            (7.0, 7.0),
        ];
        // Every comparison room through its own doorway.
        for x in [8.4_f32, 12.4, 16.4, 20.4] {
            waypoints.push((x, 7.0));
            waypoints.push((x, 11.2));
            waypoints.push((x, 7.0));
            waypoints.push((x, 2.8));
            waypoints.push((x, 7.0));
        }
        // The bright/dark pair.
        waypoints.extend([
            (24.0, 7.0),
            (40.6, 7.0),
            (42.0, 7.0),
            (43.8, 6.0),
            (43.8, 3.0),
            (43.8, 1.0),
        ]);
        assert_walkable(&waypoints);

        use crate::render::SurfaceFamily;

        // Baked colours stay in range, and floors genuinely vary across the demo
        // (the corridor has fixture pools, the rooms have their own).
        let floor = mesh.triangles_for_family(SurfaceFamily::Floor);
        let floor_min = floor.iter().map(|v| v.color[0]).fold(f32::MAX, f32::min);
        let floor_max = floor.iter().map(|v| v.color[0]).fold(f32::MIN, f32::max);
        assert!(floor_max - floor_min > 0.05, "floors must not be flat-lit");
        for vertex in mesh.all_vertices() {
            assert!(vertex.color.iter().all(|c| c.is_finite()));
            assert!(vertex.color.iter().all(|c| (0.0..=1.0).contains(c)));
        }
        // Real prop instances are baked per vertex, and every instance of a
        // model still shares one batch (no extra draw calls for lighting).
        for batch in &batches {
            let min = batch
                .vertices
                .iter()
                .map(|v| v.color[0])
                .fold(f32::MAX, f32::min);
            let max = batch
                .vertices
                .iter()
                .map(|v| v.color[0])
                .fold(f32::MIN, f32::max);
            assert!(max - min > 1e-4, "prop batch {} is flat-lit", batch.model);
        }
    }

    #[test]
    fn test_ceiling_light_intensity_is_optional_and_sanitized() {
        let base = |lights: &str| {
            format!(
                r#"{{
                    "format_version": 1,
                    "id": "intensity",
                    "name": "Intensity",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }},
                    "ceiling_lights": {lights}
                }}"#
            )
        };

        // Backward compatibility: an omitted intensity is the standard fixture.
        let omitted = LevelDef::from_json(&base(
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }]"#,
        ))
        .expect("omitted intensity parses");
        validate_level(&omitted).expect("an omitted intensity validates");
        assert_eq!(omitted.ceiling_lights[0].brightness, None);
        assert_eq!(omitted.ceiling_lights[0].intensity(), 1.0);

        // The editor's `brightness` key and the `intensity` alias both load.
        let both = LevelDef::from_json(&base(
            r#"[
                { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 0.8 },
                { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0, "intensity": 1.4 }
            ]"#,
        ))
        .expect("both spellings parse");
        assert_eq!(both.ceiling_lights[0].intensity(), 0.8);
        assert_eq!(both.ceiling_lights[1].intensity(), 1.4);
        validate_level(&both).expect("authored intensities validate");

        // A negative intensity is malformed data; the loader says so instead of
        // producing negative lighting.
        let negative = LevelDef::from_json(&base(
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": -1.0 }]"#,
        ))
        .expect("negative intensity still parses");
        let error = validate_level(&negative).expect_err("negative intensity is rejected");
        assert!(error.contains("intensity"), "unexpected error: {error}");

        // Non-finite light coordinates are rejected like every other element.
        let mut non_finite = both.clone();
        non_finite.ceiling_lights[0].x = f32::NAN;
        assert!(validate_level(&non_finite).is_err());

        // Very high intensities are allowed through validation (they saturate),
        // but sanitise to a finite, bounded value.
        let high = LevelDef::from_json(&base(
            r#"[{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0, "brightness": 1000.0 }]"#,
        ))
        .expect("high intensity parses");
        validate_level(&high).expect("high intensity still loads");
        assert_eq!(
            high.ceiling_lights[0].intensity(),
            crate::lighting::MAX_LIGHT_INTENSITY
        );
    }

    #[test]
    fn test_shipped_prop_catalog_covers_shipped_levels() {
        let catalog = PropCatalog::load_from_path(Path::new("assets/props/props.json"))
            .expect("assets/props/props.json must load");
        assert!(!catalog.is_empty());

        let mut levels_checked = 0;
        let mut props_checked = 0;
        for entry in fs::read_dir("assets/levels")
            .expect("assets/levels exists")
            .flatten()
        {
            let path = entry.path();
            if path.extension().map(|ext| ext != "json").unwrap_or(true) {
                continue;
            }
            let content = fs::read_to_string(&path).expect("level file is readable");
            let level = LevelDef::from_json(&content)
                .unwrap_or_else(|e| panic!("{} is not a valid level: {e}", path.display()));
            validate_level(&level)
                .unwrap_or_else(|e| panic!("{} failed validation: {e}", path.display()));
            levels_checked += 1;
            for prop in &level.props {
                assert!(
                    catalog.contains(&prop.model),
                    "{} uses prop '{}' which is missing from assets/props/props.json",
                    path.display(),
                    prop.model
                );
                props_checked += 1;
            }
        }
        assert!(levels_checked >= 2, "expected the shipped level files");
        assert!(props_checked >= 1, "expected at least one placed prop");
    }

    /// The three authored residential levels. They share one design idea: the
    /// building is maintained where the player starts and decays the further
    /// they walk, so these checks are about that gradient rather than about any
    /// particular room.
    const RESIDENTIAL_LEVELS: [&str; 3] = ["the_residence", "quiet_apartments", "after_the_leak"];

    const DAMAGED_FLOOR: &str = "core:carpet_damp_01";
    const DAMAGED_CEILING: &str = "core:ceiling_stained_01";
    const DAMAGED_WALL: &str = "core:wallpaper_stained_01";

    fn residential_level(name: &str) -> LevelDef {
        let path = format!("assets/levels/{name}.json");
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
        let level = LevelDef::from_json(&content)
            .unwrap_or_else(|error| panic!("{path} is not a valid level: {error}"));
        validate_level(&level).unwrap_or_else(|error| panic!("{path} failed validation: {error}"));
        level
    }

    /// Walking distance from the spawn to every room, hopping through the
    /// walk-through openings the player can actually use.
    fn walking_distances(level: &LevelDef) -> Vec<f32> {
        let rooms: Vec<&RoomDef> = level.room_iter().collect();
        let centre = |room: &RoomDef| (room.x + room.width * 0.5, room.z + room.depth * 0.5);
        let room_at = |x: f32, z: f32| {
            rooms.iter().position(|room| {
                x >= room.x.min(room.x + room.width) - 0.01
                    && x <= room.x.max(room.x + room.width) + 0.01
                    && z >= room.z.min(room.z + room.depth) - 0.01
                    && z <= room.z.max(room.z + room.depth) + 0.01
            })
        };

        let mut graph: Vec<Vec<usize>> = vec![Vec::new(); rooms.len()];
        for wall in &level.walls {
            let axis = wall.axis();
            let (origin_x, origin_z) = wall.length_origin();
            let crossing = match axis {
                WallAxis::X => wall.z + wall.depth * 0.5,
                WallAxis::Z => wall.x + wall.width * 0.5,
            };
            for opening in &wall.openings {
                if !opening.is_door() || !opening.reaches_floor() || opening.height < 1.9 {
                    continue;
                }
                let start = match axis {
                    WallAxis::X => origin_x + opening.offset,
                    WallAxis::Z => origin_z + opening.offset,
                };
                let end = start + opening.width;

                // Rooms touching this opening's span on the wall's line.
                let mut sides: Vec<(usize, f32, f32)> = Vec::new();
                for (index, room) in rooms.iter().enumerate() {
                    let (position, span_a, span_b, low, high) = match axis {
                        WallAxis::X => (
                            room.z,
                            room.x,
                            room.x + room.width,
                            room.z,
                            room.z + room.depth,
                        ),
                        WallAxis::Z => (
                            room.x,
                            room.z,
                            room.z + room.depth,
                            room.x,
                            room.x + room.width,
                        ),
                    };
                    let _ = position;
                    let (line, side_lo, side_hi) = match axis {
                        WallAxis::X => {
                            let line = if (room.z - crossing).abs() < 0.02 {
                                Some(room.z)
                            } else if (room.z + room.depth - crossing).abs() < 0.02 {
                                Some(room.z + room.depth)
                            } else {
                                None
                            };
                            (line, room.x, room.x + room.width)
                        }
                        WallAxis::Z => {
                            let line = if (room.x - crossing).abs() < 0.02 {
                                Some(room.x)
                            } else if (room.x + room.width - crossing).abs() < 0.02 {
                                Some(room.x + room.width)
                            } else {
                                None
                            };
                            (line, room.z, room.z + room.depth)
                        }
                    };
                    let _ = (span_a, span_b, low, high);
                    if line.is_some() && side_hi.min(end) - side_lo.max(start) > 0.1 {
                        sides.push((index, low, high));
                    }
                }

                for (a_index, a_low, a_high) in &sides {
                    for (b_index, b_low, b_high) in &sides {
                        if a_index == b_index {
                            continue;
                        }
                        let opposite = (*a_high <= crossing + 0.02 && *b_low >= crossing - 0.02)
                            || (*b_high <= crossing + 0.02 && *a_low >= crossing - 0.02);
                        if opposite {
                            graph[*a_index].push(*b_index);
                        }
                    }
                }
            }
        }

        let start = room_at(level.spawn.x, level.spawn.z).expect("the spawn is inside a room");
        let mut distance = vec![f32::NAN; rooms.len()];
        distance[start] = 0.0;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            let (cx, cz) = centre(rooms[current]);
            for neighbour in std::mem::take(&mut graph[current]) {
                if distance[neighbour].is_finite() {
                    continue;
                }
                let (nx, nz) = centre(rooms[neighbour]);
                distance[neighbour] =
                    distance[current] + ((nx - cx).powi(2) + (nz - cz).powi(2)).sqrt();
                queue.push_back(neighbour);
            }
        }
        distance
    }

    /// Fraction of the rooms in one walking-distance band whose floor uses the
    /// damp carpet.
    fn damp_floor_fraction(
        level: &LevelDef,
        distance: &[f32],
        reach: f32,
        low: f32,
        high: f32,
    ) -> (usize, f32) {
        let mut total = 0usize;
        let mut damp = 0usize;
        for (index, room) in level.room_iter().enumerate() {
            if !distance[index].is_finite() {
                continue;
            }
            if distance[index] < reach * low || distance[index] > reach * high {
                continue;
            }
            total += 1;
            let floor = room.material.as_deref().unwrap_or(&level.defaults.floor);
            if floor == DAMAGED_FLOOR {
                damp += 1;
            }
        }
        assert!(total > 0, "band {low}..{high} holds no rooms");
        (total, damp as f32 / total as f32)
    }

    /// Fixtures per room multiplied by their average intensity, so both a
    /// missing fixture and a failing one lower the number.
    fn fixture_strength(
        level: &LevelDef,
        distance: &[f32],
        reach: f32,
        low: f32,
        high: f32,
    ) -> f32 {
        let mut rooms_in_band = 0usize;
        let mut fixtures = 0usize;
        let mut brightness = 0.0f32;
        for (index, room) in level.room_iter().enumerate() {
            if !distance[index].is_finite()
                || distance[index] < reach * low
                || distance[index] > reach * high
            {
                continue;
            }
            rooms_in_band += 1;
            for light in level.ceiling_lights.iter().filter(|light| {
                light.x >= room.x - 0.6
                    && light.x <= room.x + room.width + 0.6
                    && light.z >= room.z - 0.6
                    && light.z <= room.z + room.depth + 0.6
            }) {
                fixtures += 1;
                brightness += light.intensity();
            }
        }
        assert!(rooms_in_band > 0, "band {low}..{high} holds no rooms");
        let average = if fixtures == 0 {
            0.0
        } else {
            brightness / fixtures as f32
        };
        fixtures as f32 / rooms_in_band as f32 * average
    }

    #[test]
    fn the_residential_levels_degrade_with_walking_distance() {
        for name in RESIDENTIAL_LEVELS {
            let level = residential_level(name);
            let distance = walking_distances(&level);
            let reach = distance.iter().copied().fold(0.0f32, f32::max);
            assert!(
                reach >= 45.0,
                "{name}: the far end is only {reach:.0} m of walking from the spawn"
            );
            let unreachable = distance.iter().filter(|d| !d.is_finite()).count();
            assert_eq!(
                unreachable, 0,
                "{name}: {unreachable} room(s) are sealed off"
            );

            // Walking away from the spawn, the carpet gets wet and the light
            // gets worse. Both gradients are what these levels are for.
            let (near_rooms, near_damp) = damp_floor_fraction(&level, &distance, reach, 0.0, 0.25);
            let (far_rooms, far_damp) = damp_floor_fraction(&level, &distance, reach, 0.75, 1.01);
            assert!(
                near_damp <= 0.25,
                "{name}: {:.0}% of the first {near_rooms} rooms already have damp carpet",
                near_damp * 100.0
            );
            assert!(
                far_damp >= 0.75 && far_damp > near_damp,
                "{name}: damp carpet must dominate the last {far_rooms} rooms, got {:.0}%",
                far_damp * 100.0
            );

            let near_light = fixture_strength(&level, &distance, reach, 0.0, 0.25);
            let far_light = fixture_strength(&level, &distance, reach, 0.75, 1.01);
            assert!(
                near_light > far_light,
                "{name}: fixture strength must fall with distance ({near_light:.2} -> {far_light:.2})"
            );
            assert!(
                far_light > 0.0,
                "{name}: the last rooms must keep at least one working fixture"
            );

            // Stained ceilings and soaked walls appear too, and never in a
            // quantity of one material only: the level is not a single sheet.
            let stained_ceilings = level
                .room_iter()
                .filter(|room| room.ceiling_material.as_deref() == Some(DAMAGED_CEILING))
                .count();
            let stained_walls = level
                .walls
                .iter()
                .filter(|wall| {
                    wall.material.as_deref() == Some(DAMAGED_WALL)
                        || wall.faces.values().any(|material| material == DAMAGED_WALL)
                })
                .count();
            let damp_rooms = level
                .room_iter()
                .filter(|room| {
                    room.material.as_deref().unwrap_or(&level.defaults.floor) == DAMAGED_FLOOR
                })
                .count();
            let total_rooms = level.room_iter().count();
            assert!(
                stained_ceilings > 0,
                "{name}: no room carries a stained ceiling"
            );
            assert!(stained_walls > 0, "{name}: no wall carries water damage");
            assert!(
                damp_rooms < total_rooms,
                "{name}: every room is damp; the level needs maintained surfaces too"
            );
        }
    }

    #[test]
    fn the_residential_levels_stay_walkable_and_residential() {
        for name in RESIDENTIAL_LEVELS {
            let level = residential_level(name);
            let rooms: Vec<&RoomDef> = level.room_iter().collect();

            // The spawn is inside a room and clear of every collider.
            let spawn = glam::Vec2::new(level.spawn.x, level.spawn.z);
            let spawn_room = rooms.iter().find(|room| {
                level.spawn.x >= room.x.min(room.x + room.width)
                    && level.spawn.x <= room.x.max(room.x + room.width)
                    && level.spawn.z >= room.z.min(room.z + room.depth)
                    && level.spawn.z <= room.z.max(room.z + room.depth)
            });
            assert!(
                spawn_room.is_some(),
                "{name}: the spawn is outside every room"
            );
            for aabb in level.collision_aabbs() {
                assert!(
                    !aabb.intersects_circle(spawn, crate::collision::PLAYER_RADIUS),
                    "{name}: the spawn sits inside level geometry"
                );
            }

            // Every walk-through opening is wide enough for the player without
            // precision movement.
            for wall in &level.walls {
                for opening in &wall.openings {
                    if opening.is_door() {
                        assert!(
                            opening.width >= 1.0,
                            "{name}: a {:.2} m doorway is too narrow to walk through",
                            opening.width
                        );
                    }
                }
            }

            // Residential rooms, not halls, and genuine circulation space.
            let mut corridors = 0;
            for room in &rooms {
                let short = room.width.min(room.depth);
                let long = room.width.max(room.depth);
                assert!(
                    long <= 12.0,
                    "{name}: a {long:.1} by {short:.1} m room is too big for a residence"
                );
                if short <= 2.6 {
                    corridors += 1;
                }
            }
            assert!(
                corridors >= 8,
                "{name}: only {corridors} hallway sections; the plan needs real circulation"
            );
        }
    }
}
