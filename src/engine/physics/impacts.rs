//! Plain impact descriptors and bounded presentation-rate bookkeeping.

use glam::Vec3;

use super::FIXED_STEP;
use super::collision::{add, subtract};

const MIN_IMPACT_SPEED: f32 = 0.35;
const MIN_IMPACT_IMPULSE: f32 = 0.01;
const ITEM_COOLDOWN: f32 = 0.12;
const GLOBAL_BURST: f32 = 8.0;
const GLOBAL_EVENTS_PER_SECOND: f32 = 24.0;

/// Broad physical surface family, independent of any playback backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SurfaceMaterial {
    #[default]
    Hard,
    Carpet,
    Wood,
    Metal,
    Glass,
    Grass,
    Gravel,
    Sand,
    Snow,
    Water,
}

impl SurfaceMaterial {
    /// Conservative fallback for existing material identifiers. Unknown pack
    /// materials retain a hard-surface response; no texture pixels are read.
    #[must_use]
    pub fn from_material_id(id: &str) -> Self {
        if id.contains("carpet") || id.contains("fabric") {
            Self::Carpet
        } else if id.contains("wood") || id.contains("timber") {
            Self::Wood
        } else if id.contains("metal") || id.contains("steel") || id.contains("brass") {
            Self::Metal
        } else if id.contains("glass") {
            Self::Glass
        } else if id.contains("grass") {
            Self::Grass
        } else if id.contains("gravel") || id.contains("dirt") {
            Self::Gravel
        } else if id.contains("sand") {
            Self::Sand
        } else if id.contains("snow") {
            Self::Snow
        } else if id.contains("water") {
            Self::Water
        } else {
            Self::Hard
        }
    }
}

/// A meaningful solved contact onset. At most 32 events wait for a consumer.
#[derive(Debug, Clone, Copy)]
pub struct ImpactEvent {
    pub key: u64,
    pub other_key: Option<u64>,
    pub position: Vec3,
    /// Solved normal impulse in newton seconds.
    pub impulse: f32,
    /// Incoming relative normal speed at the contact, in metres per second.
    pub speed: f32,
    /// Bounded perceptual intensity in `0..=1`, before consumer volume controls.
    pub strength: f32,
    pub surface: SurfaceMaterial,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ImpactMotion {
    pub linear: Vec3,
    pub angular: Vec3,
    pub center: Vec3,
}

impl ImpactMotion {
    pub fn at(self, position: Vec3) -> Vec3 {
        add(
            self.linear,
            self.angular.cross(subtract(position, self.center)),
        )
    }
}

pub(super) struct ImpactBudget {
    tokens: f32,
}

impl Default for ImpactBudget {
    fn default() -> Self {
        Self {
            tokens: GLOBAL_BURST,
        }
    }
}

impl ImpactBudget {
    pub const fn advance(&mut self) {
        self.tokens = (GLOBAL_EVENTS_PER_SECOND.mul_add(FIXED_STEP, self.tokens)).min(GLOBAL_BURST);
    }

    pub fn take(&mut self, speed: f32, impulse: f32, cooldown: f32) -> Option<f32> {
        if !speed.is_finite()
            || !impulse.is_finite()
            || speed < MIN_IMPACT_SPEED
            || impulse < MIN_IMPACT_IMPULSE
            || cooldown > 0.0
            || self.tokens < 1.0
        {
            return None;
        }
        self.tokens -= 1.0;
        Some((speed / 5.0).sqrt().clamp(0.1, 1.0))
    }

    pub const fn item_cooldown() -> f32 {
        ITEM_COOLDOWN
    }
}

#[cfg(test)]
#[path = "../tests/physics_impacts.rs"]
mod tests;
