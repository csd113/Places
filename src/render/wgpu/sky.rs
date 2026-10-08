//! The level's optional night-sky background.
//!
//! A level that declares a `sky` gets one fullscreen background draw at the
//! start of the scene body, before any world geometry. The sheet is an
//! equirectangular 2:1 PNG (`assets/environment/outdoor/textures/sky/...`)
//! sampled by view direction, repeating in U and clamping at the poles, so a
//! level can look at its own stars in any direction with one texture.
//!
//! Contract points, in one place:
//!
//! * **Background, not a light.** Nothing here reaches the baked lighting; the
//!   world body draws over this pass with depth testing on, so a solid ceiling
//!   always covers the sky indoors. The one opt-in environment term is
//!   `sky.ambient`, which belongs to the lighting solve, not to this module.
//! * **Display space.** The sheet is a plain `Rgba8Unorm` display-space texture
//!   like every other authored PNG; the raw scene target takes the product
//!   directly and only the sRGB surface entry point converts once.
//! * **Depth.** The sky pipeline tests depth with `Always` and writes nothing,
//!   so it paints the cleared background and every later draw wins. It is
//!   simply not submitted when the level declares no sky, which keeps the
//!   historical clear colour as the background.
//! * **No culling, no reflections.** The background is infinite: it is
//!   not in the reflection captures or the planar mirror. Opt-in storm weather
//!   blends it toward the storm color; the ordinary world fog
//!   does not tint it.

use std::path::Path;
use std::sync::Arc;

use glam::Mat4;

use super::surface::DEPTH_FORMAT;
use super::texture::{CacheOutcome, GpuTexture, TextureCache, TextureFiltering, TextureWrap};
use crate::assets::AssetCatalog;
use crate::level::LevelDef;
use crate::materials::TextureOrigin;
use crate::quality::{QualityLevel, TextureClass, fit_image};

/// The sky shader, from the file next to this module.
pub const SKY_SHADER_SRC: &str = include_str!("sky.wgsl");

/// Name of the sky vertex entry point.
pub const SKY_VERTEX_ENTRY: &str = "vs_main";
/// The sRGB-surface fragment entry point.
pub const SKY_FRAGMENT_ENTRY: &str = "fs_main";
/// The raw (non-sRGB) scene-target fragment entry point.
pub const SKY_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";

/// The sky uniform, 96 bytes: a 4x4 matrix and two parameter slots.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkyUniform {
    /// Inverse clip-space view-projection, for the view-ray reconstruction.
    pub inverse_view_projection: [[f32; 4]; 4],
    /// `x` is the brightness multiplier; `yzw` are reserved and zero.
    pub params: [f32; 4],
    pub storm: [f32; 4],
}

/// Size of [`SkyUniform`] in bytes.
pub const SKY_UNIFORM_SIZE: u64 = 96;

impl SkyUniform {
    /// The uniform for one frame.
    #[must_use]
    pub fn new(view_projection: Mat4, brightness: f32) -> Self {
        Self {
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            params: [brightness, 0.0, 0.0, 0.0],
            storm: [0.0; 4],
        }
    }
}

/// The sky pass's pipeline pair and its own camera uniform.
///
/// One pipeline per colour-target convention: the offscreen scene target is raw
/// and takes the display-space product, the main surface is sRGB and converts
/// once. The camera buffer and bind group are format-independent and persist for
/// the pipeline's lifetime; the camera is written only when the uploaded uniform
/// changed, exactly like [`super::world::WorldPipeline`].
pub struct SkyPipeline {
    scene: wgpu::RenderPipeline,
    surface: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The last uploaded uniform, so a still camera writes nothing.
    uploaded: Option<SkyUniform>,
}

impl SkyPipeline {
    /// Builds the sky pipelines for the scene and surface colour formats.
    ///
    /// `texture_layout` is the shared texture-cache group-1 layout: the sky's
    /// sheet is uploaded through the same cache (with the sky's repeat-U /
    /// clamp-V wrap), so its bind group, its sampler policy and this pipeline
    /// cannot drift apart.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        scene_format: wgpu::TextureFormat,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("places-wgpu-sky-shader"),
            source: wgpu::ShaderSource::Wgsl(SKY_SHADER_SRC.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("places-wgpu-sky-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(SKY_UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("places-wgpu-sky-pipeline-layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let build = |format: wgpu::TextureFormat, entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("places-wgpu-sky"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(SKY_VERTEX_ENTRY),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    // A fullscreen triangle from `@builtin(vertex_index)`: no
                    // vertex or index buffer is bound at all.
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    // The background paints over the clear and lets every later
                    // draw win; it must not write depth or it would occlude the
                    // world it is behind.
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &super::surface::color_target_constants(format),
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let surface_entry = if surface_format.is_srgb() {
            SKY_FRAGMENT_ENTRY
        } else {
            SKY_FRAGMENT_ENTRY_RAW
        };
        let surface = build(surface_format, surface_entry);
        let scene = build(scene_format, SKY_FRAGMENT_ENTRY_RAW);
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-sky-camera"),
            size: SKY_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("places-wgpu-sky-camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        Self {
            scene,
            surface,
            camera_buffer,
            bind_group,
            uploaded: None,
        }
    }

    /// Uploads the frame's view-projection and brightness when they changed.
    pub fn upload_camera_with_storm(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        brightness: f32,
        storm: [f32; 4],
    ) {
        let mut uniform = SkyUniform::new(view_projection, brightness);
        uniform.storm = storm;
        if self.uploaded == Some(uniform) {
            return;
        }
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        self.uploaded = Some(uniform);
    }

    /// Draws the sky into an open pass bound to the offscreen scene target.
    pub fn encode_scene<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        sky: &'a WgpuSky,
        filtering: TextureFiltering,
    ) {
        pass.set_pipeline(&self.scene);
        self.encode_body(pass, sky, filtering);
    }

    /// Draws the sky into an open pass bound to the main surface target.
    pub fn encode_surface<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        sky: &'a WgpuSky,
        filtering: TextureFiltering,
    ) {
        pass.set_pipeline(&self.surface);
        self.encode_body(pass, sky, filtering);
    }

    fn encode_body<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        sky: &'a WgpuSky,
        filtering: TextureFiltering,
    ) {
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, sky.texture.bind_group(filtering), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// One level's uploaded sky sheet.
pub struct WgpuSky {
    /// The uploaded sheet; its bind group carries the sky wrap contract.
    texture: Arc<GpuTexture>,
    /// The authored brightness multiplier.
    brightness: f32,
}

impl WgpuSky {
    /// Resolves, fits and uploads one level's sky.
    ///
    /// Returns `None` when the level declares no sky, the catalog does not
    /// declare the named texture as a file PNG, or the asset root is missing;
    /// each of those is a one-line warning and the level keeps the clear-colour
    /// background. A broken image is never a load failure.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        catalog: &AssetCatalog,
        asset_root: Option<&Path>,
        level: &LevelDef,
        quality: QualityLevel,
    ) -> Option<Self> {
        let sky = level.sky.as_ref()?;
        let id = sky.texture.trim();
        let Some(path) = catalog.texture_path(id).map(str::to_string) else {
            crate::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] `{id}` is not a catalogued file texture; drawing no sky"),
            );
            return None;
        };
        let Some(root) = asset_root else {
            crate::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] the asset root is missing; drawing no sky for `{id}`"),
            );
            return None;
        };
        let image = match crate::materials::load_sky_png_relative(root, &path) {
            Ok(image) => image,
            Err(error) => {
                crate::logging::warn_once(
                    format!("sky:{id}"),
                    format!("[sky] {error}; drawing no sky"),
                );
                return None;
            }
        };
        let fitted = fit_image(&image, quality, TextureClass::Sky);
        let key = super::texture::TextureKey {
            logical: format!("sky:{id}"),
            semantic: super::texture::TextureSemantic::BaseColorDisplay,
            class: TextureClass::Sky,
            level: quality,
            wrap: TextureWrap::RepeatClampV,
        };
        let (outcome, texture) = cache.get_or_upload_with_key(
            device,
            queue,
            key,
            fitted.as_ref(),
            TextureOrigin::Catalog,
        );
        let meta = texture.meta();
        if matches!(outcome, CacheOutcome::Uploaded) {
            crate::logging::info(format!(
                "[wgpu] sky `{id}`: {}x{} uploaded",
                meta.width, meta.height
            ));
        }
        Some(Self {
            texture,
            brightness: sky.brightness,
        })
    }

    /// The authored brightness multiplier.
    #[must_use]
    pub const fn brightness(&self) -> f32 {
        self.brightness
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storm_sky_uniform_and_shader_match_and_calm_keeps_zero_blend()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(std::mem::size_of::<SkyUniform>(), 96);
        assert_eq!(std::mem::offset_of!(SkyUniform, storm), 80);
        assert_eq!(SkyUniform::new(Mat4::IDENTITY, 1.0).storm, [0.0; 4]);
        let module = wgpu::naga::front::wgsl::parse_str(SKY_SHADER_SRC)?;
        let _validated = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)?;
        Ok(())
    }
}
