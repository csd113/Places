//! Binary codec for the prepared static geometry (`LevelMesh`).
//!
//! Layout (little-endian, byte-aligned, no implicit padding):
//!
//! ```text
//! magic         4 bytes  "PLMW"
//! version       u16      [`MESH_RECORD_VERSION`]
//! range_count   u32
//! batches       6 x i32  floor, ceiling, wall, light, prop, decal (indices)
//! vertex_count  u32      declared total, must equal the summed range counts
//! index_count   u32      declared total
//! ranges        range_count x range
//! ```
//!
//! ```text
//! range:
//!   kind        u8       0 floor, 1 ceiling, 2 wall, 3 light, 4 prop fallback, 5 decal
//!   material    u32      level material index, or 0xFFFFFFFF for a bare family
//!   shine       u8       0 = none, 1..=101 = Some(whole percent 0..=100)
//!   bounds_min  f32 x 3
//!   bounds_max  f32 x 3
//!   vertices    u32 count, then count x 69-byte vertex
//!   indices     u32 count, then count x u16
//! ```
//!
//! ```text
//! vertex (69 bytes):
//!   pos            f32 x 3
//!   color          f32 x 4
//!   uv             f32 x 2
//!   normal         f32 x 3   (0,0,0 is the smooth-normal sentinel)
//!   tangent        f32 x 3
//!   handedness     f32
//!   lightmap       u16 x 2
//!   lightmap_page  u8        (0xFF is LIGHTMAP_NONE)
//! ```

use crate::render::{
    BatchRange, LevelMesh, LevelMeshBatches, LevelMeshRange, MATERIAL_NONE, SurfaceKey,
    SurfaceKind, SurfaceShine, Vertex,
};
use crate::spatial::Aabb;

use super::binary::{Reader, Writer, finite3, finite4};
use super::{MAX_MESH_INDICES, MAX_MESH_MATERIALS, MAX_MESH_RANGES, MAX_MESH_VERTICES};

/// Version of the mesh record layout.
///
/// Version 2 widened a range's `material` field from `u16` to `u32`, because
/// the in-memory [`crate::render::MaterialIndex`] is 32 bits and a level may
/// declare more materials than a `u16` names. A version-1 record cannot be
/// read by this build: its material field would be reinterpreted as the first
/// half of a 32-bit index, so the reader rejects it by name instead.
pub const MESH_RECORD_VERSION: u16 = 2;

/// Magic identifying a mesh record.
pub const MESH_MAGIC: [u8; 4] = *b"PLMW";

/// Bytes one state byte carries for an optional shine.
const SHINE_NONE: u8 = 0;
const SHINE_BASE: u8 = 1;

/// Encodes a prepared level mesh.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_mesh(mesh: &LevelMesh) -> Result<Vec<u8>, String> {
    let ranges =
        u32::try_from(mesh.ranges.len()).map_err(|_| "mesh has too many ranges".to_string())?;
    let mut writer = Writer::with_capacity(mesh.ranges.len().saturating_mul(64));
    writer.bytes(&MESH_MAGIC);
    writer.u16(MESH_RECORD_VERSION);
    writer.u32(ranges);
    let batches = mesh.batches;
    for range in [
        batches.floor_batch,
        batches.ceiling_batch,
        batches.wall_batch,
        batches.light_batch,
        batches.prop_batch,
        batches.decal_batch,
    ] {
        writer.i32(range.start);
        writer.i32(range.count);
    }
    let vertex_count =
        u32::try_from(mesh.vertex_count).map_err(|_| "mesh has too many vertices".to_string())?;
    let index_count =
        u32::try_from(mesh.index_count).map_err(|_| "mesh has too many indices".to_string())?;
    writer.u32(vertex_count);
    writer.u32(index_count);
    for range in &mesh.ranges {
        write_range(&mut writer, range)?;
    }
    Ok(writer.into_bytes())
}

/// Decodes and validates a prepared level mesh.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn read_mesh(bytes: &[u8]) -> Result<LevelMesh, String> {
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != MESH_MAGIC {
        return Err("mesh record has the wrong magic".to_string());
    }
    let version = reader.u16()?;
    if version != MESH_RECORD_VERSION {
        return Err(format!(
            "mesh record version {version} is not supported (this build reads {MESH_RECORD_VERSION})"
        ));
    }
    let range_count = reader.count(
        u64::try_from(MAX_MESH_RANGES).unwrap_or(u64::MAX),
        "mesh range count",
    )?;
    let mut batch_values = [0_i32; 12];
    for slot in batch_values.as_chunks_mut::<2>().0 {
        let start = reader.i32()?;
        let count = reader.i32()?;
        *slot = [start, count];
    }
    let declared_vertices = u64::from(reader.u32()?);
    let declared_indices = u64::from(reader.u32()?);
    if declared_vertices > MAX_MESH_VERTICES || declared_indices > MAX_MESH_INDICES {
        return Err(format!(
            "mesh declares {declared_vertices} vertices and {declared_indices} indices \
             (limits {MAX_MESH_VERTICES}/{MAX_MESH_INDICES})"
        ));
    }
    let mut ranges = Vec::with_capacity(range_count);
    let mut vertices = 0_u64;
    let mut indices = 0_u64;
    for _ in 0..range_count {
        let range = read_range(&mut reader, declared_vertices, declared_indices)?;
        vertices = vertices.saturating_add(u64::try_from(range.vertices.len()).unwrap_or(u64::MAX));
        indices = indices.saturating_add(u64::try_from(range.indices.len()).unwrap_or(u64::MAX));
        ranges.push(range);
    }
    if vertices != declared_vertices {
        return Err(format!(
            "mesh declares {declared_vertices} vertices but its ranges hold {vertices}"
        ));
    }
    if indices != declared_indices {
        return Err(format!(
            "mesh declares {declared_indices} indices but its ranges hold {indices}"
        ));
    }
    if !reader.is_empty() {
        return Err(format!(
            "mesh record has {} trailing bytes",
            reader.remaining()
        ));
    }
    Ok(LevelMesh {
        ranges,
        batches: LevelMeshBatches {
            floor_batch: BatchRange {
                start: batch_values[0],
                count: batch_values[1],
            },
            ceiling_batch: BatchRange {
                start: batch_values[2],
                count: batch_values[3],
            },
            wall_batch: BatchRange {
                start: batch_values[4],
                count: batch_values[5],
            },
            light_batch: BatchRange {
                start: batch_values[6],
                count: batch_values[7],
            },
            prop_batch: BatchRange {
                start: batch_values[8],
                count: batch_values[9],
            },
            decal_batch: BatchRange {
                start: batch_values[10],
                count: batch_values[11],
            },
        },
        vertex_count: usize::try_from(declared_vertices)
            .map_err(|_| "mesh vertex count is too large".to_string())?,
        index_count: usize::try_from(declared_indices)
            .map_err(|_| "mesh index count is too large".to_string())?,
    })
}

fn write_range(writer: &mut Writer, range: &LevelMeshRange) -> Result<(), String> {
    writer.u8(surface_kind_code(range.key.kind));
    writer.u32(range.key.material);
    match range.key.shine {
        None => writer.u8(SHINE_NONE),
        Some(shine) => {
            let value = shine
                .percent()
                .checked_add(SHINE_BASE)
                .ok_or_else(|| "mesh shine is out of range".to_string())?;
            writer.u8(value);
        }
    }
    writer.f32_3(range.bounds.min);
    writer.f32_3(range.bounds.max);
    let vertex_count = u32::try_from(range.vertices.len())
        .map_err(|_| "mesh range holds too many vertices".to_string())?;
    writer.u32(vertex_count);
    for vertex in &range.vertices {
        write_vertex(writer, vertex);
    }
    writer.u16s(&range.indices)?;
    Ok(())
}

fn read_range(
    reader: &mut Reader<'_>,
    declared_vertices: u64,
    declared_indices: u64,
) -> Result<LevelMeshRange, String> {
    let kind_code = reader.u8()?;
    let kind = surface_kind_from_code(kind_code)?;
    let material = reader.u32()?;
    if material != MATERIAL_NONE && material > MAX_MESH_MATERIALS {
        return Err(format!(
            "mesh range material {material} is out of the level material budget \
             (limit {MAX_MESH_MATERIALS}, or {MATERIAL_NONE} for no material)"
        ));
    }
    let shine_code = reader.u8()?;
    let shine = if shine_code == SHINE_NONE {
        None
    } else {
        let percent = shine_code
            .checked_sub(SHINE_BASE)
            .ok_or_else(|| "mesh shine byte is invalid".to_string())?;
        if percent > 100 {
            return Err(format!("mesh shine percent {percent} is out of range"));
        }
        Some(SurfaceShine::from_percent(percent))
    };
    let bounds_min = reader.f32_3()?;
    let bounds_max = reader.f32_3()?;
    if !finite3(bounds_min) || !finite3(bounds_max) {
        return Err("mesh range has a non-finite bound".to_string());
    }
    if bounds_min
        .iter()
        .zip(bounds_max.iter())
        .any(|(low, high)| low > high)
    {
        return Err("mesh range bounds are inverted".to_string());
    }
    let vertex_count = u64::from(reader.u32()?);
    if vertex_count == 0 || vertex_count > declared_vertices {
        return Err(format!(
            "mesh range declares {vertex_count} vertices (record declares {declared_vertices})"
        ));
    }
    let vertex_count = usize::try_from(vertex_count)
        .map_err(|_| "mesh range vertex count is too large".to_string())?;
    let mut vertices = Vec::with_capacity(vertex_count.min(4096));
    for _ in 0..vertex_count {
        vertices.push(read_vertex(reader)?);
    }
    let indices = reader.u16s(declared_indices)?;
    let vertex_count_u64 = u64::try_from(vertex_count).unwrap_or(u64::MAX);
    if indices
        .iter()
        .any(|index| u64::from(*index) >= vertex_count_u64)
    {
        return Err("mesh range has an index outside its vertex list".to_string());
    }
    if indices.len() % 3 != 0 {
        return Err("mesh range index count is not a multiple of three".to_string());
    }
    Ok(LevelMeshRange {
        key: SurfaceKey::with_shine(kind, material, shine),
        vertices,
        indices,
        bounds: Aabb {
            min: bounds_min,
            max: bounds_max,
        },
    })
}

/// Writes one neutral vertex in the documented 69-byte layout.
pub(crate) fn write_vertex(writer: &mut Writer, vertex: &Vertex) {
    writer.f32_3(vertex.pos);
    writer.f32_4(vertex.color);
    for component in vertex.uv {
        writer.f32(component);
    }
    writer.f32_3(vertex.normal);
    writer.f32_3(vertex.tangent);
    writer.f32(vertex.handedness);
    for component in vertex.lightmap {
        writer.u16(component);
    }
    writer.u8(vertex.lightmap_page);
}

/// Reads one neutral vertex in the documented 69-byte layout.
pub(crate) fn read_vertex(reader: &mut Reader<'_>) -> Result<Vertex, String> {
    let pos = reader.f32_3()?;
    let color = reader.f32_4()?;
    let uv = [reader.f32()?, reader.f32()?];
    let normal = reader.f32_3()?;
    let tangent = reader.f32_3()?;
    let handedness = reader.f32()?;
    let lightmap = [reader.u16()?, reader.u16()?];
    let lightmap_page = reader.u8()?;
    if !finite3(pos) || !finite4(color) || !finite3(normal) || !finite3(tangent) {
        return Err("vertex has a non-finite component".to_string());
    }
    if !uv.iter().all(|component| component.is_finite()) || !handedness.is_finite() {
        return Err("vertex has a non-finite component".to_string());
    }
    Ok(Vertex {
        pos,
        color,
        uv,
        normal,
        tangent,
        handedness,
        lightmap,
        lightmap_page,
    })
}

/// Stable byte code of a surface family, in draw order.
#[must_use]
pub const fn surface_kind_code(kind: SurfaceKind) -> u8 {
    match kind {
        SurfaceKind::Floor => 0,
        SurfaceKind::Ceiling => 1,
        SurfaceKind::Wall => 2,
        SurfaceKind::Light => 3,
        SurfaceKind::PropFallback => 4,
        SurfaceKind::Decal => 5,
    }
}

/// Decodes a surface family byte code.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn surface_kind_from_code(code: u8) -> Result<SurfaceKind, String> {
    match code {
        0 => Ok(SurfaceKind::Floor),
        1 => Ok(SurfaceKind::Ceiling),
        2 => Ok(SurfaceKind::Wall),
        3 => Ok(SurfaceKind::Light),
        4 => Ok(SurfaceKind::PropFallback),
        5 => Ok(SurfaceKind::Decal),
        other => Err(format!("unknown surface kind code {other}")),
    }
}
