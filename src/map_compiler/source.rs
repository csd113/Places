//! Authoring adjustments applied once before a map is compiled.

use crate::engine::assets::AssetCatalog;
use crate::engine::level::{
    BASEBOARD_DEFAULT_HEIGHT_M, BASEBOARD_DEFAULT_THICKNESS_M, BaseboardDef, LevelDef,
    LevelSurfaces, WallAxis, WallDef,
};
use crate::engine::loader::point_buried_in_wall;
use crate::engine::materials::{MaterialTable, PackMaterials};

/// Distance a baseboard floor probe stands off the wall's face, in metres.
const BASEBOARD_FLOOR_PROBE_M: f32 = 0.05;
/// How far floor samples along one face may differ and still be one floor.
const BASEBOARD_FLOOR_AGREEMENT_M: f32 = 0.05;
/// How far a wall's own base may sit from the floor it fronts, in metres.
const BASEBOARD_BASE_AGREEMENT_M: f32 = 0.05;
/// Fractions along a face at which its walkable floor is sampled: both ends
/// inset, the middle and the quarter points, so a floor change anywhere along
/// the run is seen.
const BASEBOARD_FLOOR_FRACTIONS: [f32; 5] = [0.02, 0.25, 0.5, 0.75, 0.98];
/// Plane separation under which two baseboard runs count as the same plane.
const BASEBOARD_SAME_PLANE_M: f32 = 1.0e-3;
/// Run overlap under which two collinear runs do not suppress each other.
const BASEBOARD_OVERLAP_M: f32 = 1.0e-3;

/// One wall length face a baseboard run can sit on: the face's `faces` name,
/// the run's start point on the face, and the run's yaw.
///
/// The direction and across vectors follow [`BaseboardDef::direction`] and
/// [`BaseboardDef::across`] exactly, so a generated run and the emitter agree
/// on which way the board faces.
#[derive(Clone, Copy)]
struct BaseboardFace {
    name: &'static str,
    start: (f32, f32),
    rotation_degrees: f32,
}

impl BaseboardFace {
    /// Run direction as an `(x, z)` unit vector; 0 runs +X.
    fn direction(self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.cos(), -radians.sin())
    }

    /// Across-the-board direction as an `(x, z)` unit vector, pointing out of
    /// the wall's face into the room.
    fn across(self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.sin(), radians.cos())
    }
}

/// The offline source preparation pass, run after validation and before the
/// decoded material table is built.
///
/// It applies every catalog-driven adjustment that the bake, mesh, collision
/// and fixture probe must all see:
///
/// * grid-aligns fluorescent panels onto their ceiling material's world tile
///   grid ([`LevelDef::align_ceiling_fixtures`]); and
/// * generates the automatic baseboard runs a wall material declares through
///   the catalog's `baseboard` field.
///
/// `materials` starts as a **logical** table: alignment only needs
/// `tile_metres`, which is already final before images decode, and the
/// baseboard pass only needs the catalog. The caller resolves the full decoded
/// table *after* this pass, so a generated run's trim material is part of the
/// renderer's material table even when no authored trim uses it.
pub fn prepare_level(level: &mut LevelDef, catalog: &AssetCatalog, pack: Option<&PackMaterials>) {
    let materials = MaterialTable::logical(level, catalog, pack);
    let _align_ceiling_fixtures_status = level.align_ceiling_fixtures(&materials);
    let _snap_ceiling_decals_status = level.snap_ceiling_decals(&materials);
    let _generate_automatic_baseboards_status = generate_automatic_baseboards(level, catalog);
}

/// Generates the baseboard runs every wall face's resolved material declares
/// through its catalog `baseboard`, appending them after the authored runs so
/// the existing joint trimming lets authored trim win. Returns how many runs
/// were generated.
///
/// A face qualifies when it is finished with a material whose catalog entry
/// declares `baseboard`, it fronts one walkable floor along its whole length
/// (samples just outside the face at both inset ends, the middle and the
/// quarter points all exist and agree within
/// [`BASEBOARD_FLOOR_AGREEMENT_M`]), and the wall's own base meets that floor
/// (within [`BASEBOARD_BASE_AGREEMENT_M`]). Each run is split around every
/// opening whose sill reaches the board's height, pinned to the floor it
/// fronts, and skipped when it would be buried in a crossing wall or when an
/// authored run already covers the same plane and span.
fn generate_automatic_baseboards(level: &mut LevelDef, catalog: &AssetCatalog) -> usize {
    level.baseboards.retain(|board| !board.generated);
    let attached = super::trim::attached_baseboards(level);
    let authored_count = level.baseboards.len();
    let mut generated: Vec<BaseboardDef> = Vec::new();
    let mut remaining = crate::engine::level::MAX_LEVEL_BASEBOARDS.saturating_sub(
        u64::try_from(authored_count.saturating_add(attached.len())).unwrap_or(u64::MAX),
    );
    {
        let surfaces = LevelSurfaces::new(level);
        // `authored_count` was captured from this same array, so the prefix is
        // always present; the empty fallback only satisfies the no-slicing rule.
        let authored = level.baseboards.get(..authored_count).unwrap_or_default();
        for wall in &level.walls {
            if wall.trim.is_some() {
                continue;
            }
            if remaining == 0 {
                break;
            }
            generate_wall_baseboards(
                level,
                &surfaces,
                wall,
                catalog,
                authored,
                &mut generated,
                &mut remaining,
            );
        }
    }
    let count = generated.len().saturating_add(attached.len());
    level.baseboards.append(&mut generated);
    level.baseboards.extend(attached);
    count
}

/// Generates the runs for one wall's two length faces: the per-wall half of
/// [`generate_automatic_baseboards`], which owns the shared budget and output.
fn generate_wall_baseboards(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    wall: &WallDef,
    catalog: &AssetCatalog,
    authored: &[BaseboardDef],
    generated: &mut Vec<BaseboardDef>,
    remaining: &mut u64,
) {
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let length = wall.length();
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    // Deterministic order: the + face then the - face. An X-axis wall's + face
    // is south (normal +Z), a Z-axis wall's is east (+X).
    let faces: [BaseboardFace; 2] = match wall.axis() {
        WallAxis::X => [
            BaseboardFace {
                name: "south",
                start: (x0, z1),
                rotation_degrees: 0.0,
            },
            BaseboardFace {
                name: "north",
                start: (x1, z0),
                rotation_degrees: 180.0,
            },
        ],
        WallAxis::Z => [
            BaseboardFace {
                name: "east",
                start: (x1, z1),
                rotation_degrees: 90.0,
            },
            BaseboardFace {
                name: "west",
                start: (x0, z0),
                rotation_degrees: 270.0,
            },
        ],
    };
    for face in faces {
        if *remaining == 0 {
            return;
        }
        let material = wall
            .face_ref(face.name)
            .map_or(level.defaults.wall.as_str(), |reference| reference.id);
        let Some(baseboard) = catalog
            .material(material)
            .and_then(|entry| entry.baseboard.as_deref())
        else {
            continue;
        };
        let Some(floor) = baseboard_face_floor(surfaces, wall, face, length) else {
            continue;
        };
        let (dx, dz) = face.direction();
        let along_positive = {
            let (ax, az) = match wall.axis() {
                WallAxis::X => (1.0, 0.0),
                WallAxis::Z => (0.0, 1.0),
            };
            dx.mul_add(ax, dz * az) > 0.0
        };
        for (low, high) in baseboard_segments(wall, length, along_positive) {
            let run = BaseboardDef {
                generated: true,
                miters: None,
                uv_offset: low,
                x: dx.mul_add(low, face.start.0),
                z: dz.mul_add(low, face.start.1),
                length: high - low,
                rotation_degrees: face.rotation_degrees,
                height: BASEBOARD_DEFAULT_HEIGHT_M,
                thickness: BASEBOARD_DEFAULT_THICKNESS_M,
                y: Some(floor),
                material: Some(baseboard.to_string()),
                shine: None,
                uv_projection: crate::engine::level::BaseboardUvProjection::Wall,
            };
            if baseboard_run_is_hidden(level, &run) {
                continue;
            }
            if authored_run_suppresses(surfaces, authored, &run) {
                continue;
            }
            generated.push(run);
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                return;
            }
        }
    }
}

/// The walkable floor one face fronts, when the whole run fronts a single
/// floor that the wall's own base meets.
fn baseboard_face_floor(
    surfaces: &LevelSurfaces<'_>,
    wall: &WallDef,
    face: BaseboardFace,
    length: f32,
) -> Option<f32> {
    let (dx, dz) = face.direction();
    let (ax, az) = face.across();
    let mut floor: Option<f32> = None;
    for fraction in BASEBOARD_FLOOR_FRACTIONS {
        let along = length * fraction;
        let x = dx.mul_add(along, ax.mul_add(BASEBOARD_FLOOR_PROBE_M, face.start.0));
        let z = dz.mul_add(along, az.mul_add(BASEBOARD_FLOOR_PROBE_M, face.start.1));
        let sample = surfaces.floor_y_at(x, z)?;
        if floor.is_some_and(|previous| (sample - previous).abs() > BASEBOARD_FLOOR_AGREEMENT_M) {
            return None;
        }
        floor = Some(sample);
    }
    let floor_y = floor?;
    // The wall's own base is its absolute world `y`; a non-finite value falls
    // back to the room floor under the footprint centre.
    let base = if wall.y.is_finite() {
        wall.y
    } else {
        surfaces
            .floor_y_at(
                f32::midpoint(wall.x, wall.x + wall.width),
                f32::midpoint(wall.z, wall.z + wall.depth),
            )
            .unwrap_or(0.0)
    };
    ((base - floor_y).abs() <= BASEBOARD_BASE_AGREEMENT_M).then_some(floor_y)
}

/// The run spans of one face with every floor-reaching opening removed.
///
/// `along_positive` says whether the face's run direction is the wall's own
/// positive length axis; the other faces measure their span from the far end.
/// Spans ascend along the run.
fn baseboard_segments(wall: &WallDef, length: f32, along_positive: bool) -> Vec<(f32, f32)> {
    baseboard_segments_at(
        wall,
        length,
        along_positive,
        wall.y,
        wall.y + BASEBOARD_DEFAULT_HEIGHT_M,
    )
}

pub(super) fn baseboard_segments_at(
    wall: &WallDef,
    length: f32,
    along_positive: bool,
    bottom: f32,
    top: f32,
) -> Vec<(f32, f32)> {
    let mut spans = vec![(0.0, length)];
    for opening in &wall.openings {
        if !opening.sill.is_finite()
            || wall.y + opening.sill >= top - 1.0e-4
            || wall.y + opening.sill + opening.height <= bottom + 1.0e-4
        {
            continue;
        }
        let (low, high) = if along_positive {
            (opening.offset, opening.end())
        } else {
            (length - opening.end(), length - opening.offset)
        };
        let clipped_low = low.max(0.0);
        let clipped_high = high.min(length);
        if clipped_high <= clipped_low {
            continue;
        }
        let mut next: Vec<(f32, f32)> = Vec::new();
        for (span_low, span_high) in spans {
            if clipped_high <= span_low || clipped_low >= span_high {
                next.push((span_low, span_high));
                continue;
            }
            if clipped_low > span_low {
                next.push((span_low, clipped_low));
            }
            if clipped_high < span_high {
                next.push((clipped_high, span_high));
            }
        }
        spans = next;
    }
    spans.retain(|(low, high)| high - low > BASEBOARD_OVERLAP_M);
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    spans
}

/// True when a run's midpoint lies inside a wall solid: a perpendicular wall
/// crossing the face would hide the board there, so the segment is skipped.
fn baseboard_run_is_hidden(level: &LevelDef, run: &BaseboardDef) -> bool {
    let (x, z) = run.point_at(0.5, run.thickness() * 0.5);
    let y = run
        .height()
        .mul_add(0.5, run.base_y(&LevelSurfaces::new(level)));
    point_buried_in_wall(level, x, z, y).is_some()
}

/// True when an authored run already lies on this run's plane and overlaps its
/// span, in which case the authored board owns the face and the generated run
/// is skipped.
fn authored_run_suppresses(
    surfaces: &LevelSurfaces<'_>,
    authored: &[BaseboardDef],
    run: &BaseboardDef,
) -> bool {
    let (rdx, rdz) = run.direction();
    let (rax, raz) = run.across();
    let base = run.base_y(surfaces);
    for other in authored {
        let (odx, odz) = other.direction();
        if rdx.mul_add(odx, rdz * odz).abs() < 0.999 {
            continue;
        }
        if (other.base_y(surfaces) - base).abs() > BASEBOARD_SAME_PLANE_M {
            continue;
        }
        let rel_x = run.x - other.x;
        let rel_z = run.z - other.z;
        if rax.mul_add(rel_x, raz * rel_z).abs() > BASEBOARD_SAME_PLANE_M {
            continue;
        }
        // Project the generated run's interval onto the authored run's own axis
        // so the two orientations compare in one frame.
        let start = odx.mul_add(rel_x, odz * rel_z);
        let end = run.length.mul_add(rdx.mul_add(odx, rdz * odz), start);
        let (run_low, run_high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let overlap = run_high.min(other.length) - run_low.max(0.0);
        if overlap > BASEBOARD_OVERLAP_M {
            return true;
        }
    }
    false
}
