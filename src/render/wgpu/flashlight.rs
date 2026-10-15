//! The attached spotlight's bounded visibility target. Geometry, posing and
//! material coverage are submitted by the ordinary world pipeline.

use glam::{Mat4, Vec3};
use std::cell::RefCell;

use crate::render::common::Flashlight;
use crate::spatial::{DepthRange, Frustum};

use super::world::{WorldFrame, WorldPipeline};

pub const SHADOW_EDGE: u32 = 1024;

/// Only gameplay ray candidates acquire a local posed visibility resource.
/// Textures are copied once per asset identity; pose refreshes share those PNGs.
pub struct CharacterVisibility {
    identity: usize,
    asset: usize,
    revision: u64,
    opacity_bits: u32,
    textures: Vec<std::sync::Arc<crate::loader::RawImage>>,
    scene: Option<crate::lighting::transport::TransportScene>,
}

pub fn character_transmittance(
    characters: &crate::render::common::CharacterScene,
    cache_store: &RefCell<Vec<CharacterVisibility>>,
    from: [f32; 3],
    to: [f32; 3],
    excluded: Option<&str>,
) -> f32 {
    let mut cache = cache_store.borrow_mut();
    let current = || {
        characters
            .characters()
            .iter()
            .chain(characters.runtime_characters())
    };
    cache.retain(|cached| {
        current().any(|character| std::ptr::from_ref(character).addr() == cached.identity)
    });
    let mut throughput = 1.0;
    for character in current() {
        if character.opacity() <= 0.0
            || excluded.is_some_and(|id| character.instance_id() == Some(id))
        {
            continue;
        }
        let bounds = character.world_bounds();
        if !from
            .into_iter()
            .zip(to)
            .zip(bounds.min.into_iter().zip(bounds.max))
            .all(|((a, b), (minimum, maximum))| a.min(b) <= maximum && a.max(b) >= minimum)
        {
            continue;
        }
        let identity = std::ptr::from_ref(character).addr();
        let asset = std::ptr::from_ref(character.asset().as_ref()).addr();
        let index = cache
            .iter()
            .position(|cached| cached.identity == identity)
            .unwrap_or(cache.len());
        if index == cache.len() {
            let textures = cache
                .iter()
                .find(|cached| cached.asset == asset)
                .map_or_else(
                    || character.asset().model.textures.clone(),
                    |cached| cached.textures.clone(),
                );
            cache.push(CharacterVisibility {
                identity,
                asset,
                revision: u64::MAX,
                opacity_bits: u32::MAX,
                textures,
                scene: None,
            });
        }
        let Some(cached) = cache.get_mut(index) else {
            return 0.0;
        };
        if cached.asset != asset {
            cached.asset = asset;
            cached.revision = u64::MAX;
            cached
                .textures
                .clone_from(&character.asset().model.textures);
        }
        let opacity_bits = character.opacity().to_bits();
        if cached.revision != character.animator().revision() || cached.opacity_bits != opacity_bits
        {
            cached.scene = crate::render::common::light_transport::character_visibility_scene(
                character,
                &cached.textures,
            );
            cached.revision = character.animator().revision();
            cached.opacity_bits = opacity_bits;
        }
        let Some(scene) = cached.scene.as_ref() else {
            return 0.0;
        };
        let inverse = character.transform().inverse();
        if !inverse.is_finite() {
            return 0.0;
        }
        throughput *= scene.transmittance(
            inverse.transform_point3(Vec3::from_array(from)).to_array(),
            inverse.transform_point3(Vec3::from_array(to)).to_array(),
        );
        if throughput <= 0.0 {
            return 0.0;
        }
    }
    throughput
}

pub struct FlashlightShadow {
    pub view: wgpu::TextureView,
    pub pipeline: WorldPipeline,
    pub frame: WorldFrame,
}

impl FlashlightShadow {
    pub fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        environment_layout: &wgpu::BindGroupLayout,
        flashlight: Flashlight,
    ) -> Self {
        let texture = depth_texture(device, SHADOW_EDGE);
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            pipeline: WorldPipeline::shadow(
                device,
                texture_layout,
                material_layout,
                environment_layout,
            ),
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
    let projection = Mat4::perspective_rh_gl(2.0 * light.outer_cos.acos(), 1.0, 0.015, light.range);
    let view = Mat4::look_at_rh(light.position, light.position + light.direction, up);
    let matrix = projection * view;
    WorldFrame {
        view_projection: super::world::clip_correction() * matrix,
        frustum: Frustum::from_view_projection(&matrix, DepthRange::NegativeOneToOne),
        eye: light.position,
    }
}
