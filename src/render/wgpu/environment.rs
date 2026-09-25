//! Stage 9 environment bindings: group 3's uniform and textures.
//!
//! The environment is the frame/level state the world fragment stage reads
//! besides the camera, the draw's base texture and its material: the baked-light
//! switch and scale, the fog constants, the two lightmap atlas pages, the
//! reflection probe cubemap and the planar mirror image, plus the active planar
//! mirror's projection and plane. The reference stored every one of those as a
//! separate program uniform or texture unit; they are grouped here because they
//! change on the same events (level load, resize, planar capture) and are read
//! together by every world draw.
//!
//! Nothing here creates resources per frame. The uniform buffer is written
//! per frame only when a planar capture is active (matrix and plane change);
//! every texture and the bind group are level/size resources.

use super::lightmap::LightmapAtlas;
use super::texture::{TextureCache, TextureFiltering};
use super::world::{ENVIRONMENT_UNIFORM_SIZE, EnvironmentUniform, environment_bind_group};
use crate::render::common::atmosphere::FogState;

/// The 1x1 black cubemap the shader samples while no probe is resident.
///
/// The reference binds a one-texel black cube whenever no probe exists (or
/// during a capture), so a probe material always samples a complete cube. A
/// material's `reflection_mode` is zero until a probe is baked, so the value is
/// never visible; the resource exists so the binding is always valid.
#[must_use]
pub fn fallback_probe(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("places-wgpu-probe-fallback"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("places-wgpu-probe-fallback-view"),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..wgpu::TextureViewDescriptor::default()
    });
    (texture, view)
}

/// The 1x1 white planar mirror the shader samples while no plane is active.
#[must_use]
pub fn fallback_planar(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("places-wgpu-planar-fallback"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// One environment uniform buffer and its group-3 bind groups.
///
/// Four bind groups share the one uniform buffer: one per probe cubemap (the
/// reference selects the nearest probe to the camera every frame), the live
/// environment (planar mirror and the first probe) and the capture
/// environment, which binds the fallback probe and planar textures. A capture
/// pass must never bind the texture it is rendering into, even when the
/// material modes are zero; the reference binds its black cube and white planar
/// for exactly the same reason.
pub struct EnvironmentBindings {
    buffer: wgpu::Buffer,
    probe_bind_groups: Vec<wgpu::BindGroup>,
    capture_bind_group: wgpu::BindGroup,
    uploaded: EnvironmentUniform,
}

impl EnvironmentBindings {
    /// Creates the uniform buffer and the bind groups over the level's
    /// lightmap pages, probe cubemaps and planar target.
    ///
    /// `probes` is the level's probe cubemaps in bake order (at most
    /// [`crate::render::common::view::MAX_REFLECTION_PROBES`]); an empty list
    /// binds the fallback cube. `planar` is the fallback view when no mirror
    /// target exists. The lightmap sampler follows the player's filtering
    /// setting, exactly like the reference's `set_lightmap_filter`; the
    /// reflection sampler is clamped linear, matching the reference's probe and
    /// planar textures.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        cache: &TextureCache,
        lightmaps: &LightmapAtlas,
        probes: &[&wgpu::TextureView],
        planar: &wgpu::TextureView,
        fallback_probe: &wgpu::TextureView,
        fallback_planar: &wgpu::TextureView,
        filtering: TextureFiltering,
        uniform: EnvironmentUniform,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-environment"),
            size: ENVIRONMENT_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));
        let lightmap_sampler = cache.sampler(filtering.clamp_sampler_policy());
        let reflection_sampler = cache.sampler(super::texture::SamplerPolicy::ClampLinear);
        let mut probe_bind_groups: Vec<wgpu::BindGroup> = probes
            .iter()
            .map(|probe| {
                environment_bind_group(
                    device,
                    layout,
                    &buffer,
                    lightmaps.page_view(0),
                    lightmaps.page_view(1),
                    lightmap_sampler,
                    probe,
                    planar,
                    reflection_sampler,
                )
            })
            .collect();
        if probe_bind_groups.is_empty() {
            probe_bind_groups.push(environment_bind_group(
                device,
                layout,
                &buffer,
                lightmaps.page_view(0),
                lightmaps.page_view(1),
                lightmap_sampler,
                fallback_probe,
                planar,
                reflection_sampler,
            ));
        }
        let capture_bind_group = environment_bind_group(
            device,
            layout,
            &buffer,
            lightmaps.page_view(0),
            lightmaps.page_view(1),
            lightmap_sampler,
            fallback_probe,
            fallback_planar,
            reflection_sampler,
        );
        Self {
            buffer,
            probe_bind_groups,
            capture_bind_group,
            uploaded: uniform,
        }
    }

    /// The group-3 bind group of ordinary draws (the first probe).
    #[must_use]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group_for_probe(0)
    }

    /// The bind group for one probe, or the first when `probe` is out of range.
    #[must_use]
    pub fn bind_group_for_probe(&self, probe: usize) -> &wgpu::BindGroup {
        self.probe_bind_groups
            .get(probe)
            .or_else(|| self.probe_bind_groups.first())
            .unwrap_or(&self.capture_bind_group)
    }

    /// The group-3 bind group of a capture pass (fallback probe and planar).
    #[must_use]
    pub const fn capture_bind_group(&self) -> &wgpu::BindGroup {
        &self.capture_bind_group
    }

    /// Writes a changed uniform value.
    ///
    /// Returns true when the buffer was written. The predicate compares every
    /// field the shader reads; a frame whose environment did not change writes
    /// nothing.
    pub fn update(&mut self, queue: &wgpu::Queue, uniform: EnvironmentUniform) -> bool {
        if self.uploaded == uniform {
            return false;
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = uniform;
        true
    }
}

/// The environment value for a static-world frame: unit light scale, the fog
/// constants, no active mirror and the identity model.
#[must_use]
pub const fn static_environment(lightmap_enabled: bool, fog: FogState) -> EnvironmentUniform {
    EnvironmentUniform::new([1.0; 3], lightmap_enabled, fog)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use super::*;

    #[test]
    fn the_static_environment_is_unit_scale_and_fogged() {
        let fog = FogState::SHIPPED;
        let environment = static_environment(true, fog);
        assert_eq!(environment.light_scale, [1.0; 3]);
        assert_eq!(environment.lightmap_enabled, 1.0);
        assert_eq!(environment.fog_color, fog.color);
        assert_eq!(environment.fog_density, fog.density);
        assert_eq!(environment.planar_plane, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(environment.model, glam::Mat4::IDENTITY.to_cols_array_2d());
    }

    #[test]
    fn a_disabled_atlas_sets_the_switch_to_zero() {
        let environment = static_environment(false, FogState::SHIPPED);
        assert_eq!(environment.lightmap_enabled, 0.0);
    }
}
