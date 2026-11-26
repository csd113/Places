//! The attached spotlight's bounded visibility target. Geometry, posing and
//! material coverage are submitted by the ordinary world pipeline.

use glam::Vec3;
use std::cell::RefCell;

use crate::engine::lighting::flashlight::{Flashlight, PosedBlocker, PosedVisibilityCache};
use crate::engine::spatial::{DepthRange, Frustum};

use super::world::{WorldFrame, WorldPipeline};

pub const SHADOW_EDGE: u32 = 1024;

pub fn character_transmittance(
    characters: &crate::engine::render::common::CharacterScene,
    cache_store: &RefCell<PosedVisibilityCache>,
    from: [f32; 3],
    to: [f32; 3],
    excluded: Option<&str>,
) -> f32 {
    let current = || {
        characters
            .characters()
            .iter()
            .chain(characters.runtime_characters())
            .map(|character| (character.blood_identity(), character))
    };
    cache_store.borrow_mut().transmittance(
        current,
        |character| PosedBlocker {
            asset: std::ptr::from_ref(character.asset().as_ref()).addr(),
            instance: character.instance_id(),
            opacity: character.opacity(),
            revision: character.animator().revision(),
            bounds: character.world_bounds(),
            model: character.transform(),
            textures: &character.asset().model.textures,
        },
        crate::engine::render::common::light_transport::character_visibility_scene,
        from,
        to,
        excluded,
    )
}

pub struct FlashlightShadow {
    pub view: wgpu::TextureView,
    pub pipeline: WorldPipeline,
    pub frame: WorldFrame,
}

impl FlashlightShadow {
    pub fn new(device: &wgpu::Device, pipeline: WorldPipeline, flashlight: Flashlight) -> Self {
        let texture = depth_texture(device, SHADOW_EDGE);
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            pipeline,
            frame: flashlight_frame(flashlight),
        }
    }
}

pub fn depth_texture(device: &wgpu::Device, edge: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("places-flashlight-visibility"),
        size: wgpu::Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: super::surface::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Validated finite spotlight inputs use glam floating-point vector/matrix arithmetic."
)]
pub fn flashlight_frame(light: Flashlight) -> WorldFrame {
    let up = if light.direction.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let projection = glam::camera::rh::proj::opengl::perspective(
        2.0 * light.outer_cos.acos(),
        1.0,
        0.015,
        light.range,
    );
    let view =
        glam::camera::rh::view::look_at_mat4(light.position, light.position + light.direction, up);
    let matrix = projection * view;
    WorldFrame {
        view_projection: super::world::clip_correction() * matrix,
        frustum: Frustum::from_view_projection(&matrix, DepthRange::NegativeOneToOne),
        eye: light.position,
        occlusion: crate::engine::render::common::occlusion::OcclusionView::EMPTY,
        clip_plane: [0.0; 4],
    }
}
