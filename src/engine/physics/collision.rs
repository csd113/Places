//! Static world construction and model-bound collision shapes.

use glam::{Quat, Vec3};
use rapier3d::prelude::CoefficientCombineRule;
use rapier3d::prelude::{
    ActiveHooks, Collider, ColliderBuilder, ColliderHandle, MassProperties, PhysicsWorld, Pose,
    QueryFilter, Rotation, SharedShape, Vector,
};

use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::level::{WalkableCeiling, WalkableFloor};

use super::config::{CompoundPart, PhysicsDef, PrimitiveShapeDef, ShapeDef};
use super::surfaces::{FloorContactHooks, FloorSurfaces};

fn enabled_collider(_handle: ColliderHandle, collider: &Collider) -> bool {
    collider.is_enabled()
}

pub(super) fn query_filter() -> QueryFilter<'static> {
    QueryFilter::default()
        .exclude_sensors()
        .predicate(&enabled_collider)
}

/// Retain modification flags for the actual solver. The collision-only
/// pipeline consumes new-body/type-change flags without registering islands.
pub(super) fn refresh_collider(world: &mut PhysicsWorld, handle: ColliderHandle) {
    if let Some(collider) = world
        .colliders
        .get(handle)
        .filter(|collider| collider.is_enabled())
    {
        world.broad_phase.set_aabb(
            &world.integration_parameters,
            handle,
            collider.compute_aabb(),
        );
    }
}

pub(super) const fn vector(value: Vec3) -> Vector {
    Vector::from_array(value.to_array())
}

pub(super) const fn point(value: Vector) -> Vec3 {
    Vec3::from_array(value.to_array())
}

pub(super) fn pose(position: Vec3, rotation: Quat) -> Pose {
    Pose::from_parts(vector(position), Rotation::from_array(rotation.to_array()))
}

pub(super) const fn quaternion(rotation: Rotation) -> Quat {
    Quat::from_array(rotation.to_array())
}

pub(super) const fn add(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x + right.x, left.y + right.y, left.z + right.z)
}

pub(super) const fn subtract(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

pub(super) const fn scaled(value: Vec3, scale: f32) -> Vec3 {
    Vec3::new(value.x * scale, value.y * scale, value.z * scale)
}

/// Derive shape inertia at a stable numerical magnitude, then restore the
/// requested physical mass. No collision geometry or contact behavior changes.
pub(super) fn mass_properties(shape: &SharedShape, mass_kg: f32) -> Result<MassProperties, String> {
    if !mass_kg.is_finite() || !(0.001..=1000.0).contains(&mass_kg) {
        return Err("physics mass must be finite and within 0.001..=1000 kilograms".into());
    }
    let minimum = if let Some(compound) = shape.as_compound() {
        compound
            .shapes()
            .iter()
            .map(|(_, child)| minimum_unit_inertia(child))
            .try_fold(f32::INFINITY, |least_so_far, result| {
                result.map(|value| least_so_far.min(value))
            })?
    } else {
        minimum_unit_inertia(shape)?
    };
    // glamx's eigenvector fallback compares a fourth-power matrix magnitude
    // against an absolute 1e-20. Unit-density centimetre hulls fall below it,
    // selecting an unrelated inertia axis. Normalize even the smallest child
    // principal moment to one before Parry combines the mass properties.
    let density = 1.0 / minimum;
    if !density.is_finite() || density <= 0.0 {
        return Err("shape inertia cannot be normalized to a finite density".into());
    }
    let mut properties = shape.mass_properties(density);
    if !valid_mass_properties(&properties) {
        return Err("shape must have finite positive mass and inertia".into());
    }
    properties.set_mass(mass_kg, true);
    if !valid_mass_properties(&properties) {
        return Err("scaled shape inertia must remain finite and positive".into());
    }
    Ok(properties)
}

fn minimum_unit_inertia(shape: &SharedShape) -> Result<f32, String> {
    let properties = shape.mass_properties(1.0);
    let minimum = properties.principal_inertia().min_element();
    if !minimum.is_finite() || minimum <= 0.0 {
        return Err("shape must enclose finite positive rotational inertia".into());
    }
    Ok(minimum)
}

fn valid_mass_properties(properties: &MassProperties) -> bool {
    let mass = properties.mass();
    let inertia = properties.principal_inertia();
    mass.is_finite()
        && mass > 0.0
        && properties.local_com.is_finite()
        && properties.principal_inertia_local_frame.is_finite()
        && inertia.is_finite()
        && inertia.min_element() > 0.0
}

pub(super) fn model_shape(
    def: &PhysicsDef,
    bounds: (Vec3, Vec3),
    scale: f32,
) -> Result<(SharedShape, Vec3, f32), String> {
    def.validate_scale(scale)?;
    let (minimum, maximum) = bounds;
    if !minimum.is_finite() || !maximum.is_finite() {
        return Err("physics model bounds must be finite".into());
    }
    let size = scaled(subtract(maximum, minimum), scale);
    if !size.is_finite() || size.min_element() < 0.001 || size.max_element() > 100.0 {
        return Err("physics model bounds must span 0.001..=100 metres after scaling".into());
    }
    let center = scaled(add(scaled(minimum, 0.5), scaled(maximum, 0.5)), scale);
    if !center.is_finite() {
        return Err("physics model center must be finite".into());
    }
    let (shape, radius) = match def.shape.as_ref() {
        None => {
            let half = scaled(size, 0.5);
            (SharedShape::cuboid(half.x, half.y, half.z), half.length())
        }
        Some(ShapeDef::Cuboid { half_extents }) => {
            let half = scaled(Vec3::from_array(*half_extents), scale);
            (SharedShape::cuboid(half.x, half.y, half.z), half.length())
        }
        Some(ShapeDef::Sphere { radius }) => {
            let scaled_radius = *radius * scale;
            (SharedShape::ball(scaled_radius), scaled_radius)
        }
        Some(ShapeDef::Cylinder {
            half_height,
            radius,
        }) => {
            let height = *half_height * scale;
            let scaled_radius = *radius * scale;
            (
                SharedShape::cylinder(height, scaled_radius),
                height.hypot(scaled_radius),
            )
        }
        Some(ShapeDef::ConvexHull { points }) => hull_shape(points, scale)?,
        Some(ShapeDef::Compound { parts }) => compound_shape(parts, center, scale)?,
    };
    Ok((shape, center, radius))
}

fn hull_shape(points: &[[f32; 3]], scale: f32) -> Result<(SharedShape, f32), String> {
    let vertices: Vec<Vector> = points
        .iter()
        .map(|point| vector(scaled(Vec3::from_array(*point), scale)))
        .collect();
    let radius = vertices
        .iter()
        .map(|vertex| vertex.length())
        .fold(0.0_f32, f32::max);
    let hull = SharedShape::convex_hull(&vertices)
        .ok_or("convex hull points must form a nondegenerate three-dimensional solid")?;
    let mass = hull.mass_properties(1.0).mass();
    if !mass.is_finite() || mass <= 0.0 {
        return Err("scaled convex hull must enclose a finite positive volume".into());
    }
    Ok((hull, radius))
}

fn compound_shape(
    parts: &[CompoundPart],
    center: Vec3,
    scale: f32,
) -> Result<(SharedShape, f32), String> {
    let mut shapes = Vec::with_capacity(parts.len());
    let mut bounding_radius = 0.0_f32;
    for part in parts {
        let offset = subtract(scaled(Vec3::from_array(part.offset), scale), center);
        let (shape, part_radius) = match &part.shape {
            PrimitiveShapeDef::Cuboid { half_extents } => {
                let half = scaled(Vec3::from_array(*half_extents), scale);
                (SharedShape::cuboid(half.x, half.y, half.z), half.length())
            }
            PrimitiveShapeDef::Sphere { radius } => {
                let scaled_radius = *radius * scale;
                (SharedShape::ball(scaled_radius), scaled_radius)
            }
            PrimitiveShapeDef::Cylinder {
                half_height,
                radius,
            } => {
                let height = *half_height * scale;
                let scaled_radius = *radius * scale;
                (
                    SharedShape::cylinder(height, scaled_radius),
                    height.hypot(scaled_radius),
                )
            }
            PrimitiveShapeDef::ConvexHull { points } => hull_shape(points, scale)?,
        };
        bounding_radius = bounding_radius.max(offset.length() + part_radius);
        shapes.push((pose(offset, Quat::IDENTITY), shape));
    }
    // Rapier derives the compound's mass distribution and rotational inertia
    // from its primitives; the model-bounds cuboid never supplies its inertia.
    Ok((SharedShape::compound(shapes), bounding_radius))
}

pub(super) fn install_static(
    world: &mut PhysicsWorld,
    walls: &[WallAabb],
    floor: &WalkableFloor,
    ceiling: &WalkableCeiling,
    surfaces: &FloorSurfaces,
) -> FloorContactHooks {
    let mut floor_contacts = FloorContactHooks::default();
    for wall in walls {
        let size = Vec3::new(
            wall.max_x - wall.min_x,
            wall.max_y - wall.min_y,
            wall.max_z - wall.min_z,
        );
        if size.min_element() <= 0.0 || !size.is_finite() {
            continue;
        }
        let center = Vec3::new(
            f32::midpoint(wall.min_x, wall.max_x),
            f32::midpoint(wall.min_y, wall.max_y),
            f32::midpoint(wall.min_z, wall.max_z),
        );
        let half = scaled(size, 0.5);
        let handle = world.insert_collider(
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(vector(center))
                .friction(0.7)
                .restitution(0.02),
            None,
        );
        refresh_collider(world, handle);
    }
    let (floors, barriers) = floor.physical_geometry();
    for patch in floors.into_iter().chain(ceiling.physical_patches()) {
        let (friction, combine, material_tag) = if patch.ceiling {
            (0.6, CoefficientCombineRule::Average, None)
        } else {
            let material = surfaces.material(patch.corners, patch.room_floor_y);
            let friction = material.base_friction;
            (
                friction,
                CoefficientCombineRule::Multiply,
                floor_contacts.register(material),
            )
        };
        install_slab(
            world,
            patch.corners,
            patch.backing,
            friction,
            combine,
            material_tag,
        );
    }
    for [a, b, c, d] in barriers {
        for [p, q, r] in [[a, b, c], [a, c, d]] {
            if subtract(q, p).cross(subtract(r, p)).length_squared() <= 1.0e-12 {
                continue;
            }
            let handle = world.insert_collider(
                ColliderBuilder::triangle(vector(p), vector(q), vector(r))
                    .friction(0.7)
                    .restitution(0.02),
                None,
            );
            refresh_collider(world, handle);
        }
    }
    floor_contacts
}

fn install_slab(
    world: &mut PhysicsWorld,
    corners: [Vec3; 4],
    backing: f32,
    friction: f32,
    combine: CoefficientCombineRule,
    material_tag: Option<u128>,
) {
    let [a, b, c, d] = corners;
    let vertices = [
        a,
        b,
        c,
        d,
        Vec3::new(a.x, backing, a.z),
        Vec3::new(b.x, backing, b.z),
        Vec3::new(c.x, backing, c.z),
        Vec3::new(d.x, backing, d.z),
    ]
    .map(vector);
    // Each authored planar patch is a convex slab/wedge. Its exact corners
    // retain slopes, risers and recesses without importing visual triangles.
    if let Some(mut builder) = ColliderBuilder::convex_hull(&vertices) {
        if let Some(tag) = material_tag {
            builder = builder
                .user_data(tag)
                .active_hooks(ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let handle = world.insert_collider(
            builder
                .friction(friction)
                .friction_combine_rule(combine)
                .restitution(0.02),
            None,
        );
        refresh_collider(world, handle);
    }
}

pub(super) fn door_pose(door: &DoorCollider) -> Pose {
    let center = Vec3::new(
        door.dir_x.mul_add(door.width * 0.5, door.hinge_x),
        door.height.mul_add(0.5, door.hinge_y),
        door.dir_z.mul_add(door.width * 0.5, door.hinge_z),
    );
    pose(center, Quat::from_rotation_y(-door.dir_z.atan2(door.dir_x)))
}
