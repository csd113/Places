//! First-use pipeline preparation. Only immutable device resources cross the
//! worker boundary; SDL windows, surface acquisition and presentation never do.
use std::sync::mpsc::{Receiver, TryRecvError};

pub(super) fn owner_wait<T>(
    future: impl std::future::Future<Output = T>,
    pump: &mut impl FnMut() -> Result<(), String>,
) -> Result<T, String> {
    let mut pending = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        pump()?;
        match pending.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return Ok(value),
            std::task::Poll::Pending => std::thread::sleep(std::time::Duration::from_millis(5)),
        }
    }
}

use super::super::{skinning, world};

use super::{
    Acquired, DecalPipeline, EffectsPipeline, PostProcess, SCENE_FORMAT, SkyPipeline, WgpuRenderer,
    WorldPipeline,
};

pub(super) struct CharacterPipelines {
    pub(super) skin: skinning::SkinPipeline,
    pub(super) blend: Vec<world::PreparedCharacterBlend>,
}

pub(super) struct CharacterPipelineWorker {
    pub(super) receive: Receiver<CharacterPipelines>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CharacterPipelineWorker {
    pub(super) fn start(
        device: wgpu::Device,
        skin: Option<skinning::SkinPipeline>,
        blend: Vec<world::CharacterBlendPreparation>,
        textures: wgpu::BindGroupLayout,
        materials: wgpu::BindGroupLayout,
    ) -> Result<Self, String> {
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("places-character-pipeline".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let prepared_skin = skin.unwrap_or_else(|| skinning::SkinPipeline::new(&device));
                let prepared_blend = blend
                    .into_iter()
                    .map(|preparation| preparation.build(&device, &textures, &materials))
                    .collect();
                crate::engine::logging::info(format!(
                    "[loading-shader-worker] character_pipelines_ms={:.3}",
                    started.elapsed().as_secs_f64() * 1_000.0_f64
                ));
                let _delivered = send.send(CharacterPipelines {
                    skin: prepared_skin,
                    blend: prepared_blend,
                });
            })
            .map_err(|error| format!("Cannot prepare character pipelines: {error}"))?;
        Ok(Self {
            receive,
            thread: Some(thread),
        })
    }

    fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }
}

impl Drop for CharacterPipelineWorker {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            crate::engine::logging::warn("Character pipeline worker stopped unexpectedly");
        }
    }
}

pub(super) struct WorldPipelines {
    shadow: WorldPipeline,
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
        let emissive = if SCENE_FORMAT == super::EMISSIVE_FORMAT {
            scene.fork(device)
        } else {
            WorldPipeline::with_state(
                device,
                &shader,
                super::EMISSIVE_FORMAT,
                textures,
                materials,
                environment,
                wgpu::FrontFace::Ccw,
                false,
                true,
            )
        };
        timer.step("world-emissive");
        let shadow = WorldPipeline::with_state(
            device,
            &shader,
            super::super::surface::DEPTH_FORMAT,
            textures,
            materials,
            environment,
            wgpu::FrontFace::Ccw,
            false,
            true,
        );
        timer.step("world-shadow");
        Self {
            shadow,
            surface,
            capture,
            scene,
            emissive,
        }
    }

    pub(super) fn install(self, renderer: &mut WgpuRenderer) {
        renderer.flashlight_pipeline = Some(self.shadow);
        renderer.world_pipeline = Some(self.surface);
        renderer.capture_pipeline = Some(self.capture);
        renderer.scene_pipeline = Some(self.scene);
        renderer.emissive_pipeline = Some(self.emissive);
    }
}

/// One worker prepares all driver programs. Only publication of bindings to the
/// current caustics/cache happens on the owner; it never shares mutable state.
struct AuxiliaryPipelines {
    decals: [super::super::decals::DecalPrograms; 3],
    sky: SkyPipeline,
    effects: EffectsPipeline,
    scene_effects: EffectsPipeline,
    post: super::super::postprocess::PostPrograms,
}

impl AuxiliaryPipelines {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        textures: &wgpu::BindGroupLayout,
        post_layouts: Option<super::super::postprocess::PostLayouts>,
    ) -> Self {
        let mut timer =
            crate::engine::perf::loading::StageTimer::new("renderer-auxiliary-pipelines");
        let decals = [
            format,
            super::super::reflections::REFLECTION_FORMAT,
            SCENE_FORMAT,
        ]
        .map(|target| super::super::decals::DecalPrograms::new(device, target));
        timer.step("decals");
        let sky = SkyPipeline::new(device, format, SCENE_FORMAT, textures);
        timer.step("sky");
        let effects = EffectsPipeline::new(device, format, textures);
        let scene_effects = EffectsPipeline::new(device, SCENE_FORMAT, textures);
        timer.step("effects");
        let post = super::super::postprocess::PostPrograms::new(device, format, post_layouts);
        timer.step("post");
        Self {
            decals,
            sky,
            effects,
            scene_effects,
            post,
        }
    }

    fn install(self, renderer: &mut WgpuRenderer) {
        let started = std::time::Instant::now();
        let [surface, capture, scene] = self.decals.map(|programs| {
            DecalPipeline::from_programs(
                &renderer.device,
                programs,
                &renderer.caustics,
                &renderer.textures,
            )
        });
        renderer.decal_pipeline = Some(surface);
        renderer.decal_reflection_pipeline = Some(capture);
        renderer.decal_scene_pipeline = Some(scene);
        renderer.sky_pipeline = Some(self.sky);
        renderer.effects_pipeline = Some(self.effects);
        renderer.effects_scene_pipeline = Some(self.scene_effects);
        renderer.post = Some(self.post.install(&renderer.device, renderer.post.take()));
        crate::engine::logging::info(format!(
            "[loading-pipeline-main] publish-auxiliary {:.3} ms",
            started.elapsed().as_secs_f64() * 1_000.0_f64
        ));
    }
}

struct PreparedPipelines {
    world: WorldPipelines,
    auxiliary: AuxiliaryPipelines,
}

struct PipelineWorker {
    receive: Receiver<PreparedPipelines>,
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

pub(super) struct StartupPipelines {
    format: wgpu::TextureFormat,
    worker: Option<PipelineWorker>,
}

impl WgpuRenderer {
    /// Shared player/editor preparation; offline baking retains the synchronous
    /// constructor. At most one world/auxiliary pipeline worker exists.
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
            // Loading UI is ready before the driver worker starts.
            self.ensure_ui_pipeline(self.config.format);
            StartupPipelines {
                format: self.config.format,
                worker: None,
            }
        };
        if let Some(worker) = pending.worker.as_ref() {
            match worker.receive.try_recv() {
                Ok(prepared) => {
                    if pending.format != self.config.format {
                        // Retire this completed job before another starts. No stale
                        // format or cache-dependent binding can be published.
                        drop(pending.worker.take());
                        pending.format = self.config.format;
                        self.startup_pipelines = Some(pending);
                        return Ok(false);
                    }
                    prepared.auxiliary.install(self);
                    prepared.world.install(self);
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
            }
        } else {
            pending.format = self.config.format;
            let device = self.device.clone();
            let textures = self.textures.layout().clone();
            let materials = self.material_layout.clone();
            let environment = self.environment_layout.clone();
            let post = self.post.as_ref().map(PostProcess::preparation_layouts);
            let format = pending.format;
            let (send, receive) = std::sync::mpsc::channel();
            let worker = std::thread::Builder::new()
                .name("places-world-pipelines".into())
                .spawn(move || {
                    let started = std::time::Instant::now();
                    let auxiliary = AuxiliaryPipelines::new(&device, format, &textures, post);
                    let world =
                        WorldPipelines::new(&device, format, &textures, &materials, &environment);
                    crate::engine::logging::info(format!(
                        "[loading-shader-worker] world_auxiliary_pipelines_ms={:.3}",
                        started.elapsed().as_secs_f64() * 1_000.0_f64
                    ));
                    let _delivered = send.send(PreparedPipelines { world, auxiliary });
                })
                .map_err(|error| format!("Cannot prepare world pipelines: {error}"))?;
            pending.worker = Some(PipelineWorker {
                receive,
                thread: Some(worker),
            });
        }
        self.startup_pipelines = Some(pending);
        Ok(false)
    }

    /// Pump native events until non-cancellable driver work finishes, then join.
    pub(crate) fn retire_startup_pipelines(&mut self) -> bool {
        let finished = self.startup_pipelines.as_ref().is_none_or(|pending| {
            pending.worker.as_ref().is_none_or(|worker| {
                worker
                    .thread
                    .as_ref()
                    .is_none_or(std::thread::JoinHandle::is_finished)
            })
        });
        if finished {
            drop(self.startup_pipelines.take());
        }
        let character_finished = self
            .character_pipeline_job
            .as_ref()
            .is_none_or(CharacterPipelineWorker::is_finished);
        if character_finished {
            drop(self.character_pipeline_job.take());
        }
        finished && character_finished
    }

    pub(super) fn prepare_all_auxiliary(&mut self) {
        AuxiliaryPipelines::new(
            &self.device,
            self.config.format,
            self.textures.layout(),
            self.post.as_ref().map(PostProcess::preparation_layouts),
        )
        .install(self);
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
