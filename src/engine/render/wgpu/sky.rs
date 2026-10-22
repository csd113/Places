//! The level's optional night-sky background.
//!
//! A level that declares a `sky` gets one fullscreen background draw at the
//! start of the scene body, before any world geometry. The sheet is an
//! equirectangular 2:1 PNG (`assets/environments/outdoor/textures/sky/...`)
//! sampled by view direction, repeating in U and clamping at the poles, so a
//! level can look at its own stars in any direction with one texture.
//!
//! Contract points, in one place:
//!
//! * **Background, not a light.** Nothing here reaches the baked lighting; the
//!   world body draws over this pass with depth testing on, so a solid ceiling
//!   always covers the sky indoors. The one opt-in environment term is
//!   `sky.ambient`, which belongs to the lighting solve, not to this module.
//! * **Colour.** The authored sRGB sheet uses the texture cache's colour
//!   semantic: fitting and mips average linear light; sampling decodes once.
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

use glam::{Mat4, Vec3};

use super::surface::DEPTH_FORMAT;
use super::texture::{CacheOutcome, GpuTexture, TextureCache, TextureFiltering, TextureWrap};
use crate::engine::assets::AssetCatalog;
use crate::engine::level::LevelDef;
use crate::engine::materials::TextureOrigin;
use crate::engine::quality::{QualityLevel, TextureClass};

/// The sky shader, from the file next to this module.
pub const SKY_SHADER_SRC: &str = include_str!("sky.wgsl");

/// Name of the sky vertex entry point.
pub const SKY_VERTEX_ENTRY: &str = "vs_main";
/// The sRGB-surface fragment entry point.
pub const SKY_FRAGMENT_ENTRY: &str = "fs_main";
/// The raw (non-sRGB) scene-target fragment entry point.
pub const SKY_FRAGMENT_ENTRY_RAW: &str = "fs_main_raw";

/// One cached source projection. Zero diameter disables the slot.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CelestialUniform {
    pub source: [f32; 4],
    pub right: [f32; 4],
    pub up: [f32; 4],
}

impl CelestialUniform {
    fn active_count(celestial: &[Self; 8]) -> u8 {
        celestial
            .iter()
            .rposition(|slot| slot.source[3] > 0.0)
            .and_then(|index| u8::try_from(index.saturating_add(1)).ok())
            .unwrap_or(0)
    }

    fn new(
        light: &crate::engine::level::GlobalIlluminatorDef,
        visual: &crate::engine::level::CelestialVisualDef,
        is_moon: bool,
    ) -> Self {
        let source = Vec3::from_array(light.source_direction());
        let pole = if source.y.abs() > 0.99 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let right = source.cross(pole).normalize_or_zero();
        let up = right.cross(source).normalize_or_zero();
        let projected_source = source
            .extend(0.5 / (0.5 * visual.angular_size_degrees.to_radians()).tan())
            .to_array();
        let projected_right = right.extend(visual.brightness).to_array();
        let projected_up = up.extend(f32::from(is_moon)).to_array();
        Self {
            source: projected_source,
            right: projected_right,
            up: projected_up,
        }
    }
}

/// Bounded background uniform, with at most eight source projections.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SkyUniform {
    /// Inverse clip-space view-projection, for the view-ray reconstruction.
    pub inverse_view_projection: [[f32; 4]; 4],
    /// `x` is panorama brightness; `y` enables the authored panorama.
    pub params: [f32; 4],
    pub storm: [f32; 4],
    pub clear_color: [f32; 4],
    pub celestial: [CelestialUniform; 8],
}

/// Size of [`SkyUniform`] in bytes.
pub const SKY_UNIFORM_SIZE: u64 = 496;

impl SkyUniform {
    /// The uniform for one frame.
    #[must_use]
    pub fn new(view_projection: Mat4, brightness: f32) -> Self {
        Self {
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            params: [brightness, 0.0, 0.0, 0.0],
            storm: [0.0; 4],
            clear_color: glam::DVec4::new(
                super::surface::CLEAR_COLOR.r,
                super::surface::CLEAR_COLOR.g,
                super::surface::CLEAR_COLOR.b,
                1.0,
            )
            .as_vec4()
            .to_array(),
            celestial: [CelestialUniform {
                source: [0.0; 4],
                right: [0.0; 4],
                up: [0.0; 4],
            }; 8],
        }
    }
}

/// The sky pass's pipeline pair and its own camera uniform.
///
/// One pipeline per colour-target convention. The camera buffer and bind group
/// are format-independent and persist for
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
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(texture_layout),
                Some(texture_layout),
                Some(texture_layout),
            ],
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
        sky: &WgpuSky,
        storm: [f32; 4],
    ) {
        let mut uniform = SkyUniform::new(view_projection, sky.brightness());
        uniform.params[1] = f32::from(sky.panorama);
        uniform.params[2] = f32::from(sky.celestial_count);
        uniform.celestial = sky.celestial;
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
        pass.set_bind_group(2, sky.sun.bind_group(filtering), &[]);
        pass.set_bind_group(3, sky.moon.bind_group(filtering), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// One level's uploaded sky sheet.
pub struct WgpuSky {
    /// The uploaded sheet; its bind group carries the sky wrap contract.
    texture: Arc<GpuTexture>,
    /// The authored brightness multiplier.
    brightness: f32,
    panorama: bool,
    sun: Arc<GpuTexture>,
    moon: Arc<GpuTexture>,
    celestial: [CelestialUniform; 8],
    celestial_count: u8,
}

impl WgpuSky {
    /// Resolves, fits and uploads one level's sky.
    ///
    /// Returns `None` when the level declares neither panorama nor celestial visual, the catalog does not
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
        if level.sky.is_none()
            && !level
                .global_illuminators
                .iter()
                .any(|light| light.visual.is_some())
        {
            return None;
        }
        let id = level
            .sky
            .as_ref()
            .map_or("core:tex_white_fallback_01", |sky| sky.texture.trim());
        let Some(path) = catalog.texture_path(id).map(str::to_string) else {
            crate::engine::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] `{id}` is not a catalogued file texture; drawing no sky"),
            );
            return None;
        };
        let Some(root) = asset_root else {
            crate::engine::logging::warn_once(
                format!("sky:{id}"),
                format!("[sky] the asset root is missing; drawing no sky for `{id}`"),
            );
            return None;
        };
        let decoded = if level.sky.is_some() {
            crate::engine::materials::load_sky_png_relative(root, &path)
        } else {
            crate::engine::materials::load_png_relative(root, &path)
        };
        let image = match decoded {
            Ok(image) => image,
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("sky:{id}"),
                    format!("[sky] {error}; drawing no sky"),
                );
                return None;
            }
        };
        let key = super::texture::TextureKey {
            logical: format!("sky:{id}"),
            semantic: super::texture::TextureSemantic::BaseColorDisplay,
            class: TextureClass::Sky,
            level: quality,
            wrap: TextureWrap::RepeatClampV,
        };
        let (outcome, texture) =
            cache.get_or_upload_with_key(device, queue, key, &image, TextureOrigin::Catalog);
        let meta = texture.meta();
        if matches!(outcome, CacheOutcome::Uploaded) {
            crate::engine::logging::info(format!(
                "[wgpu] sky `{id}`: {}x{} uploaded",
                meta.width, meta.height
            ));
        }
        let mut celestial = [CelestialUniform {
            source: [0.0; 4],
            right: [0.0; 4],
            up: [0.0; 4],
        }; 8];
        let mut sun = Arc::clone(&texture);
        let mut moon = Arc::clone(&texture);
        let mut uploaded_sun = false;
        let mut uploaded_moon = false;
        for (slot, light) in celestial.iter_mut().zip(level.global_illuminators.iter()) {
            let Some(visual) = &light.visual else {
                continue;
            };
            if !light.enabled || !light.bake || light.intensity == 0.0 || visual.brightness == 0.0 {
                continue;
            }
            let visual_id = light.visual_texture()?;
            let is_moon = light.kind == crate::engine::level::GlobalIlluminatorKind::Moon;
            if !(if is_moon { uploaded_moon } else { uploaded_sun }) {
                let uploaded =
                    Self::upload_disc(device, queue, cache, catalog, root, visual_id, quality)?;
                if is_moon {
                    moon = uploaded;
                    uploaded_moon = true;
                } else {
                    sun = uploaded;
                    uploaded_sun = true;
                }
            }
            *slot = CelestialUniform::new(light, visual, is_moon);
        }
        let celestial_count = CelestialUniform::active_count(&celestial);
        Some(Self {
            texture,
            sun,
            moon,
            celestial,
            celestial_count,
            panorama: level.sky.is_some(),
            brightness: level.sky.as_ref().map_or(1.0, |sky| sky.brightness),
        })
    }

    fn upload_disc(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        catalog: &AssetCatalog,
        root: &Path,
        id: &str,
        quality: QualityLevel,
    ) -> Option<Arc<GpuTexture>> {
        let path = catalog.texture_path(id)?;
        let image = match crate::engine::materials::load_celestial_png_relative(root, path) {
            Ok(image) => image,
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("celestial:{id}"),
                    format!("[sky] {error}"),
                );
                return None;
            }
        };
        let key = super::texture::TextureKey {
            logical: format!("celestial:{id}"),
            semantic: super::texture::TextureSemantic::BaseColorDisplay,
            class: TextureClass::DecalSheet,
            level: quality,
            wrap: TextureWrap::Clamp,
        };
        let (_, texture) =
            cache.get_or_upload_with_key(device, queue, key, &image, TextureOrigin::Catalog);
        Some(texture)
    }

    /// The authored brightness multiplier.
    #[must_use]
    pub const fn brightness(&self) -> f32 {
        self.brightness
    }
}
