//! Reusable, presentation-only weather configuration. Omission costs nothing.
use serde::{Deserialize, Serialize};

pub const MAX_SNOW_PARTICLES: usize = 2048;
pub const MAX_STORM_SHELTERS: usize = 32;
pub const DEFAULT_SNOW_MATERIAL: &str = "core:snowflake_01";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeatherDef {
    Snow(SnowfallDef),
}

#[must_use]
pub fn shelter_count(level: &crate::level::LevelDef) -> usize {
    level
        .rooms
        .iter()
        .filter(|room| !room.ceiling.is_open())
        .count()
        .saturating_add(level.void_walls.iter().filter(|wall| wall.occludes).count())
}

impl WeatherDef {
    #[must_use]
    pub const fn snowfall(&self) -> &SnowfallDef {
        match self {
            Self::Snow(snow) => snow,
        }
    }
}

/// Light snow in a camera-centred volume; all distances are world metres.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnowfallDef {
    pub count: u32,
    pub radius: f32,
    pub height: f32,
    /// Horizontal velocity along world X/Z, metres per second.
    pub wind: [f32; 2],
    pub size: [f32; 2],
    pub speed: [f32; 2],
    pub opacity: f32,
    pub material: String,
    /// Stable seed fraction, independent of storm extinction.
    pub intensity: f32,
    /// 0 preserves calm rendering; 1 enables severe whiteout.
    pub storm_severity: f32,
    /// Outdoor sightline at which a full storm removes 98.2% of contrast.
    pub visibility_m: f32,
    pub fog_color: [f32; 3],
}

impl Default for SnowfallDef {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            storm_severity: 0.0,
            visibility_m: 5.0,
            fog_color: [0.68, 0.73, 0.79],
            count: 1400,
            radius: 16.0,
            height: 12.0,
            wind: [0.18, 0.06],
            size: [0.025, 0.075],
            speed: [0.45, 1.05],
            opacity: 0.85,
            material: DEFAULT_SNOW_MATERIAL.to_string(),
        }
    }
}

impl SnowfallDef {
    /// A compact whiteout preset using the same seed budget as calm snow.
    #[must_use]
    pub fn blizzard() -> Self {
        Self {
            radius: 6.0,
            height: 6.0,
            wind: [8.0, 3.0],
            size: [0.025, 0.06],
            speed: [0.8, 1.8],
            storm_severity: 1.0,
            ..Self::default()
        }
    }

    /// Rejects unbounded or malformed authoring before building resources.
    ///
    /// # Errors
    /// Returns a named weather field error for an invalid parameter.
    pub fn validate(&self) -> Result<(), String> {
        if self.count == 0 || usize::try_from(self.count).unwrap_or(usize::MAX) > MAX_SNOW_PARTICLES
        {
            return Err(format!("weather count must be 1..={MAX_SNOW_PARTICLES}"));
        }
        for (name, value, min, max) in [
            ("radius", self.radius, 4.0, 32.0),
            ("height", self.height, 4.0, 24.0),
            ("opacity", self.opacity, 0.0, 1.0),
            ("intensity", self.intensity, 0.0, 1.0),
            ("storm_severity", self.storm_severity, 0.0, 1.0),
            ("visibility_m", self.visibility_m, 2.0, 100.0),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("weather {name} must be finite and {min}..={max}"));
            }
        }
        for (name, [min, max], low, high) in [
            ("size", self.size, 0.005, 0.15),
            ("speed", self.speed, 0.1, 2.0),
        ] {
            if !min.is_finite() || !max.is_finite() || min < low || max > high || min > max {
                return Err(format!(
                    "weather {name} must be an ordered range in {low}..={high}"
                ));
            }
        }
        let wind_limit = if self.storm_severity > 0.0 { 20.0 } else { 0.5 };
        if self
            .wind
            .iter()
            .any(|v| !v.is_finite() || v.abs() > wind_limit)
        {
            return Err(format!(
                "weather wind components must be finite and within ±{wind_limit}"
            ));
        }
        if self
            .fog_color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("weather fog_color must contain three finite 0..=1 components".to_string());
        }
        if !crate::assets::is_valid_asset_id(self.material.trim()) {
            return Err("weather material must be a logical asset id".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_opt_in_and_round_trip_in_semantics() -> Result<(), Box<dyn std::error::Error>> {
        let mut level: crate::level::LevelDef =
            serde_json::from_str(include_str!("../assets/levels/winter.json"))?;
        let snow = level
            .weather
            .as_ref()
            .ok_or("winter needs snow")?
            .snowfall();
        assert_eq!(snow.count, 1400, "light default budget");
        snow.validate()?;
        assert!(
            crate::materials::referenced_material_ids(&level)
                .iter()
                .any(|id| id == DEFAULT_SNOW_MATERIAL),
            "compiler must package the snow PNG dependency"
        );
        let encoded = serde_json::to_string(&level)?;
        let decoded: crate::level::LevelDef = serde_json::from_str(&encoded)?;
        assert_eq!(
            decoded
                .weather
                .as_ref()
                .ok_or("lost weather")?
                .snowfall()
                .count,
            snow.count,
            "package semantics retain weather"
        );
        level.weather = None;
        assert!(
            !serde_json::to_value(&level)?
                .as_object()
                .ok_or("not object")?
                .contains_key("weather"),
            "existing packages retain absent weather"
        );
        let demo: crate::level::LevelDef =
            serde_json::from_str(include_str!("../assets/levels/places_demo.json"))?;
        assert!(
            demo.weather.is_none(),
            "non-Winter defaults remain unchanged"
        );
        Ok(())
    }

    #[test]
    fn invalid_configuration_is_rejected_by_loader() -> Result<(), Box<dyn std::error::Error>> {
        for patch in [
            serde_json::from_str::<serde_json::Value>(r#"{"count":0}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"count":2049}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"radius":100}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"height":0}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"wind":[0.6,0]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"size":[0.1,0.01]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"speed":[0.1,2.1]}"#)?,
            serde_json::from_str::<serde_json::Value>(r#"{"opacity":1.1}"#)?,
            serde_json::json!({"material":"../broken"}),
            serde_json::json!({"intensity":1.1_f64}),
            serde_json::json!({"storm_severity":-1.0_f64}),
            serde_json::json!({"visibility_m":1.9_f64}),
            serde_json::json!({"fog_color":[0.5_f64,0.5_f64,2.0_f64]}),
            serde_json::json!({"storm_severity":1.0_f64,"wind":[20.1_f64,0.0_f64]}),
        ] {
            let mut value = serde_json::json!({"kind":"snow"});
            value
                .as_object_mut()
                .ok_or("not object")?
                .extend(patch.as_object().ok_or("not patch")?.clone());
            let mut level: crate::level::LevelDef =
                serde_json::from_str(include_str!("../assets/levels/winter.json"))?;
            level.weather = Some(serde_json::from_value(value)?);
            assert!(
                crate::loader::validate_level(&level).is_err(),
                "invalid weather must fail before resource allocation: {patch}"
            );
        }
        assert!(
            serde_json::from_value::<WeatherDef>(serde_json::json!({"kind":"rain"})).is_err(),
            "unknown weather kind cannot silently disappear"
        );
        assert!(
            serde_json::from_value::<WeatherDef>(serde_json::from_str::<serde_json::Value>(
                r#"{"kind":"snow","cout":12}"#
            )?)
            .is_err(),
            "weather typo cannot silently take a default"
        );
        let mut config = SnowfallDef {
            radius: f32::NAN,
            ..SnowfallDef::default()
        };
        assert!(config.validate().is_err(), "non-finite dimension rejected");
        config.radius = 16.0;
        config.speed = [0.5, f32::INFINITY];
        assert!(config.validate().is_err(), "non-finite speed rejected");
        Ok(())
    }
}
