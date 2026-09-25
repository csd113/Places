//! Places' quality levels: how much texture data may reach the GPU, how the
//! world is rendered and how the static lighting is baked.
//!
//! Three player-facing levels — [`QualityLevel::Low`], [`QualityLevel::Medium`]
//! and [`QualityLevel::High`] — answer one question: **how much work may the
//! renderer do?** There are no hardware-specific tiers, no auto-detection and
//! no per-setting zoo.
//!
//! ```text
//! source PNG (asset)  ──decode──▶  High runtime image  (native artwork)
//!                     ──decode──▶  Medium runtime image (half the sheet budget)
//!                     ──decode──▶  Low runtime image (a quarter of it)
//! ```
//!
//! All three levels use the same assets and the same source images. A lower
//! level is not a second art library: it is the same PNG, downscaled further,
//! so the visual identity (and every id, material and fixture) stays identical.
//!
//! High uploads the native artwork unchanged — a 256x256 prop atlas stays
//! 256x256, and a 1024x1024 surface sheet stays 1024x1024. Medium and Low are
//! optional display-budget reductions for players who want them: sheets at 512
//! and 256. The trade is an intentional quality/performance choice, not a
//! hardware requirement of the desktop target, and it never changes the size
//! of the asset stored in the repository.
//!
//! Texture discipline is unchanged: the decoder still refuses anything above
//! [`crate::assets::MAX_TEXTURE_DIMENSION`], and the shipped prop art budget
//! still caps a prop atlas at its 256x256 native size. A quality level only
//! decides how much of an *accepted* source reaches the GPU; it never raises
//! the source limit.
//!
//! The same level also budgets the static lightmap atlas and the baked shadow
//! quality: High bakes at 16 texels per metre onto up to two 1024-texel pages
//! with a two-tap penumbra and 7.5 cm prop-occlusion cells, Medium at 12
//! texels per metre onto the same pages with the same taps and 11 cm cells,
//! and Low at 9 texels per metre onto two 512-texel pages with a single tap
//! and 15 cm cells (see [`QualityLevel::lightmap_config`] and
//! [`QualityLevel::shadow_taps_per_axis`]). Every level bakes from the *same*
//! patch set — density, page size and tap count are the differences, never a
//! different set of surfaces — and all use the same shared chart-span cap, so
//! the geometry splits in the same places.
//!
//! [`QualityProfile`] is the two-variant form of the same budgets that the
//! lightmap content key and the protected lightmap planner consume.
//! [`QualityLevel::profile`] maps a level onto it exactly at that boundary
//! (Low to Low, Medium and High to Full), and Low and High delegate every
//! other decision to the validated profile values, so only Medium is new here.
//!
//! Downscaling happens once, at upload/level-load time, through
//! [`crate::materials::RawImage::downscaled_to`] and is cached with the texture
//! it produced — never per frame, and never twice for the same image. A
//! lightmap atlas is likewise baked once per level load and cached by content
//! key; nothing here runs per frame.

/// The two-variant profile form of the validated budgets.
///
/// [`QualityLevel`] is the player-facing three-level setting; this type is the
/// two-tier boundary the lightmap planner, the lightmap content key and other
/// validated callers consume. [`QualityLevel::profile`] maps a level onto it,
/// and every [`QualityLevel`] decision for Low and High delegates back to the
/// values here, so the validated profile behaviour cannot drift.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QualityProfile {
    /// The intended normal Places presentation: native textures, unchanged.
    #[default]
    Full,
    /// The optional reduced-texture presentation: same assets, smaller sheets.
    Low,
}

/// The texture classes a quality level budgets separately.
///
/// They are separate because their authored sizes and their sampling duties
/// differ: a tiling surface sheet covers metres of wall, a fitted fixture face
/// covers one panel, and a prop sheet covers one model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureClass {
    /// A tiling surface sheet (wall, floor, ceiling).
    Surface,
    /// A fitted fixture face.
    FixtureFace,
    /// A decal cut-out sheet.
    DecalSheet,
    /// A prop model's embedded sheet.
    Prop,
    /// A material's emissive mask.
    EmissionMask,
}

/// Full-quality edge budget: surfaces and fitted sheets at 1024.
const FULL_SHEET_EDGE: u32 = 1_024;
/// Full-quality edge budget: props at their native 256, uploaded unchanged.
const FULL_PROP_EDGE: u32 = 256;
/// Full-quality edge budget: emissive masks at 512.
const FULL_MASK_EDGE: u32 = 512;
/// Medium-quality edge budget: sheets at 512.
const MEDIUM_SHEET_EDGE: u32 = 512;
/// Medium-quality edge budget: emissive masks at 256.
const MEDIUM_MASK_EDGE: u32 = 256;
/// Low-quality edge budget: sheets at 256.
const LOW_SHEET_EDGE: u32 = 256;
/// Low-quality edge budget: native props halved once, 256 -> 128.
const LOW_PROP_EDGE: u32 = 128;
/// Low-quality edge budget: emissive masks at 128.
const LOW_MASK_EDGE: u32 = 128;

/// Medium-quality lightmap density: 12 texels per world metre, between Low's
/// 9 and High's 16.
const MEDIUM_LIGHTMAP_DENSITY: f32 = 12.0;
/// Medium-quality prop-occlusion grid cell, in metres: between Low's 0.15 and
/// High's 0.075.
const MEDIUM_PROP_OCCLUSION_CELL_M: f32 = 0.11;

impl QualityProfile {
    /// Every profile, in report order.
    pub const ALL: [Self; 2] = [Self::Full, Self::Low];

    /// The profile a level loads with when nothing is authored.
    pub const DEFAULT: Self = Self::Full;

    /// Stable lowercase name, as written in `settings.json` and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Low => "low",
        }
    }

    /// Parses a profile name, case-insensitively and ignoring surrounding
    /// whitespace. Unknown names are `None` (the caller keeps its current
    /// profile), never a silent fallback.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|profile| profile.name().eq_ignore_ascii_case(trimmed))
    }

    /// Largest edge length, in texels, an image of `class` may reach the GPU
    /// with under this profile.
    ///
    /// The returned value is a *runtime* budget: the source PNG may legally be
    /// larger (up to [`crate::assets::MAX_TEXTURE_DIMENSION`]) and is downscaled
    /// once, at load time, to fit.
    #[must_use]
    pub const fn budget(self, class: TextureClass) -> u32 {
        match (self, class) {
            (
                Self::Full,
                TextureClass::Surface | TextureClass::FixtureFace | TextureClass::DecalSheet,
            ) => FULL_SHEET_EDGE,
            (Self::Full, TextureClass::Prop) => FULL_PROP_EDGE,
            (Self::Full, TextureClass::EmissionMask) => FULL_MASK_EDGE,
            (
                Self::Low,
                TextureClass::Surface | TextureClass::FixtureFace | TextureClass::DecalSheet,
            ) => LOW_SHEET_EDGE,
            (Self::Low, TextureClass::Prop) => LOW_PROP_EDGE,
            (Self::Low, TextureClass::EmissionMask) => LOW_MASK_EDGE,
        }
    }

    /// True when this profile drops optional per-pixel material work.
    ///
    /// Reserving the hook now keeps later effect work (a future reflection or
    /// post-processing path) from having to invent its own tier names: it asks
    /// the active profile instead.
    #[must_use]
    pub const fn reduces_optional_features(self) -> bool {
        matches!(self, Self::Low)
    }

    /// True when this profile draws the optional surface response.
    ///
    /// The response is the normal-map perturbation and the view-dependent sheen
    /// a material may author ([`crate::materials::response`]). Both profiles draw
    /// the same geometry and the same albedo, emission and alpha; Low simply
    /// leaves the response term out, which is the one per-fragment cost a
    /// constrained GPU can drop without changing what an author authored. It is
    /// a shader gate, not a different asset: the same PNGs and the same
    /// materials reach the GPU under either profile.
    #[must_use]
    pub const fn draws_surface_response(self) -> bool {
        matches!(self, Self::Full)
    }

    /// True when this profile renders the 3D scene at the drawable's own
    /// resolution.
    ///
    /// See [`crate::render::framebuffer`]: Low renders the scene no wider than
    /// the historical 480x272 reference width and presents it across the
    /// drawable, trading scene pixels for performance in the optional Low
    /// presentation.
    #[must_use]
    pub const fn draws_scene_at_drawable_resolution(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Emitter taps per axis for the baked local-pool visibility test.
    ///
    /// `1` is the historical centre-only test (a hard shadow edge), `2` the
    /// five-tap quincunx and `3` the nine-tap 3x3 grid. See
    /// [`crate::lighting::ShadowSampling`]: the taps average a pool's visibility
    /// over the fixture's own emitting rectangle, so a partially blocked pool
    /// fades over a real penumbra instead of ending on a hard line.
    ///
    /// The values are measured, not guessed. On the shipped demo Full's 3x3 grid
    /// is indistinguishable from the quincunx (all 19 fixed views: 0.00 % of
    /// pixels differ by more than 24/255, worst case 13/255) while costing 1.8x
    /// the lightmap fill, so Full takes the quincunx. Low keeps the single
    /// centre tap: it is the cheapest bake (one visibility test per shaded
    /// sample, ~2.3x cheaper than Low with five taps), its 11 cm texels already
    /// smooth the edge, and the five-tap penumbra moves at most 5.5 % of a view's
    /// pixels there. The tap count is a *cost* tier as much as a look tier.
    #[must_use]
    pub const fn shadow_taps_per_axis(self) -> u8 {
        match self {
            Self::Full => 2,
            Self::Low => 1,
        }
    }

    /// Grid cell, in metres, a prop model's triangles are ground into for the
    /// bake's occlusion boxes.
    ///
    /// A finer cell derives more, smaller boxes: a prop's contact shadow and
    /// the pool it blocks follow the model more closely, at a higher bake cost
    /// and a larger occluder set. `Full` uses 0.075 m (half the historical
    /// grid, the finest cell the shipped models resolve without hitting the
    /// box caps); `Low` keeps the historical 0.15 m.
    #[must_use]
    pub const fn prop_occlusion_cell_m(self) -> f32 {
        match self {
            Self::Full => 0.075,
            Self::Low => 0.15,
        }
    }

    /// Every shadow and lightmap setting this profile implies, in one value.
    ///
    /// The lightmap cache key and the bake both read this, so a density, page
    /// or padding change can never leave the two disagreeing about which atlas
    /// a profile describes.
    #[must_use]
    pub const fn lightmap_config(self) -> crate::lighting::lightmap::LightmapConfig {
        crate::lighting::lightmap::LightmapConfig::for_profile(self)
    }

    /// The bake settings this profile implies, in the one value
    /// [`crate::lighting::LevelLighting::bake_with`] consumes.
    ///
    /// This is what keeps the shadow quality differences *centralised*: a
    /// profile answers with its tap count and prop-occlusion cell here, and
    /// nothing else in the renderer needs to know either number.
    #[must_use]
    pub const fn bake_config(self) -> crate::lighting::BakeConfig {
        crate::lighting::BakeConfig {
            sampling: crate::lighting::ShadowSampling {
                taps_per_axis: self.shadow_taps_per_axis(),
            },
            prop_occlusion_cell_m: self.prop_occlusion_cell_m(),
        }
    }
}

/// One of Places' three player-facing quality levels.
///
/// The level is the authoritative player setting and the one every renderer
/// subsystem is parameterised by. It answers with the texture edge budgets
/// ([`Self::budget`]), the scene-target scale (through `scene_target_size`),
/// the lightmap and bake configuration ([`Self::lightmap_config`],
/// [`Self::bake_config`]), the optional-feature gates
/// ([`Self::draws_surface_response`], [`Self::reduces_optional_features`]) and
/// the reflection probe face size.
///
/// Low and High delegate to the validated [`QualityProfile`] values, so a level
/// can never drift from the profile the lightmap planner and content key use;
/// only [`Self::Medium`] introduces new intermediate values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QualityLevel {
    /// The reduced presentation: quarter-size sheets and the cheapest bake.
    Low,
    /// The intermediate presentation: half-size sheets, the drawable scene
    /// target and a denser bake than Low.
    Medium,
    /// The intended normal Places presentation: native textures, unchanged.
    #[default]
    High,
}

impl QualityLevel {
    /// Every level, in player-facing order.
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    /// The level a fresh install runs with.
    pub const DEFAULT: Self = Self::High;

    /// Stable lowercase name, as written in `settings.json` and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Player-facing label for the settings screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// Parses a level name, case-insensitively and ignoring surrounding
    /// whitespace.
    ///
    /// The legacy `"full"` profile name means [`Self::High`]; any other unknown
    /// name is `None` (the caller decides whether to keep its current level or
    /// fall back to the default), never a silent surprise here.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        if let Some(level) = Self::ALL
            .into_iter()
            .find(|level| level.name().eq_ignore_ascii_case(trimmed))
        {
            return Some(level);
        }
        trimmed
            .eq_ignore_ascii_case(QualityProfile::Full.name())
            .then_some(Self::High)
    }

    /// The validated profile this level maps onto at the protected lightmap
    /// content-key boundary.
    ///
    /// Low maps to Low; Medium and High both map to Full, because the content
    /// key already folds the full lightmap configuration (density, page edge,
    /// padding) and the bake settings into the hash, so Medium and High never
    /// share a cache entry.
    #[must_use]
    pub const fn profile(self) -> QualityProfile {
        match self {
            Self::Low => QualityProfile::Low,
            Self::Medium | Self::High => QualityProfile::Full,
        }
    }

    /// Largest edge length, in texels, an image of `class` may reach the GPU
    /// with under this level.
    ///
    /// The returned value is a *runtime* budget: the source PNG may legally be
    /// larger (up to [`crate::assets::MAX_TEXTURE_DIMENSION`]) and is downscaled
    /// once, at load time, to fit.
    #[must_use]
    pub const fn budget(self, class: TextureClass) -> u32 {
        match self {
            Self::Low => QualityProfile::Low.budget(class),
            Self::Medium => match class {
                TextureClass::Surface | TextureClass::FixtureFace | TextureClass::DecalSheet => {
                    MEDIUM_SHEET_EDGE
                }
                TextureClass::Prop => FULL_PROP_EDGE,
                TextureClass::EmissionMask => MEDIUM_MASK_EDGE,
            },
            Self::High => QualityProfile::Full.budget(class),
        }
    }

    /// True when this level drops optional per-pixel material work.
    #[must_use]
    pub const fn reduces_optional_features(self) -> bool {
        match self {
            Self::Low => QualityProfile::Low.reduces_optional_features(),
            Self::Medium | Self::High => QualityProfile::Full.reduces_optional_features(),
        }
    }

    /// True when this level draws the optional surface response.
    ///
    /// The response is the normal-map perturbation and the view-dependent sheen
    /// a material may author ([`crate::materials::response`]). Low leaves it
    /// out; Medium and High draw it.
    #[must_use]
    pub const fn draws_surface_response(self) -> bool {
        match self {
            Self::Low => QualityProfile::Low.draws_surface_response(),
            Self::Medium | Self::High => QualityProfile::Full.draws_surface_response(),
        }
    }

    /// True when this level renders the 3D scene at the drawable's own
    /// resolution.
    ///
    /// Only High always does; Medium caps the scene target's scale at one half
    /// of the drawable and Low at the 480-pixel reference width. See
    /// `scene_target_size` in `render::common::framebuffer`.
    #[must_use]
    pub const fn draws_scene_at_drawable_resolution(self) -> bool {
        match self {
            Self::High => QualityProfile::Full.draws_scene_at_drawable_resolution(),
            Self::Low | Self::Medium => false,
        }
    }

    /// Emitter taps per axis for the baked local-pool visibility test.
    ///
    /// `1` is the historical centre-only test (a hard shadow edge) and `2` the
    /// five-tap quincunx; see [`QualityProfile::shadow_taps_per_axis`] for the
    /// measurement behind the two-tap choice. Low keeps the single centre tap,
    /// Medium and High take the quincunx.
    #[must_use]
    pub const fn shadow_taps_per_axis(self) -> u8 {
        match self {
            Self::Low => QualityProfile::Low.shadow_taps_per_axis(),
            Self::Medium | Self::High => QualityProfile::Full.shadow_taps_per_axis(),
        }
    }

    /// Grid cell, in metres, a prop model's triangles are ground into for the
    /// bake's occlusion boxes.
    ///
    /// High resolves the finest contact shadows (0.075 m), Medium an
    /// intermediate 0.11 m and Low the historical 0.15 m.
    #[must_use]
    pub const fn prop_occlusion_cell_m(self) -> f32 {
        match self {
            Self::Low => QualityProfile::Low.prop_occlusion_cell_m(),
            Self::Medium => MEDIUM_PROP_OCCLUSION_CELL_M,
            Self::High => QualityProfile::Full.prop_occlusion_cell_m(),
        }
    }

    /// Every shadow and lightmap setting this level implies, in one value.
    ///
    /// The lightmap cache key and the bake both read this, so a density, page
    /// or padding change can never leave the two disagreeing about which atlas
    /// a level describes.
    #[must_use]
    pub const fn lightmap_config(self) -> crate::lighting::lightmap::LightmapConfig {
        let full = crate::lighting::lightmap::LightmapConfig::for_profile(QualityProfile::Full);
        match self {
            Self::Low => {
                crate::lighting::lightmap::LightmapConfig::for_profile(QualityProfile::Low)
            }
            Self::Medium => crate::lighting::lightmap::LightmapConfig {
                texels_per_metre: MEDIUM_LIGHTMAP_DENSITY,
                page_edge: full.page_edge,
                max_pages: full.max_pages,
                padding: full.padding,
                bytes_per_texel: full.bytes_per_texel,
            },
            Self::High => full,
        }
    }

    /// The bake settings this level implies, in the one value
    /// [`crate::lighting::LevelLighting::bake_with`] consumes.
    ///
    /// This is what keeps the shadow quality differences *centralised*: a level
    /// answers with its tap count and prop-occlusion cell here, and nothing
    /// else in the renderer needs to know either number.
    #[must_use]
    pub const fn bake_config(self) -> crate::lighting::BakeConfig {
        let full = QualityProfile::Full.bake_config();
        match self {
            Self::Low => QualityProfile::Low.bake_config(),
            Self::Medium => crate::lighting::BakeConfig {
                sampling: crate::lighting::ShadowSampling {
                    taps_per_axis: full.sampling.taps_per_axis,
                },
                prop_occlusion_cell_m: MEDIUM_PROP_OCCLUSION_CELL_M,
            },
            Self::High => full,
        }
    }
}

/// The player-facing Lightmaps quality: whether a baked atlas exists and how
/// dense it is.
///
/// This is deliberately independent of [`QualityLevel`]: the overall quality
/// preset selects one of these as its default, but the player may override it,
/// so `Low + Lightmaps Full` and `High + Lightmaps Off` are both valid. The
/// setting owns the whole bake configuration (atlas density/page and the
/// shadow taps/prop-occlusion cell), so two configurations that produce
/// different baked texels can never share a cache entry.
///
/// The sampled atlas keeps its own fixed clamped linear sampler; this setting
/// never touches ordinary world Texture Filtering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LightmapQuality {
    /// No bake at all: the validated historical vertex-lit path.
    Off,
    /// The intermediate atlas: Medium density at the Full page budget.
    Medium,
    /// The full/high-quality atlas.
    #[default]
    Full,
}

impl LightmapQuality {
    /// Every level, in selector order.
    pub const ALL: [Self; 3] = [Self::Off, Self::Medium, Self::Full];

    /// The default a fresh install runs with.
    pub const DEFAULT: Self = Self::Full;

    /// Stable lowercase name, as written in `settings.json` and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Medium => "medium",
            Self::Full => "full",
        }
    }

    /// Player-facing label for the settings screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Medium => "Medium",
            Self::Full => "Full",
        }
    }

    /// Parses a level name, case-insensitively and ignoring surrounding
    /// whitespace. Unknown names are `None`, never a silent fallback.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|level| level.name().eq_ignore_ascii_case(trimmed))
    }

    /// True when no lightmap is baked or sampled.
    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    /// The atlas configuration this level bakes against, or `None` for
    /// [`Self::Off`] (which never builds a plan).
    ///
    /// Medium and Full read the same validated values the quality levels used
    /// before this setting existed, so an atlas baked by an earlier build keeps
    /// its content key and is reused.
    #[must_use]
    pub const fn lightmap_config(self) -> Option<crate::lighting::lightmap::LightmapConfig> {
        let full = crate::lighting::lightmap::LightmapConfig::for_profile(QualityProfile::Full);
        match self {
            Self::Off => None,
            Self::Medium => Some(crate::lighting::lightmap::LightmapConfig {
                texels_per_metre: MEDIUM_LIGHTMAP_DENSITY,
                page_edge: full.page_edge,
                max_pages: full.max_pages,
                padding: full.padding,
                bytes_per_texel: full.bytes_per_texel,
            }),
            Self::Full => Some(full),
        }
    }

    /// The shadow bake this level runs, or `None` for [`Self::Off`].
    ///
    /// The vertex-lit fallback always bakes with the historical hard-shadow
    /// configuration, exactly as it did before lightmap qualities existed; that
    /// is the build path's decision, not this setting's.
    #[must_use]
    pub const fn bake_config(self) -> Option<crate::lighting::BakeConfig> {
        match self {
            Self::Off => None,
            Self::Medium => Some(QualityLevel::Medium.bake_config()),
            Self::Full => Some(QualityProfile::Full.bake_config()),
        }
    }

    /// The profile the protected lightmap content-key boundary consumes.
    ///
    /// The key hashes the concrete [`Self::lightmap_config`] and
    /// [`Self::bake_config`] values as well, so Medium and Full still never
    /// share an entry even though both answer `Full` here.
    #[must_use]
    pub const fn profile(self) -> QualityProfile {
        QualityProfile::Full
    }

    /// The preset default for an overall quality level.
    #[must_use]
    pub const fn default_for(quality: QualityLevel) -> Self {
        match quality {
            QualityLevel::Low => Self::Off,
            QualityLevel::Medium => Self::Medium,
            QualityLevel::High => Self::Full,
        }
    }
}

/// The player-facing Reflections quality: which optional reflection sources
/// exist and at what probe resolution.
///
/// Independent of [`QualityLevel`] and of the Lightmaps setting, so
/// `Low + Reflections Full` is valid. Turning reflections off retires the probe
/// cubemaps and the planar target instead of leaving stale captures bound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReflectionQuality {
    /// No dynamic reflection work: no probes, no planar mirror.
    Off,
    /// The intermediate probe resolution with the planar mirror enabled.
    Medium,
    /// The full probe resolution with the planar mirror enabled.
    #[default]
    Full,
}

impl ReflectionQuality {
    /// Every level, in selector order.
    pub const ALL: [Self; 3] = [Self::Off, Self::Medium, Self::Full];

    /// The default a fresh install runs with.
    pub const DEFAULT: Self = Self::Full;

    /// Stable lowercase name, as written in `settings.json` and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Medium => "medium",
            Self::Full => "full",
        }
    }

    /// Player-facing label for the settings screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Medium => "Medium",
            Self::Full => "Full",
        }
    }

    /// Parses a level name, case-insensitively and ignoring surrounding
    /// whitespace. Unknown names are `None`, never a silent fallback.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|level| level.name().eq_ignore_ascii_case(trimmed))
    }

    /// True when the probe cubemaps exist and are sampled.
    #[must_use]
    pub const fn draws_probes(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// True when the planar mirror pass may run.
    #[must_use]
    pub const fn draws_planar(self) -> bool {
        matches!(self, Self::Medium | Self::Full)
    }

    /// The preset default for an overall quality level.
    #[must_use]
    pub const fn default_for(quality: QualityLevel) -> Self {
        match quality {
            QualityLevel::Low => Self::Off,
            QualityLevel::Medium => Self::Medium,
            QualityLevel::High => Self::Full,
        }
    }
}

#[cfg(test)]
mod tests;

/// Returns the image a texture uploads with under `level`.
///
/// High keeps the decoded image exactly as it is — native 256x256 prop sheets
/// and 1024x1024 surfaces are uploaded unchanged, with no rescale and no copy.
/// Medium and Low box-filter it until it fits the level's budget for its class.
/// The caller does this at upload time and keeps the result with the texture it
/// uploaded, so an image is never rescaled per frame, nor twice for one upload.
#[must_use]
pub fn fit_image(
    image: &crate::materials::RawImage,
    level: QualityLevel,
    class: TextureClass,
) -> std::borrow::Cow<'_, crate::materials::RawImage> {
    image.downscaled_to(level.budget(class)).map_or_else(
        || std::borrow::Cow::Borrowed(image),
        std::borrow::Cow::Owned,
    )
}
