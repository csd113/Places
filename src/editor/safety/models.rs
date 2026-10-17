//! Real transformed model triangles. Bounds only reject candidates; a plane
//! test and clipped positive overlap area provide the coplanarity evidence.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use glam::{Vec2, Vec3};

use super::{SafetyCategory, SafetyFinding, SafetyReport, SafetySeverity, bounds_center};
use crate::editor::document::ObjectId;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::loader::PropCatalog;
use crate::engine::materials::AlphaMode;
use crate::engine::materials::MaterialTable;
use crate::engine::props::PropAssets;
use crate::engine::render::SurfaceKind;
use crate::engine::spatial::Aabb;

const MAX_TRIANGLES: usize = 200_000;
const MAX_PAIR_VISITS: usize = 2_000_000;
const PLANE_EPS: f32 = 0.0015;
const AREA_EPS: f32 = 0.0001;

struct Triangle {
    points: [Vec3; 3],
    bounds: Aabb,
    normal: Vec3,
    prop: Option<usize>,
    alpha: AlphaMode,
    double_sided: bool,
}

impl Triangle {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite floating-point vector subtraction; zero-area normals are rejected."
    )]
    fn new(
        points: [Vec3; 3],
        prop: Option<usize>,
        alpha: AlphaMode,
        double_sided: bool,
    ) -> Option<Self> {
        let [a, b, c] = points;
        let normal = (b - a).cross(c - a).try_normalize()?;
        let bounds = points.iter().fold(Aabb::EMPTY, |bounds, point| {
            bounds.expanded(point.to_array())
        });
        Some(Self {
            points,
            bounds,
            normal,
            prop,
            alpha,
            double_sided,
        })
    }
}

pub(super) fn check(level: &LevelDef, report: &mut SafetyReport) {
    if level.props.is_empty() {
        return;
    }
    let catalog = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    let materials = crate::engine::render::logical_materials(level);
    let mut triangles = Vec::new();
    if !architecture(level, &catalog, &materials, &mut triangles, report) {
        return;
    }
    let surfaces = LevelSurfaces::new(level);
    for (index, prop) in level.props.iter().enumerate() {
        let definition = catalog.get(&prop.model);
        let Some(path) = definition.model else {
            continue;
        };
        let asset = match assets.resolve(&path) {
            Ok(asset) => asset,
            Err(error) => {
                let _model_limit = write!(
                    report.limitations,
                    " Model {} not checked: {error}.",
                    prop.model
                );
                incomplete(
                    report,
                    &format!("Model {} was not examined: {error}", prop.model),
                );
                continue;
            }
        };
        let matrix = prop.placement_matrix(surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0));
        for submesh in &asset.model.submeshes {
            let first = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
            let end =
                first.saturating_add(usize::try_from(submesh.index_count).unwrap_or(usize::MAX));
            let Some(indices) = asset.model.indices.get(first..end) else {
                continue;
            };
            let alpha = prop
                .material_overrides
                .get(&submesh.material)
                .and_then(|material| materials.entry_of(material))
                .map_or(submesh.alpha.mode, |material| material.alpha.mode);
            for &[ia, ib, ic] in indices.as_chunks::<3>().0 {
                let point = |slot| {
                    asset
                        .model
                        .vertices
                        .get(usize::from(slot))
                        .map(|vertex| matrix.transform_point3(Vec3::from_array(vertex.pos)))
                };
                let (Some(a), Some(b), Some(c)) = (point(ia), point(ib), point(ic)) else {
                    continue;
                };
                if triangles.len() >= MAX_TRIANGLES {
                    incomplete(
                        report,
                        "Triangle storage limit reached; later models were not examined.",
                    );
                    return;
                }
                if let Some(triangle) =
                    Triangle::new([a, b, c], Some(index), alpha, submesh.double_sided)
                {
                    triangles.push(triangle);
                }
            }
        }
        if !prop.solid && definition.solid && prop.physics.is_none() {
            let [x, y, z] = [
                prop.x,
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                prop.z,
            ];
            if level
                .geometry_intent
                .iter()
                .any(|intent| intent.covers("prop-no-collision", x, z))
            {
                continue;
            }
            let object = ObjectId::Prop(index);
            let bounds = prop.placement_bounds(
                surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0),
                prop.size.unwrap_or(definition.size),
            );
            report.findings.push(SafetyFinding {
                check: "prop-no-collision", category: SafetyCategory::Collision, severity: SafetySeverity::Warning,
                title: format!("{}: collision disabled",prop.model), risk: "This catalog model normally blocks the player, but this placement has solid=false and no physics body. The player can pass through its visible mesh. This may be intentional scenery; no collision is enabled automatically.".into(),
                position: [x,y,z], bounds, objects: vec![object],surfaces: Vec::new(),fix: None, placement_ignore: None,
            });
        }
    }
    compare(&mut triangles, report);
}

fn architecture(
    level: &LevelDef,
    catalog: &PropCatalog,
    materials: &MaterialTable,
    triangles: &mut Vec<Triangle>,
    report: &mut SafetyReport,
) -> bool {
    let mesh = crate::engine::render::build_level_geometry_with_catalog_and_materials(
        level, catalog, materials,
    );
    for range in &mesh.ranges {
        if !matches!(
            range.key.kind,
            SurfaceKind::Floor | SurfaceKind::Wall | SurfaceKind::Ceiling
        ) {
            continue;
        }
        for &[ia, ib, ic] in range.indices.as_chunks::<3>().0 {
            let point = |slot| {
                range
                    .vertices
                    .get(usize::from(slot))
                    .map(|vertex| Vec3::from_array(vertex.pos))
            };
            let (Some(a), Some(b), Some(c)) = (point(ia), point(ib), point(ic)) else {
                continue;
            };
            if triangles.len() >= MAX_TRIANGLES {
                incomplete(
                    report,
                    "Triangle storage limit reached; later geometry was not examined.",
                );
                return false;
            }
            let alpha = materials
                .entry(range.key.material)
                .map_or(AlphaMode::Opaque, |material| material.alpha.mode);
            if let Some(triangle) = Triangle::new([a, b, c], None, alpha, false) {
                triangles.push(triangle);
            }
        }
    }
    true
}

fn compare(triangles: &mut [Triangle], report: &mut SafetyReport) {
    triangles.sort_by(|first, second| {
        first
            .bounds
            .min
            .first()
            .copied()
            .unwrap_or(0.0)
            .total_cmp(&second.bounds.min.first().copied().unwrap_or(0.0))
    });
    let mut visits = 0_usize;
    let mut reported = BTreeSet::new();
    for (slot, first) in triangles.iter().enumerate() {
        for second in triangles.iter().skip(slot.saturating_add(1)) {
            let [bx, _, _] = second.bounds.min;
            let [ax, _, _] = first.bounds.max;
            if bx > ax + PLANE_EPS {
                break;
            }
            if visits >= MAX_PAIR_VISITS {
                incomplete(
                    report,
                    "Triangle pair budget reached; remaining candidates were not examined.",
                );
                return;
            }
            visits = visits.saturating_add(1);
            if first.prop == second.prop || !bounds_overlap(first.bounds, second.bounds) {
                continue;
            }
            let pair = if first.prop <= second.prop {
                (first.prop, second.prop)
            } else {
                (second.prop, first.prop)
            };
            if reported.contains(&pair) {
                continue;
            }
            let dot = first.normal.dot(second.normal);
            // Ordinary back-to-back contact (chair foot/floor, cabinet/wall)
            // has only one visible face and is not by itself depth flicker.
            if dot < 0.0 && !(first.double_sided || second.double_sided) {
                continue;
            }
            let Some(area) = coplanar_overlap(first.points, second.points) else {
                continue;
            };
            if area <= AREA_EPS {
                continue;
            }
            let _new_pair = reported.insert(pair);
            let bounds = first.bounds.union(&second.bounds);
            let objects = [first.prop, second.prop]
                .into_iter()
                .flatten()
                .map(ObjectId::Prop)
                .collect();
            let depth = first.alpha != AlphaMode::Opaque || second.alpha != AlphaMode::Opaque;
            report.findings.push(SafetyFinding {
                check: "model-coplanar-surface",
                category: if depth { SafetyCategory::MaterialDepth } else { SafetyCategory::Coplanar },
                severity: SafetySeverity::Warning,
                title: if depth { "Model material / depth contact".into() } else { "Model has overlapping coplanar faces".into() },
                risk: format!("Transformed triangles overlap {area:.5} m² on parallel planes within {PLANE_EPS:.4} m: {} / {} ({} / {}). {} Geometric risk, not image proof; camera and intended contact matter. No surface is moved automatically.", owner(first.prop),owner(second.prop),first.alpha.name(),second.alpha.name(),if depth { "Alpha pixels are not sampled; cutout coverage or blended sorting may affect the result." } else { "Visible opaque sheets sharing depth can flicker." }),
                position: bounds_center(bounds), bounds, objects,
                surfaces: vec![first.points.map(|point| point.to_array()),second.points.map(|point| point.to_array())], fix: None, placement_ignore: None,
            });
        }
    }
}

fn owner(prop: Option<usize>) -> String {
    prop.map_or_else(
        || "map architecture".into(),
        |index| format!("prop {}", index.saturating_add(1)),
    )
}

fn incomplete(report: &mut SafetyReport, reason: &str) {
    report.findings.push(SafetyFinding {
        check: "model-check-incomplete",
        category: SafetyCategory::Review,
        severity: SafetySeverity::Warning,
        title: "Model audit incomplete".into(),
        risk: reason.into(),
        position: [0.0; 3],
        bounds: Aabb::from_point([0.0; 3]),
        objects: Vec::new(),
        surfaces: Vec::new(),
        fix: None,
        placement_ignore: None,
    });
}

fn bounds_overlap(first: Aabb, second: Aabb) -> bool {
    first
        .min
        .iter()
        .zip(&first.max)
        .zip(second.min.iter().zip(&second.max))
        .all(|((a0, a1), (b0, b1))| a0 <= &(b1 + PLANE_EPS) && b0 <= &(a1 + PLANE_EPS))
}

/// Positive coplanar overlap area in world square metres. Transverse solid
/// intersections and AABB-only overlaps intentionally return no match.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite floating-point clipping math; degenerate normals are rejected and crossing divisions require opposite signs."
)]
pub(in crate::editor) fn coplanar_overlap(a: [Vec3; 3], b: [Vec3; 3]) -> Option<f32> {
    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    let normal = (a1 - a0).cross(a2 - a0).try_normalize()?;
    let other = (b1 - b0).cross(b2 - b0).try_normalize()?;
    if normal.dot(other).abs() < 0.999_999
        || b.iter()
            .any(|point| normal.dot(*point - a0).abs() > PLANE_EPS)
    {
        return None;
    }
    let [nx, ny, nz] = normal.abs().to_array();
    let axis = if nx >= ny && nx >= nz {
        0_u8
    } else if ny >= nz {
        1_u8
    } else {
        2_u8
    };
    let project = |point: Vec3| match axis {
        0 => Vec2::new(point.y, point.z),
        1 => Vec2::new(point.x, point.z),
        _ => Vec2::new(point.x, point.y),
    };
    let clip = b.map(project);
    let [c0, c1, c2] = clip;
    let sign = (c1 - c0).perp_dot(c2 - c0).signum();
    let mut polygon = a.map(project).to_vec();
    for (start, end) in [(c0, c1), (c1, c2), (c2, c0)] {
        let previous_polygon = std::mem::take(&mut polygon);
        let Some(mut previous) = previous_polygon.last().copied() else {
            return Some(0.0);
        };
        let signed_distance = |point| (end - start).perp_dot(point - start) * sign;
        for current in previous_polygon {
            let before = signed_distance(previous);
            let after = signed_distance(current);
            if (before >= 0.0) != (after >= 0.0) {
                polygon.push(previous.lerp(current, before / (before - after)));
            }
            if after >= 0.0 {
                polygon.push(current);
            }
            previous = current;
        }
    }
    let origin = polygon.first().copied()?;
    let projected_area = polygon
        .windows(2)
        .map(|pair| {
            let (Some(first), Some(second)) = (pair.first(), pair.get(1)) else {
                return 0.0;
            };
            (*first - origin).perp_dot(*second - origin)
        })
        .sum::<f32>()
        .abs()
        * 0.5;
    let dominant = nx.max(ny).max(nz);
    Some(projected_area / dominant)
}
