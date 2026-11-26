//! WGPU packing for the floor receiver approximation.

/// A soft projected bounds shadow for a movable opaque subject. This is a
/// floor receiver approximation, not a mesh silhouette or shadow map.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ContactShadowUniform {
    pub position_floor: [f32; 4],
    pub half_height: [f32; 4],
    pub incoming: [f32; 4],
    pub receiver_plane: [f32; 4],
}

impl ContactShadowUniform {
    pub const ZERO: Self = Self {
        position_floor: [0.0; 4],
        half_height: [0.0; 4],
        incoming: [0.0; 4],
        receiver_plane: [0.0; 4],
    };
}

impl From<crate::engine::lighting::contact_shadows::ContactShadow> for ContactShadowUniform {
    fn from(shadow: crate::engine::lighting::contact_shadows::ContactShadow) -> Self {
        Self {
            position_floor: shadow.position_floor,
            half_height: shadow.half_height,
            incoming: shadow.incoming,
            receiver_plane: shadow.receiver_plane,
        }
    }
}
