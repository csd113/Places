//! On-demand authoring diagnostics. Findings describe measured evidence or
//! bounded samples, never a proof that a level is safe.
//!
//! This owns no document mutation: a preview returns a candidate for the
//! ordinary validated history.

mod boundaries;
mod items;
mod models;

#[cfg(test)]
#[path = "tests/safety.rs"]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use serde_json::Value;

use crate::editor::document::ObjectId;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::spatial::Aabb;
use crate::map_compiler::geometry_check::{self, Severity};

pub const LIMITATIONS: &str = "Bounded review, not an exhaustive collision or watertightness proof. Edges are sampled per storey; door barriers use their authored starting pose and can change when opened. Play-test jumps, openings and pools. The room flood samples one reachable height per cell. Model checks inspect static triangles in their authored pose; animation, physics, alpha pixels and every camera/depth condition are not simulated. Lowest opaque model faces touching a floor are reported separately as support contact; thin sheets and prop/prop duplicates remain eligible. Decals use their own depth-biased pass and are excluded. Intent annotations suppress only named heuristic warnings. Automatic fixes never resolve artistic intersections.";
const MAX_FINDINGS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyCategory {
    Void,
    Collision,
    Coplanar,
    GeometryOverlap,
    MaterialDepth,
    Review,
}

impl SafetyCategory {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Void => "Void / openings",
            Self::Collision => "Player collision",
            Self::Coplanar => "Duplicate / coplanar",
            Self::GeometryOverlap => "Geometry overlap",
            Self::MaterialDepth => "Material / depth",
            Self::Review => "Review limits",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetySeverity {
    Error,
    Warning,
    Info,
}

impl SafetySeverity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Review",
            Self::Info => "Note",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SafetyReport {
    pub findings: Vec<SafetyFinding>,
    pub limitations: String,
}

#[derive(Clone, Debug)]
pub struct SafetyFinding {
    pub check: &'static str,
    pub category: SafetyCategory,
    pub severity: SafetySeverity,
    pub title: String,
    pub risk: String,
    pub position: [f32; 3],
    pub bounds: Aabb,
    pub objects: Vec<ObjectId>,
    /// Actual measured triangles when available, rather than invented patches.
    pub surfaces: Vec<[[f32; 3]; 3]>,
    pub fix: Option<SafetyFix>,
    pub placement_ignore: Option<crate::editor::document::intent::PlacementIgnoreState>,
}

#[derive(Clone, Debug)]
pub struct SafetyFix {
    pub title: String,
    pub explanation: String,
    expected: Arc<Value>,
    remove_wall: usize,
}

impl SafetyFix {
    /// Produces a complete candidate without mutating the document. Any source
    /// edit invalidates it; acceptance must use the document's normal validation
    /// and one undo step. Reanalysis is required after every applied fix.
    ///
    /// # Errors
    /// Rejects changed source or a missing wall, leaving the source untouched.
    pub fn preview(&self, current_raw: &Value) -> Result<Value, String> {
        if current_raw != self.expected.as_ref() {
            return Err(
                "Safety result is stale. Run Safety again before applying this fix.".into(),
            );
        }
        let mut candidate = current_raw.clone();
        let walls = candidate
            .get_mut("walls")
            .and_then(Value::as_array_mut)
            .ok_or("The duplicate wall collection no longer exists")?;
        if self.remove_wall >= walls.len() {
            return Err("The duplicate wall no longer exists".into());
        }
        drop(walls.remove(self.remove_wall));
        crate::editor::document::intent::ensure_automatic_edit_allowed(current_raw, &candidate)?;
        Ok(candidate)
    }
}

/// Shared candidate gate for individual or future batch correction workflows.
///
/// Ignored issues are skipped; every remaining candidate independently checks
/// that no actively protected placement or affected support context changes.
#[must_use]
pub fn preview_corrections(raw: &Value, findings: &[SafetyFinding]) -> Vec<Result<Value, String>> {
    findings
        .iter()
        .filter(|finding| {
            !finding
                .placement_ignore
                .as_ref()
                .is_some_and(|state| state.active)
        })
        .filter_map(|finding| finding.fix.as_ref().map(|fix| fix.preview(raw)))
        .collect()
}

/// Runs production architecture checks and bounded editor-specific checks.
/// Asset loading and CPU geometry preparation belong on the editor worker.
#[must_use]
pub fn analyze(level: &LevelDef, raw: &Value) -> SafetyReport {
    if let Some(reason) = complexity_limit(level) {
        return budget_report(&reason);
    }
    let mut prepared = level.clone();
    let catalog = crate::engine::loader::PropCatalog::load_default();
    crate::map_compiler::prepare_level(&mut prepared, catalog.assets(), None);
    if let Some(reason) = complexity_limit(&prepared) {
        return budget_report(&reason);
    }
    let checked = geometry_check::check_level(&prepared, "editor document", true);
    let mut report = SafetyReport {
        findings: checked
            .findings
            .into_iter()
            .map(|finding| {
                let objects = references(level, &finding.id, finding.position);
                let bounds = if finding.surfaces.is_empty() {
                    reference_bounds(level, &objects)
                        .unwrap_or_else(|| marker_bounds(finding.position))
                } else {
                    finding
                        .surfaces
                        .iter()
                        .flatten()
                        .fold(Aabb::EMPTY, |bounds, point| bounds.expanded(*point))
                };
                SafetyFinding {
                    check: finding.check,
                    category: category(finding.check),
                    severity: match finding.severity {
                        Severity::Error => SafetySeverity::Error,
                        Severity::Warning => {
                            if finding.check == "shared-storey-plane" {
                                SafetySeverity::Info
                            } else {
                                SafetySeverity::Warning
                            }
                        }
                    },
                    title: format!("{}: {}", finding.check, finding.id),
                    risk: corrected_risk(finding.check, finding.message),
                    position: finding.position,
                    bounds,
                    objects,
                    surfaces: finding.surfaces,
                    fix: None,
                    placement_ignore: None,
                }
            })
            .collect(),
        limitations: LIMITATIONS.into(),
    };
    if !checked.suppressed.is_empty() {
        let _suppression_details = write!(
            report.limitations,
            " Production checker intent suppressions: {:?}.",
            checked.suppressed
        );
    }
    duplicate_wall_fixes(level, raw, &mut report);
    items::check(&prepared, &mut report);
    boundaries::check(&prepared, &mut report);
    models::check(&prepared, &mut report);
    prioritize(&mut report.findings);
    if report.findings.len() > MAX_FINDINGS {
        let omitted = report.findings.len().saturating_sub(MAX_FINDINGS);
        report.findings.truncate(MAX_FINDINGS);
        let _limit_details = write!(
            report.limitations,
            " Finding display limit reached; {omitted} additional findings omitted."
        );
        report.findings.push(SafetyFinding {
            check: "finding-display-limit", category: SafetyCategory::Review, severity: SafetySeverity::Warning,
            title: "Additional findings omitted".into(),risk: format!("The interactive display retained {MAX_FINDINGS} findings and omitted {omitted} more. This review is incomplete. Fix individually, then run Safety again."),
            position: [0.0;3],bounds: marker_bounds([0.0;3]),objects: Vec::new(),surfaces: Vec::new(),fix: None, placement_ignore: None,
        });
    }
    report.findings.insert(0, SafetyFinding {
        check: "audit-scope", category: SafetyCategory::Review, severity: SafetySeverity::Info,
        title: "Review scope and limits".into(),
        risk: "Bounded checks, not a safety proof. Edges are sampled; Play-test jumps, openings and pools. Model checks use authored static triangles; motion and alpha pixels are not simulated. Decals use a separate depth-biased pass. Artistic intersections are never auto-fixed. Missing models or budget limits produce separate findings.".into(),
        position: [0.0;3], bounds: marker_bounds([0.0;3]), objects: Vec::new(), surfaces: Vec::new(), fix: None, placement_ignore: None,
    });
    crate::editor::document::intent::annotate_report(raw, &mut report);
    report
}

fn budget_report(reason: &str) -> SafetyReport {
    SafetyReport {
        findings: vec![SafetyFinding {
            check: "analysis-budget",
            category: SafetyCategory::Review,
            severity: SafetySeverity::Warning,
            title: "Safety analysis exceeds the interactive budget".into(),
            risk: format!(
                "{reason} This review is incomplete; no fixes were generated. Review a smaller scene or use the explicit geometry checker. The scene is unchanged."
            ),
            position: [0.0; 3],
            bounds: marker_bounds([0.0; 3]),
            objects: Vec::new(),
            surfaces: Vec::new(),
            fix: None,
            placement_ignore: None,
        }],
        limitations: LIMITATIONS.into(),
    }
}

// The offline checker accepts large jobs. Bound this interactive worker before
// allocating its mesh or flood grids; source/build limits remain unchanged.
fn complexity_limit(level: &LevelDef) -> Option<String> {
    let count = [
        level.rooms.len(),
        level.walls.len(),
        level.props.len(),
        level.doors.len(),
        level.ramps.len(),
        level.stairs.len(),
        level.floor_regions.len(),
        level.floor_patches.len(),
        level.half_walls.len(),
        level.columns.len(),
        level.arc_walls.len(),
        level.pillars.len(),
        level.archways.len(),
        level.guardrails.len(),
        level.baseboards.len(),
        level.ceiling_lights.len(),
        level.decals.len(),
        level.thresholds.len(),
        level.void_walls.len(),
        level.water.len(),
    ]
    .into_iter()
    .fold(0_usize, usize::saturating_add);
    if count > 4_096 || level.rooms.len() > 128 || level.props.len() > 512 {
        return Some(format!(
            "Source has {count} geometry objects; interactive limits are 4096 total, 128 rooms and 512 props."
        ));
    }
    let estimate = level.estimate_geometry();
    if estimate.total_vertices > 600_000 {
        return Some(format!(
            "Estimated geometry has {} vertices; the interactive limit is 600000.",
            estimate.total_vertices
        ));
    }
    // 25 cm cells contribute sixteen samples per square metre. Include the
    // outer flood margin; this overestimates the checker's coarsened grids.
    let cells = level
        .room_iter()
        .map(|room| {
            (f64::from(room.width.abs()) + 4.5_f64)
                * (f64::from(room.depth.abs()) + 4.5_f64)
                * 16.0_f64
        })
        .sum::<f64>();
    let collider_bound = u32::try_from(
        level
            .collision_aabbs()
            .len()
            .saturating_add(level.doors.len()),
    )
    .unwrap_or(u32::MAX);
    let collider_work = cells * f64::from(collider_bound.max(1));
    if collider_work > 32_000_000.0_f64 {
        return Some(format!(
            "Estimated floor/collider review exceeds 32 million sample checks ({collider_work:.0})."
        ));
    }
    None
}

fn prioritize(findings: &mut [SafetyFinding]) {
    findings.sort_by_key(|finding| {
        (
            finding.fix.is_none(),
            match finding.severity {
                SafetySeverity::Error => 0_u8,
                SafetySeverity::Warning => 1_u8,
                SafetySeverity::Info => 2_u8,
            },
            finding.category != SafetyCategory::Collision,
        )
    });
}

fn corrected_risk(check: &str, message: String) -> String {
    let observation = message
        .split_once(';')
        .map_or(message.as_str(), |(first, _)| first);
    match check {
        "prop-layer-coplanar" => format!(
            "{observation}. Catalog/declared-envelope estimate, not measured GLB coplanarity. Fitted or sloped models may be intentional. Compare actual triangle findings; no automatic move is offered."
        ),
        "prop-duplicate" => format!(
            "{observation}. Repeated placement evidence, not a material/depth proof. Compare actual triangle findings. No automatic prop deletion: instance behavior may differ."
        ),
        _ => message,
    }
}

fn category(check: &str) -> SafetyCategory {
    match check {
        "room-leak" | "missing-wall" | "spawn-outside-room" => SafetyCategory::Void,
        "duplicate-surface" | "coplanar-sliver" | "overlap-emission" | "reversed-face" => {
            SafetyCategory::Coplanar
        }
        "prop-layer-coplanar" => SafetyCategory::MaterialDepth,
        "prop-duplicate" | "opening-overlap" => SafetyCategory::GeometryOverlap,
        "geometry-check-incomplete" => SafetyCategory::Review,
        _ => SafetyCategory::Collision,
    }
}

fn duplicate_wall_fixes(level: &LevelDef, raw: &Value, report: &mut SafetyReport) {
    let Some(walls) = raw.get("walls").and_then(Value::as_array) else {
        return;
    };
    let expected = Arc::new(raw.clone());
    let mut first_records = BTreeMap::new();
    for (remove, wall) in walls.iter().enumerate() {
        let mut comparable = wall.clone();
        if let Some(record) = comparable.as_object_mut() {
            drop(record.remove("editor_instance_id"));
        }
        let Ok(signature) = serde_json::to_string(&comparable) else {
            continue;
        };
        let keep = match first_records.entry(signature) {
            std::collections::btree_map::Entry::Vacant(record) => {
                let _first = record.insert(remove);
                continue;
            }
            std::collections::btree_map::Entry::Occupied(record) => *record.get(),
        };
        let objects = vec![ObjectId::Wall(keep), ObjectId::Wall(remove)];
        let Some(bounds) = reference_bounds(level, &objects) else {
            continue;
        };
        let safe = wall.as_object().is_some_and(|record| {
            record.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "x" | "y"
                        | "z"
                        | "width"
                        | "depth"
                        | "height"
                        | "material"
                        | "shine"
                        | "editor_instance_id"
                )
            })
        });
        report.findings.push(SafetyFinding {
            check: "exact-duplicate-wall",
            category: SafetyCategory::Coplanar,
            severity: SafetySeverity::Warning,
            title: format!("Identical walls {} and {}", keep.saturating_add(1), remove.saturating_add(1)),
            risk: "These raw wall records are identical. Duplicate collision and faces can cause redundant work and depth flicker. Only walls with recognized scalar fields qualify for removal; unknown metadata, openings and per-face data require manual review.".into(),
            position: bounds_center(bounds),
            bounds,
            objects,
            surfaces: Vec::new(),
            placement_ignore: None,
            fix: safe.then(|| SafetyFix {
                title: format!("Remove duplicate wall {}", remove.saturating_add(1)),
                explanation: format!("Removes only wall {}; keeps identical wall {}. Apply creates one undo step. Any source edit invalidates this preview.", remove.saturating_add(1), keep.saturating_add(1)),
                expected: Arc::clone(&expected),
                remove_wall: remove,
            }),
        });
    }
}

fn references(level: &LevelDef, id: &str, point: [f32; 3]) -> Vec<ObjectId> {
    let mut result = Vec::new();
    let words: Vec<_> = id.split_whitespace().collect();
    for pair in words.windows(2) {
        let (Some(kind), Some(index_text)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let Ok(index) = index_text.parse::<usize>() else {
            continue;
        };
        let candidate = match *kind {
            "room" => Some(ObjectId::Room(index)),
            "wall" => Some(ObjectId::Wall(index)),
            "prop" => Some(ObjectId::Prop(index)),
            "ramp" => Some(ObjectId::Ramp(index)),
            _ => None,
        };
        if let Some(object) = candidate
            && !result.contains(&object)
        {
            result.push(object);
        }
    }
    for (index, prop) in level.props.iter().enumerate() {
        if prop.id.as_deref().unwrap_or(&prop.model) == id {
            result.push(ObjectId::Prop(index));
        }
    }
    if result.is_empty() {
        let [x, _, z] = point;
        for (index, room) in level.rooms.iter().enumerate() {
            if room.contains(x, z) {
                result.push(ObjectId::Room(index));
            }
        }
    }
    result
}

pub(super) fn reference_bounds(level: &LevelDef, objects: &[ObjectId]) -> Option<Aabb> {
    let surfaces = LevelSurfaces::new(level);
    objects
        .iter()
        .filter_map(|object| match *object {
            ObjectId::Room(index) => level.rooms.get(index).map(|room| {
                let (x0, x1, z0, z1) = room.bounds();
                Aabb {
                    min: [x0, room.floor_y, z0],
                    max: [x1, room.eave_y(), z1],
                }
            }),
            ObjectId::Wall(index) => level.walls.get(index).map(|wall| {
                let (x0, z0) = wall.min_corner();
                Aabb {
                    min: [x0, wall.y, z0],
                    max: [
                        x0 + wall.width.abs(),
                        wall.y
                            + wall.resolved_height(
                                surfaces.clear_ceiling_height_along(wall, wall.length() * 0.5),
                            ),
                        z0 + wall.depth.abs(),
                    ],
                }
            }),
            ObjectId::Ramp(index) => level.ramps.get(index).map(|ramp| {
                let (x0, x1, z0, z1) = ramp.bounds();
                let floor = surfaces
                    .ramp_room(ramp)
                    .map_or(0.0, |(_, room)| room.floor_y);
                Aabb {
                    min: [x0, floor + ramp.surface().low_offset(), z0],
                    max: [x1, floor + ramp.surface().high_offset(), z1],
                }
            }),
            ObjectId::FloorRegion(index) => level.floor_regions.get(index).map(|region| {
                let (x0, x1, z0, z1) = region.bounds();
                let floor = surfaces
                    .room_floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
                    .unwrap_or(0.0);
                Aabb {
                    min: [x0, floor + region.offset().min(0.0), z0],
                    max: [x1, floor + region.offset().max(0.0), z1],
                }
            }),
            ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_)
            | ObjectId::SpawnPoint(_) => None,
            ObjectId::Prop(index) => level.props.get(index).map(|prop| {
                prop.placement_bounds(
                    surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0),
                    prop.size
                        .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE),
                )
            }),
        })
        .reduce(|first, second| first.union(&second))
}

pub(super) fn bounds_center(bounds: Aabb) -> [f32; 3] {
    std::array::from_fn(|axis| {
        f32::midpoint(
            bounds.min.get(axis).copied().unwrap_or(0.0),
            bounds.max.get(axis).copied().unwrap_or(0.0),
        )
    })
}

fn marker_bounds(position: [f32; 3]) -> Aabb {
    Aabb {
        min: position.map(|value| value - 0.15),
        max: position.map(|value| value + 0.15),
    }
}
