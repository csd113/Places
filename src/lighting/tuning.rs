//! Centralised numbers and fixture families.
//!
//! These are the values the baked model was calibrated with, plus the table that
//! says which appearance each built-in fixture id draws as. They are kept
//! together because they are declarations rather than computation: nothing here
//! loops, allocates or reads a level.

use super::color::LightColor;
use super::light::LightShape;

/// Floor area one standard fixture is expected to illuminate, in square metres.
///
/// This is the reference point of the density curve used by [`room_baseline`].
/// It is calibrated to the sparsest fixture grid the game shows: a large hall
/// with a handful of widely spaced panels (as the generated benchmark levels
/// and the demo's long corridor do) must still read as a lit commercial space
/// rather than a dark hall.
///
/// Because rooms sit somewhere between "one panel in a huge hall" and "a dense
/// grid in a small room", the raw area ratio spans more than two orders of
/// magnitude. [`compressed_density`] compresses that range logarithmically so a
/// regular grid lights the whole floor at roughly half brightness while small
/// rooms saturate slowly instead of clipping to white.
pub const REFERENCE_LIGHT_AREA_M2: f32 = 500.0;

/// Ceiling height at which a fixture delivers its nominal output, in metres.
pub const REFERENCE_CEILING_HEIGHT_M: f32 = 3.5;

/// Exponent of the ceiling-height correction: `(reference / height) ^ falloff`.
///
/// 0.5 is a deliberately gentle inverse square root: a 2.6 m corridor makes its
/// fixtures ~16% more effective, a 4 m room ~7% less. A physically accurate
/// inverse square would make tall spaces unusably dark.
pub const HEIGHT_FALLOFF: f32 = 0.5;

/// Brightness of a room with no effective fixtures at all, per channel.
///
/// Deliberately small: ambient may provide just enough visibility to keep
/// unlit geometry readable, but it must never substitute for fixtures. A room
/// without lights is dark by design; see [`ambient_color`].
pub const AMBIENT_LEVEL: f32 = 0.10;

/// Hard upper bound on baked brightness, per channel. Values above 1.0 would
/// clip textured surfaces to flat white and wash the level out.
pub const MAX_BRIGHTNESS: f32 = 1.0;

/// Largest share of [`MAX_BRIGHTNESS`] a room baseline may claim, per channel.
///
/// The baseline is the room-wide *fill* every surface in a room receives; the
/// local fixture pools are the light that actually shapes a room, and they are
/// also the only term a static occluder can remove — every shadow in the game
/// is a surface losing a pool. Both terms are summed and then clamped
/// ([`MAX_BRIGHTNESS`]), so the fill has to leave the pool term room:
/// `BASELINE_MAX + LOCAL_LIGHT_MAX` is `1.05`, and only a surface that both
/// sits in the densest room the density curve allows *and* sees essentially
/// the entire pool cap (directly under several overlapping fixtures) reaches
/// that last 0.05. Everywhere else a pool's removal moves the final value by
/// its full amount.
///
/// Measured on the shipped demo's real Full lightmap texels: with the
/// historical full-range fill (rooms baked 0.58..0.77) 5.2% of surface texels
/// sat at the clamp, 12.7% of the texels an occluder darkened showed no change
/// after it, and a fully shadowed sample lost 30% of its light; with this fill
/// nothing bakes at the clamp, 100% of what occluders remove shows, and a fully
/// shadowed sample loses 43%. The sweep that chose 0.60 is in
/// `bake/range_audit.rs`: the clamp damage collapses below ~0.65, while the
/// fill still reaches two thirds of the unit range so rooms stay lit.
///
/// The span is not the baseline's *shape* — the density compression and the
/// saturating curve are unchanged — only how much of the unit range the fill
/// is allowed to occupy.
pub const BASELINE_MAX: f32 = 0.60;

/// Emitted colour used by fixtures that do not author one.
///
/// A restrained, slightly aged institutional fluorescent: warm enough to read
/// as artificial light, far from a saturated yellow. Centralised here so the
/// level schema, the bake and its local pools cannot drift apart;
/// `level-editor/js/lighting.js` mirrors this constant for the preview. The
/// fixture's visible face is texture-first and never consumes this colour.
pub const DEFAULT_LIGHT_COLOR: LightColor = LightColor::rgb(1.0, 0.96, 0.88);

/// Ambient fill colour per channel: neutral, small and fixed.
#[must_use]
pub const fn ambient_color() -> LightColor {
    LightColor::grey(AMBIENT_LEVEL)
}

/// Radius in metres over which one fixture's local pool fades to nothing.
pub const LOCAL_LIGHT_RADIUS_M: f32 = 6.0;

/// Extra brightness one fixture adds directly beneath itself.
pub const LOCAL_LIGHT_STRENGTH: f32 = 0.42;

/// Cap on the summed local fixture contribution, so a dense cluster of
/// fixtures cannot drive a whole room to white.
pub const LOCAL_LIGHT_MAX: f32 = 0.45;

/// Radius in metres over which light leaks through a doorway or passage.
pub const OPENING_BLEND_RADIUS_M: f32 = 6.0;

/// Fraction of the neighbouring room's baseline mixed in at the opening itself.
/// 0.5 makes both sides of a threshold meet at the average of the two rooms,
/// which is what removes the hard brightness step.
pub const OPENING_BLEND_STRENGTH: f32 = 0.5;

/// Vertical distance above an opening over which the blend fades out: light
/// does not pass through the solid wall above a door header.
pub const OPENING_VERTICAL_FADE_M: f32 = 1.0;

/// Half-extents of a fixture's luminous panel, matching the ceiling-light
/// geometry in [`crate::render`] (a 1.2 x 0.6 m panel).
pub const FIXTURE_HALF_WIDTH_M: f32 = 0.6;
/// See [`FIXTURE_HALF_WIDTH_M`].
pub const FIXTURE_HALF_DEPTH_M: f32 = 0.3;

/// Distance a fixture hangs below its room's ceiling, in metres.
pub const FIXTURE_DROP_M: f32 = 0.01;

/// Fallback height of a wall-mounted fixture with no authored `y`, as a
/// distance above its room's floor.
///
/// Validation requires a `y` for new content; this only keeps a hand-edited
/// level finite instead of panicking.
pub const WALL_LIGHT_DEFAULT_HEIGHT_M: f32 = 1.7;

/// Distance a wall face is probed away from itself when the bake resolves which
/// room the face opens into, in metres.
///
/// Shared with the geometry emitter, which uses the same probe when it samples
/// the face's baked illumination, so the room a face is lit by and the room its
/// light samples are resolved in cannot drift apart.
pub const WALL_FACE_PROBE_M: f32 = 0.25;

/// Which built-in fixture family a `fixture` id draws as.
///
/// The catalog owns the *identity* of a fixture (`core:pool_light_round`) and
/// the PNG that is its visible face; this table owns only the mesh family the
/// code generates for it: the luminous footprint the bake treats as a light
/// source, and the generated-quad budget the level estimate reserves. Fixture
/// appearance and emitted light colour stay separate concepts — a fixture's
/// `color` is authored per placed light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureKind {
    /// Recessed twin-tube ceiling panel: the office fluorescent.
    FluorescentPanel,
    /// Round recessed ceiling downlight.
    RoundRecessed,
    /// Wall-mounted luminaire; needs `mount: "wall"` and a `y`.
    WallSconce,
    /// Shallow round drum with a diffuser disc: the residential flush mount.
    FlushMount,
}

impl FixtureKind {
    /// Every family, in the order a level's fixture sheets are indexed.
    ///
    /// [`FixtureKind::index`] is the slot a level's resolved fixture sheets are
    /// stored under and the key its light batches carry, so the order is part
    /// of the mesh format. It is not a draw order or a bake order.
    pub const ALL: [Self; 4] = [
        Self::FluorescentPanel,
        Self::RoundRecessed,
        Self::WallSconce,
        Self::FlushMount,
    ];

    /// Slot of this family in a level's resolved fixture sheets.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::FluorescentPanel => 0,
            Self::RoundRecessed => 1,
            Self::WallSconce => 2,
            Self::FlushMount => 3,
        }
    }
}

/// Appearance, footprint and geometry budget of one fixture family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixtureProfile {
    pub kind: FixtureKind,
    /// Half-extent along the fixture's own width axis, in metres.
    pub half_width: f32,
    /// Half-extent along the fixture's own depth axis, in metres.
    pub half_depth: f32,
    /// Upper bound on the quads the renderer emits for one fixture, used by
    /// the level geometry estimate.
    pub quads: u64,
}

impl FixtureProfile {
    /// The generic light shape this family's luminous surface casts from.
    ///
    /// The bake consumes this rather than the family: a fixture family is a
    /// piece of visible geometry that happens to own a light, and any future
    /// family only has to describe its emitting rectangle here.
    #[must_use]
    pub const fn shape(self) -> LightShape {
        LightShape::Rect {
            half_width: self.half_width,
            half_depth: self.half_depth,
        }
    }
}

/// Outer radius of the residential flush-mount fixture's diffuser, in metres.
///
/// The fixture's luminous footprint is the bounding square of that disc, exactly
/// like the round pool downlight: the bake's local pool is a rectangle model, and
/// a 32 cm lamp's square footprint is within a few centimetres of its disc.
pub const FLUSH_MOUNT_RADIUS_M: f32 = 0.16;

/// Every fixture id with a built-in appearance, in catalog order.
///
/// The catalog/renderer consistency test keeps this list, the catalog's
/// `asset_type: "light"` entries and each entry's visible-face PNG in
/// agreement, so a catalogued fixture can never silently render as some other
/// fixture or fall back to an untextured sheet.
pub const LIGHT_FIXTURE_IDS: [&str; 4] = [
    "core:fluorescent_panel_01",
    "core:pool_light_round",
    "core:pool_light_wall",
    "home:ceiling_light_round",
];

/// The profile of a fixture family.
#[must_use]
pub const fn fixture_profile_for_kind(kind: FixtureKind) -> FixtureProfile {
    match kind {
        FixtureKind::FluorescentPanel => FixtureProfile {
            kind,
            half_width: FIXTURE_HALF_WIDTH_M,
            half_depth: FIXTURE_HALF_DEPTH_M,
            // One luminous diffuser plus four housing side walls, a four-strip
            // bottom frame and a four-strip top flange.
            quads: 13,
        },
        FixtureKind::RoundRecessed => FixtureProfile {
            kind,
            half_width: 0.22,
            half_depth: 0.22,
            quads: 18,
        },
        FixtureKind::WallSconce => FixtureProfile {
            kind,
            half_width: 0.20,
            half_depth: 0.09,
            quads: 6,
        },
        FixtureKind::FlushMount => FixtureProfile {
            kind,
            half_width: FLUSH_MOUNT_RADIUS_M,
            half_depth: FLUSH_MOUNT_RADIUS_M,
            quads: 32,
        },
    }
}

/// The profile a fixture id draws with.
///
/// Unknown ids deliberately resolve to the office fluorescent panel: levels
/// written before the fixture table existed, or against a future catalog, keep
/// loading and keep lighting the room. The catalog consistency test reports the
/// mismatch instead of the renderer failing at load.
#[must_use]
pub fn fixture_profile(fixture_id: &str) -> FixtureProfile {
    let kind = match fixture_id {
        "core:pool_light_round" => FixtureKind::RoundRecessed,
        "core:pool_light_wall" => FixtureKind::WallSconce,
        "home:ceiling_light_round" => FixtureKind::FlushMount,
        _ => FixtureKind::FluorescentPanel,
    };
    fixture_profile_for_kind(kind)
}

/// Cell size of the baked lighting grid used to tessellate floors, ceilings and
/// wall faces.
///
/// Smaller cells sample the pools more smoothly; the cell count is capped per
/// surface so the generated geometry stays bounded.
pub const LIGHT_GRID_CELL_M: f32 = 2.5;

/// Maximum subdivisions per axis of one floor or ceiling.
pub const MAX_LIGHT_GRID_CELLS: u32 = 12;

/// Maximum segments one wall face is split into along its length.
pub const MAX_WALL_LIGHT_SEGMENTS: u32 = 8;

/// Highest fixture intensity that still adds light while baking. Authored
/// values above this are clamped rather than rejected (see
/// [`crate::level::CeilingLightDef::intensity`]).
pub const MAX_LIGHT_INTENSITY: f32 = 8.0;

/// Smallest floor area used by the density calculation, guarding degenerate
/// zero-area rooms against division by zero.
pub const MIN_ROOM_AREA_M2: f32 = 0.01;

/// Distance a sample may sit outside a room's footprint and still count as
/// inside it. Wall faces, floors and ceilings sit exactly on room boundaries.
/// The level model owns the value so baked lighting, geometry and collision
/// share one ownership tolerance.
pub(super) const ROOM_EDGE_EPS_M: f32 = crate::level::ROOM_EDGE_EPS_M;

/// Distance inside a room probed when deciding which rooms an opening joins.
pub(super) const OPENING_PROBE_M: f32 = 0.05;

/// How far below the lower of two adjacent baseline cells' ceilings the
/// partition-connectivity probe runs, in metres.
///
/// The probe has to be high enough to be blocked by the solid header above a
/// door (which stops light from being shared as if the wall were not there)
/// while staying clear of a wall that deliberately stops short of the ceiling
/// (which light passes over). 15 cm below the ceiling separates all real
/// partitions from all real openings in this engine.
pub(super) const ZONE_PROBE_DROP_M: f32 = 0.15;

/// Minimum height of a baseline-connectivity probe above the room's base floor,
/// in metres. Keeps a probe inside a room whose clear height is barely above the
/// room floor (a raised region can push the ceiling close).
pub(super) const ZONE_PROBE_MIN_ABOVE_FLOOR_M: f32 = 0.1;

/// Step size, in metres, of the walk that moves a surface sample out of an
/// opaque wall it happens to lie inside (see [`LevelLighting::clear_sample`]).
pub(super) const CLEAR_SAMPLE_STEP_M: f32 = 0.05;

/// Maximum number of steps that walk may take before it gives up and uses the
/// room centre, bounding the cost of a pathological level.
pub(super) const CLEAR_SAMPLE_MAX_STEPS: u32 = 64;

/// Resolution of the X/Z occupancy grid a prop model's triangles are ground
/// into before their boxes are merged, in metres.
///
/// This is the width of the smallest shadow detail a prop can cast. 0.15 m is
/// far below the 2.5 m surface sampling grid while leaving a chair as a
/// handful of boxes, and it is the same order as a real piece of furniture's
/// leg/rail spacing.
pub(super) const PROP_OCCLUSION_CELL_M: f32 = 0.15;

/// Largest occupancy grid a prop model may produce per axis.
///
/// A model larger than `CELL_M x this` is ground at a proportionally coarser
/// cell instead, so one pathological asset cannot turn box derivation into an
/// unbounded scan. Ordinary furniture (couch, vending machine) stays well
/// inside it at the calibrated cell size.
pub(super) const PROP_OCCLUSION_MAX_CELLS_PER_AXIS: usize = 96;

/// Two adjacent occupied columns merge into one box only when their Y spans
/// agree within this tolerance, in metres.
pub(super) const PROP_OCCLUSION_MERGE_EPS_M: f32 = 1.0e-4;

/// Thickness given to a flat, zero-height occupied span (a single-quad shelf,
/// a rug, a curtain rail), in metres.
///
/// Without a minimum a horizontal quad would collapse to a zero-height box and
/// silently stop occluding; a couple of centimetres is enough for a crossing
/// segment to be robustly inside it, without visibly inflating the prop.
pub(super) const PROP_OCCLUSION_MIN_THICKNESS_M: f32 = 0.02;

/// Squared X/Z area below which a triangle counts as a line when it is ground
/// into the occupancy grid, in square metres.
///
/// A closed mesh's vertical faces project to zero area in X/Z, and they are
/// exactly the panels — a guardrail, a cabinet door, a curtain — that must
/// still occlude.
pub(super) const PROP_OCCLUSION_DEGENERATE_AREA2_M2: f32 = 1.0e-6;

/// Hard cap on the occlusion boxes one prop model may contribute.
///
/// Boxes are emitted in the deterministic grid scan order (rows of Z, then X),
/// so hitting the cap keeps the low-Z / low-X part of the model. Ordinary
/// shipped assets merge into far fewer boxes (a crate is one, a chair about
/// eight); the cap exists so a pathological mesh cannot make one placement
/// expensive.
pub(super) const MAX_PROP_OCCLUSION_BOXES_PER_MODEL: usize = 64;

/// Hard cap on the occlusion boxes every static prop of one level contributes.
///
/// Props are processed in level order and a prop that would exceed the cap
/// contributes no further boxes, so the occluder set (and the bake cost) stays
/// bounded no matter how many props a level places.
pub(super) const MAX_PROP_OCCLUSION_BOXES_PER_LEVEL: usize = 4096;

/// Distance a lightmap wall or skirt texel is pushed off its own face, along
/// the face normal, before it is evaluated, in metres.
///
/// A wall face texel is generated exactly on the wall's solid boundary. The
/// visibility clip is happy with that (an endpoint on a face is not blocked by
/// it), but point containment is inclusive, so the sample also counts as
/// buried: the vertex bake moves such a face sample [`ROOM_EDGE_EPS_M`] into
/// its room before measuring light, and a lightmap texel gets the same nudge
/// along its patch normal. The distance is shared with the vertex path so the
/// two cannot disagree about which side of a wall face is sampled.
pub(super) const LIGHTMAP_FACE_NORMAL_BIAS_M: f32 = ROOM_EDGE_EPS_M;
