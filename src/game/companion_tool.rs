//! World tool presentation follows the companion's actual posed hand. It owns no
//! inventory, health, collision or damage; those remain simulation authorities.
use crate::engine::{
    entities::{EntityWorld, encounter::MAX_COMPANIONS},
    render::Renderer,
};
use glam::{Mat4, Quat, Vec3};

// Separate from console props and player representation keys.
const KEY: u64 = u64::MAX - 1030;

pub fn clear(renderer: &mut Renderer) {
    for index in 0..MAX_COMPANIONS {
        let _removed = renderer.despawn_runtime_model(KEY.saturating_sub(u64::from(index)));
    }
}

/// # Errors
/// Reports an unavailable model, socket or dynamic-scene allocation.
pub fn sync(world: &EntityWorld, renderer: &mut Renderer) -> Result<(), String> {
    for index in 0..MAX_COMPANIONS {
        let key = KEY.saturating_sub(u64::from(index));
        if let Some(id) = world.companion_tool_id(index) {
            sync_tool(renderer, key, id)?;
        } else {
            let _removed = renderer.despawn_runtime_model(key);
        }
    }
    Ok(())
}

fn sync_tool(renderer: &mut Renderer, key: u64, id: &str) -> Result<(), String> {
    let Some(hand) = renderer.character_socket_transform(id, "hand.R") else {
        let _removed = renderer.despawn_runtime_model(key);
        return Err(format!("Companion {id} has no posed right-hand socket"));
    };
    let grip = hand
        .mul_mat4(&Mat4::from_translation(Vec3::new(0.0, 0.055, 0.012)))
        .mul_mat4(&Mat4::from_quat(Quat::from_rotation_z(
            -std::f32::consts::FRAC_PI_2,
        )));
    let (scale, rotation, position) = grip.to_scale_rotation_translation();
    if renderer.runtime_model_world_bounds(key).is_none()
        && !renderer.spawn_runtime_model(
            key,
            "shared/tools/crowbar.glb",
            position.to_array(),
            0.0,
            scale.x,
        )
    {
        return Err("Could not create the companion crowbar".into());
    }
    if !renderer.set_runtime_model_pose(key, position.to_array(), rotation.to_array()) {
        return Err("Could not update the companion crowbar".into());
    }
    Ok(())
}
