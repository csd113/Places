//! Renderer-neutral material draw state: alpha classification, emission
//! routing and the per-material parameters derived from the resolved material
//! table.
//!
//! None of this touches a GPU resource; the renderer binds the resolved
//! texture slots to its own GPU resources when it records a draw.

use super::mesh::{SurfaceKey, SurfaceKind};
use crate::materials::{
    AlphaMode, MaterialAlpha, MaterialEmission, MaterialReflection, MaterialResponse, MaterialTable,
};

/// Which draw pass one batch belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchPass {
    /// Written in the opaque pass with depth writes on.
    Opaque,
    /// Written in the opaque pass through the alpha-tested program.
    Cutout,
    /// Written after everything opaque, sorted and blended, with depth writes
    /// off.
    Translucent,
}

impl BatchPass {
    /// The pass a material's alpha contract implies.
    #[must_use]
    pub const fn of(alpha: MaterialAlpha) -> Self {
        match alpha.mode {
            AlphaMode::Opaque => Self::Opaque,
            AlphaMode::Cutout => Self::Cutout,
            AlphaMode::Blend => {
                if alpha.opacity > 0.0 {
                    Self::Translucent
                } else {
                    // Nothing to see through: an invisible surface must not be
                    // submitted at all, and the opaque pass is where a
                    // zero-opacity surface is cheapest to skip.
                    Self::Opaque
                }
            }
        }
    }
}

/// Which pass one static surface key belongs to, given its material's alpha.
///
/// Fixtures, placeholder prop boxes and decals are opaque by construction: their
/// alpha never reaches the framebuffer. Only a floor, ceiling or wall that binds
/// a level material can be translucent, because only a level material authors an
/// alpha contract at all.
#[must_use]
pub fn batch_pass_for(
    kind: SurfaceKind,
    has_material: bool,
    alpha: Option<MaterialAlpha>,
) -> BatchPass {
    match kind {
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall if has_material => {
            alpha.map_or(BatchPass::Opaque, BatchPass::of)
        }
        SurfaceKind::Floor
        | SurfaceKind::Ceiling
        | SurfaceKind::Wall
        | SurfaceKind::Light
        | SurfaceKind::PropFallback
        | SurfaceKind::Decal => BatchPass::Opaque,
    }
}

/// Which emission term one static surface batch draws with.
///
/// Split out from [`Renderer::static_emission`] so the routing — the part that
/// decides *whether* a surface emits — is testable without a GL context, and so
/// the rule lives in one place instead of in a match spread over the draw path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmissionRouting {
    /// No emission term: plain lit surfaces.
    None,
    /// The batch's material decides (floors, ceilings, walls).
    Material,
    /// The vertex colour *is* the emission (a fixture's luminous face, whose
    /// glow is per placement while the batch is shared).
    Vertex,
}

/// The emission term a static surface kind uses.
///
/// `has_material` is [`SurfaceKey::has_material`]: a light batch without one is
/// the fixture's untextured housing, which is lit like any other surface.
pub const fn emission_routing(kind: SurfaceKind, has_material: bool) -> EmissionRouting {
    match kind {
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall => EmissionRouting::Material,
        SurfaceKind::Light if has_material => EmissionRouting::Vertex,
        SurfaceKind::Light | SurfaceKind::PropFallback | SurfaceKind::Decal => {
            EmissionRouting::None
        }
    }
}

/// The per-material parameters the draw path needs, derived once per level from
/// the resolved material table.
///
/// One place derives all four vectors, so a new material property cannot be
/// resolved correctly and then simply not reach the draw path: the failure this
/// type exists to prevent. Every vector is parallel and indexed by *material
/// index* (what a batch's `SurfaceKey` carries), not by texture index.
#[derive(Default)]
pub struct MaterialRenderState {
    /// Texture slot per material index (what `Renderer::material_textures` is
    /// indexed by). Two materials that share one texture map to one slot.
    pub texture_slots: Vec<u16>,
    /// Emission per material index.
    pub emissions: Vec<MaterialEmission>,
    /// Surface response per material index.
    pub responses: Vec<MaterialResponse>,
    /// Alpha contract per material index.
    pub alphas: Vec<MaterialAlpha>,
    /// Reflection contract per material index.
    pub reflections: Vec<MaterialReflection>,
}

impl MaterialRenderState {
    /// Derives every per-material parameter from a resolved table.
    pub fn from_table(table: &MaterialTable) -> Self {
        let mut state = Self::default();
        for entry in table.entries() {
            state.texture_slots.push(entry.texture_index);
            state.emissions.push(entry.emission);
            state.responses.push(entry.response);
            state.alphas.push(entry.alpha);
            state.reflections.push(entry.reflection);
        }
        state
    }
}

/// The final renderer-neutral material state one static surface draws with.
///
/// This is the material contract every draw resolves through: it folds the
/// resolved material table, the surface's per-surface shine override and the
/// active quality level into one description, applying exactly the rules the
/// reference renderer's `static_surface_state` applied for a floor, ceiling or
/// wall. It references the neutral material table by index and never names a
/// GPU object, so the renderer and the tests share it.
///
/// Fixture, prop-placeholder and decal keys never bind a level material, so
/// they resolve to the plain state (fallback texture, no response, opaque, no
/// reflection), which is what their own draw paths expect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedSurfaceMaterial {
    /// Base-colour texture index into `MaterialTable::textures`, or `None` for
    /// the shared fallback sheet.
    pub texture: Option<u16>,
    /// Normal-map texture index into `MaterialTable::textures`, or `None` when
    /// the material authors none or the profile gate is off.
    pub normal: Option<u16>,
    /// Multiplier applied to the decoded normal's `xy`.
    pub normal_strength: f32,
    /// Sheen colour; zeroed when the response is not drawn.
    pub specular: [f32; 3],
    /// Shader-facing roughness: the per-surface shine override when authored,
    /// the material's own resolved roughness otherwise.
    pub roughness: f32,
    /// Alpha mode, opacity multiplier and cut-out threshold.
    pub alpha: MaterialAlpha,
    /// The material's authored reflection contract. Reflection *strength* as
    /// the shader would fold it in is `specular × reflection.strength`, so the
    /// profile gate that zeroes the sheen zeroes the reflection with it.
    pub reflection: MaterialReflection,
    /// True when the normal/sheen response is drawn: the quality level allows
    /// the response and the material authors a normal map or a sheen.
    pub response_enabled: bool,
}

impl ResolvedSurfaceMaterial {
    /// The state a key without a level material draws with: the fallback sheet,
    /// the geometric normal, no sheen and opaque coverage.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            texture: None,
            normal: None,
            normal_strength: crate::materials::DEFAULT_NORMAL_STRENGTH,
            specular: [0.0; 3],
            roughness: crate::materials::DEFAULT_ROUGHNESS,
            alpha: MaterialAlpha::OPAQUE,
            reflection: MaterialReflection::NONE,
            response_enabled: false,
        }
    }

    /// The reflection strength the reference shader folds in, per channel:
    /// the material's specular colour times the authored reflection strength.
    ///
    /// The world shader scales every reflected sample by this value.
    #[must_use]
    pub fn reflection_strength(&self) -> [f32; 3] {
        [
            self.specular[0] * self.reflection.strength,
            self.specular[1] * self.reflection.strength,
            self.specular[2] * self.reflection.strength,
        ]
    }

    /// True when this material would participate in a reflection if one were
    /// rendered: an authored mode and strength plus a non-zero weighted
    /// strength. The material path records this; the reflection bindings
    /// consume it.
    #[must_use]
    pub fn reflection_eligible(&self) -> bool {
        self.reflection.is_active() && self.reflection_strength().iter().any(|c| *c > 0.0)
    }
}

/// Resolves one static surface key to its final neutral material state.
///
/// `response_allowed` is the active quality level's response gate
/// ([`crate::quality::QualityLevel::draws_surface_response`]): with it false
/// the normal map is gated off and the sheen is zeroed, but the albedo, alpha,
/// vertex colour and reflection *mode* are untouched — exactly the reference's
/// Low behaviour. The reflection strength follows the zeroed sheen.
///
/// This is the one place the surface-key → material-state rules live: the
/// renderer consumes the result directly, and the preserved reference
/// renderer applied the same rules to its own GL objects.
#[must_use]
pub fn resolve_surface_material(
    key: SurfaceKey,
    materials: &MaterialRenderState,
    table: &MaterialTable,
    response_allowed: bool,
) -> ResolvedSurfaceMaterial {
    if !matches!(
        key.kind,
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
    ) || !key.has_material()
    {
        return ResolvedSurfaceMaterial::plain();
    }
    let material = usize::from(key.material);
    let texture = materials
        .texture_slots
        .get(material)
        .copied()
        .filter(|slot| table.textures().get(usize::from(*slot)).is_some());
    let response = materials
        .responses
        .get(material)
        .copied()
        .unwrap_or(MaterialResponse::NONE);
    let alpha = materials
        .alphas
        .get(material)
        .copied()
        .unwrap_or(MaterialAlpha::OPAQUE);
    let reflection = materials
        .reflections
        .get(material)
        .copied()
        .unwrap_or(MaterialReflection::NONE);
    // The master response gate: a profile without surface response draws no
    // normal map and no sheen, and therefore no reflection strength either.
    let live = response.is_active() && response_allowed;
    ResolvedSurfaceMaterial {
        texture,
        normal: if live { response.normal } else { None },
        normal_strength: response.normal_strength,
        specular: if live { response.specular } else { [0.0; 3] },
        roughness: key.roughness(response.roughness),
        alpha,
        reflection,
        response_enabled: live,
    }
}
