//! Bounded authored rigid-body settings shared by placements and catalog items.

use std::fmt;

use rapier3d::prelude::{SharedShape, Vector};
use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// Smallest accepted collider length in local or scaled metres.
pub const MIN_SHAPE_LENGTH_M: f32 = 0.001;
/// Largest accepted collider length in local or scaled metres.
pub const MAX_SHAPE_LENGTH_M: f32 = 100.0;
/// Largest absolute authored coordinate of a rigid item, in metres.
pub const MAX_ITEM_POSITION_M: f32 = 100_000.0;
/// Maximum primitive parts in an authored compound collider.
pub const MAX_COMPOUND_PARTS: usize = 8;
/// Maximum vertices accepted by an authored convex primitive.
pub const MAX_HULL_POINTS: usize = 96;
/// Maximum byte length of a reusable reset-group identifier.
pub const MAX_RESET_GROUP_BYTES: usize = 64;

/// Smallest authored or placement-scaled displaced volume in cubic metres.
pub const MIN_DISPLACED_VOLUME_M3: f32 = 1.0e-9;
/// Largest authored or placement-scaled displaced volume in cubic metres.
pub const MAX_DISPLACED_VOLUME_M3: f32 = 1000.0;
/// Supported floats have density at least 10 kg/m³ (at most 100 times water lift).
pub const MAX_DISPLACED_VOLUME_PER_KG: f32 = 0.1;

/// Material-independent response to the level's enabled swimming water.
///
/// Mass remains in `PhysicsDef::mass_kg`; an explicit displaced volume can
/// describe a hollow float or open metal basket independently of its collider.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WaterPhysicsDef {
    /// Full-submersion volume in source cubic metres, multiplied by scale cubed.
    /// Omission uses the actual collision shape's solid volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displaced_volume_m3: Option<f32>,
    /// Linear decay per second per displaced-water-mass/body-mass ratio.
    pub linear_drag: f32,
    /// Angular decay per second per displaced-water-mass/body-mass ratio.
    pub angular_drag: f32,
}

impl Default for WaterPhysicsDef {
    fn default() -> Self {
        Self {
            displaced_volume_m3: None,
            linear_drag: 3.0,
            angular_drag: 2.0,
        }
    }
}

impl WaterPhysicsDef {
    /// Checks finite bounded volume and drag coefficients.
    ///
    /// # Errors
    /// Returns a field-specific error for a nonfinite or out-of-range value.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(volume) = self.displaced_volume_m3 {
            validate_displaced_volume(volume)?;
        }
        for (name, value) in [
            ("linear_drag", self.linear_drag),
            ("angular_drag", self.angular_drag),
        ] {
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(format!(
                    "physics water {name} must be finite and between 0 and 100"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn scaled_volume(&self, scale: f32) -> Result<Option<f32>, String> {
        self.displaced_volume_m3
            .map(|volume| {
                let scaled_volume = volume * scale * scale * scale;
                validate_displaced_volume(scaled_volume)?;
                Ok(scaled_volume)
            })
            .transpose()
    }
}

pub(super) fn validate_volume_mass(volume: f32, mass_kg: f32) -> Result<(), String> {
    if volume > mass_kg * MAX_DISPLACED_VOLUME_PER_KG {
        return Err(
            "physics water displaced volume must not exceed 0.1 cubic metres per kilogram before and after scale".into(),
        );
    }
    Ok(())
}

fn validate_displaced_volume(volume: f32) -> Result<(), String> {
    if !volume.is_finite() || !(MIN_DISPLACED_VOLUME_M3..=MAX_DISPLACED_VOLUME_M3).contains(&volume)
    {
        return Err(
            "physics water displaced_volume_m3 must be finite and between 0.000000001 and 1000 before and after scale".into(),
        );
    }
    Ok(())
}

/// How the simulator updates the body's pose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyMotion {
    /// Immutable scenery, preserving the existing placement behavior.
    #[default]
    Fixed,
    /// A body driven by gravity, contacts and impulses.
    Dynamic,
    /// A body whose pose is controlled externally rather than by forces.
    Kinematic,
}

impl BodyMotion {
    /// Whether a placement must stay out of static geometry and lighting.
    #[must_use]
    pub const fn is_movable(self) -> bool {
        matches!(self, Self::Dynamic | Self::Kinematic)
    }
}

/// Economical collider dimensions in the model's local metre frame.
///
/// Primitive colliders are centred on model bounds; compound offsets instead
/// use the original model origin. All receive the placement's uniform scale.
/// Omitting the shape uses a cuboid from those bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeDef {
    /// Box half lengths along local X, Y and Z.
    Cuboid { half_extents: [f32; 3] },
    /// Sphere radius.
    Sphere { radius: f32 },
    /// Cylinder along local Y with its height divided by two.
    Cylinder { half_height: f32, radius: f32 },
    /// A solid convex envelope from four to 96 local-frame vertices.
    ConvexHull {
        #[serde(deserialize_with = "deserialize_hull_points")]
        points: Vec<[f32; 3]>,
    },
    /// At most eight primitive parts positioned in the model-origin frame.
    Compound {
        #[serde(deserialize_with = "deserialize_compound_parts")]
        parts: Vec<CompoundPart>,
    },
}

/// A compound part's bounded primitive; recursive compounds are unsupported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrimitiveShapeDef {
    Cuboid {
        half_extents: [f32; 3],
    },
    Sphere {
        radius: f32,
    },
    Cylinder {
        half_height: f32,
        radius: f32,
    },
    ConvexHull {
        #[serde(deserialize_with = "deserialize_hull_points")]
        points: Vec<[f32; 3]>,
    },
}

/// A primitive offset in local model-origin metres, uniformly placement-scaled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompoundPart {
    pub offset: [f32; 3],
    pub shape: PrimitiveShapeDef,
}

impl PrimitiveShapeDef {
    pub(super) fn validate_scale(&self, scale: f32) -> Result<(), String> {
        let dimensions = match self {
            Self::Cuboid { half_extents } => *half_extents,
            Self::Sphere { radius } => [*radius; 3],
            Self::Cylinder {
                half_height,
                radius,
            } => [*radius, *half_height, *radius],
            Self::ConvexHull { points } => return validate_hull_points(points, scale),
        };
        validate_dimensions(dimensions, scale)
    }
}

fn deserialize_hull_points<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<[f32; 3]>, D::Error> {
    struct PointsVisitor;
    impl<'de> Visitor<'de> for PointsVisitor {
        type Value = Vec<[f32; 3]>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "between four and {MAX_HULL_POINTS} hull points")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut points = Vec::with_capacity(MAX_HULL_POINTS);
            while let Some(point) = sequence.next_element()? {
                if points.len() == MAX_HULL_POINTS {
                    return Err(serde::de::Error::custom(
                        "convex hull has more than 96 points",
                    ));
                }
                points.push(point);
            }
            if points.len() < 4 {
                return Err(serde::de::Error::custom(
                    "convex hull requires at least four points",
                ));
            }
            Ok(points)
        }
    }
    deserializer.deserialize_seq(PointsVisitor)
}

fn validate_hull_points(points: &[[f32; 3]], scale: f32) -> Result<(), String> {
    if !(4..=MAX_HULL_POINTS).contains(&points.len()) {
        return Err("convex hull requires between four and 96 points".into());
    }
    let mut minimum = Vector::splat(f32::INFINITY);
    let mut maximum = Vector::splat(f32::NEG_INFINITY);
    let mut vertices = Vec::with_capacity(points.len());
    for point in points {
        if point.iter().any(|value| {
            !value.is_finite()
                || value.abs() > MAX_SHAPE_LENGTH_M
                || !(*value * scale).is_finite()
                || (*value * scale).abs() > MAX_SHAPE_LENGTH_M
        }) {
            return Err(
                "convex hull points must be finite and within 100 metres before and after scale"
                    .into(),
            );
        }
        let vertex = Vector::from_array(*point);
        minimum = minimum.min(vertex);
        maximum = maximum.max(vertex);
        vertices.push(vertex);
    }
    validate_dimensions(
        [
            maximum.x - minimum.x,
            maximum.y - minimum.y,
            maximum.z - minimum.z,
        ],
        scale,
    )?;
    // Work around the local centre for stable volume measurement even when
    // an author gives a small shape a large local offset.
    let centre = Vector::new(
        f32::midpoint(minimum.x, maximum.x),
        f32::midpoint(minimum.y, maximum.y),
        f32::midpoint(minimum.z, maximum.z),
    );
    for vertex in &mut vertices {
        *vertex = Vector::new(
            vertex.x - centre.x,
            vertex.y - centre.y,
            vertex.z - centre.z,
        );
    }
    let hull = SharedShape::convex_hull(&vertices)
        .ok_or("convex hull points must form a nondegenerate three-dimensional solid")?;
    let mass = hull.mass_properties(1.0).mass();
    if !mass.is_finite() || mass <= 0.0 {
        return Err("convex hull points must enclose a finite positive volume".into());
    }
    Ok(())
}

fn deserialize_compound_parts<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CompoundPart>, D::Error> {
    struct PartsVisitor;
    impl<'de> Visitor<'de> for PartsVisitor {
        type Value = Vec<CompoundPart>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "between one and {MAX_COMPOUND_PARTS} primitive parts"
            )
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut parts = Vec::with_capacity(MAX_COMPOUND_PARTS);
            while let Some(part) = sequence.next_element()? {
                if parts.len() == MAX_COMPOUND_PARTS {
                    return Err(serde::de::Error::custom(
                        "compound has more than eight parts",
                    ));
                }
                parts.push(part);
            }
            if parts.is_empty() {
                return Err(serde::de::Error::custom(
                    "compound requires at least one part",
                ));
            }
            Ok(parts)
        }
    }
    deserializer.deserialize_seq(PartsVisitor)
}

impl ShapeDef {
    /// Checks local dimensions and dimensions after a uniform scale.
    ///
    /// # Errors
    /// Returns an error if a length is non-finite or outside the item bounds.
    pub fn validate_scale(&self, scale: f32) -> Result<(), String> {
        let dimensions = match self {
            Self::Cuboid { half_extents } => *half_extents,
            Self::Sphere { radius } => [*radius; 3],
            Self::Cylinder {
                half_height,
                radius,
            } => [*radius, *half_height, *radius],
            Self::ConvexHull { points } => return validate_hull_points(points, scale),
            Self::Compound { parts } => {
                if parts.is_empty() || parts.len() > MAX_COMPOUND_PARTS {
                    return Err("compound requires between one and eight primitive parts".into());
                }
                for part in parts {
                    part.shape.validate_scale(scale)?;
                    if part.offset.iter().any(|value| {
                        !value.is_finite()
                            || value.abs() > MAX_SHAPE_LENGTH_M
                            || (*value * scale).abs() > MAX_SHAPE_LENGTH_M
                    }) {
                        return Err("compound offsets must be finite and within 100 metres before and after scale".into());
                    }
                }
                return Ok(());
            }
        };
        validate_dimensions(dimensions, scale)
    }
}

/// A reusable physics profile. An absent placement profile keeps static behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsDef {
    /// Fixed, simulated or externally controlled motion.
    pub motion: BodyMotion,
    /// Explicit collider, or a cuboid resolved from model bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<ShapeDef>,
    /// Total body mass in kilograms; finite `0.001..=1000`.
    pub mass_kg: f32,
    /// Contact friction coefficient; finite `0..=2`.
    pub friction: f32,
    /// Contact bounce coefficient; finite `0..=1`.
    pub restitution: f32,
    /// Linear velocity damping; finite `0..=100`.
    pub linear_damping: f32,
    /// Angular velocity damping; finite `0..=100`.
    pub angular_damping: f32,
    /// Whether the player can pick up this dynamic body.
    pub pickup: bool,
    /// Optional group restored atomically by an authored reset action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_group: Option<String>,
    /// Optional volume/drag override; dynamic bodies otherwise use solid volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterPhysicsDef>,
}

impl Default for PhysicsDef {
    fn default() -> Self {
        Self {
            motion: BodyMotion::Fixed,
            shape: None,
            mass_kg: 1.0,
            friction: 0.6,
            restitution: 0.05,
            linear_damping: 0.1,
            angular_damping: 0.2,
            pickup: false,
            reset_group: None,
            water: None,
        }
    }
}

impl PhysicsDef {
    /// Checks all authored coefficients and local shape dimensions.
    ///
    /// # Errors
    /// Returns a field-specific error for invalid coefficients, dimensions or
    /// pickup on a body that is not dynamic.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, minimum, maximum) in [
            ("mass_kg", self.mass_kg, 0.001, 1000.0),
            ("friction", self.friction, 0.0, 2.0),
            ("restitution", self.restitution, 0.0, 1.0),
            ("linear_damping", self.linear_damping, 0.0, 100.0),
            ("angular_damping", self.angular_damping, 0.0, 100.0),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(format!(
                    "physics {name} must be finite and between {minimum} and {maximum}"
                ));
            }
        }
        if self.pickup && self.motion != BodyMotion::Dynamic {
            return Err("physics pickup requires dynamic motion".to_string());
        }
        if let Some(water) = self.water {
            if self.motion != BodyMotion::Dynamic {
                return Err("physics water requires dynamic motion".into());
            }
            water.validate()?;
            if let Some(volume) = water.displaced_volume_m3 {
                validate_volume_mass(volume, self.mass_kg)?;
            }
        }
        if let Some(group) = self.reset_group.as_deref() {
            validate_reset_group(group)?;
            if !self.motion.is_movable() {
                return Err("physics reset_group requires movable motion".into());
            }
        }
        if let Some(shape) = self.shape.as_ref() {
            shape.validate_scale(1.0)?;
        }
        Ok(())
    }

    /// Checks bounded placement scale and scaled explicit collider dimensions.
    ///
    /// # Errors
    /// Returns an error if the scale or scaled collider is outside the bounds.
    pub fn validate_scale(&self, scale: f32) -> Result<(), String> {
        if !bounded_length(scale) {
            return Err(format!(
                "physics scale must be finite and between {MIN_SHAPE_LENGTH_M} and \
                 {MAX_SHAPE_LENGTH_M}"
            ));
        }
        if let Some(shape) = self.shape.as_ref() {
            shape.validate_scale(scale)?;
        }
        if let Some(water) = self.water
            && let Some(volume) = water.scaled_volume(scale)?
        {
            validate_volume_mass(volume, self.mass_kg)?;
        }
        Ok(())
    }
}

/// Checks a short, portable group name shared by profiles and reset actions.
///
/// # Errors
/// Rejects empty/overlong names and characters other than ASCII letters,
/// digits, underscores and hyphens.
pub fn validate_reset_group(group: &str) -> Result<(), String> {
    if group.is_empty()
        || group.len() > MAX_RESET_GROUP_BYTES
        || !group
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "physics reset_group must contain 1..=64 ASCII letters, digits, underscores or hyphens"
                .into(),
        );
    }
    Ok(())
}

fn validate_dimensions(dimensions: [f32; 3], scale: f32) -> Result<(), String> {
    if dimensions
        .iter()
        .any(|length| !bounded_length(*length) || !bounded_length(*length * scale))
    {
        return Err(format!(
            "shape dimensions before and after scale must be finite and between \
             {MIN_SHAPE_LENGTH_M} and {MAX_SHAPE_LENGTH_M} metres"
        ));
    }
    Ok(())
}

fn bounded_length(value: f32) -> bool {
    value.is_finite() && (MIN_SHAPE_LENGTH_M..=MAX_SHAPE_LENGTH_M).contains(&value)
}
