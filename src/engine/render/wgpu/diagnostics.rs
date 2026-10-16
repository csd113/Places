//! Opt-in views of data the existing native renderer actually consumes.
//!
//! No bake, mesh or material is changed. The camera's padding word carries the
//! selector, and the production fragment body remains the final-mode body.
//! Unsupported decomposition and chart/probe metadata are named explicitly.

use std::borrow::Cow;

/// Native world diagnostic selected independently of graphics settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisualDiagnosticMode {
    #[default]
    Final,
    Albedo,
    VertexNormal,
    WorldNormal,
    BakedLight,
    LightmapValues,
    ChartUv,
    Roughness,
    Depth,
    Distance,
    LightingState,
    ProbeField,
    ProbeNeighborhood,
    EntityDirect,
    EntityIndirect,
}

impl VisualDiagnosticMode {
    /// F8's stable order; mode codes and capture names are part of the contract.
    pub const ALL: [Self; 15] = [
        Self::Final,
        Self::Albedo,
        Self::VertexNormal,
        Self::WorldNormal,
        Self::BakedLight,
        Self::LightmapValues,
        Self::ChartUv,
        Self::Roughness,
        Self::Depth,
        Self::Distance,
        Self::LightingState,
        Self::ProbeField,
        Self::ProbeNeighborhood,
        Self::EntityDirect,
        Self::EntityIndirect,
    ];

    /// Parses a supported view, reporting unavailable data separately from typos.
    ///
    /// # Errors
    /// Returns the unavailable-data reason or the list of supported names.
    pub fn parse(selector: &str) -> Result<Self, String> {
        let name = selector.trim().to_ascii_lowercase();
        if let Some(reason) = Self::unsupported_reason(&name) {
            return Err(format!(
                "visual diagnostic `{name}` is unavailable: {reason}"
            ));
        }
        Self::ALL
            .into_iter()
            .find(|mode| mode.name() == name)
            .ok_or_else(|| {
                let names = Self::ALL.map(Self::name).join(", ");
                format!("unknown visual diagnostic `{name}`; supported: {names}")
            })
    }

    /// Data absent from the resident GPU payload must not get a substitute view.
    #[must_use]
    pub fn unsupported_reason(name: &str) -> Option<&'static str> {
        match name {
            "direct" | "indirect" | "filtered" | "filled" => Some(
                "runtime payload contains combined baked coefficients only; solver stages are offline exports",
            ),
            "shadow" | "ao" => Some("there is no standalone shadow or AO coefficient payload"),
            "metallic" => Some("the renderer has no metallic shader input"),
            "chart-boundaries" | "seams" => Some(
                "true chart IDs and bounds are not resident; chart-uv shows atlas UVs and a grid only",
            ),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::Albedo => "albedo",
            Self::VertexNormal => "vertex-normal",
            Self::WorldNormal => "world-normal",
            Self::BakedLight => "baked-light",
            Self::LightmapValues => "lightmap-values",
            Self::ChartUv => "chart-uv",
            Self::Roughness => "roughness",
            Self::Depth => "depth",
            Self::Distance => "distance",
            Self::LightingState => "lighting-state",
            Self::ProbeField => "probe-field",
            Self::ProbeNeighborhood => "probe-neighborhood",
            Self::EntityDirect => "entity-direct",
            Self::EntityIndirect => "entity-indirect",
        }
    }

    /// Exact small integer represented in the camera's f32 padding word.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Final => 0,
            Self::Albedo => 1,
            Self::VertexNormal => 2,
            Self::WorldNormal => 3,
            Self::BakedLight => 4,
            Self::LightmapValues => 5,
            Self::ChartUv => 6,
            Self::Roughness => 7,
            Self::Depth => 8,
            Self::Distance => 9,
            Self::LightingState => 10,
            Self::ProbeField => 11,
            Self::ProbeNeighborhood => 12,
            Self::EntityDirect => 13,
            Self::EntityIndirect => 14,
        }
    }

    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Final)
    }

    #[must_use]
    pub fn next(self) -> Self {
        let position = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL
            .get(position.saturating_add(1))
            .copied()
            .unwrap_or(Self::Final)
    }

    /// Machine-readable capture provenance includes the view's physical meaning.
    #[must_use]
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Final => {
                "ordinary final composition including sky, fog, response, emission, reflections and post"
            }
            Self::Albedo => {
                "sampled authored base PNG RGB only; excludes tint, lighting and post; source tint is not recoverable from vertex-lit static colors"
            }
            Self::VertexNormal => {
                "uploaded interpolated world normal, normalized; RGB=(N+1)/2; missing source normals use geometric shading in world-normal mode"
            }
            Self::WorldNormal => {
                "actual oriented shading normal including normal map and posed actor geometric derivative; RGB=(N+1)/2"
            }
            Self::BakedLight => {
                "combined linear HDR baked diffuse reconstructed at actual shading normal before material and dynamic lights; RGB=max(value,0)/(1+max(value,0)); entity fallback uses authored linear lighting; static vertex-lit coefficients unavailable"
            }
            Self::LightmapValues => {
                "combined atlas irradiance RGB planes plus enabled switchable groups, before normal reconstruction; RGB=max(HDR,0)/(1+max(HDR,0)); actors and vertex fallback unavailable"
            }
            Self::ChartUv => {
                "atlas UV in red/green, page identity in blue, white 16x16 atlas grid; grid is not chart IDs, boundaries or seam truth"
            }
            Self::Roughness => {
                "actual material shader roughness scalar in RGB, including default plain-material input and disabled response; not metallic"
            }
            Self::Depth => {
                "actual hardware fragment depth z in [0,1], grayscale; nonlinear perspective device depth"
            }
            Self::Distance => {
                "radial eye-to-fragment distance in metres divided by 100 and clamped; not linear view depth"
            }
            Self::LightingState => {
                "cyan=resident atlas, green=prepared entity probe, red=entity fallback, amber=static vertex fallback; state color is not illumination energy"
            }
            Self::ProbeField => {
                "native markers at prepared lattice positions: green=valid air-labelled coefficient, red=invalid slot; grayscale background is actual baked response; markers depth-test against real scene"
            }
            Self::ProbeNeighborhood => {
                "first movable object's bounds centre; cyan markers/links are accepted static+moving visibility contributors, brightness shows normalized weight; orange=rejected valid slot within support, red=invalid slot, yellow=query; grayscale background is actual baked response"
            }
            Self::EntityDirect => {
                "selected local direct sources evaluated at the actual entity fragment, with finite taps, visibility and cosine; HDR/(1+HDR); static and legacy data unavailable"
            }
            Self::EntityIndirect => {
                "entity residual probe response after selected local direct coefficients are subtracted before reconstruction; HDR/(1+HDR); static and legacy data unavailable"
            }
        }
    }
}

/// The exact insertion point is checked by tests; production WGSL is untouched.
const SHADE_HOOK: &str = "    let normal = normals.material;";
const DIAGNOSTIC_HOOK: &str = "
    let visual_mode = u32(camera._padding);
    if (visual_mode != 0u) {
        var visual_result: Shaded;
        visual_result.color = visual_diagnostic_color(in, base_linear, normal, visual_mode);
        visual_result.alpha = alpha;
        return visual_result;
    }";

/// Adds a branch only in feature builds. Mode zero executes the original body.
#[must_use]
pub fn world_shader_source(production: &'static str) -> Cow<'static, str> {
    let replacement = format!("{SHADE_HOOK}{DIAGNOSTIC_HOOK}");
    let mut source = production.replacen(SHADE_HOOK, &replacement, 1);
    source.push_str(include_str!("diagnostics.wgsl"));
    Cow::Owned(source)
}

/// Facts from the capture's actual base-scene encoding, before presentation
/// and emissive duplicates. Vertex totals retain the draw route's definition;
/// they are never converted into an inferred triangle count.
#[must_use]
pub fn capture_submission(totals: &super::world::WorldDrawTotals) -> serde_json::Value {
    serde_json::json!({
        "draw_calls": totals.draw_calls,
        "frustum_visible_batches": totals.visible_batches,
        "frustum_visible_distinct_vertices": totals.visible_vertices,
        "submitted_indices": totals.visible_indices,
        "submitted_triangles": totals.visible_indices.checked_div(3).unwrap_or_default(),
        "texture_binds": totals.texture_binds,
        "material_binds": totals.material_binds,
        "scope": "base-scene accounted world/props/dynamics/actors plus decals/effects; excludes sky, reflections, emissive duplicates, post and UI",
        "visibility": "CPU frustum acceptance and encoded draws, not GPU occlusion visibility or FPS",
        "vertices": "sum of distinct indexed vertices per submitted range/object, not a globally deduplicated count or triangle estimate",
    })
}
