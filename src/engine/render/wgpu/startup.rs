//! First-use pipeline preparation. Only immutable device resources cross the
//! worker boundary; SDL windows, surface acquisition and presentation never do.
use std::sync::mpsc::{Receiver, TryRecvError};

use super::{
    Acquired, DecalPipeline, EffectsPipeline, PostProcess, SCENE_FORMAT, SkyPipeline, WgpuRenderer,
    WorldPipeline,
};

pub(super) struct WorldPipelines {
    surface: WorldPipeline,
    capture: WorldPipeline,
    scene: WorldPipeline,
    emissive: WorldPipeline,
}

impl WorldPipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        textures: &wgpu::BindGroupLayout,
        materials: &wgpu::BindGroupLayout,
        environment: &wgpu::BindGroupLayout,
    ) -> Self {
        let mut timer = crate::engine::perf::loading::StageTimer::new("renderer-world-pipelines");
        let shader = WorldPipeline::shader(device);
        timer.step("world-shader");
        let surface = WorldPipeline::with_state(
            device,
            &shader,
            format,
            textures,
            materials,
            environment,
            wgpu::FrontFace::Ccw,
            false,
            false,
        );
        timer.step("world-surface");
        // Reflection projections reverse winding; both sides remain visible.
        let capture = WorldPipeline::with_state(
            device,
            &shader,
            super::super::reflections::REFLECTION_FORMAT,
            textures,
            materials,
            environment,
            wgpu::FrontFace::Cw,
            false,
            true,
        );
        timer.step("world-capture");
        let scene = WorldPipeline::with_state(
            device,
            &shader,
            SCENE_FORMAT,
            textures,
            materials,
            environment,
            wgpu::FrontFace::Ccw,
            false,
            true,
        );
        timer.step("world-scene");
        let emissive = WorldPipeline::with_state(
            device,
            &shader,
            super::EMISSIVE_FORMAT,
            textures,
            materials,
            environment,
            wgpu::FrontFace::Ccw,
            false,
            true,
        );
        timer.step("world-emissive");
        Self {
            surface,
            capture,
            scene,
            emissive,
        }
    }

    pub(super) fn install(self, renderer: &mut WgpuRenderer) {
        renderer.world_pipeline = Some(self.surface);
        renderer.capture_pipeline = Some(self.capture);
        renderer.scene_pipeline = Some(self.scene);
        renderer.emissive_pipeline = Some(self.emissive);
    }
}

#[derive(Clone, Copy)]
enum Auxiliary {
    SurfaceDecals,
    ReflectionDecals,
    SceneDecals,
    Sky,
    SurfaceEffects,
    SceneEffects,
    Post,
}

struct PipelineWorker {
    receive: Receiver<WorldPipelines>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for PipelineWorker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            crate::engine::logging::warn("World pipeline worker stopped unexpectedly");
        }
    }
}

enum Preparation {
    Main(Auxiliary),
    Worker(PipelineWorker),
}

pub(super) struct StartupPipelines {
    format: wgpu::TextureFormat,
    preparation: Preparation,
}

impl WgpuRenderer {
    /// Shared player/editor preparation; offline baking retains the synchronous
    /// constructor and the same shader module reuse. At most one worker exists.
    pub fn prepare_startup_pipelines(&mut self) -> Result<bool, String> {
        if self.check_device_lost() {
            return Err(self
                .fatal
                .clone()
                .unwrap_or_else(|| "GPU device unavailable during startup".into()));
        }
        if self.world_pipeline.as_ref().map(WorldPipeline::format) == Some(self.config.format)
            && self.startup_pipelines.is_none()
        {
            return Ok(true);
        }
        let mut pending = if let Some(pending) = self.startup_pipelines.take() {
            pending
        } else {
            // Build the small loading UI before the worker can enter driver compilation.
            self.ensure_ui_pipeline(self.config.format);
            StartupPipelines {
                format: self.config.format,
                preparation: Preparation::Main(Auxiliary::SurfaceDecals),
            }
        };
        match pending.preparation {
            Preparation::Main(stage) => {
                if pending.format != self.config.format {
                    // Nothing outstanding yet; restart for the new surface format.
                    pending.format = self.config.format;
                    pending.preparation = Preparation::Main(Auxiliary::SurfaceDecals);
                    self.startup_pipelines = Some(pending);
                    return Ok(false);
                }
                pending.preparation = if let Some(next) = self.prepare_startup_auxiliary(stage) {
                    Preparation::Main(next)
                } else {
                    let device = self.device.clone();
                    let textures = self.textures.layout().clone();
                    let materials = self.material_layout.clone();
                    let environment = self.environment_layout.clone();
                    let format = pending.format;
                    let (send, receive) = std::sync::mpsc::channel();
                    let worker = std::thread::Builder::new()
                        .name("places-world-pipelines".into())
                        .spawn(move || {
                            let started = std::time::Instant::now();
                            let pipeline = WorldPipelines::new(
                                &device,
                                format,
                                &textures,
                                &materials,
                                &environment,
                            );
                            crate::engine::logging::info(format!(
                                "[loading-shader-worker] world_pipelines_ms={:.3}",
                                started.elapsed().as_secs_f64() * 1_000.0_f64
                            ));
                            // Dropping the receiver on shutdown safely retires the immutable resources.
                            let _delivered = send.send(pipeline);
                        })
                        .map_err(|error| format!("Cannot prepare world pipelines: {error}"))?;
                    Preparation::Worker(PipelineWorker {
                        receive,
                        thread: Some(worker),
                    })
                };
            }
            Preparation::Worker(ref worker) => match worker.receive.try_recv() {
                Ok(world) => {
                    // A recreated surface can change format. Finish/discard the old
                    // worker before scheduling another, rather than accumulating jobs.
                    if pending.format != self.config.format {
                        pending.format = self.config.format;
                        pending.preparation = Preparation::Main(Auxiliary::SurfaceDecals);
                        self.startup_pipelines = Some(pending);
                        return Ok(false);
                    }
                    world.install(self);
                    crate::engine::logging::info(format!(
                        "[wgpu] prepared world pipelines for {:?}",
                        self.config.format
                    ));
                    return Ok(true);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err("World pipeline preparation stopped; see the graphics log".into());
                }
            },
        }
        self.startup_pipelines = Some(pending);
        Ok(false)
    }

    /// The editor pumps native events while a non-cancellable driver call
    /// finishes. Taking the owner joins the one worker and drops its result.
    pub(crate) fn retire_startup_pipelines(&mut self) -> bool {
        let finished =
            self.startup_pipelines
                .as_ref()
                .is_none_or(|pending| match &pending.preparation {
                    Preparation::Main(_) => true,
                    Preparation::Worker(worker) => worker
                        .thread
                        .as_ref()
                        .is_none_or(std::thread::JoinHandle::is_finished),
                });
        if finished {
            drop(self.startup_pipelines.take());
        }
        finished
    }

    pub(super) fn prepare_all_auxiliary(&mut self) {
        let mut next = Some(Auxiliary::SurfaceDecals);
        while let Some(stage) = next {
            next = self.prepare_startup_auxiliary(stage);
        }
    }

    fn prepare_startup_auxiliary(&mut self, stage: Auxiliary) -> Option<Auxiliary> {
        use Auxiliary::{
            Post, ReflectionDecals, SceneDecals, SceneEffects, Sky, SurfaceDecals, SurfaceEffects,
        };
        let started = std::time::Instant::now();
        let (next, label) = match stage {
            SurfaceDecals => {
                self.decal_pipeline = Some(DecalPipeline::new(
                    &self.device,
                    self.config.format,
                    &self.caustics,
                    &self.textures,
                ));
                (Some(ReflectionDecals), "surface-decals")
            }
            ReflectionDecals => {
                self.decal_reflection_pipeline = Some(DecalPipeline::new(
                    &self.device,
                    super::super::reflections::REFLECTION_FORMAT,
                    &self.caustics,
                    &self.textures,
                ));
                (Some(SceneDecals), "reflection-decals")
            }
            SceneDecals => {
                self.decal_scene_pipeline = Some(DecalPipeline::new(
                    &self.device,
                    SCENE_FORMAT,
                    &self.caustics,
                    &self.textures,
                ));
                (Some(Sky), "scene-decals")
            }
            Sky => {
                self.sky_pipeline = Some(SkyPipeline::new(
                    &self.device,
                    self.config.format,
                    SCENE_FORMAT,
                    self.textures.layout(),
                ));
                (Some(SurfaceEffects), "sky")
            }
            SurfaceEffects => {
                self.effects_pipeline = Some(EffectsPipeline::new(
                    &self.device,
                    self.config.format,
                    self.textures.layout(),
                ));
                (Some(SceneEffects), "surface-effects")
            }
            SceneEffects => {
                self.effects_scene_pipeline = Some(EffectsPipeline::new(
                    &self.device,
                    SCENE_FORMAT,
                    self.textures.layout(),
                ));
                (Some(Post), "scene-effects")
            }
            Post => {
                self.post = Some(match self.post.take() {
                    Some(mut post) => {
                        let _changed = post.set_surface_format(&self.device, self.config.format);
                        post
                    }
                    None => PostProcess::new(&self.device, self.config.format),
                });
                (None, "postprocess")
            }
        };
        crate::engine::logging::info(format!(
            "[loading-pipeline-main] {label} {:.3} ms",
            started.elapsed().as_secs_f64() * 1_000.0_f64
        ));
        next
    }

    pub(super) fn render_startup_frame(&mut self) {
        let Acquired::Frame(frame) = self.acquire_frame() else {
            return;
        };
        self.gpu_timer.begin_frame(&self.queue);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.clear_startup_target(&view);
        self.pending_frame = Some(frame);
    }

    fn clear_startup_target(&self, view: &wgpu::TextureView) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("places-loading-clear"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("places-loading-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.direct_clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        let _submission = self.queue.submit([encoder.finish()]);
    }

    pub(super) fn capture_startup_frame(
        &mut self,
    ) -> Result<crate::engine::loader::RawImage, String> {
        let width = self.drawable_size.width.max(1);
        let height = self.drawable_size.height.max(1);
        let format = super::capture_format(self.config.format);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-loading-capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.clear_startup_target(&view);
        self.replay_capture_ui(&view, format, self.drawable_size);
        super::read_back_rgba(&self.device, &self.queue, &texture, width, height, format)
    }
}
