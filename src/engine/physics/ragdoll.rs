//! Articulated skeletal ownership in the ordinary item solver, never a second world.

use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, GenericJointBuilder, Group, InteractionGroups, InteractionTestMode,
    JointAxesMask, JointAxis, RigidBodyBuilder, RigidBodyHandle, SpringCoefficients,
};
use serde::Serialize;

use super::collision::{add, point, pose, quaternion, scaled, subtract, vector};
use super::world::{ItemWorld, Runtime};
use crate::engine::gltf::PropSkin;

pub const MAX_RAGDOLLS: usize = 10;

/// Immutable current/previous displayed poses; captured only at handoff.
pub struct RagdollInput<'a> {
    pub skin: &'a PropSkin,
    pub globals: &'a [Mat4],
    pub previous: &'a [Mat4],
    pub placement: Mat4,
    pub previous_placement: Mat4,
    pub delta: f32,
}

struct Segment {
    node: usize,
    parent: Option<usize>,
    radius: f32,
    length: f32,
    mass: f32,
    limits: [f32; 3],
}

/// Explicit rig contract. Unknown skeletons are refused before any allocation.
struct Profile {
    segments: Vec<Segment>,
    order: Vec<usize>,
}

type SegmentSpec = (&'static str, Option<&'static str>, f32, f32, f32, [f32; 3]);
const SEGMENT_SPECS: [SegmentSpec; 15] = [
    ("pelvis", None, 0.12, 0.20, 10.0, [0.4, 0.3, 0.3]),
    (
        "spine",
        Some("pelvis"),
        0.13,
        0.42,
        22.0,
        [0.65, 0.45, 0.45],
    ),
    ("neck", Some("spine"), 0.095, 0.34, 5.0, [0.7, 0.8, 0.65]),
    (
        "upper_arm.L",
        Some("spine"),
        0.052,
        0.264,
        2.5,
        [2.5, 1.3, 2.5],
    ),
    (
        "forearm.L",
        Some("upper_arm.L"),
        0.044,
        0.239,
        1.5,
        [2.5, 0.18, 0.18],
    ),
    (
        "hand.L",
        Some("forearm.L"),
        0.035,
        0.15,
        0.5,
        [0.8, 0.45, 0.5],
    ),
    (
        "upper_arm.R",
        Some("spine"),
        0.052,
        0.264,
        2.5,
        [2.5, 1.3, 2.5],
    ),
    (
        "forearm.R",
        Some("upper_arm.R"),
        0.044,
        0.239,
        1.5,
        [2.5, 0.18, 0.18],
    ),
    (
        "hand.R",
        Some("forearm.R"),
        0.035,
        0.15,
        0.5,
        [0.8, 0.45, 0.5],
    ),
    (
        "thigh.L",
        Some("pelvis"),
        0.070,
        0.385,
        7.0,
        [1.6, 0.6, 0.8],
    ),
    (
        "shin.L",
        Some("thigh.L"),
        0.055,
        0.398,
        3.0,
        [2.4, 0.08, 0.08],
    ),
    (
        "foot.L",
        Some("shin.L"),
        0.045,
        0.18,
        1.0,
        [0.65, 0.25, 0.3],
    ),
    (
        "thigh.R",
        Some("pelvis"),
        0.070,
        0.385,
        7.0,
        [1.6, 0.6, 0.8],
    ),
    (
        "shin.R",
        Some("thigh.R"),
        0.055,
        0.398,
        3.0,
        [2.4, 0.08, 0.08],
    ),
    (
        "foot.R",
        Some("shin.R"),
        0.045,
        0.18,
        1.0,
        [0.65, 0.25, 0.3],
    ),
];

impl Profile {
    fn mannequin(skin: &PropSkin) -> Result<Self, String> {
        let mut segments: Vec<Segment> = Vec::new();
        for (name, parent, radius, length, mass, limits) in SEGMENT_SPECS {
            let matches: Vec<_> = skin
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.name == name)
                .map(|(index, _)| index)
                .collect();
            let [node] = matches.as_slice() else {
                return Err(format!("ragdoll requires a unique mannequin bone '{name}'"));
            };
            let profile_node = skin.nodes.get(*node).ok_or("missing profile node")?;
            let reference_length = match name {
                "spine" => Some(0.1212),
                "neck" => Some(0.4195),
                "upper_arm.L" | "upper_arm.R" => Some(0.4192),
                "forearm.L" | "forearm.R" => Some(0.2636),
                "hand.L" | "hand.R" => Some(0.2388),
                "thigh.L" | "thigh.R" => Some(0.0984),
                "shin.L" | "shin.R" => Some(0.3852),
                "foot.L" | "foot.R" => Some(0.3981),
                _ => None,
            };
            if reference_length.is_some_and(|reference| {
                (Vec3::from_array(profile_node.translation).length() - reference).abs() > 0.015
            }) {
                return Err(format!("unsupported mannequin proportions at '{name}'"));
            }
            if !skin.joints.iter().any(|joint| usize::from(*joint) == *node) {
                return Err(format!("ragdoll bone '{name}' is not a skin joint"));
            }
            let parent_slot = parent.and_then(|id| {
                segments.iter().position(|segment| {
                    skin.nodes
                        .get(segment.node)
                        .is_some_and(|joint| joint.name == id)
                })
            });
            if let Some(slot) = parent_slot {
                let expected = segments.get(slot).ok_or("missing ragdoll parent")?.node;
                if skin
                    .nodes
                    .get(*node)
                    .and_then(|joint| joint.parent)
                    .map(usize::from)
                    != Some(expected)
                {
                    return Err(format!("unsupported mannequin hierarchy at '{name}'"));
                }
            }
            segments.push(Segment {
                node: *node,
                parent: parent_slot,
                radius,
                length,
                mass,
                limits,
            });
        }
        let mut order = Vec::with_capacity(skin.nodes.len());
        for _ in 0..skin.nodes.len() {
            for (index, node) in skin.nodes.iter().enumerate() {
                if !order.contains(&index)
                    && node
                        .parent
                        .is_none_or(|parent| order.contains(&usize::from(parent)))
                {
                    order.push(index);
                }
            }
        }
        if order.len() != skin.nodes.len() {
            return Err("cyclic ragdoll skeleton".into());
        }
        Ok(Self { segments, order })
    }
}

pub(super) struct BoneBody {
    node: usize,
    pub body: RigidBodyHandle,
    previous: Mat4,
    current: Mat4,
    scale: Vec3,
}

pub(super) struct Ragdoll {
    pub bodies: Vec<BoneBody>,
    slot: u32,
    parents: Vec<Option<u16>>,
    locals: Vec<Mat4>,
    order: Vec<usize>,
    placement_inverse: Mat4,
}

#[derive(Serialize)]
pub struct RagdollSnapshot {
    pub id: String,
    pub bodies: usize,
    pub joints: usize,
    pub sleeping: usize,
    pub max_anchor_error_m: f32,
    pub max_speed_mps: f32,
    pub max_angular_speed_rps: f32,
    pub min_quiet_seconds: f32,
    pub max_penetration_m: f32,
    pub world_positions: Vec<[f32; 3]>,
}

fn validate_pose(input: &RagdollInput<'_>, velocity: Vec3) -> Result<Vec3, String> {
    if input.globals.len() != input.skin.nodes.len()
        || input.previous.len() != input.globals.len()
        || !velocity.is_finite()
        || velocity.length() > 15.0
        || !input.placement.is_finite()
        || !input.previous_placement.is_finite()
        || !input.delta.is_finite()
        || input.placement.w_axis.truncate().abs().max_element() > 100_000.0
        || input
            .globals
            .iter()
            .chain(input.previous)
            .any(|matrix| !matrix.is_finite() || matrix.determinant() <= 1.0e-8)
    {
        return Err("ragdoll needs finite, invertible poses and a velocity at most 15 m/s".into());
    }
    let (scale, _, _) = input.placement.to_scale_rotation_translation();
    if !scale.is_finite()
        || !(0.25..=3.0).contains(&scale.x)
        || subtract(scale, Vec3::splat(scale.x)).abs().max_element() > 0.001
    {
        return Err("ragdoll requires a positive uniform placement scale in 0.25..=3".into());
    }
    if input
        .skin
        .mesh_node
        .and_then(|node| input.globals.get(usize::from(node)))
        .is_some_and(|mesh| !mesh.abs_diff_eq(Mat4::IDENTITY, 0.001))
    {
        return Err("ragdoll profile requires an identity skinned mesh transform".into());
    }
    // Reject scaled/sheared bone poses: dynamic rigid bodies cannot preserve them.
    for matrix in input.globals {
        let (bone_scale, bone_rotation, bone_position) = matrix.to_scale_rotation_translation();
        if !Mat4::from_scale_rotation_translation(bone_scale, bone_rotation, bone_position)
            .abs_diff_eq(*matrix, 0.001)
            || bone_position.abs().max_element() > 1_024.0
        {
            return Err("ragdoll does not support sheared or out-of-range bones".into());
        }
        if subtract(bone_scale, Vec3::ONE).abs().max_element() > 0.01 {
            return Err("ragdoll does not support animated bone scaling".into());
        }
    }
    Ok(scale)
}

fn install_bodies(
    runtime: &mut Runtime,
    input: &RagdollInput<'_>,
    profile: &Profile,
    scale: Vec3,
    groups: InteractionGroups,
    velocity: Vec3,
) -> Result<(Vec<Mat4>, Vec<BoneBody>), String> {
    let mut rest_globals = vec![Mat4::IDENTITY; input.skin.nodes.len()];
    for index in &profile.order {
        let node = input.skin.nodes.get(*index).ok_or("missing rest node")?;
        let local = node.local_transform();
        let global = node
            .parent
            .and_then(|parent| rest_globals.get(usize::from(parent)))
            .map_or(local, |parent| parent.mul_mat4(&local));
        if let Some(target) = rest_globals.get_mut(*index) {
            *target = global;
        }
    }
    let mut bodies: Vec<BoneBody> = Vec::with_capacity(profile.segments.len());
    for segment in &profile.segments {
        let global = input
            .globals
            .get(segment.node)
            .copied()
            .ok_or("missing bone pose")?;
        let world = input.placement.mul_mat4(&global);
        let (bone_scale, rotation, position) = world.to_scale_rotation_translation();
        let previous = input.previous_placement.mul_mat4(
            input
                .previous
                .get(segment.node)
                .ok_or("missing previous bone")?,
        );
        let (_, previous_rotation, previous_position) = previous.to_scale_rotation_translation();
        let inverse_delta = if (0.000_01..=0.1).contains(&input.delta) {
            input.delta.recip()
        } else {
            0.0
        };
        let linear = add(
            scaled(subtract(position, previous_position), inverse_delta).clamp_length_max(5.0),
            velocity,
        );
        let angular = scaled(
            rotation
                .mul_quat(previous_rotation.conjugate())
                .to_scaled_axis(),
            inverse_delta,
        );
        let radius = segment.radius * scale.x;
        let length = segment.length * scale.x;
        let center = scaled(Vec3::Y, length * 0.5);
        let name = input
            .skin
            .nodes
            .get(segment.node)
            .map_or("", |node| node.name.as_str());
        let collider = match name {
            "pelvis" => ColliderBuilder::cuboid(0.14 * scale.x, 0.085 * scale.x, 0.09 * scale.x)
                .translation(vector(scaled(Vec3::Y, 0.075 * scale.x))),
            "spine" => ColliderBuilder::cuboid(0.17 * scale.x, 0.20 * scale.x, 0.10 * scale.x)
                .translation(vector(scaled(Vec3::Y, 0.23 * scale.x))),
            _ => ColliderBuilder::capsule_y(length.mul_add(0.5, -radius).max(0.005), radius)
                .translation(vector(center)),
        };
        let (body, _) = runtime.world.insert(
            RigidBodyBuilder::dynamic()
                .pose(pose(position, rotation))
                .linvel(vector(linear))
                .angvel(vector(angular.clamp_length_max(12.0)))
                .linear_damping(0.35)
                // Miniature distal capsules need proportional contact padding
                // and stronger angular drag to dissipate pile corrections.
                // Adult behavior and natural sleep thresholds remain unchanged.
                .angular_damping(2.0 / scale.x.min(1.0))
                .ccd_enabled(true)
                .additional_solver_iterations(8)
                .can_sleep(true),
            collider
                .mass(segment.mass * scale.x.powi(3))
                .friction(0.65)
                .restitution(0.0)
                .contact_skin(0.001 * scale.x.min(1.0))
                .collision_groups(groups),
        );
        if let Some(installed) = runtime.world.bodies.get_mut(body) {
            // Ignore sub-centimetre contact corrections after a full quiet second;
            // ordinary Rapier contacts still wake the whole articulated island.
            let activation = installed.activation_mut();
            activation.normalized_linear_threshold = 0.10;
            activation.time_until_sleep = 1.0;
        }
        bodies.push(BoneBody {
            node: segment.node,
            body,
            previous: world,
            current: world,
            scale: bone_scale,
        });
    }
    Ok((rest_globals, bodies))
}

fn install_joints(
    runtime: &mut Runtime,
    input: &RagdollInput<'_>,
    profile: &Profile,
    scale: Vec3,
    rest_globals: &[Mat4],
    bodies: &[BoneBody],
) -> Result<(), String> {
    for (segment, child) in profile.segments.iter().zip(bodies) {
        let Some(parent) = segment.parent.and_then(|index| bodies.get(index)) else {
            continue;
        };
        let parent_global = input
            .globals
            .get(parent.node)
            .copied()
            .ok_or("missing parent pose")?;
        let child_global = input
            .globals
            .get(child.node)
            .copied()
            .ok_or("missing child pose")?;
        // Rest orientation supplies anatomical axes; current anchors avoid an
        // entry correction. An unusual authored pose expands only its violated
        // angular bound, so a clip can never explode on handoff.
        let anchor = parent_global.inverse().mul_mat4(&child_global);
        let (_, current_rotation, frame_position) = anchor.to_scale_rotation_translation();
        let rest_parent = rest_globals
            .get(parent.node)
            .copied()
            .ok_or("missing parent rest")?;
        let rest_child = rest_globals
            .get(child.node)
            .copied()
            .ok_or("missing child rest")?;
        let (_, frame_rotation, _) = rest_parent
            .inverse()
            .mul_mat4(&rest_child)
            .to_scale_rotation_translation();
        let relative = frame_rotation
            .conjugate()
            .mul_quat(current_rotation)
            .to_scaled_axis();
        let initial_angles = relative.to_array();
        let mut joint = GenericJointBuilder::new(JointAxesMask::LOCKED_SPHERICAL_AXES)
            .local_frame1(pose(scaled(frame_position, scale.x), frame_rotation))
            .local_frame2(pose(Vec3::ZERO, Quat::IDENTITY))
            .contacts_enabled(false)
            // Damped joint compliance prevents hard angular-limit
            // rows pumping tiny table-contact corrections back into the island.
            // Opposing wall/floor contacts need overdamped feedback even on
            // small distal bodies; sleep thresholds and real contacts stay intact.
            .softness(SpringCoefficients::new(120.0, 4.0));
        for (index, (axis, limit)) in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ]
            .into_iter()
            .zip(segment.limits)
            .enumerate()
        {
            let mut range = [-limit, limit];
            if index == 0 {
                let name = input
                    .skin
                    .nodes
                    .get(child.node)
                    .map_or("", |node| node.name.as_str());
                if name.starts_with("shin.") {
                    range = [-0.1, limit];
                }
                if name.starts_with("forearm.") {
                    range = [-limit, 0.1];
                }
            }
            let angle = initial_angles.get(index).copied().unwrap_or(0.0);
            joint = joint.limits(
                axis,
                [range[0].min(angle - 0.05), range[1].max(angle + 0.05)],
            );
        }
        let _joint = runtime
            .world
            .insert_impulse_joint(parent.body, child.body, joint);
    }
    Ok(())
}

impl ItemWorld {
    /// Validates the entire rig/pose/budget before creating any solver handle.
    ///
    /// # Errors
    /// Unsupported rigs, invalid transforms, duplicate ids and exhausted budgets.
    pub fn start_ragdoll(
        &mut self,
        id: &str,
        input: &RagdollInput<'_>,
        velocity: Vec3,
    ) -> Result<(), String> {
        let profile = Profile::mannequin(input.skin)?;
        let scale = validate_pose(input, velocity)?;
        if self.runtime.as_ref().is_some_and(|runtime| {
            runtime.ragdolls.contains_key(id)
                || runtime.ragdolls.len() >= MAX_RAGDOLLS
                || runtime
                    .owned_body_count()
                    .saturating_add(profile.segments.len())
                    > super::MAX_SOLVER_BODIES
        }) {
            return Err("ragdoll already active or shared body/ragdoll budget exhausted".into());
        }
        #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
        let serial_solver_pool = crate::engine::runtime_work::serial_pool()?;
        let runtime = self.initialize(
            #[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
            serial_solver_pool,
        );
        let slot = (8_u32..18)
            .find(|candidate| {
                runtime
                    .ragdolls
                    .values()
                    .all(|ragdoll| ragdoll.slot != *candidate)
            })
            .ok_or("ragdoll collision group budget exhausted")?;
        let membership = Group::from_bits_retain(1_u32.checked_shl(slot).unwrap_or(0));
        let groups = InteractionGroups::new(
            membership,
            Group::ALL & !membership,
            InteractionTestMode::And,
        );
        let (rest_globals, bodies) =
            install_bodies(runtime, input, &profile, scale, groups, velocity)?;
        install_joints(runtime, input, &profile, scale, &rest_globals, &bodies)?;
        let locals = input
            .skin
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let global = input.globals.get(index).copied().unwrap_or(Mat4::IDENTITY);
                node.parent
                    .and_then(|parent| input.globals.get(usize::from(parent)))
                    .map_or(global, |parent| parent.inverse().mul_mat4(&global))
            })
            .collect();
        let _previous = runtime.ragdolls.insert(
            id.to_string(),
            Ragdoll {
                bodies,
                slot,
                locals,
                parents: input.skin.nodes.iter().map(|node| node.parent).collect(),
                order: profile.order,
                placement_inverse: input.placement.inverse(),
            },
        );
        Ok(())
    }

    /// Reuses a former item identity for physical contact reporting only.
    pub fn alias_ragdoll_contacts(&mut self, id: &str, key: u64) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let Some(ragdoll) = runtime.ragdolls.get(id) else {
            return;
        };
        for bone in &ragdoll.bodies {
            if let Some(body) = runtime.world.bodies.get(bone.body) {
                for collider in body.colliders() {
                    let _old = runtime.contact_aliases.insert(*collider, key);
                }
            }
        }
    }

    /// Removes all articulated bodies; Rapier retires their joints/colliders too.
    pub fn remove_ragdoll(&mut self, id: &str) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let Some(ragdoll) = runtime.ragdolls.remove(id) else {
            return false;
        };
        for bone in ragdoll.bodies {
            if let Some(body) = runtime.world.bodies.get(bone.body) {
                for collider in body.colliders() {
                    let _old = runtime.contact_aliases.remove(collider);
                }
            }
            let _removed = runtime.world.remove_body(bone.body);
        }
        true
    }

    #[must_use]
    pub fn has_ragdoll(&self, id: &str) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(|runtime| runtime.ragdolls.contains_key(id))
    }

    /// Writes interpolated global bones into retained animator storage. Descendants
    /// without a body (fingers, head tip) retain their entry local transform.
    pub fn write_ragdoll_pose(&self, id: &str, output: &mut [Mat4]) -> bool {
        let Some(ragdoll) = self
            .runtime
            .as_ref()
            .and_then(|runtime| runtime.ragdolls.get(id))
        else {
            return false;
        };
        if output.len() != ragdoll.locals.len() {
            return false;
        }
        let alpha = (self.accumulator / super::FIXED_STEP).clamp(0.0, 1.0);
        let mut changed = false;
        for index in &ragdoll.order {
            let global = if let Some(bone) = ragdoll.bodies.iter().find(|bone| bone.node == *index)
            {
                if bone.previous == bone.current {
                    let global = ragdoll.placement_inverse.mul_mat4(&bone.current);
                    if let Some(target) = output.get_mut(*index) {
                        changed |= *target != global;
                        *target = global;
                    }
                    continue;
                }
                let (_, before_rotation, before_position) =
                    bone.previous.to_scale_rotation_translation();
                let (_, rotation, position) = bone.current.to_scale_rotation_translation();
                ragdoll
                    .placement_inverse
                    .mul_mat4(&Mat4::from_scale_rotation_translation(
                        bone.scale,
                        before_rotation.slerp(rotation, alpha),
                        before_position.lerp(position, alpha),
                    ))
            } else {
                let local = ragdoll
                    .locals
                    .get(*index)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                ragdoll
                    .parents
                    .get(*index)
                    .copied()
                    .flatten()
                    .and_then(|parent| output.get(usize::from(parent)))
                    .map_or(local, |parent| parent.mul_mat4(&local))
            };
            if let Some(target) = output.get_mut(*index) {
                changed |= *target != global;
                *target = global;
            }
        }
        changed
    }

    /// Live pelvis positions and collider envelopes for scripts and interaction.
    pub fn ragdoll_roots(
        &self,
    ) -> impl Iterator<Item = (&str, Vec3, crate::engine::spatial::Aabb)> {
        self.runtime.iter().flat_map(|runtime| {
            runtime.ragdolls.iter().filter_map(|(id, ragdoll)| {
                let root = runtime.world.bodies.get(ragdoll.bodies.first()?.body)?;
                let mut bounds = crate::engine::spatial::Aabb::EMPTY;
                for bone in &ragdoll.bodies {
                    let body = runtime.world.bodies.get(bone.body)?;
                    for handle in body.colliders() {
                        let collider = runtime.world.colliders.get(*handle)?;
                        let aabb = collider.compute_aabb();
                        bounds.expand(aabb.mins.to_array());
                        bounds.expand(aabb.maxs.to_array());
                    }
                }
                Some((id.as_str(), point(root.translation()), bounds))
            })
        })
    }

    /// Exact retained solver resources, including the player proxy and static geometry.
    #[must_use]
    pub fn solver_resources(&self) -> serde_json::Value {
        self.runtime.as_ref().map_or_else(|| serde_json::json!({"bodies":0_usize,"colliders":0_usize,"joints":0_usize,"ragdolls":0_usize,"bone_pose_caches":0_usize}), |runtime|
            serde_json::json!({"bodies":runtime.world.bodies.len(),"colliders":runtime.world.colliders.len(),
                "joints":runtime.world.impulse_joints.len(),"ragdolls":runtime.ragdolls.len(),
                "bone_pose_caches":runtime.ragdolls.values().map(|ragdoll| ragdoll.locals.len()).sum::<usize>()}))
    }

    #[must_use]
    pub fn ragdoll_snapshots(&self) -> Vec<RagdollSnapshot> {
        let Some(runtime) = self.runtime.as_ref() else {
            return Vec::new();
        };
        runtime
            .ragdolls
            .iter()
            .map(|(id, ragdoll)| {
                let handles: Vec<_> = ragdoll.bodies.iter().map(|bone| bone.body).collect();
                let bodies: Vec<_> = handles
                    .iter()
                    .filter_map(|handle| runtime.world.bodies.get(*handle))
                    .collect();
                let anchor_error = runtime
                    .world
                    .impulse_joints()
                    .filter(|(_, joint)| handles.contains(&joint.body1()))
                    .filter_map(|(_, joint)| {
                        let first = runtime
                            .world
                            .bodies
                            .get(joint.body1())?
                            .position()
                            .transform_point(joint.data.local_frame1.translation);
                        let second = runtime
                            .world
                            .bodies
                            .get(joint.body2())?
                            .position()
                            .transform_point(joint.data.local_frame2.translation);
                        Some(subtract(point(first), point(second)).length())
                    })
                    .fold(0.0_f32, f32::max);
                let penetration = runtime
                    .world
                    .contact_pairs()
                    .filter(|contact| {
                        [contact.collider1, contact.collider2].iter().any(|handle| {
                            runtime
                                .world
                                .colliders
                                .get(*handle)
                                .and_then(rapier3d::prelude::Collider::parent)
                                .is_some_and(|body| handles.contains(&body))
                        })
                    })
                    .flat_map(|contact| contact.manifolds().iter())
                    .flat_map(|manifold| &manifold.points)
                    .map(|point| (-point.dist).max(0.0))
                    .fold(0.0_f32, f32::max);
                RagdollSnapshot {
                    id: id.clone(),
                    bodies: bodies.len(),
                    joints: runtime
                        .world
                        .impulse_joints()
                        .filter(|(_, joint)| handles.contains(&joint.body1()))
                        .count(),
                    sleeping: bodies.iter().filter(|body| body.is_sleeping()).count(),
                    max_anchor_error_m: anchor_error,
                    max_penetration_m: penetration,
                    max_speed_mps: bodies
                        .iter()
                        .map(|body| body.linvel().length())
                        .fold(0.0_f32, f32::max),
                    max_angular_speed_rps: bodies
                        .iter()
                        .map(|body| body.angvel().length())
                        .fold(0.0_f32, f32::max),
                    min_quiet_seconds: bodies
                        .iter()
                        .map(|body| body.activation().time_since_can_sleep)
                        .fold(f32::INFINITY, f32::min),
                    world_positions: bodies
                        .iter()
                        .map(|body| point(body.translation()).to_array())
                        .collect(),
                }
            })
            .collect()
    }
}

impl Runtime {
    pub(super) fn owned_body_count(&self) -> usize {
        self.entries.len().saturating_add(
            self.ragdolls
                .values()
                .map(|ragdoll| ragdoll.bodies.len())
                .sum::<usize>(),
        )
    }

    pub(super) fn update_ragdolls(&mut self, before_step: bool) {
        for ragdoll in self.ragdolls.values_mut() {
            for bone in &mut ragdoll.bodies {
                if before_step {
                    bone.previous = bone.current;
                } else if let Some(body) = self.world.bodies.get(bone.body) {
                    bone.current = Mat4::from_scale_rotation_translation(
                        bone.scale,
                        quaternion(*body.rotation()),
                        point(body.translation()),
                    );
                }
            }
        }
    }
}
