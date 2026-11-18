//! Authored presentation and atmosphere. These controls never change stored light.

use serde::{Deserialize, Serialize};

/// Stable presentation shared by every quality preset and every draw family.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresentationDef {
    /// Linear multiplier; fixed for the level, without automatic adaptation.
    pub exposure: f32,
    /// Linear scene value where the highlight shoulder starts.
    pub tone_knee: f32,
    /// Display-space saturation and contrast, both centred on identity.
    pub saturation: f32,
    pub contrast: f32,
}

impl Default for PresentationDef {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            tone_knee: 0.75,
            saturation: 1.03,
            contrast: 1.02,
        }
    }
}

/// Squared-exponential distance haze with a bounded low-height density gain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AtmosphereDef {
    /// Display sRGB fog colour; decoded once before mixing with HDR surfaces.
    pub color: [f32; 3],
    pub density: f32,
    pub reference_y: f32,
    pub height_gain: f32,
}

impl Default for AtmosphereDef {
    fn default() -> Self {
        Self {
            color: [0.60, 0.63, 0.68],
            density: 0.0095,
            reference_y: 2.0,
            height_gain: 0.045,
        }
    }
}

/// Optional level controls. Omission retains established High presentation and fog.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnvironmentDef {
    pub presentation: PresentationDef,
    pub fog: AtmosphereDef,
}

impl EnvironmentDef {
    /// Reject malformed authoring before it reaches GPU uniforms.
    /// # Errors
    /// Returns a named error for an out-of-range or non-finite field.
    pub fn validate(self) -> Result<(), String> {
        let presentation = self.presentation;
        for (name, value, min, max) in [
            ("exposure", presentation.exposure, 0.125, 8.0),
            ("tone_knee", presentation.tone_knee, 0.25, 0.95),
            ("saturation", presentation.saturation, 0.8, 1.2),
            ("contrast", presentation.contrast, 0.8, 1.2),
            ("fog density", self.fog.density, 0.0, 0.5),
            ("fog reference_y", self.fog.reference_y, -10_000.0, 10_000.0),
            ("fog height_gain", self.fog.height_gain, 0.0, 1.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!(
                    "environment {name} must be finite in {min}..={max}"
                ));
            }
        }
        if self
            .fog
            .color
            .iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
        {
            return Err("environment fog color must have finite sRGB channels in 0..=1".into());
        }
        Ok(())
    }
}
