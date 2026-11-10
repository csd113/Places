//! World tool presentation follows the companion's actual posed hand. It owns no
//! inventory, health, collision or damage; those remain simulation authorities.
use crate::engine::{entities::EntityWorld, render::Renderer};
use glam::{Mat4, Quat, Vec3};

// Separate from console props and player representation keys.
const KEY: u64 = u64::MAX - 1030;

pub fn clear(renderer: &mut Renderer) {
    let _removed = renderer.despawn_runtime_model(KEY);
}

/// # Errors
/// Reports an unavailable model, socket or dynamic-scene allocation.
pub fn sync(world: &EntityWorld, renderer: &mut Renderer) -> Result<(), String> {
    if !world.companion_tool_visible() {
        clear(renderer);
        return Ok(());
    }
    let Some(hand) = renderer.character_socket_transform("encounter:robot", "hand.R") else {
        clear(renderer);
        return Err("The companion has no posed right-hand socket".into());
    };
    let grip = hand
        .mul_mat4(&Mat4::from_translation(Vec3::new(0.0, 0.055, 0.012)))
        .mul_mat4(&Mat4::from_quat(Quat::from_rotation_z(
            -std::f32::consts::FRAC_PI_2,
        )));
    let (scale, rotation, position) = grip.to_scale_rotation_translation();
    if renderer.runtime_model_world_bounds(KEY).is_none()
        && !renderer.spawn_runtime_model(
            KEY,
            "shared/tools/crowbar.glb",
            position.to_array(),
            0.0,
            scale.x,
        )
    {
        return Err("Could not create the companion crowbar".into());
    }
    if !renderer.set_runtime_model_pose(KEY, position.to_array(), rotation.to_array()) {
        return Err("Could not update the companion crowbar".into());
    }
    Ok(())
}
