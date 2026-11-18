//! Rendered tread support over the shared continuous walking surface. The
//! approved clip remains the source of limb lift and foot orientation.
use super::Feltling;
use crate::engine::entities::WorldContext;
use crate::engine::render::CharacterAnimator;
use glam::{Mat4, Quat, Vec3};

pub(super) fn sole_shapes(
    model: &crate::engine::gltf::PropModel,
    animator: &CharacterAnimator,
) -> Result<[Vec<Vec3>; 2], String> {
    let skin = model.skin.as_ref().ok_or("Feltling requires a skin")?;
    let mut shapes = [Vec::new(), Vec::new()];
    for (shape, name) in shapes.iter_mut().zip(["foot.L", "foot.R"]) {
        let slot = skin
            .joints
            .iter()
            .position(|index| {
                skin.nodes
                    .get(usize::from(*index))
                    .is_some_and(|node| node.name == name)
            })
            .ok_or("Missing Feltling foot joint")?;
        let inverse = animator
            .node_global(name)
            .ok_or("Missing Feltling foot transform")?
            .inverse();
        for ((vertex, joints), weights) in
            model.vertices.iter().zip(&model.joints).zip(&model.weights)
        {
            let influence: f32 = joints
                .iter()
                .zip(weights)
                .filter_map(|(joint, weight)| (usize::from(*joint) == slot).then_some(*weight))
                .sum();
            if vertex.pos.get(1).is_some_and(|y| *y < 0.009) && influence > 0.5 {
                shape.push(inverse.transform_point3(Vec3::from(vertex.pos)));
            }
        }
        if !(3..=128).contains(&shape.len()) {
            return Err("Feltling requires bounded flat sole samples".into());
        }
    }
    Ok(shapes)
}

impl Feltling {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite local foot points transform through a unit yaw; support remains within the creature's 18 cm step allowance."
    )]
    pub(super) fn sample_terrain(&mut self, ctx: &WorldContext<'_>) {
        let turn = Quat::from_rotation_y(self.state.yaw);
        for ((offset, name), shape) in self
            .state
            .terrain_offsets
            .iter_mut()
            .zip(["foot.L", "foot.R"])
            .zip(&self.sole_shapes)
        {
            let Some(foot) = self.animator.node_global(name) else {
                continue;
            };
            let height = shape
                .iter()
                .filter_map(|point| {
                    let world = self.state.position + turn * foot.transform_point3(*point);
                    ctx.floor
                        .height_below(world.x, world.z, self.state.position.y + 0.18)
                })
                .max_by(f32::total_cmp)
                .unwrap_or(self.state.position.y);
            *offset = (height - self.state.position.y).clamp(-0.18, 0.18);
        }
    }

    pub(super) fn apply_terrain(&mut self) {
        if self
            .state
            .terrain_offsets
            .iter()
            .all(|offset| offset.abs() < 0.0001)
        {
            return;
        }
        let feet = ["foot.L", "foot.R"].map(|name| self.animator.node_global(name));
        let Some(mut root) = self.animator.node_global("root") else {
            return;
        };
        let drop = self
            .state
            .terrain_offsets
            .into_iter()
            .fold(0.0_f32, f32::min);
        root.w_axis.y += drop;
        let _root = self.animator.set_node_global("root", root);
        // A jump keeps its delivered airborne trajectory over the supported base.
        if self.state.clip == "Excited_Jumping" {
            return;
        }
        for ((foot_option, suffix), offset) in feet
            .into_iter()
            .zip(["L", "R"])
            .zip(self.state.terrain_offsets)
        {
            let Some(foot) = foot_option else {
                continue;
            };
            let (_, rotation, mut target) = foot.to_scale_rotation_translation();
            target.y += offset;
            self.solve_terrain_leg(suffix, target, rotation);
        }
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Two-bone IK reuses the established Tall Guest construction with the Feltling's own finite limb lengths; targets clamp to reachable length without scaling bones."
    )]
    fn solve_terrain_leg(&mut self, suffix: &str, target: Vec3, rotation: Quat) {
        let upper_name = format!("thigh.{suffix}");
        let lower_name = format!("shin.{suffix}");
        let foot_name = format!("foot.{suffix}");
        let Some(upper) = self.animator.node_global(&upper_name) else {
            return;
        };
        let Some(lower) = self.animator.node_global(&lower_name) else {
            return;
        };
        let Some(foot) = self.animator.node_global(&foot_name) else {
            return;
        };
        let (upper_scale, upper_rotation, hip) = upper.to_scale_rotation_translation();
        let (lower_scale, lower_rotation, knee) = lower.to_scale_rotation_translation();
        let ankle = foot.transform_point3(Vec3::ZERO);
        let first = hip.distance(knee);
        let second = knee.distance(ankle);
        if first < 0.01 || second < 0.01 || hip.distance(target) < 0.001 {
            return;
        }
        let direction = (target - hip).normalize_or_zero();
        let distance = hip
            .distance(target)
            .clamp((first - second).abs() + 0.0001, first + second - 0.0001);
        let solved_target = hip + direction * distance;
        let along =
            distance.mul_add(distance, second.mul_add(-second, first * first)) / (2.0 * distance);
        let height = first.mul_add(first, -along * along).max(0.0).sqrt();
        let authored = (knee - hip) - direction * (knee - hip).dot(direction);
        let forward = Vec3::Z - direction * direction.z;
        let bend = authored
            .try_normalize()
            .or_else(|| forward.try_normalize())
            .unwrap_or(Vec3::X);
        let solved_knee = hip + direction * along + bend * height;
        let upper_delta = Quat::from_rotation_arc(
            (knee - hip).normalize_or_zero(),
            (solved_knee - hip).normalize_or_zero(),
        );
        let lower_delta = Quat::from_rotation_arc(
            (ankle - knee).normalize_or_zero(),
            (solved_target - solved_knee).normalize_or_zero(),
        );
        let _upper = self.animator.set_node_global(
            &upper_name,
            Mat4::from_scale_rotation_translation(upper_scale, upper_delta * upper_rotation, hip),
        );
        let _lower = self.animator.set_node_global(
            &lower_name,
            Mat4::from_scale_rotation_translation(
                lower_scale,
                lower_delta * lower_rotation,
                solved_knee,
            ),
        );
        let _foot = self.animator.set_node_global(
            &foot_name,
            Mat4::from_scale_rotation_translation(Vec3::ONE, rotation, solved_target),
        );
    }
}
