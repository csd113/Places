//! One direction owns both celestial artwork and baked illumination.
use super::{GlobalIlluminatorDef, GlobalIlluminatorKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CelestialVisualDef {
    /// Full fitted image diameter; independent of the source's shadow softness.
    #[serde(default = "default_diameter")]
    pub angular_size_degrees: f32,
    #[serde(default = "default_brightness")]
    pub brightness: f32,
}
const fn default_diameter() -> f32 {
    6.0
}
const fn default_brightness() -> f32 {
    1.0
}
impl CelestialVisualDef {
    /// # Errors
    /// Rejects singular projections and unbounded display energy.
    pub fn validate(&self) -> Result<(), String> {
        if !self.angular_size_degrees.is_finite()
            || !(0.1..=30.0).contains(&self.angular_size_degrees)
            || !self.brightness.is_finite()
            || !(0.0..=4.0).contains(&self.brightness)
        {
            return Err(
                "Celestial visual needs diameter 0.1..=30 degrees and brightness 0..=4".into(),
            );
        }
        Ok(())
    }
}
impl GlobalIlluminatorDef {
    /// Direction towards the source, also used by the lighting bake. f64 avoids
    /// overflow for otherwise valid, very large finite authored directions.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Negating widened finite f32 components cannot overflow f64"
    )]
    pub fn source_direction(&self) -> [f32; 3] {
        (-glam::DVec3::from_array(self.direction.map(f64::from)))
            .normalize_or_zero()
            .as_vec3()
            .to_array()
    }
    /// Bearing clockwise from -Z, and elevation above the horizon, in degrees.
    #[must_use]
    pub fn celestial_angles(&self) -> [f32; 2] {
        let [x, y, z] = self.source_direction();
        [
            x.atan2(-z).to_degrees().rem_euclid(360.0),
            y.clamp(-1.0, 1.0).asin().to_degrees(),
        ]
    }
    /// # Errors
    /// Rejects nonfinite angles or elevations outside the sky sphere.
    pub fn direction_from_angles(azimuth: f32, elevation: f32) -> Result<[f32; 3], String> {
        if !azimuth.is_finite()
            || !(0.0..=360.0).contains(&azimuth)
            || !elevation.is_finite()
            || !(-90.0..=90.0).contains(&elevation)
        {
            return Err("Celestial bearing needs 0..=360 degrees; elevation needs -90..=90".into());
        }
        let (s, c) = azimuth.to_radians().sin_cos();
        let (e, h) = elevation.to_radians().sin_cos();
        Ok([-s * h, -e, c * h])
    }
    /// Catalog identity of the fitted file image; old sources have no visual.
    #[must_use]
    pub fn visual_texture(&self) -> Option<&'static str> {
        let _visual = self.visual.as_ref()?;
        match self.kind {
            GlobalIlluminatorKind::Sun => Some("core:celestial_sun"),
            GlobalIlluminatorKind::Moon => Some("core:celestial_moon"),
            GlobalIlluminatorKind::Directional => None,
        }
    }
}
