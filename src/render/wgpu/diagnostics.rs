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
}

impl VisualDiagnosticMode {
    /// F8's stable order; mode codes and capture names are part of the contract.
    pub const ALL: [Self; 11] = [
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

    /// Explicit Stage 4 extension seam: availability changes only when a real
    /// probe visualization provider is implemented, never by reusing lightmaps.
    #[must_use]
    pub const fn probe_visualization_unavailable() -> &'static str {
        "probe positions, validity and visibility labels have no runtime visualization provider"
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
            "probe-field" => Some(Self::probe_visualization_unavailable()),
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
        }
    }
}

/// The exact insertion point is checked by tests; production WGSL is untouched.
const SHADE_HOOK: &str = "    let normal = material_normal(in, front_facing);";
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
        "texture_binds": totals.texture_binds,
        "material_binds": totals.material_binds,
        "scope": "base-scene accounted world/props/dynamics/actors plus decals/effects; excludes sky, reflections, emissive duplicates, post and UI",
        "visibility": "CPU frustum acceptance and encoded draws, not GPU occlusion visibility or FPS",
        "vertices": "sum of distinct indexed vertices per submitted range/object, not a globally deduplicated count or triangle estimate",
    })
}

#[cfg(test)]
mod tests {
    use super::{DIAGNOSTIC_HOOK, SHADE_HOOK, VisualDiagnosticMode, world_shader_source};

    #[test]
    fn capture_submission_retains_scene_counts_and_their_scope() {
        let totals = super::super::world::WorldDrawTotals {
            draw_calls: 7,
            visible_batches: 5,
            visible_vertices: 61,
            texture_binds: 3,
            material_binds: 4,
            opaque_draws: 2,
            cutout_draws: 1,
            translucent_draws: 2,
            emissive_visible: true,
        };
        let receipt = super::capture_submission(&totals);
        assert_eq!(
            receipt
                .get("draw_calls")
                .and_then(serde_json::Value::as_u64),
            Some(7),
            "capture uses exact encoded calls"
        );
        assert_eq!(
            receipt
                .get("frustum_visible_batches")
                .and_then(serde_json::Value::as_u64),
            Some(5),
            "capture uses exact range count"
        );
        assert_eq!(
            receipt
                .get("frustum_visible_distinct_vertices")
                .and_then(serde_json::Value::as_u64),
            Some(61),
            "distinct vertices stay vertices"
        );
        assert_eq!(
            receipt
                .get("texture_binds")
                .and_then(serde_json::Value::as_u64),
            Some(3),
            "capture uses exact texture binds"
        );
        assert_eq!(
            receipt
                .get("material_binds")
                .and_then(serde_json::Value::as_u64),
            Some(4),
            "capture uses exact material binds"
        );
        assert!(
            receipt.get("triangles").is_none(),
            "no triangles are inferred from distinct vertices"
        );
        assert!(
            receipt
                .get("scope")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|scope| scope
                    .contains("excludes sky, reflections, emissive duplicates, post and UI")),
            "duplicate and unaccounted passes are explicit"
        );
    }

    #[test]
    fn supported_modes_round_trip_and_cycle_once() {
        let mut mode = VisualDiagnosticMode::Final;
        for (index, expected) in VisualDiagnosticMode::ALL.into_iter().enumerate() {
            assert_eq!(mode, expected, "cycle order is stable");
            assert_eq!(
                VisualDiagnosticMode::parse(mode.name()),
                Ok(mode),
                "names round trip"
            );
            assert_eq!(
                usize::from(mode.code()),
                index,
                "shader selector code is stable"
            );
            mode = mode.next();
        }
        assert_eq!(mode, VisualDiagnosticMode::Final, "cycle returns to final");
        assert_eq!(
            VisualDiagnosticMode::parse(" WORLD-NORMAL "),
            Ok(VisualDiagnosticMode::WorldNormal),
            "selector ignores case and outer space"
        );
    }

    #[test]
    fn absent_data_is_rejected_instead_of_relabelled() {
        for name in [
            "direct",
            "indirect",
            "shadow",
            "ao",
            "metallic",
            "chart-boundaries",
            "seams",
            "probe-field",
        ] {
            let result = VisualDiagnosticMode::parse(name);
            assert!(result.is_err(), "{name} must not select fabricated data");
            assert!(
                result
                    .err()
                    .is_some_and(|error| error.contains("unavailable")),
                "{name} describes its limitation"
            );
        }
        assert!(
            VisualDiagnosticMode::parse("typo").is_err(),
            "unknown selectors fail"
        );
    }

    #[test]
    fn final_mode_retains_the_production_body_and_wire_layout() {
        let production = super::super::world::WORLD_SHADER_SRC;
        assert_eq!(
            production.matches(SHADE_HOOK).count(),
            1,
            "one precise shader extension seam"
        );
        let source = world_shader_source(production);
        let diagnostic_shader = include_str!("diagnostics.wgsl");
        let restored = source.replace(DIAGNOSTIC_HOOK, "");
        assert_eq!(
            restored.strip_suffix(diagnostic_shader),
            Some(production),
            "final body stays byte-for-byte unchanged"
        );
        assert!(
            source.contains("if (visual_mode != 0u)"),
            "final skips diagnostics"
        );
        assert!(
            DIAGNOSTIC_HOOK.contains("visual_result.alpha = alpha;"),
            "diagnostics preserve the actual alpha input"
        );
        assert!(
            production
                .find("if (cutout && alpha < material.alpha_cutoff)")
                .zip(production.find(SHADE_HOOK))
                .is_some_and(|(cutout, hook)| cutout < hook),
            "diagnostics retain production alpha cutoff before the view branch"
        );
        assert_eq!(
            std::mem::size_of::<super::super::world::CameraUniform>(),
            80,
            "camera layout remains compatible"
        );
        assert_eq!(
            std::mem::offset_of!(super::super::world::CameraUniform, diagnostic_selector),
            76,
            "selector occupies existing padding"
        );
    }

    #[test]
    fn composed_diagnostic_shader_parses_and_validates() -> Result<(), String> {
        use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};
        let source = world_shader_source(super::super::world::WORLD_SHADER_SRC);
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .map_err(|error| error.emit_to_string(&source))?;
        let _validated = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
