//! Map-owned tool inventory and tuning. Omission grants no tools.
use serde::{Deserialize, Serialize};

pub const CROWBAR_MODEL: &str = "shared/tools/crowbar.glb";
pub const TOOL_ARM_MODEL: &str = "shared/tools/right_arm.glb";

#[derive(Clone, Copy)]
pub struct ToolViewPose {
    pub grip: glam::Vec3,
    pub rotation: glam::Quat,
    pub eye: glam::Vec3,
    pub arm_tint: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolSettings {
    pub crowbar: bool,
    pub damage_hp: u16,
    pub reach_m: f32,
    pub swings_per_second: f32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            crowbar: false,
            damage_hp: 2,
            reach_m: 1.75,
            swings_per_second: 1.8,
        }
    }
}

impl ToolSettings {
    /// # Errors
    /// Rejects invalid tuning even when this map currently grants no tool.
    pub fn validate(self) -> Result<(), String> {
        if self.damage_hp == 0 {
            return Err("Tool damage must be positive whole HP".into());
        }
        if !self.reach_m.is_finite() || !(0.1..=4.0).contains(&self.reach_m) {
            return Err("Tool reach must be finite and within 0.1..=4 metres".into());
        }
        if !self.swings_per_second.is_finite() || !(0.25..=5.0).contains(&self.swings_per_second) {
            return Err("Tool swing speed must be finite and within 0.25..=5 swings/second".into());
        }
        Ok(())
    }
}
