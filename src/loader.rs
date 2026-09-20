use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::level::LevelDef;
use crate::render::{
    generate_carpet_texture, generate_ceiling_texture, generate_wall_texture, generate_white_texture,
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
        Self { width, height, rgba }
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
            wall: RawImage::new(64, 64, generate_wall_texture().to_vec()),
            floor: RawImage::new(64, 64, generate_carpet_texture().to_vec()),
            ceiling: RawImage::new(64, 64, generate_ceiling_texture().to_vec()),
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
            for chunk in buf.chunks_exact(3) {
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
            for chunk in buf.chunks_exact(2) {
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
#[derive(Default, Debug)]
pub struct RawPackContents {
    pub level_json: String,
    pub materials_json: Option<String>,
    pub textures: HashMap<String, Vec<u8>>,
}

/// Safely extracts a ZIP level pack with path traversal and size limits enforcement.
pub fn extract_zip<R: Read + Seek>(reader: R) -> Result<RawPackContents, String> {
    let mut archive =
        ZipArchive::new(reader).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

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
            pack.textures.insert(normalized.clone(), bytes.clone());
            if let Some(tex_sub) = normalized.split("textures/").nth(1) {
                pack.textures.insert(format!("textures/{tex_sub}"), bytes.clone());
                pack.textures.insert(tex_sub.to_string(), bytes.clone());
            }
            pack.textures.insert(file_name.to_string(), bytes);
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
    let rooms = level.all_rooms();
    if rooms.len() > 500 {
        return Err(format!(
            "Level contains too many rooms: {} (limit: 500)",
            rooms.len()
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

    for (i, r) in rooms.iter().enumerate() {
        if !r.width.is_finite() || !r.depth.is_finite() || !r.height.is_finite() {
            return Err(format!("Room {i} dimensions must be finite numbers"));
        }
        if r.width <= 0.0 || r.depth <= 0.0 || r.height <= 0.0 {
            return Err(format!("Room {i} width, depth, and height must be positive"));
        }
        if r.width > 2000.0 || r.depth > 2000.0 || r.height > 50.0 {
            return Err(format!(
                "Room {i} dimensions exceed maximum limits (max 2000x2000x50m)"
            ));
        }
    }

    for (i, w) in level.walls.iter().enumerate() {
        if !w.width.is_finite() || !w.depth.is_finite() || !w.height.is_finite() {
            return Err(format!("Wall {i} dimensions must be finite numbers"));
        }
        if w.width <= 0.0 || w.depth <= 0.0 || w.height <= 0.0 {
            return Err(format!("Wall {i} width, depth, and height must be positive"));
        }
    }

    Ok(())
}

/// Stained wall texture generator for core:wallpaper_stained_01.
fn generate_stained_wall_texture() -> [u8; 64 * 64 * 4] {
    let mut data = generate_wall_texture();
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            // Darker damp/stained streaks
            let stain = if (x + y * 2) % 31 < 8 { 0.75 } else { 1.0 };
            data[idx] = (data[idx] as f32 * stain) as u8;
            data[idx + 1] = (data[idx + 1] as f32 * stain) as u8;
            data[idx + 2] = (data[idx + 2] as f32 * stain) as u8;
        }
    }
    data
}

/// Damp carpet texture generator for core:carpet_damp_01.
fn generate_damp_carpet_texture() -> [u8; 64 * 64 * 4] {
    let mut data = generate_carpet_texture();
    for y in 0..64 {
        for x in 0..64 {
            let idx = (y * 64 + x) * 4;
            data[idx] = (data[idx] as f32 * 0.65) as u8;
            data[idx + 1] = (data[idx + 1] as f32 * 0.60) as u8;
            data[idx + 2] = (data[idx + 2] as f32 * 0.55) as u8;
        }
    }
    data
}

/// Resolves materials and textures for a level.
/// Missing custom resources fail gracefully with an obvious fallback rather than crashing.
pub fn resolve_textures(
    level: &LevelDef,
    material_map: &HashMap<String, String>,
    texture_blobs: &HashMap<String, Vec<u8>>,
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
            {
                if let Ok(img) = decode_png(bytes) {
                    return Some(img);
                }
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
            if let Some(bytes) = texture_blobs.get(cand) {
                if let Ok(img) = decode_png(bytes) {
                    return Some(img);
                }
            }
        }
        None
    };

    // Wall texture
    if let Some(img) = try_decode_material(&level.defaults.wall) {
        loaded.wall = img;
    } else if level.defaults.wall == "core:wallpaper_stained_01" {
        loaded.wall = RawImage::new(64, 64, generate_stained_wall_texture().to_vec());
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
        };
        manager.refresh();
        manager
    }

    pub fn entries(&self) -> &[LevelEntry] {
        &self.entries
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
                if p.extension().is_some_and(|ext| ext == "json") {
                    if let Ok(meta) = self.probe_level_file(&p, LevelSourceType::Official) {
                        discovered.push(meta);
                    }
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
                } else if p.extension().is_some_and(|ext| ext == "zip") {
                    if let Ok(meta) = self.probe_zip_file(&p) {
                        discovered.push(meta);
                    }
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
            if let Some(pos) = discovered.iter().position(|e| e.id == "level_1") {
                if pos != 0 {
                    let e = discovered.remove(pos);
                    discovered.insert(0, e);
                }
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
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let pack = extract_zip(file)?;
        let level = LevelDef::from_json(&pack.level_json).map_err(|e| e.to_string())?;
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
            return Err(format!("Source file does not exist: {}", source_path.display()));
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
            let level = LevelDef::from_json(&content)
                .map_err(|e| format!("Invalid level JSON: {e}"))?;
            validate_level(&level)?;
            (level.id, level.name, level.author, LevelSourceType::CustomJson)
        } else if ext == "zip" {
            let file = fs::File::open(source_path)
                .map_err(|e| format!("Failed to open {}: {e}", source_path.display()))?;
            let pack = extract_zip(file)?;
            let level = LevelDef::from_json(&pack.level_json)
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
                    if ext == "json" || ext == "zip" {
                        if self.import_file(&path).is_ok() {
                            imported_count += 1;
                        }
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
    use crate::level::{RoomDef, WallDef};

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
            }],
            spawn: crate::level::SpawnDef {
                x: 5.0,
                z: 5.0,
                yaw_degrees: 0.0,
            },
            defaults: Default::default(),
            walls: vec![WallDef {
                x: 10.0,
                z: 10.0,
                width: 2.0,
                depth: 1.0,
                height: 3.5,
                faces: HashMap::new(),
            }],
            floor_patches: vec![],
            ceiling_lights: vec![],
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
                },
                RoomDef {
                    x: 5.0,
                    z: 5.0,
                    width: 10.0,
                    depth: 10.0,
                    height: 3.5,
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
                    z: 2.0,
                    width: 4.0,
                    depth: 4.0,
                    height: 3.5,
                    faces: HashMap::new(),
                },
                WallDef {
                    x: 3.0,
                    z: 3.0,
                    width: 4.0,
                    depth: 4.0,
                    height: 3.5,
                    faces: HashMap::new(),
                },
            ],
            floor_patches: vec![],
            ceiling_lights: vec![],
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
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

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
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

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
        };

        // No custom textures supplied -> should gracefully fallback
        let textures = resolve_textures(&level, &HashMap::new(), &HashMap::new());
        assert_eq!(textures.wall.width, 64);
        assert_eq!(textures.floor.width, 64);
        assert_eq!(textures.ceiling.width, 64);
    }
}
