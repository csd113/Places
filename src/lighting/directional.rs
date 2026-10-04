//! Static map-wide sources. No position, range or distance attenuation.
use crate::level::GlobalIlluminatorDef;
use crate::package::binary::{Reader, Writer};

/// Normalized and validated source shared by the vertex and transport bakes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    /// Direction from every receiver towards the source.
    pub incoming: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub cast_shadows: bool,
    /// Half angular diameter, radians.
    pub angular_radius: f32,
}

#[cfg(test)]
mod tests;

impl DirectionalLight {
    #[must_use]
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "normalized f64 components are finite in [-1, 1]; f32 is the stored representation"
    )] // normalized f64 components are finite in [-1, 1]; f32 is the stored representation
    pub fn from_definition(def: &GlobalIlluminatorDef) -> Option<Self> {
        if def.validate().is_err() || !def.enabled || !def.bake || def.intensity == 0.0 {
            return None;
        }
        // f64 normalization accepts every finite f32 direction without overflow.
        let length = def
            .direction
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        Some(Self {
            incoming: def.direction.map(|v| (-f64::from(v) / length) as f32),
            color: def.color,
            intensity: def.intensity,
            cast_shadows: def.cast_shadows,
            angular_radius: 0.5 * def.angular_size_degrees.to_radians(),
        })
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        let norm = self.incoming.iter().map(|v| v * v).sum::<f32>();
        self.incoming.iter().all(|v| v.is_finite())
            && (norm - 1.0).abs() < 1.0e-4
            && self
                .color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.intensity.is_finite()
            && (0.0..=8.0).contains(&self.intensity)
            && self.angular_radius.is_finite()
            && (0.0..=5.0_f32.to_radians()).contains(&self.angular_radius)
    }

    /// Equal-area deterministic disk samples, independent of receiver/chart id.
    #[must_use]
    pub fn sample_direction(&self, sample: u16, count: u16) -> [f32; 3] {
        if count <= 1 || self.angular_radius == 0.0 {
            return self.incoming;
        }
        let axis = glam::Vec3::from_array(self.incoming);
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);
        let radius =
            ((f32::from(sample) + 0.5) / f32::from(count)).sqrt() * self.angular_radius.tan();
        let angle = f32::from(sample) * 2.399_963_1;
        let (sin, cos) = angle.sin_cos();
        // Unit vectors and a bounded angular radius cannot overflow vector arithmetic.
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "Unit vectors and a bounded angular radius cannot overflow vector arithmetic."
        )]
        let direction = axis + radius * (u * cos + v * sin);
        direction.normalize_or_zero().to_array()
    }

    pub(crate) fn write_compiled(self, writer: &mut Writer) {
        for value in self.incoming.into_iter().chain(self.color) {
            writer.f32(value);
        }
        writer.f32(self.intensity);
        writer.bool(self.cast_shadows);
        writer.f32(self.angular_radius);
    }

    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let light = Self {
            incoming: reader.f32_3()?,
            color: reader.f32_3()?,
            intensity: reader.f32()?,
            cast_shadows: reader.bool()?,
            angular_radius: reader.f32()?,
        };
        if !light.is_valid() {
            return Err("invalid compiled directional illuminator".to_string());
        }
        Ok(light)
    }
}
