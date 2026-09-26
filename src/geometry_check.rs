//! The map geometry checker: a read-only, deterministic validator for level
//! geometry.
//!
//! Run it from the game binary:
//!
//! ```text
//! places --check-geometry --level assets/levels/places_demo.json
//! places --check-geometry --level levels/level0_pit.json --json report.json
//! places --check-geometry --level places_demo --markers-obj markers.obj
//! ```
//!
//! It builds the level exactly the way the game does — the loader validation,
//! the level preparation pass (fixture/decal snapping and automatic trim), the
//! static geometry emitter and the collision derivation — and then reports
//! what is measurably wrong in that interpretation. Nothing here re-implements
//! an emitter: the checks read the produced [`LevelMesh`], the engine's own
//! collision boxes and the level data.
//!
//! # Confirmed defects and heuristic warnings
//!
//! Every finding is either an **error** (a confirmed, measurable defect such as
//! a degenerate triangle, a duplicated coplanar surface, an overlapping
//! opening pair or an uncovered curved primitive) or a **warning** (a
//! heuristic such as a face pair with opposing normals, a room leaking into
//! the void, a perimeter run with no wall or opening, or a collider with no
//! nearby mesh). Warnings
//! can be deliberate and are then suppressed by a narrow
//! [`crate::level::GeometryIntentDef`] annotation in `geometry_intent`; errors
//! are never suppressed.
//!
//! # Honest limitations
//!
//! The checker proves nothing about levels it does not read, and it is not a
//! watertightness proof: open-plan edges, doorways, stair openings, pools,
//! carpet holes and every other intentionally open space are legitimate and
//! are only flagged when they read as a leak into the *void*. It sees the
//! asset-less mesh (prop placeholders, no GLB geometry) and does not validate
//! prop models, textures or the rendered image. It reports findings, not
//! fixes.
//!
//! Exit statuses: `0` no confirmed defects, `1` confirmed defects, `2` usage or
//! file/parse failure.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;

use crate::collision::{CROUCH_HEIGHT, WallAabb};
use crate::level::{
    ArcWallDef, ArchitectureBox, LevelDef, LevelSurfaces, PillarDef, RoomDef, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use crate::render::{LevelMesh, SurfaceKey, SurfaceKind, Vertex};

/// Stable format id written into the machine-readable report.
pub const CHECK_FORMAT: &str = "places-geometry-check";
/// Report schema version; bumped when a field changes meaning.
pub const CHECK_VERSION: u32 = 1;

/// Tolerance within which two planes are "the same plane", in metres.
const PLANE_EPS_M: f32 = 1.0e-3;
/// Minimum real overlap area before two coplanar surfaces are reported at all.
const OVERLAP_AREA_EPS_M2: f32 = 1.0e-4;
/// Coplanar overlaps up to this area are reported as `coplanar-sliver`
/// warnings rather than confirmed errors: a square centimetre of coincident
/// geometry at a joint is measurable but sub-pixel in normal play, while a
/// patch above this is worth a confirmed finding.
const SLIVER_AREA_EPS_M2: f32 = 1.0e-3;
/// Distance within which a mesh triangle counts as lying on a collider face.
const FACE_SAMPLE_EPS_M: f32 = 0.003;
/// The player body used for the room-leak flood fill: the crouched height, so
/// a route a crouched player could take still counts as connected.
const LEAK_BODY_HEIGHT_M: f32 = CROUCH_HEIGHT;
/// Body radius used for the room-leak flood fill, slightly under the player's,
/// so a cell gap narrower than the real body is still blocked.
const LEAK_BODY_RADIUS_M: f32 = 0.25;
/// Cell size of the leak flood fill, in metres.
const LEAK_CELL_M: f32 = 0.25;
/// How far outside its own footprint a room's flood fill may spread, in metres.
const LEAK_MARGIN_M: f32 = 2.0;
/// Spacing of the perimeter coverage samples, in metres.
const PERIMETER_STEP_M: f32 = 0.5;
/// Uncovered perimeter run length that is worth reporting, in metres.
const MISSING_RUN_M: f32 = 1.0;
/// Curve sagitta above which a tessellation is reported as coarse, in metres.
const CURVE_COARSE_SAGITTA_M: f32 = 0.02;
/// Caps on how many findings one check lists; the summary still counts every
/// finding, and one final entry names the truncation.
const MAX_FINDINGS_PER_CHECK: usize = 200;
/// Cell size of the triangle spatial hash, in metres.
const SPATIAL_CELL_M: f32 = 1.0;

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    /// A confirmed defect.
    Error,
    /// A heuristic warning; may be intentional and can be annotated.
    Warning,
}

impl Severity {
    /// Lower-case name used in the reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// One reported finding: its check, severity, element id and a locatable
/// position.
#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    /// Stable check id, e.g. `duplicate-surface`.
    pub check: &'static str,
    /// `error` or `warning`.
    pub severity: Severity,
    /// The element the finding belongs to (`wall 3`, `room 2`, `pillar 0`, ...).
    pub id: String,
    /// Human-readable explanation, including the numbers that proved it.
    pub message: String,
    /// Anchor position for the finding, world `(x, y, z)`.
    pub position: [f32; 3],
}

/// One reproducible marker anchor, for a viewer or an overlay.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Marker {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Everything one checker run found.
#[derive(Clone, Debug)]
pub struct CheckReport {
    /// Where the level was read from (a path, or a labelled embedded source).
    pub source: String,
    /// The level's own `id`.
    pub level_id: String,
    /// Whether the loader's own validation accepted the level.
    pub validated: bool,
    /// Every finding, in deterministic order.
    pub findings: Vec<Finding>,
    /// How many findings were suppressed by a `geometry_intent` annotation.
    pub suppressed: BTreeMap<String, usize>,
}

impl CheckReport {
    /// Confirmed defects.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    /// Heuristic warnings.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }

    /// The process exit status the report implies.
    #[must_use]
    pub fn exit_status(&self, strict: bool) -> i32 {
        i32::from(self.error_count() > 0 || (strict && self.warning_count() > 0))
    }

    /// Marker anchors for every finding (errors first).
    #[must_use]
    pub fn markers(&self) -> Vec<Marker> {
        self.findings
            .iter()
            .map(|finding| Marker {
                x: finding.position[0],
                y: finding.position[1],
                z: finding.position[2],
            })
            .collect()
    }

    /// The human-readable report.
    #[must_use]
    pub fn to_human(&self, verbose: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "geometry check: {} ({}){}",
            self.level_id,
            self.source,
            if self.validated {
                ""
            } else {
                " [loader validation failed]"
            }
        );
        let mut by_check: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for finding in &self.findings {
            let entry = by_check.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 = entry.0.saturating_add(1);
            } else {
                entry.1 = entry.1.saturating_add(1);
            }
        }
        for (check, (errors, warnings)) in &by_check {
            let _ = writeln!(out, "  {check}: {errors} error(s), {warnings} warning(s)");
        }
        for (check, count) in &self.suppressed {
            if *count > 0 {
                let _ = writeln!(out, "  {check}: {count} suppressed by geometry_intent");
            }
        }
        for finding in &self.findings {
            if !verbose && finding.severity == Severity::Warning {
                continue;
            }
            let _ = writeln!(
                out,
                "{} [{}] {}: {} @ ({:.3}, {:.3}, {:.3})",
                match finding.severity {
                    Severity::Error => "ERROR  ",
                    Severity::Warning => "WARNING",
                },
                finding.check,
                finding.id,
                finding.message,
                finding.position[0],
                finding.position[1],
                finding.position[2]
            );
        }
        let _ = writeln!(
            out,
            "summary: {} error(s), {} warning(s){}",
            self.error_count(),
            self.warning_count(),
            if self.suppressed.values().sum::<usize>() > 0 {
                ", some warnings suppressed by intent annotations"
            } else {
                ""
            }
        );
        out
    }

    /// The machine-readable report.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` error if the report cannot be serialized,
    /// which cannot happen for finite geometry; callers may treat it as fatal.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut checks: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for finding in &self.findings {
            let entry = checks.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 = entry.0.saturating_add(1);
            } else {
                entry.1 = entry.1.saturating_add(1);
            }
        }
        let report = serde_json::json!({
            "format": CHECK_FORMAT,
            "version": CHECK_VERSION,
            "level": { "id": self.level_id, "source": self.source, "validated": self.validated },
            "summary": {
                "errors": self.error_count(),
                "warnings": self.warning_count(),
                "findings": self.findings.len(),
                "checks": checks
                    .into_iter()
                    .map(|(check, (errors, warnings))| {
                        (check.to_string(), serde_json::json!({ "errors": errors, "warnings": warnings }))
                    })
                    .collect::<serde_json::Map<_, _>>(),
                "suppressed": self.suppressed,
            },
            "findings": self.findings,
            "markers": self.markers(),
        });
        serde_json::to_string_pretty(&report)
    }
}

/// The CLI options of `--check-geometry`.
#[derive(Clone, Debug)]
pub struct CliOptions {
    /// Level path or id (`places_demo`, `assets/levels/places_demo.json`, ...).
    pub level: String,
    /// Write the machine-readable report here, in addition to the human one.
    pub json: Option<PathBuf>,
    /// Write deterministic marker anchors (JSON) here.
    pub markers: Option<PathBuf>,
    /// Write a marker OBJ (a small cross per finding) here.
    pub markers_obj: Option<PathBuf>,
    /// Treat warnings as a failing status.
    pub strict: bool,
    /// Print warnings in the human report too.
    pub verbose: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            level: "places_demo".to_string(),
            json: None,
            markers: None,
            markers_obj: None,
            strict: false,
            verbose: true,
        }
    }
}

/// Parses the process arguments, or `None` when this is not a checker run.
///
/// # Errors
///
/// Returns a usage message for an unknown argument or a missing value.
pub fn options_from_args(args: &[String]) -> Result<Option<CliOptions>, String> {
    if !args.iter().any(|arg| arg == "--check-geometry") {
        return Ok(None);
    }
    let mut options = CliOptions::default();
    let mut index = 0usize;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--check-geometry" => {}
            "--level" => {
                index = index.saturating_add(1);
                options.level = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--level needs a value".to_string())?;
            }
            "--json" => {
                index = index.saturating_add(1);
                options.json = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--json needs a value".to_string())?,
                ));
            }
            "--markers" => {
                index = index.saturating_add(1);
                options.markers = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--markers needs a value".to_string())?,
                ));
            }
            "--markers-obj" => {
                index = index.saturating_add(1);
                options.markers_obj = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--markers-obj needs a value".to_string())?,
                ));
            }
            "--strict" => options.strict = true,
            "--quiet" => options.verbose = false,
            other if other.starts_with("--") => {
                return Err(format!("unknown checker argument `{other}`"));
            }
            other => options.level = other.to_string(),
        }
        index = index.saturating_add(1);
    }
    Ok(Some(options))
}

/// Resolves a level argument to a readable source: an explicit path, a
/// shipped level id, or a drop-in level id under `levels/`.
fn resolve_source(level: &str) -> Result<(PathBuf, String), String> {
    let candidates: Vec<PathBuf> = if level.to_ascii_lowercase().ends_with(".json") {
        vec![PathBuf::from(level)]
    } else {
        vec![
            PathBuf::from(format!("assets/levels/{level}.json")),
            PathBuf::from(format!("levels/{level}.json")),
        ]
    };
    for candidate in candidates {
        match std::fs::read_to_string(&candidate) {
            Ok(text) => return Ok((candidate, text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("cannot read {}: {error}", candidate.display()));
            }
        }
    }
    Err(format!("no level matched `{level}`"))
}

/// Runs the checker for one already-parsed level.
#[must_use]
pub fn check_level(level: &LevelDef, source: &str, validated: bool) -> CheckReport {
    let mut checker = Checker::new(level);
    let surfaces = LevelSurfaces::new(level);

    let materials = crate::render::logical_materials(level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mesh =
        crate::render::build_level_geometry_with_catalog_and_materials(level, &catalog, &materials);

    checker.check_mesh(&mesh);
    let triangles = collect_triangles(&mesh);
    checker.check_duplicate_surfaces(&triangles);

    let colliders = authored_colliders(level, &surfaces);
    let engine_boxes = level.collision_aabbs();
    let index = SpatialHash::build(&triangles);
    checker.check_colliders(&colliders, &engine_boxes, &triangles, &index);
    checker.check_openings(&surfaces);
    checker.check_curves(&surfaces);
    checker.check_rooms(&surfaces, &colliders);
    checker.check_spawn(&surfaces);

    let mut report = CheckReport {
        source: source.to_string(),
        level_id: level.id.clone(),
        validated,
        findings: checker.findings,
        suppressed: checker.suppressed,
    };
    report.findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.check.cmp(b.check))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.message.cmp(&b.message))
    });
    report
}

/// Runs the checker end to end and returns the exit status.
///
/// # Errors
///
/// Returns a usage or file error, which the caller reports as status `2`.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn run(options: &CliOptions) -> Result<i32, String> {
    let (path, text) = resolve_source(&options.level)?;
    let mut level = match LevelDef::from_json(&text) {
        Ok(level) => level,
        Err(error) => {
            eprintln!("geometry check: {} does not parse: {error}", path.display());
            return Ok(2);
        }
    };
    let validation = crate::loader::validate_level(&level);
    let validated = validation.is_ok();
    if let Err(error) = validation {
        // The loader rejected the level; still report every measurable defect
        // that can be read from the parsed document, but do not run the
        // preparation pass the game would.
        let mut report = CheckReport {
            source: path.display().to_string(),
            level_id: level.id.clone(),
            validated: false,
            findings: vec![Finding {
                check: "level-invalid",
                severity: Severity::Error,
                id: level.id.clone(),
                message: format!("loader validation rejected the level: {error}"),
                position: [0.0, 0.0, 0.0],
            }],
            suppressed: BTreeMap::new(),
        };
        let checked = check_level(&level, &path.display().to_string(), false);
        report.findings.extend(checked.findings);
        report.suppressed = checked.suppressed;
        return finish(&report, options);
    }
    let catalog = crate::assets::AssetCatalog::load_default();
    crate::loader::prepare_level(&mut level, &catalog, None);
    let report = check_level(&level, &path.display().to_string(), validated);
    finish(&report, options)
}

/// Prints and writes one report, returning the exit status.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn finish(report: &CheckReport, options: &CliOptions) -> Result<i32, String> {
    let status = report.exit_status(options.strict);
    if options.verbose || report.error_count() > 0 {
        print!("{}", report.to_human(options.verbose));
    }
    if let Some(path) = &options.json {
        let json = report
            .to_json()
            .map_err(|error| format!("cannot serialize the report: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if let Some(path) = &options.markers {
        let json = serde_json::to_string_pretty(&report.markers())
            .map_err(|error| format!("cannot serialize markers: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if let Some(path) = &options.markers_obj {
        let obj = marker_obj(&report.markers());
        std::fs::write(path, obj)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(status)
}

/// A tiny OBJ of axis-aligned crosses, one per marker.
fn marker_obj(markers: &[Marker]) -> String {
    let mut out = String::from("# places geometry-check markers\n");
    if markers.is_empty() {
        return out;
    }
    for marker in markers {
        let size = 0.2_f32;
        let anchor = [marker.x, marker.y, marker.z];
        for axis in 0..3usize {
            let mut low = anchor;
            let mut high = anchor;
            if let Some(slot) = low.get_mut(axis) {
                *slot -= size;
            }
            if let Some(slot) = high.get_mut(axis) {
                *slot += size;
            }
            let _ = writeln!(out, "v {} {} {}", low[0], low[1], low[2]);
            let _ = writeln!(out, "v {} {} {}", high[0], high[1], high[2]);
        }
    }
    for line in 0..markers.len() {
        let base = line.saturating_mul(6).saturating_add(1);
        let _ = writeln!(out, "l {base} {}", base.saturating_add(1));
        let _ = writeln!(
            out,
            "l {} {}",
            base.saturating_add(2),
            base.saturating_add(3)
        );
        let _ = writeln!(
            out,
            "l {} {}",
            base.saturating_add(4),
            base.saturating_add(5)
        );
    }
    out
}

/// Which surface families count as architecture for the duplicate check.
const fn is_architecture_kind(kind: SurfaceKind) -> bool {
    matches!(
        kind,
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
    )
}

/// One emitted triangle, reduced to what the checks read.
#[derive(Clone, Copy, Debug)]
struct Tri {
    key: SurfaceKey,
    points: [[f32; 3]; 3],
    normal: [f32; 3],
    centroid: [f32; 3],
    area: f32,
}

/// Collects every triangle of the architecture families.
fn collect_triangles(mesh: &LevelMesh) -> Vec<Tri> {
    let mut out = Vec::new();
    for range in &mesh.ranges {
        if !is_architecture_kind(range.key.kind) {
            continue;
        }
        let mut chunk = [0u16; 3];
        let mut filled = 0usize;
        for index in &range.indices {
            if let Some(slot) = chunk.get_mut(filled) {
                *slot = *index;
            }
            filled = filled.saturating_add(1);
            if filled < 3 {
                continue;
            }
            filled = 0;
            let a = range
                .vertices
                .get(usize::from(chunk[0]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let b = range
                .vertices
                .get(usize::from(chunk[1]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let c = range
                .vertices
                .get(usize::from(chunk[2]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let points = [a.pos, b.pos, c.pos];
            let cross = cross3(sub3(points[1], points[0]), sub3(points[2], points[0]));
            let area = 0.5 * length3(cross);
            let normal = normalized3(cross);
            let centroid = [
                (points[0][0] + points[1][0] + points[2][0]) / 3.0,
                (points[0][1] + points[1][1] + points[2][1]) / 3.0,
                (points[0][2] + points[1][2] + points[2][2]) / 3.0,
            ];
            out.push(Tri {
                key: range.key,
                points,
                normal,
                centroid,
                area,
            });
        }
    }
    out
}

/// A collider with the authored element it came from.
#[derive(Clone, Debug)]
struct Collider {
    /// Element label (`wall 3`, `arc wall 0`, ...).
    source: String,
    aabb: WallAabb,
    /// True for a solid whose AABB is *not* surface-tight by design — a
    /// curved primitive (over-covering at the tessellation corners) or a
    /// guardrail (its barrier box deliberately reaches below the rails). Its
    /// coverage is proven by its own checks instead of by ghost probing.
    ghost_checked: bool,
}

/// Every wall and architectural solid as a collider, with provenance.
///
/// This replays the engine's own decomposition (`wall_solid_slices_profiled`
/// plus the architecture solids) so each box can be named; the checker then
/// compares the result against the engine's own `collision_aabbs()` to prove
/// the two interpretations agree.
fn authored_colliders(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<Collider> {
    let mut out = Vec::new();
    for (index, wall) in level.walls.iter().enumerate() {
        let breaks = surfaces.wall_profile_breaks(wall);
        let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
        for slice in wall_solid_slices_profiled(wall, clear, &breaks) {
            out.push(Collider {
                source: format!("wall {index}"),
                aabb: wall_slice_aabb(wall, &slice),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.half_walls.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("half wall {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.columns.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("column {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.arc_walls.iter().enumerate() {
        for boxed in piece.collision_boxes(surfaces) {
            out.push(Collider {
                source: format!("arc wall {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    for (index, piece) in level.pillars.iter().enumerate() {
        for boxed in piece.collision_boxes(surfaces) {
            out.push(Collider {
                source: format!("pillar {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    for (index, piece) in level.archways.iter().enumerate() {
        for boxed in piece.solid_boxes(surfaces) {
            out.push(Collider {
                source: format!("archway {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.guardrails.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("guardrail {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    out
}

/// One solid wall slice as an AABB, mirroring the engine's collision walk.
fn wall_slice_aabb(wall: &WallDef, slice: &crate::level::WallSlice) -> WallAabb {
    let (min_x, max_x) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (min_z, max_z) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let (origin_x, origin_z) = wall.length_origin();
    let (x, z, width, depth) = match wall.axis() {
        WallAxis::X => (
            origin_x + slice.start,
            min_z,
            slice.end - slice.start,
            max_z - min_z,
        ),
        WallAxis::Z => (
            min_x,
            origin_z + slice.start,
            max_x - min_x,
            slice.end - slice.start,
        ),
    };
    WallAabb::with_y(x, slice.bottom, z, width, slice.top - slice.bottom, depth)
}

/// A uniform-grid index from world position to triangle indices.
struct SpatialHash {
    cells: HashMap<(i32, i32, i32), Vec<u32>>,
}

impl SpatialHash {
    fn build(triangles: &[Tri]) -> Self {
        let mut cells: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (index, triangle) in triangles.iter().enumerate() {
            let Some(slot) = u32::try_from(index).ok() else {
                continue;
            };
            for cell in triangle_cells(triangle.points) {
                cells.entry(cell).or_default().push(slot);
            }
        }
        Self { cells }
    }

    fn near(&self, point: [f32; 3]) -> Option<&[u32]> {
        self.cells
            .get(&cell_of(point[0], point[1], point[2]))
            .map(Vec::as_slice)
    }
}

/// The grid cells one triangle touches.
fn triangle_cells(points: [[f32; 3]; 3]) -> Vec<(i32, i32, i32)> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in points {
        for (axis, value) in point.iter().enumerate() {
            if let (Some(lo), Some(hi)) = (min.get_mut(axis), max.get_mut(axis)) {
                *lo = lo.min(*value);
                *hi = hi.max(*value);
            }
        }
    }
    let mut out = Vec::new();
    for ix in cell_range(min[0], max[0]) {
        for iy in cell_range(min[1], max[1]) {
            for iz in cell_range(min[2], max[2]) {
                out.push((ix, iy, iz));
                if out.len() > 64 {
                    return out;
                }
            }
        }
    }
    out
}

/// The inclusive cell indices one span touches, capped.
fn cell_range(low: f32, high: f32) -> std::ops::RangeInclusive<i32> {
    let first = cell_index(low);
    let last = cell_index(high).max(first);
    first..=last.min(first.saturating_add(8))
}

/// Grid cell index of one world coordinate.
fn cell_index(value: f32) -> i32 {
    let scaled = (value / SPATIAL_CELL_M).floor();
    if !scaled.is_finite() {
        return 0;
    }
    // Bounded to a sane world extent before the cast so a wild coordinate can
    // never wrap.
    #[allow(clippy::cast_possible_truncation)]
    let index = scaled.clamp(-1.0e6, 1.0e6) as i32;
    index
}

/// Grid cell containing a point.
fn cell_of(x: f32, y: f32, z: f32) -> (i32, i32, i32) {
    (cell_index(x), cell_index(y), cell_index(z))
}

/// The checker's own state while one level is examined.
struct Checker<'a> {
    level: &'a LevelDef,
    findings: Vec<Finding>,
    suppressed: BTreeMap<String, usize>,
    counts: HashMap<&'static str, usize>,
}

impl<'a> Checker<'a> {
    fn new(level: &'a LevelDef) -> Self {
        Self {
            level,
            findings: Vec::new(),
            suppressed: BTreeMap::new(),
            counts: HashMap::new(),
        }
    }

    /// True when an intent annotation covers this heuristic position.
    fn annotated(&self, check: &str, x: f32, z: f32) -> bool {
        self.level
            .geometry_intent
            .iter()
            .any(|annotation| annotation.covers(check, x, z))
    }

    /// Records one finding, applying the caps and the intent annotations.
    fn push(
        &mut self,
        check: &'static str,
        severity: Severity,
        id: impl Into<String>,
        message: impl Into<String>,
        position: [f32; 3],
    ) {
        if severity == Severity::Warning && self.annotated(check, position[0], position[2]) {
            let entry = self.suppressed.entry(check.to_string()).or_insert(0);
            *entry = entry.saturating_add(1);
            return;
        }
        let count = self.counts.entry(check).or_insert(0);
        *count = count.saturating_add(1);
        if *count > MAX_FINDINGS_PER_CHECK {
            if *count == MAX_FINDINGS_PER_CHECK.saturating_add(1) {
                self.findings.push(Finding {
                    check,
                    severity,
                    id: id.into(),
                    message: format!(
                        "more than {MAX_FINDINGS_PER_CHECK} `{check}` findings; the summary counts them all"
                    ),
                    position,
                });
            }
            return;
        }
        self.findings.push(Finding {
            check,
            severity,
            id: id.into(),
            message: message.into(),
            position,
        });
    }
}

// ---------------------------------------------------------------------------
// Mesh integrity
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Non-finite vertices and degenerate triangles in the emitted mesh.
    fn check_mesh(&mut self, mesh: &LevelMesh) {
        for vertex in mesh.all_vertices() {
            let finite = vertex.pos.iter().all(|value| value.is_finite())
                && vertex.uv.iter().all(|value| value.is_finite())
                && vertex.color.iter().all(|value| value.is_finite())
                && vertex.normal.iter().all(|value| value.is_finite())
                && vertex.tangent.iter().all(|value| value.is_finite());
            if !finite {
                self.push(
                    "non-finite-vertex",
                    Severity::Error,
                    "mesh",
                    "a generated vertex carries a non-finite position, uv, colour or frame",
                    vertex.pos,
                );
                break;
            }
        }
        for range in &mesh.ranges {
            if !is_architecture_kind(range.key.kind) {
                continue;
            }
            let mut chunk: [[f32; 3]; 3] = [[0.0; 3]; 3];
            let mut filled = 0usize;
            for index in &range.indices {
                let Some(vertex) = range.vertices.get(usize::from(*index)) else {
                    continue;
                };
                if let Some(slot) = chunk.get_mut(filled) {
                    *slot = vertex.pos;
                }
                filled = filled.saturating_add(1);
                if filled < 3 {
                    continue;
                }
                filled = 0;
                let area =
                    0.5 * length3(cross3(sub3(chunk[1], chunk[0]), sub3(chunk[2], chunk[0])));
                let coincident = distance_sq(chunk[0], chunk[1]) <= 1.0e-12
                    || distance_sq(chunk[1], chunk[2]) <= 1.0e-12
                    || distance_sq(chunk[0], chunk[2]) <= 1.0e-12;
                if area <= 1.0e-9 || coincident {
                    self.push(
                        "degenerate-face",
                        Severity::Error,
                        format!("{:?} m{}", range.key.kind, range.key.material),
                        format!(
                            "zero-area or repeated-corner triangle (area {area:.3e} m², points {chunk:?})"
                        ),
                        chunk[0],
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Duplicate and coplanar surfaces
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Confirmed duplicated coplanar surfaces: two triangles on one plane
    /// whose projected areas overlap by more than a threshold.
    #[allow(clippy::too_many_lines)] // one coplanar-overlap pass over the bucket map
    fn check_duplicate_surfaces(&mut self, triangles: &[Tri]) {
        // Canonical plane key: a sign-canonical normal and plane distance,
        // quantised to 1 mm so exactly-coincident planes bucket together.
        let mut planes: HashMap<[i32; 4], Vec<usize>> = HashMap::new();
        for (index, triangle) in triangles.iter().enumerate() {
            if triangle.area <= 1.0e-9 {
                continue;
            }
            let mut normal = triangle.normal;
            let mut distance = dot3(normal, triangle.points[0]);
            let flip = normal[0] < 0.0
                || (normal[0] == 0.0 && normal[1] < 0.0)
                || (normal[0] == 0.0 && normal[1] == 0.0 && normal[2] < 0.0);
            if flip {
                normal = [-normal[0], -normal[1], -normal[2]];
                distance = -distance;
            }
            let key = [
                quantize(normal[0]),
                quantize(normal[1]),
                quantize(normal[2]),
                quantize(distance),
            ];
            planes.entry(key).or_default().push(index);
        }
        let mut reported: HashSet<(usize, usize)> = HashSet::new();
        for bucket in planes.values() {
            for (slot, first) in bucket.iter().enumerate() {
                for second in bucket.iter().skip(slot.saturating_add(1)) {
                    if !reported.insert((*first.min(second), *first.max(second))) {
                        continue;
                    }
                    let (Some(a), Some(b)) = (triangles.get(*first), triangles.get(*second)) else {
                        continue;
                    };
                    // Two planes can quantise together while lying up to 1.5 mm
                    // apart; require a real coplanarity before flagging.
                    let separation = dot3(a.normal, sub3(b.centroid, a.points[0])).abs();
                    if separation > PLANE_EPS_M * 1.5 {
                        continue;
                    }
                    let area = triangle_overlap_area(a, b);
                    if area <= OVERLAP_AREA_EPS_M2 {
                        continue;
                    }
                    if area <= SLIVER_AREA_EPS_M2 {
                        self.push(
                            "coplanar-sliver",
                            Severity::Warning,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "a {area:.5} m² coplanar overlap sliver at a joint \
                                 (a at {:.2},{:.2},{:.2}; b at {:.2},{:.2},{:.2})",
                                a.centroid[0],
                                a.centroid[1],
                                a.centroid[2],
                                b.centroid[0],
                                b.centroid[1],
                                b.centroid[2]
                            ),
                            a.centroid,
                        );
                        continue;
                    }
                    let same_family_horizontal = a.key.kind == b.key.kind
                        && matches!(a.key.kind, SurfaceKind::Floor | SurfaceKind::Ceiling);
                    if same_family_horizontal {
                        // Overlapping rooms emit both floors/ceilings by design
                        // (documented), so this is a heuristic: unless an
                        // author annotates the area, it is reported so a
                        // genuinely doubled slab is still visible in the
                        // report.
                        self.push(
                            "overlap-emission",
                            Severity::Warning,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "two {:?} surfaces share a plane and overlap by {area:.4} m² \
                                 (a at {:.2},{:.2},{:.2}; b at {:.2},{:.2},{:.2}); \
                                 overlapping rooms emit both, a doubled slab is a defect",
                                a.key.kind,
                                a.centroid[0],
                                a.centroid[1],
                                a.centroid[2],
                                b.centroid[0],
                                b.centroid[1],
                                b.centroid[2]
                            ),
                            a.centroid,
                        );
                        continue;
                    }
                    if dot3(a.normal, b.normal) < -0.5 {
                        // The same plane covered twice with opposite facings is
                        // a reversed face *or* a legitimate back-to-back pair
                        // (two stacked walls' caps at one junction); without
                        // solid semantics the checker cannot tell them apart,
                        // so this stays a heuristic warning.
                        self.push(
                            "reversed-face",
                            Severity::Warning,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "two coplanar faces overlap by {area:.4} m² with opposite normals \
                                 (a m{} at {:.2},{:.2},{:.2}; b m{} at {:.2},{:.2},{:.2})",
                                a.key.material,
                                a.centroid[0],
                                a.centroid[1],
                                a.centroid[2],
                                b.key.material,
                                b.centroid[0],
                                b.centroid[1],
                                b.centroid[2]
                            ),
                            a.centroid,
                        );
                    } else {
                        self.push(
                            "duplicate-surface",
                            Severity::Error,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "coplanar surfaces overlap by {area:.4} m²: a m{} {:?} n{:?} b m{} {:?} n{:?}",
                                a.key.material, a.points, a.normal, b.key.material, b.points, b.normal
                            ),
                            a.centroid,
                        );
                    }
                }
            }
        }
    }
}

/// Quantises a float to whole millimetres as an integer key.
fn quantize(value: f32) -> i32 {
    let scaled = (value * 1000.0).round();
    if !scaled.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let key = scaled.clamp(-2.0e9, 2.0e9) as i32;
    key
}

/// Overlap area of two triangles projected onto their shared plane.
fn triangle_overlap_area(a: &Tri, b: &Tri) -> f32 {
    let (axis, _) = dominant_axis(a.normal);
    let mut subject: Vec<[f32; 2]> = project_triangle(a.points, axis);
    let mut clip: Vec<[f32; 2]> = project_triangle(b.points, axis);
    if polygon_area(&subject) < 0.0 {
        subject.reverse();
    }
    if polygon_area(&clip) < 0.0 {
        clip.reverse();
    }
    let len = clip.len();
    for index in 0..len {
        let (Some(from), Some(to)) = (clip.get(index), clip.get(wrap_next(index, len))) else {
            continue;
        };
        subject = clip_polygon_left(&subject, *from, *to);
        if subject.len() < 3 {
            return 0.0;
        }
    }
    polygon_area(&subject).abs()
}

/// Projects a triangle to 2D by dropping `axis`.
fn project_triangle(points: [[f32; 3]; 3], axis: usize) -> Vec<[f32; 2]> {
    points
        .iter()
        .map(|point| match axis {
            0 => [point[1], point[2]],
            1 => [point[0], point[2]],
            _ => [point[0], point[1]],
        })
        .collect()
}

/// The next index in a cyclic polygon.
const fn wrap_next(index: usize, len: usize) -> usize {
    let next = index.saturating_add(1);
    if next >= len { 0 } else { next }
}

/// Keeps the part of `polygon` left of `from -> to`.
fn clip_polygon_left(polygon: &[[f32; 2]], from: [f32; 2], to: [f32; 2]) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(polygon.len().saturating_add(2));
    let edge = [to[0] - from[0], to[1] - from[1]];
    let inside = |point: [f32; 2]| {
        let relative = [point[0] - from[0], point[1] - from[1]];
        edge[0].mul_add(relative[1], -(edge[1] * relative[0])) >= -1.0e-6
    };
    let len = polygon.len();
    for index in 0..len {
        let (Some(current), Some(next)) = (polygon.get(index), polygon.get(wrap_next(index, len)))
        else {
            continue;
        };
        let current_inside = inside(*current);
        let next_inside = inside(*next);
        if current_inside {
            out.push(*current);
        }
        if current_inside != next_inside {
            let relative = [next[0] - current[0], next[1] - current[1]];
            let denominator = edge[0].mul_add(relative[1], -(edge[1] * relative[0]));
            if denominator.abs() > 1.0e-9 {
                let numerator =
                    edge[0].mul_add(from[1] - current[1], -(edge[1] * (from[0] - current[0])));
                let t = numerator / denominator;
                out.push([
                    t.mul_add(relative[0], current[0]),
                    t.mul_add(relative[1], current[1]),
                ]);
            }
        }
    }
    out
}

/// Signed area of a 2D polygon (positive counter-clockwise).
fn polygon_area(polygon: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0_f32;
    let len = polygon.len();
    for index in 0..len {
        let (Some(a), Some(b)) = (polygon.get(index), polygon.get(wrap_next(index, len))) else {
            continue;
        };
        sum += a[0].mul_add(b[1], -(b[0] * a[1]));
    }
    sum * 0.5
}

/// The axis a plane's normal is most aligned with.
fn dominant_axis(normal: [f32; 3]) -> (usize, f32) {
    let mut axis = 0usize;
    let mut best = normal[0].abs();
    for (candidate, value) in normal.iter().enumerate().skip(1) {
        if value.abs() > best {
            best = value.abs();
            axis = candidate;
        }
    }
    (axis, best)
}

// ---------------------------------------------------------------------------
// Collision
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Collider duplicates, missing engine boxes and ghost colliders.
    ///
    /// Reversed faces are found by the coplanar overlap pass (an opposing
    /// facing pair on one plane), never by proximity to a collider face: an
    /// archway's soffit or a threshold top inside a bounding box is ordinary
    /// geometry, and a box AABB cannot tell ownership.
    #[allow(clippy::too_many_lines)] // one collider pass, kept together for the shared index
    fn check_colliders(
        &mut self,
        colliders: &[Collider],
        engine_boxes: &[WallAabb],
        triangles: &[Tri],
        index: &SpatialHash,
    ) {
        // Exact duplicates between two authored elements.
        let mut groups: HashMap<[i32; 6], Vec<usize>> = HashMap::new();
        for (slot, collider) in colliders.iter().enumerate() {
            groups
                .entry(aabb_key(&collider.aabb))
                .or_default()
                .push(slot);
        }
        for (key, group) in &groups {
            if group.len() < 2 {
                continue;
            }
            let names: Vec<String> = group
                .iter()
                .filter_map(|slot| colliders.get(*slot).map(|c| c.source.clone()))
                .collect();
            if let Some(first) = group.first().and_then(|slot| colliders.get(*slot)) {
                self.push(
                    "collision-duplicate",
                    Severity::Error,
                    names.join(" / "),
                    format!(
                        "{} authored solids share an identical collision box",
                        group.len()
                    ),
                    aabb_centre(&first.aabb),
                );
            }
            let _ = key;
        }

        // Every authored solid must exist in the engine's own collision set.
        for collider in colliders {
            if !engine_boxes
                .iter()
                .any(|engine| aabb_close(engine, &collider.aabb, 1.0e-3))
            {
                self.push(
                    "collision-mismatch",
                    Severity::Error,
                    collider.source.clone(),
                    "the engine's collision set has no box matching this authored solid",
                    aabb_centre(&collider.aabb),
                );
            }
        }

        // Reversed faces and ghost colliders.
        for collider in colliders {
            if !collider.ghost_checked {
                continue;
            }
            let boxed = &collider.aabb;
            let mut covered_faces = 0usize;
            for (axis, sign) in [
                (0usize, -1.0_f32),
                (0, 1.0),
                (1, -1.0),
                (1, 1.0),
                (2, -1.0),
                (2, 1.0),
            ] {
                let centre = aabb_face_centre(boxed, axis, sign);
                let mut outward = [0.0_f32; 3];
                if let Some(slot) = outward.get_mut(axis) {
                    *slot = sign;
                }
                let mut face_covered = false;
                if let Some(candidates) = index.near(centre) {
                    for slot in candidates {
                        let Some(triangle) = usize::try_from(*slot)
                            .ok()
                            .and_then(|slot| triangles.get(slot))
                        else {
                            continue;
                        };
                        let offset = dot3(triangle.normal, sub3(centre, triangle.points[0]));
                        if offset.abs() > FACE_SAMPLE_EPS_M {
                            continue;
                        }
                        if !triangle_on_face(triangle, boxed, axis) {
                            continue;
                        }
                        if !point_in_triangle(centre, triangle) {
                            continue;
                        }
                        let _ = outward;
                        face_covered = true;
                    }
                }
                if face_covered {
                    covered_faces = covered_faces.saturating_add(1);
                }
            }
            if covered_faces == 0 {
                self.push(
                    "ghost-collider",
                    Severity::Warning,
                    collider.source.clone(),
                    "no static mesh surface lies on any face of this collider",
                    aabb_centre(boxed),
                );
            }
        }
    }
}

/// True when a point lies inside a triangle, tested on the triangle's own
/// dominant projection plane.
fn point_in_triangle(point: [f32; 3], triangle: &Tri) -> bool {
    let (axis, _) = dominant_axis(triangle.normal);
    let project = |p: [f32; 3]| match axis {
        0 => [p[1], p[2]],
        1 => [p[0], p[2]],
        _ => [p[0], p[1]],
    };
    let p = project(point);
    let a = project(triangle.points[0]);
    let b = project(triangle.points[1]);
    let c = project(triangle.points[2]);
    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[1] - o[1]).mul_add(-(v[0] - o[0]), (u[0] - o[0]) * (v[1] - o[1]))
    };
    let ab = cross(a, b, p);
    let bc = cross(b, c, p);
    let ca = cross(c, a, p);
    let epsilon = -1.0e-6;
    (ab >= epsilon && bc >= epsilon && ca >= epsilon)
        || (ab <= -epsilon && bc <= -epsilon && ca <= -epsilon)
}

/// True when a triangle's centroid lies within a collider's face rectangle.
fn triangle_on_face(triangle: &Tri, boxed: &WallAabb, axis: usize) -> bool {
    let centre = triangle.centroid;
    let mut within = true;
    for (candidate, value) in centre.iter().enumerate() {
        if candidate == axis {
            continue;
        }
        let (low, high) = match candidate {
            0 => (boxed.min_x, boxed.max_x),
            1 => (boxed.min_y, boxed.max_y),
            _ => (boxed.min_z, boxed.max_z),
        };
        within = within && *value >= low - 0.001 && *value <= high + 0.001;
    }
    within
}

/// Exact-box key for duplicate detection, in millimetres.
fn aabb_key(aabb: &WallAabb) -> [i32; 6] {
    [
        quantize(aabb.min_x),
        quantize(aabb.min_y),
        quantize(aabb.min_z),
        quantize(aabb.max_x),
        quantize(aabb.max_y),
        quantize(aabb.max_z),
    ]
}

/// True when two boxes agree within `tolerance`.
fn aabb_close(a: &WallAabb, b: &WallAabb, tolerance: f32) -> bool {
    (a.min_x - b.min_x).abs() <= tolerance
        && (a.min_y - b.min_y).abs() <= tolerance
        && (a.min_z - b.min_z).abs() <= tolerance
        && (a.max_x - b.max_x).abs() <= tolerance
        && (a.max_y - b.max_y).abs() <= tolerance
        && (a.max_z - b.max_z).abs() <= tolerance
}

/// The centre of a box, for reporting.
const fn aabb_centre(aabb: &WallAabb) -> [f32; 3] {
    [
        f32::midpoint(aabb.min_x, aabb.max_x),
        f32::midpoint(aabb.min_y, aabb.max_y),
        f32::midpoint(aabb.min_z, aabb.max_z),
    ]
}

/// The centre of one box face.
fn aabb_face_centre(aabb: &WallAabb, axis: usize, sign: f32) -> [f32; 3] {
    let mut centre = aabb_centre(aabb);
    let value = match axis {
        0 => {
            if sign < 0.0 {
                aabb.min_x
            } else {
                aabb.max_x
            }
        }
        1 => {
            if sign < 0.0 {
                aabb.min_y
            } else {
                aabb.max_y
            }
        }
        _ => {
            if sign < 0.0 {
                aabb.min_z
            } else {
                aabb.max_z
            }
        }
    };
    if let Some(slot) = centre.get_mut(axis) {
        *slot = value;
    }
    centre
}

// ---------------------------------------------------------------------------
// Openings
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Overlapping opening pairs and openings that miss the wall solid.
    fn check_openings(&mut self, surfaces: &LevelSurfaces<'_>) {
        for (index, wall) in self.level.walls.iter().enumerate() {
            let (wall_base, wall_top) = wall_extent_pair(wall, surfaces);
            let length = wall.length();
            for (first, opening) in wall.openings.iter().enumerate() {
                let start = opening.offset;
                let end = opening.end();
                let bottom = opening.bottom(wall_base);
                let top = opening.top(wall_base);
                if end <= start || top <= bottom {
                    continue;
                }
                let outside = end < -1.0e-3
                    || start > length + 1.0e-3
                    || top < wall_base - 1.0e-3
                    || bottom > wall_top + 1.0e-3;
                if outside {
                    self.push(
                        "opening-unused",
                        Severity::Warning,
                        format!("wall {index} opening {first}"),
                        "the opening does not intersect the wall solid; it cuts nothing",
                        wall_point(wall, f32::midpoint(start, end), wall_base),
                    );
                    continue;
                }
                for (second, other) in wall
                    .openings
                    .iter()
                    .enumerate()
                    .skip(first.saturating_add(1))
                {
                    let other_start = other.offset;
                    let other_end = other.end();
                    let overlaps_length = end > other_start + 1.0e-3 && start < other_end - 1.0e-3;
                    let other_bottom = other.bottom(wall_base);
                    let other_top = other.top(wall_base);
                    let overlaps_vertically =
                        top > other_bottom + 1.0e-3 && bottom < other_top - 1.0e-3;
                    if overlaps_length && overlaps_vertically {
                        self.push(
                            "opening-overlap",
                            Severity::Error,
                            format!("wall {index} openings {first}/{second}"),
                            "two openings overlap; the wall resolves them as one merged hole",
                            wall_point(
                                wall,
                                f32::midpoint(start.max(other_start), end.min(other_end)),
                                bottom,
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// A wall's base and top, mirroring the engine's conservative vertical extent.
fn wall_extent_pair(wall: &WallDef, surfaces: &LevelSurfaces<'_>) -> (f32, f32) {
    let length = wall.length();
    let breaks = surfaces.wall_profile_breaks(wall);
    let mut base = wall.y;
    let mut top = f32::NEG_INFINITY;
    let probes = std::iter::once(0.0)
        .chain(std::iter::once(length))
        .chain(breaks.iter().copied());
    for at in probes {
        let clear = wall
            .height
            .unwrap_or_else(|| surfaces.clear_ceiling_height_along(wall, at));
        let candidate = wall.y + clear;
        base = base.min(candidate);
        top = top.max(candidate);
    }
    (base, top)
}

/// A world point on the wall's centre plane.
fn wall_point(wall: &WallDef, along: f32, y: f32) -> [f32; 3] {
    let (x, z) = crate::level::wall_point(wall, along);
    [x, y, z]
}

// ---------------------------------------------------------------------------
// Curved primitives
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Invalid, coarse or collision-uncovered arc walls and circular pillars.
    fn check_curves(&mut self, surfaces: &LevelSurfaces<'_>) {
        for (index, piece) in self.level.arc_walls.iter().enumerate() {
            self.check_arc_wall(index, piece, surfaces);
        }
        for (index, piece) in self.level.pillars.iter().enumerate() {
            self.check_pillar(index, piece, surfaces);
        }
    }

    #[allow(clippy::too_many_lines)] // one curved primitive's checks, kept together
    fn check_arc_wall(&mut self, index: usize, piece: &ArcWallDef, surfaces: &LevelSurfaces<'_>) {
        let label = format!("arc wall {index}");
        let centre = [piece.x, piece.base_y(surfaces), piece.z];
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.radius.is_finite()
            || !piece.thickness.is_finite()
        {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "centre, radius and thickness must be finite numbers",
                centre,
            );
            return;
        }
        if piece.radius <= 0.0 || piece.thickness <= 0.0 || piece.inner_radius() <= 0.0 {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!(
                    "thickness {} must be positive and thinner than twice the radius {}",
                    piece.thickness, piece.radius
                ),
                centre,
            );
            return;
        }
        if !piece.sweep_degrees.is_finite()
            || piece.sweep_degrees.abs() <= 1.0e-3
            || piece.sweep_degrees.abs() > 360.0 + 1.0e-3
        {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!(
                    "sweep must be a non-zero angle up to 360 degrees, found {}",
                    piece.sweep_degrees
                ),
                centre,
            );
            return;
        }
        let segments = piece.resolved_segments();
        let segment_angle = piece.sweep_degrees.abs().to_radians()
            / f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let sagitta = piece.outer_radius() * (1.0 - (segment_angle * 0.5).cos());
        let coarse = sagitta > CURVE_COARSE_SAGITTA_M;
        if coarse {
            self.push(
                "curve-coarse",
                Severity::Warning,
                label.clone(),
                format!(
                    "{segments} segments leave a {:.3} m sagitta on a {:.2} m outer radius; raise `segments`",
                    sagitta,
                    piece.outer_radius()
                ),
                centre,
            );
        }
        let boxes: Vec<ArchitectureBox> = piece.collision_boxes(surfaces);
        if boxes.is_empty() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "the primitive resolved to no collision geometry",
                centre,
            );
            return;
        }
        let base = piece.base_y(surfaces);
        let top = piece.top_y_at(surfaces, piece.x, piece.z);
        let probe_y = if top > base {
            f32::midpoint(base, top)
        } else {
            base + 0.5
        };
        for segment in 0..segments {
            let count = f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
            let fraction = (f32::from(u16::try_from(segment).unwrap_or(u16::MAX)) + 0.5) / count;
            let angle = piece.sweep_degrees.mul_add(fraction, piece.start_degrees);
            for radius in [piece.inner_radius(), piece.radius, piece.outer_radius()] {
                let (x, z) = crate::level::round_point(piece.x, piece.z, radius, angle);
                if !covered_by_boxes(&boxes, x, probe_y, z) {
                    self.push(
                        "curved-collision-gap",
                        Severity::Error,
                        label.clone(),
                        format!(
                            "collision does not cover the drawn ring at segment {segment} (radius {radius:.2})"
                        ),
                        [x, probe_y, z],
                    );
                    break;
                }
            }
        }
        // A coarse tessellation already gets the `curve-coarse` warning, and
        // its row-AABB slack is the direct consequence of that tessellation;
        // only a fine curve is checked for grossly oversized collision.
        for boxed in &boxes {
            if coarse {
                break;
            }
            let tolerance = (0.05_f32).max(piece.outer_radius() * 0.05);
            let limit = piece.outer_radius() + tolerance;
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - piece.x).hypot(corner[1] - piece.z);
                if distance > limit {
                    self.push(
                        "curved-collision-overshoot",
                        Severity::Error,
                        label.clone(),
                        format!(
                            "a collision box corner reaches {distance:.3} m from the axis, past the {:.2} m outer radius",
                            piece.outer_radius()
                        ),
                        [corner[0], probe_y, corner[1]],
                    );
                    break;
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)] // one curved primitive's checks, kept together
    fn check_pillar(&mut self, index: usize, piece: &PillarDef, surfaces: &LevelSurfaces<'_>) {
        let label = format!("pillar {index}");
        let centre = [piece.x, piece.base_y(surfaces), piece.z];
        if !piece.x.is_finite() || !piece.z.is_finite() || !piece.radius.is_finite() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "centre and radius must be finite numbers",
                centre,
            );
            return;
        }
        if piece.radius <= 0.0 {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!("radius must be positive, found {}", piece.radius),
                centre,
            );
            return;
        }
        let segments = piece.resolved_segments();
        let half = std::f32::consts::PI / f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let sagitta = piece.radius * (1.0 - half.cos());
        let coarse = sagitta > CURVE_COARSE_SAGITTA_M;
        if coarse {
            self.push(
                "curve-coarse",
                Severity::Warning,
                label.clone(),
                format!(
                    "{segments} segments leave a {:.3} m sagitta on a {:.2} m radius; raise `segments`",
                    sagitta, piece.radius
                ),
                centre,
            );
        }
        let boxes = piece.collision_boxes(surfaces);
        if boxes.is_empty() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "the primitive resolved to no collision geometry",
                centre,
            );
            return;
        }
        let base = piece.base_y(surfaces);
        let top = piece.top_y(surfaces);
        let probe_y = if top > base {
            f32::midpoint(base, top)
        } else {
            base + 0.5
        };
        let points = piece.polygon_points();
        if !covered_by_boxes(&boxes, piece.x, probe_y, piece.z) {
            self.push(
                "curved-collision-gap",
                Severity::Error,
                label.clone(),
                "collision does not cover the pillar's centre",
                centre,
            );
        }
        let len = points.len();
        for slot in 0..len {
            let (Some(a), Some(b)) = (points.get(slot), points.get(wrap_next(slot, len))) else {
                continue;
            };
            let mid = (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
            for probe in [*a, *b, mid] {
                if !covered_by_boxes(&boxes, probe.0, probe_y, probe.1) {
                    self.push(
                        "curved-collision-gap",
                        Severity::Error,
                        label,
                        format!(
                            "collision does not cover the drawn polygon near ({:.2}, {:.2})",
                            probe.0, probe.1
                        ),
                        [probe.0, probe_y, probe.1],
                    );
                    return;
                }
            }
        }
        // See the arc-wall note: a coarse tessellation explains its own slack.
        let limit = piece.radius + (0.05_f32).max(piece.radius * 0.12);
        for boxed in &boxes {
            if coarse {
                break;
            }
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - piece.x).hypot(corner[1] - piece.z);
                if distance > limit {
                    self.push(
                        "curved-collision-overshoot",
                        Severity::Error,
                        label,
                        format!(
                            "a collision box corner reaches {distance:.3} m from the centre, past the {:.2} m radius",
                            piece.radius
                        ),
                        [corner[0], probe_y, corner[1]],
                    );
                    return;
                }
            }
        }
    }
}

/// The four plan corners of an architecture box.
const fn footprint_corners(boxed: &ArchitectureBox) -> [[f32; 2]; 4] {
    [
        [boxed.min[0], boxed.min[2]],
        [boxed.max[0], boxed.min[2]],
        [boxed.max[0], boxed.max[2]],
        [boxed.min[0], boxed.max[2]],
    ]
}

/// True when a point lies inside any architecture box.
fn covered_by_boxes(boxes: &[ArchitectureBox], x: f32, y: f32, z: f32) -> bool {
    boxes.iter().any(|boxed| {
        x >= boxed.min[0] - 1.0e-4
            && x <= boxed.max[0] + 1.0e-4
            && z >= boxed.min[2] - 1.0e-4
            && z <= boxed.max[2] + 1.0e-4
            && y >= boxed.min[1] - 1.0e-4
            && y <= boxed.max[1] + 1.0e-4
    })
}

// ---------------------------------------------------------------------------
// Rooms: leaks, perimeter coverage and spawn
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Suspicious void leaks and wall runs with no wall or opening.
    fn check_rooms(&mut self, surfaces: &LevelSurfaces<'_>, colliders: &[Collider]) {
        let boxes: Vec<WallAabb> = colliders.iter().map(|collider| collider.aabb).collect();
        for (index, room) in self.level.room_iter().enumerate() {
            if !room.width.is_finite() || !room.depth.is_finite() {
                continue;
            }
            self.check_room_leak(index, room, surfaces, &boxes);
            self.check_room_perimeter(index, room, surfaces, &boxes);
        }
    }

    fn check_room_leak(
        &mut self,
        index: usize,
        room: &RoomDef,
        surfaces: &LevelSurfaces<'_>,
        boxes: &[WallAabb],
    ) {
        let (x0, x1, z0, z1) = room.bounds();
        let (foot_x, foot_z) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
        let Some(foot) = surfaces.floor_y_at(foot_x, foot_z) else {
            return;
        };
        let ex0 = x0 - LEAK_MARGIN_M;
        let ex1 = x1 + LEAK_MARGIN_M;
        let ez0 = z0 - LEAK_MARGIN_M;
        let ez1 = z1 + LEAK_MARGIN_M;
        let mut cell = LEAK_CELL_M;
        // Bound the grid: coarsen until the cell budget fits.
        while cell < 4.0 && grid_cells(ex0, ex1, ez0, ez1, cell) > 250_000 {
            cell *= 2.0;
        }
        let cells_x = span_cells(ex0, ex1, cell);
        let cells_z = span_cells(ez0, ez1, cell);
        let point = |ix: usize, iz: usize| -> (f32, f32) {
            (
                (usize_to_f32(ix) + 0.5).mul_add(cell, ex0),
                (usize_to_f32(iz) + 0.5).mul_add(cell, ez0),
            )
        };
        // Seeds: every cell inside the room's footprint.
        let mut visited = vec![false; cells_x.saturating_mul(cells_z)];
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        for ix in 0..cells_x {
            for iz in 0..cells_z {
                let (x, z) = point(ix, iz);
                if room.contains(x, z) {
                    let slot = iz.saturating_mul(cells_x).saturating_add(ix);
                    if let Some(entry) = visited.get_mut(slot)
                        && !*entry
                    {
                        *entry = true;
                        queue.push_back((ix, iz));
                    }
                }
            }
        }
        let mut exit: Option<(f32, f32, f32)> = None;
        // Each cell resolves its own walkable floor: a room that contains a
        // deep basin is blocked at the basin's level by the basin's own rims,
        // and a deck-level wall must not be treated as passable just because
        // the room centre happens to be lower.
        let blocked = |x: f32, z: f32, from_foot: f32| {
            // Outside every room there is no floor; the step keeps the foot it
            // came from, so a wall at the room edge still blocks an escape
            // instead of becoming passable because the void resolves to no
            // walkable surface.
            let cell_foot = surfaces.floor_y_at(x, z).unwrap_or(from_foot);
            boxes.iter().any(|boxed| {
                boxed.overlaps_disc(x, z, LEAK_BODY_RADIUS_M)
                    && boxed.blocks_body(cell_foot, LEAK_BODY_HEIGHT_M)
            })
        };
        while let Some((ix, iz)) = queue.pop_front() {
            let (cx, cz) = point(ix, iz);
            let current_foot = surfaces.floor_y_at(cx, cz).unwrap_or(foot);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let Some(nx) = step_index(ix, dx) else {
                    continue;
                };
                let Some(nz) = step_index(iz, dz) else {
                    continue;
                };
                if nx >= cells_x || nz >= cells_z {
                    continue;
                }
                let slot = nz.saturating_mul(cells_x).saturating_add(nx);
                let Some(entry) = visited.get_mut(slot) else {
                    continue;
                };
                if *entry {
                    continue;
                }
                let (px, pz) = point(nx, nz);
                if blocked(px, pz, current_foot) {
                    continue;
                }
                *entry = true;
                queue.push_back((nx, nz));
                // Outside the room and outside every room: the void.
                if !room.contains(px, pz) && surfaces.room_at(px, pz).is_none() {
                    let penetration = (x0 - px).max(px - x1).max(z0 - pz).max(pz - z1);
                    let candidate = (px, pz, penetration);
                    if exit.is_none_or(|(_, _, depth)| penetration > depth) {
                        exit = Some(candidate);
                    }
                }
            }
        }
        if let Some((x, z, depth)) = exit {
            self.push(
                "room-leak",
                Severity::Warning,
                format!("room {index}"),
                format!(
                    "the room's walkable space reaches the void {depth:.2} m outside its footprint; \
                     no wall, opening or annotation explains it"
                ),
                [x, foot, z],
            );
        }
    }

    fn check_room_perimeter(
        &mut self,
        index: usize,
        room: &RoomDef,
        surfaces: &LevelSurfaces<'_>,
        boxes: &[WallAabb],
    ) {
        let (x0, x1, z0, z1) = room.bounds();
        if x1 - x0 <= 0.0 || z1 - z0 <= 0.0 {
            return;
        }
        // The perimeter is sampled at the room's own floor plane, which is the
        // plane walls and openings are authored against; a recess deeper in
        // the room does not move its walls.
        let foot = if room.floor_y.is_finite() {
            room.floor_y
        } else {
            0.0
        };
        let probe_y = foot + 0.5;
        let mut runs: Vec<([f32; 3], [f32; 3], f32)> = Vec::new();
        for (ex0, ez0, ex1, ez1) in [
            (x0, z0, x1, z0),
            (x0, z1, x1, z1),
            (x0, z0, x0, z1),
            (x1, z0, x1, z1),
        ] {
            let length = (ex1 - ex0).abs().max((ez1 - ez0).abs());
            let steps = span_cells(0.0, length, PERIMETER_STEP_M).max(1);
            let mut open_run: Option<([f32; 3], [f32; 3], f32)> = None;
            for step in 0..=steps {
                let fraction = usize_to_f32(step) / usize_to_f32(steps);
                let x = (ex1 - ex0).mul_add(fraction, ex0);
                let z = (ez1 - ez0).mul_add(fraction, ez0);
                let covered = boxes.iter().any(|boxed| {
                    boxed.overlaps_disc(x, z, 0.05) && boxed.blocks_body(foot, LEAK_BODY_HEIGHT_M)
                }) || opening_covers(self.level, surfaces, x, z, probe_y);
                let piece = length / usize_to_f32(steps);
                if covered {
                    if let Some((start, end, run_length)) = open_run.take()
                        && run_length >= MISSING_RUN_M
                    {
                        runs.push((start, end, run_length));
                    }
                    continue;
                }
                match open_run.as_mut() {
                    Some((_, end, run_length)) => {
                        *end = [x, probe_y, z];
                        *run_length += piece;
                    }
                    None => {
                        open_run = Some(([x, probe_y, z], [x, probe_y, z], piece));
                    }
                }
            }
            if let Some((start, end, run_length)) = open_run
                && run_length >= MISSING_RUN_M
            {
                runs.push((start, end, run_length));
            }
        }
        for (start, end, length) in runs {
            let position = [
                f32::midpoint(start[0], end[0]),
                probe_y,
                f32::midpoint(start[2], end[2]),
            ];
            self.push(
                "missing-wall",
                Severity::Warning,
                format!("room {index}"),
                format!("a {length:.2} m perimeter run has no wall solid and no authored opening"),
                position,
            );
        }
    }

    /// The spawn point should be inside a room, or the player boots over the
    /// void at the fallback floor.
    fn check_spawn(&mut self, surfaces: &LevelSurfaces<'_>) {
        let spawn = &self.level.spawn;
        if !spawn.x.is_finite() || !spawn.z.is_finite() {
            self.push(
                "spawn-invalid",
                Severity::Error,
                "spawn",
                "spawn coordinates must be finite",
                [spawn.x, 0.0, spawn.z],
            );
            return;
        }
        if surfaces.room_at(spawn.x, spawn.z).is_none() {
            self.push(
                "spawn-outside-room",
                Severity::Warning,
                "spawn",
                "the spawn point is outside every room; the walkable floor falls back to y = 0",
                [spawn.x, 0.0, spawn.z],
            );
        }
    }
}

/// Number of cells a span resolves to.
fn span_cells(low: f32, high: f32, cell: f32) -> usize {
    let value = ((high - low) / cell).ceil();
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Bounded below by 1 and above by the caller's own cell budget.
    let cells = value.clamp(1.0, 1.0e6) as u64;
    usize::try_from(cells).unwrap_or(1)
}

/// Cell count of a 2D grid over a span pair.
fn grid_cells(x0: f32, x1: f32, z0: f32, z1: f32, cell: f32) -> usize {
    span_cells(x0, x1, cell).saturating_mul(span_cells(z0, z1, cell))
}

/// `index + step` as a `usize`, or `None` when it would go negative.
fn step_index(index: usize, step: i32) -> Option<usize> {
    let value = i64::try_from(index).unwrap_or(0);
    let stepped = value.checked_add(i64::from(step))?;
    usize::try_from(stepped).ok()
}

/// True when a point at `y` passes through some wall opening or archway.
fn opening_covers(level: &LevelDef, surfaces: &LevelSurfaces<'_>, x: f32, z: f32, y: f32) -> bool {
    for wall in &level.walls {
        let (origin_x, origin_z) = wall.length_origin();
        let (along, across) = match wall.axis() {
            WallAxis::X => (x - origin_x, z),
            WallAxis::Z => (z - origin_z, x),
        };
        let (t0, t1) = wall_t0_t1(wall);
        if across < t0 - 0.3 || across > t1 + 0.3 {
            continue;
        }
        let local_y = y - wall.y;
        for opening in &wall.openings {
            if along >= opening.offset - 0.05
                && along <= opening.end() + 0.05
                && local_y >= opening.sill - 0.05
                && local_y <= opening.sill + opening.height + 0.05
            {
                return true;
            }
        }
    }
    for archway in &level.archways {
        let (x0, x1, z0, z1) = archway.bounds();
        if x < x0 - 0.1 || x > x1 + 0.1 || z < z0 - 0.1 || z > z1 + 0.1 {
            continue;
        }
        let along = match archway.axis() {
            WallAxis::X => x - x0,
            WallAxis::Z => z - z0,
        };
        let (open_start, open_end) = archway.opening_span();
        let base = archway.base_y(surfaces);
        let height = archway.arch_height_at(along);
        if along >= open_start - 0.05 && along <= open_end + 0.05 && y <= base + height + 0.05 {
            return true;
        }
    }
    false
}

/// A wall's two across-thickness planes.
fn wall_t0_t1(wall: &WallDef) -> (f32, f32) {
    let (min_x, max_x) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (min_z, max_z) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    match wall.axis() {
        WallAxis::X => (min_z, max_z),
        WallAxis::Z => (min_x, max_x),
    }
}

// ---------------------------------------------------------------------------
// Small vector helpers
// ---------------------------------------------------------------------------

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalized3(value: [f32; 3]) -> [f32; 3] {
    let length = length3(value);
    if !length.is_finite() || length <= 1.0e-12 {
        return [0.0, 0.0, 1.0];
    }
    [value[0] / length, value[1] / length, value[2] / length]
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub3(a, b);
    dot3(d, d)
}

const fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // bounded grid indices
    let converted = value as f32;
    converted
}

/// Prints a usage error and ends the process with the documented status `2`.
///
/// Kept with the checker's process-exit path so the narrow `exit` allow stays
/// in one place.
#[allow(clippy::exit, clippy::print_stderr)]
pub fn exit_usage(error: &str) -> ! {
    eprintln!("geometry check: {error}");
    std::process::exit(2);
}

/// Rewrites `--check-geometry` runs as the process exit point.
///
/// # Errors
///
/// Returns the usage or IO error, reported as status `2`.
// `std::process::exit` is the correct way for a CLI mode to end the process
// from inside `main`; this one narrow allow keeps the crate-wide `exit` lint
// for every other path.
#[allow(clippy::exit, clippy::print_stderr)]
pub fn main(options: &CliOptions) -> Result<(), Box<dyn std::error::Error>> {
    match run(options) {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("geometry check: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    // Test code: fixture parsing, exact float compares, unwraps and indexing
    // are idiomatic here (the crate's production lints stay enforced above).
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::panic,
        clippy::arithmetic_side_effects,
        clippy::cast_precision_loss,
        clippy::redundant_closure_for_method_calls,
        clippy::suboptimal_flops
    )]

    use super::*;
    use std::collections::BTreeSet;

    use crate::collision::{
        CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, highest_support_top, lowest_underside,
        resolve_player_collision, resolve_player_collision_for_body,
    };
    use glam::Vec2;

    /// Parses one fixture the way the CLI does: parse, validate, and run the
    /// preparation pass when the loader accepts it.
    fn fixture(text: &str, name: &str) -> (LevelDef, CheckReport) {
        let mut level = LevelDef::from_json(text).expect("the fixture parses");
        let validated = crate::loader::validate_level(&level).is_ok();
        if validated {
            let catalog = crate::assets::AssetCatalog::load_default();
            crate::loader::prepare_level(&mut level, &catalog, None);
        }
        let report = check_level(&level, name, validated);
        (level, report)
    }

    fn counts(report: &CheckReport) -> BTreeMap<&'static str, (usize, usize)> {
        let mut counts: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
        for finding in &report.findings {
            let entry = counts.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
        counts
    }

    fn has(report: &CheckReport, check: &str, severity: Severity) -> bool {
        report
            .findings
            .iter()
            .any(|finding| finding.check == check && finding.severity == severity)
    }

    /// True when a plan point lies inside a convex polygon.
    fn inside_polygon(point: Vec2, polygon: &[(f32, f32)]) -> bool {
        let mut sign = 0.0_f32;
        let len = polygon.len();
        for index in 0..len {
            let (ax, az) = polygon[index];
            let (bx, bz) = polygon[(index + 1) % len];
            let cross = (bx - ax) * (point.y - az) - (bz - az) * (point.x - ax);
            if cross.abs() > 1.0e-6 {
                if sign == 0.0 {
                    sign = cross.signum();
                } else if cross.signum() != sign {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn the_broken_fixture_reports_its_planted_defects() {
        let (_, report) = fixture(
            include_str!("../tests/fixtures/levels/geometry_broken.json"),
            "geometry_broken",
        );
        assert!(report.validated, "the broken fixture must still load");
        assert!(
            has(&report, "opening-overlap", Severity::Error),
            "overlapping openings must be a confirmed defect: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "opening-unused", Severity::Warning),
            "an opening above the wall must be an unused-opening warning: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "collision-duplicate", Severity::Error),
            "two identical guardrails must duplicate collision: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "duplicate-surface", Severity::Error),
            "two identical guardrails must duplicate coplanar surfaces: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "missing-wall", Severity::Warning)
                && has(&report, "room-leak", Severity::Warning),
            "the open west edge must be reported as a missing wall and a leak: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "curve-coarse", Severity::Warning),
            "a 4-segment pillar must be reported as coarse: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "spawn-outside-room", Severity::Warning),
            "a spawn in the void must be reported: {:?}",
            counts(&report)
        );
    }

    #[test]
    fn the_intentional_fixture_is_clean() {
        let (level, report) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        assert!(report.validated);
        assert_eq!(
            report.error_count(),
            0,
            "no confirmed defects expected: {:#?}",
            counts(&report)
        );
        // The only warnings left are the sub-square-centimetre coplanar
        // slivers where perpendicular wall caps meet at the fixture's corners;
        // they are reported, not hidden, and are below the confirmed-defect
        // area. Nothing else may warn.
        for finding in &report.findings {
            assert_eq!(
                finding.check, "coplanar-sliver",
                "unexplained warning: {finding:?}"
            );
            assert!(finding.message.contains("m²"), "{finding:?}");
        }
        assert!(
            report.suppressed.get("missing-wall").copied().unwrap_or(0) > 0,
            "the open bay edge must be suppressed by its annotation"
        );

        // The east room has no authored tile frame, so its vent snaps on the
        // world 1 m panel grid; the west room's frame is rotated and offset.
        let decals = &level.decals;
        assert_eq!(decals.len(), 2);
        assert!((decals[0].x - 12.5).abs() < 1.0e-3, "{:?}", decals[0]);
        assert!((decals[0].z - 3.5).abs() < 1.0e-3, "{:?}", decals[0]);
        assert!((level.ceiling_decal_rotation(&decals[0])).abs() < 1.0e-3);
        // The decal authored on a frame lattice point does not move, and its
        // in-plane rotation composes the room's 90 degree tile rotation.
        assert!((decals[1].x - 3.25).abs() < 1.0e-3, "{:?}", decals[1]);
        assert!((decals[1].z - 3.75).abs() < 1.0e-3, "{:?}", decals[1]);
        assert!((level.ceiling_decal_rotation(&decals[1]) - 90.0).abs() < 1.0e-3);
    }

    #[test]
    fn the_shipped_demo_has_no_confirmed_defects() {
        let (_, report) = fixture(
            include_str!("../assets/levels/places_demo.json"),
            "places_demo",
        );
        assert!(report.validated);
        assert_eq!(
            report.error_count(),
            0,
            "the shipped demo must not carry confirmed geometry defects: {:#?}",
            counts(&report)
        );
    }

    #[test]
    fn the_invalid_fixture_is_rejected_and_the_checker_still_names_the_problems() {
        let text = include_str!("../tests/fixtures/levels/invalid/geometry_invalid.json");
        let (_, report) = fixture(text, "geometry_invalid");
        assert!(!report.validated, "the loader must reject the fixture");
        assert!(
            has(&report, "curved-invalid", Severity::Error),
            "every degenerate curve must be named: {:#?}",
            counts(&report)
        );
        let messages: Vec<&str> = report
            .findings
            .iter()
            .filter(|finding| finding.check == "curved-invalid")
            .map(|finding| finding.message.as_str())
            .collect();
        assert!(
            messages.iter().any(|message| message.contains("thinner")),
            "the thickness diagnostic must name the constraint: {messages:?}"
        );
        assert!(
            messages.iter().any(|message| message.contains("sweep")),
            "the sweep diagnostic must name the constraint: {messages:?}"
        );
        assert!(
            messages.iter().any(|message| message.contains("radius")),
            "the radius diagnostic must name the constraint: {messages:?}"
        );
    }

    #[test]
    fn curved_primitives_generate_tight_collision_and_material_variety() {
        let (level, _) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        let surfaces = LevelSurfaces::new(&level);
        let materials = crate::render::logical_materials(&level);

        // Pillar collision follows the drawn polygon.
        let pillar = &level.pillars[0];
        let boxes = pillar.collision_boxes(&surfaces);
        assert!(
            boxes.len() >= 8,
            "a 24-segment pillar should decompose into several rows, got {}",
            boxes.len()
        );
        let full_square = (pillar.radius * 2.0) * (pillar.radius * 2.0);
        for boxed in &boxes {
            let area = (boxed.max[0] - boxed.min[0]) * (boxed.max[2] - boxed.min[2]);
            assert!(
                area < full_square * 0.6,
                "no collision row may be a square around the whole pillar (area {area:.3} of {full_square:.3})"
            );
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - pillar.x).hypot(corner[1] - pillar.z);
                assert!(
                    distance <= pillar.radius + 0.05,
                    "a collision corner reaches {distance:.3} m, past the {:.2} m radius",
                    pillar.radius
                );
            }
        }
        for (x, z) in pillar.polygon_points() {
            assert!(
                covered_by_boxes(&boxes, x, 1.0, z),
                "the drawn polygon vertex ({x:.2}, {z:.2}) must be collision-covered"
            );
        }

        // Arc-wall collision is one tight box per rendered segment.
        let arc = &level.arc_walls[0];
        let arc_boxes = arc.collision_boxes(&surfaces);
        assert_eq!(
            arc_boxes.len(),
            usize::try_from(arc.resolved_segments()).unwrap() * crate::level::ARC_COLLISION_STEPS,
            "one collision box per rendered arc segment sub-step"
        );

        // Material variety: the fixture's curves together resolve several
        // distinct wall materials in the emitted mesh.
        let mesh = crate::render::build_level_geometry_with_materials(&level, &materials);
        let used: BTreeSet<u16> = mesh
            .ranges
            .iter()
            .filter(|range| range.key.kind == SurfaceKind::Wall)
            .map(|range| range.key.material)
            .collect();
        for id in [
            "core:pool_tile_wall_01",
            "core:metal_brushed_01",
            "core:linoleum_polished_01",
            "core:baseboard_office_01",
            "core:wallpaper_stained_01",
        ] {
            let index = materials.index_of(id).expect(id);
            assert!(
                used.contains(&index),
                "{id} must reach the mesh through a curve face"
            );
        }
    }

    #[test]
    fn walking_crouching_and_landing_at_curved_surfaces() {
        let (level, _) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        let surfaces = LevelSurfaces::new(&level);
        let boxes = level.collision_aabbs();
        let pillar = &level.pillars[0];
        let polygon = pillar.polygon_points();
        let centre = Vec2::new(pillar.x, pillar.z);

        // Walking: from eight directions the resolved centre never ends up
        // inside the drawn polygon, and the body is pushed back.
        for step in 0..8 {
            let angle = std::f32::consts::TAU * (step as f32 / 8.0);
            let direction = Vec2::new(angle.cos(), angle.sin());
            let start = centre + direction * (pillar.radius + 1.0);
            let resolved = resolve_player_collision(start, PLAYER_RADIUS, 0.0, &boxes);
            assert!(
                !inside_polygon(resolved, &polygon),
                "walking from {start:?} resolved inside the pillar at {resolved:?}"
            );
            assert!(
                resolved.distance(start) < 0.5,
                "the controller must stop at the pillar, not teleport"
            );
        }

        // Crouching changes nothing about a full-height pillar except that the
        // body is shorter; it still cannot enter the drawn polygon.
        let start = centre + Vec2::new(1.0, 0.0);
        let resolved =
            resolve_player_collision_for_body(start, PLAYER_RADIUS, 0.0, CROUCH_HEIGHT, &boxes);
        assert!(!inside_polygon(resolved, &polygon));

        // Jumping: the low pillar's top is the landing surface over its whole
        // polygon.
        let low = &level.pillars[1];
        let top = low.top_y(&surfaces);
        let landing = highest_support_top(low.x, low.z, top + 0.2, &boxes);
        assert_eq!(landing, Some(top));

        // A raised arc wall is a header: a standing body is blocked, a
        // crouched one passes, and head collision resolves its underside.
        let raised = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "raised_arc",
                "name": "Raised Arc",
                "spawn": { "x": 1.0, "z": 1.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 },
                "arc_walls": [
                    { "x": 4.0, "z": 4.0, "radius": 2.0, "thickness": 0.3,
                      "start_degrees": 90.0, "sweep_degrees": 90.0,
                      "y": 1.5, "height": 0.6, "material": "core:metal_brushed_01" }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("the raised arc level parses");
        let raised_boxes = raised.collision_aabbs();
        assert!(
            raised_boxes
                .iter()
                .any(|boxed| boxed.min_y >= 1.49 && boxed.max_y <= 2.11),
            "the raised arc must collide from 1.5 m up"
        );
        let head = lowest_underside(5.4, 5.4, PLAYER_RADIUS, 0.0, &raised_boxes);
        assert_eq!(
            head,
            Some(1.5),
            "head collision must resolve the arc underside"
        );
        let inside_arc = Vec2::new(5.6, 4.8);
        let standing = resolve_player_collision_for_body(
            inside_arc,
            PLAYER_RADIUS,
            0.0,
            PLAYER_HEIGHT,
            &raised_boxes,
        );
        assert_ne!(
            standing, inside_arc,
            "standing under the header must be blocked"
        );
        let crouched = resolve_player_collision_for_body(
            inside_arc,
            PLAYER_RADIUS,
            0.0,
            CROUCH_HEIGHT,
            &raised_boxes,
        );
        assert_eq!(
            crouched, inside_arc,
            "a crouched body must pass under the header"
        );
    }

    #[test]
    fn checker_arguments_and_reports_are_stable() {
        let args: Vec<String> = [
            "--check-geometry",
            "--level",
            "levels/level0_pit.json",
            "--json",
            "target/agent-work/pit.json",
            "--strict",
            "--quiet",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let options = options_from_args(&args)
            .expect("parses")
            .expect("checker mode");
        assert_eq!(options.level, "levels/level0_pit.json");
        assert!(options.strict);
        assert!(!options.verbose);
        assert!(
            options_from_args(&["--level".to_string()])
                .expect("no mode")
                .is_none()
        );
        assert!(
            options_from_args(&["--check-geometry".to_string(), "--bogus".to_string()]).is_err()
        );

        let report = CheckReport {
            source: "fixture".to_string(),
            level_id: "fixture".to_string(),
            validated: true,
            findings: vec![Finding {
                check: "degenerate-face",
                severity: Severity::Error,
                id: "wall 0".to_string(),
                message: "zero-area triangle".to_string(),
                position: [1.0, 2.0, 3.0],
            }],
            suppressed: BTreeMap::new(),
        };
        let json = report.to_json().expect("serializes");
        assert!(json.contains(CHECK_FORMAT));
        assert!(json.contains("\"errors\": 1"));
        assert_eq!(report.exit_status(false), 1);
        assert_eq!(report.markers().len(), 1);
        let obj = marker_obj(&report.markers());
        assert!(
            obj.contains("\nl "),
            "the marker OBJ must carry line elements"
        );
    }
}
