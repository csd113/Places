//! Retained camera-owned meshes with private depth, no collider/shadow/reflection.
use super::super as gpu;
use super::{DepthTarget, WgpuRenderer};
use crate::engine::{
    quality::QualityLevel,
    render::common::{
        dynamic::{DynamicId, DynamicScene},
        view::DrawableSize,
    },
    tools::{CROWBAR_MODEL, TOOL_ARM_MODEL, ToolViewPose},
};
use gpu::{
    dynamic::{DynamicUploadContext, WgpuDynamic},
    world::{WorldDrawTotals, WorldEncodeInputs, WorldFrame},
};

pub(super) struct ToolView {
    scene: DynamicScene,
    ids: [DynamicId; 2],
    gpu: WgpuDynamic,
    quality: QualityLevel,
    tint: [u32; 3],
    visible: bool,
    depth: Option<(DrawableSize, DepthTarget)>,
}

impl WgpuRenderer {
    pub(super) fn clear_actor_scenes(&mut self) {
        self.characters = crate::engine::render::common::character::CharacterScene::new();
        self.dynamic = DynamicScene::new();
        self.tool_view = None;
    }
    /// # Errors
    /// Reports invalid/missing installed meshes before showing a partial pair.
    pub fn set_tool_view(&mut self, requested: Option<ToolViewPose>) -> Result<(), String> {
        let Some(pose) = requested else {
            if let Some(view) = self.tool_view.as_mut() {
                view.visible = false;
            }
            return Ok(());
        };
        if !pose.grip.is_finite()
            || !pose.rotation.is_finite()
            || !pose.eye.is_finite()
            || pose.rotation.length_squared() < 1.0e-8
            || !pose
                .arm_tint
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return Err("Invalid first-person tool pose".into());
        }
        let mut view = match self
            .tool_view
            .take()
            .filter(|view| view.quality == self.quality)
        {
            Some(view) => view,
            None => self.create_tool_view(pose.arm_tint)?,
        };
        for id in view.ids {
            let _posed = view
                .scene
                .set_pose(id, pose.grip.to_array(), pose.rotation.to_array());
            let _receiver = view
                .scene
                .set_lighting_receiver(id, Some(pose.eye.to_array()), None);
        }
        if view.tint != pose.arm_tint.map(f32::to_bits) {
            let _tinted = view.scene.set_albedo_tint(view.ids[0], pose.arm_tint);
            view.gpu = self.upload_tool_view(&view.scene, Some(view.gpu));
            view.tint = pose.arm_tint.map(f32::to_bits);
        }
        let _lighting = view.scene.update_with_field(
            0.0,
            self.dynamic_lighting.as_ref(),
            self.dynamic_field.as_deref(),
        );
        let environment = self.level_environment();
        view.gpu
            .sync_with_blacklight(&self.queue, &view.scene, &environment, None, None);
        view.visible = true;
        self.tool_view = Some(view);
        Ok(())
    }

    fn create_tool_view(&mut self, tint: [f32; 3]) -> Result<ToolView, String> {
        let mut scene = DynamicScene::new();
        let arm = self.prop_assets.resolve(TOOL_ARM_MODEL)?;
        let bar = self.prop_assets.resolve(CROWBAR_MODEL)?;
        let arm_id = scene
            .spawn(&arm, [0.0; 3], 0.0, 1.0, 0.0)
            .ok_or("First-person arm has no drawable mesh")?;
        let bar_id = scene
            .spawn(&bar, [0.0; 3], 0.0, 1.0, 0.0)
            .ok_or("Crowbar has no drawable mesh")?;
        let _tinted = scene.set_albedo_tint(arm_id, tint);
        let gpu = self.upload_tool_view(&scene, None);
        Ok(ToolView {
            scene,
            ids: [arm_id, bar_id],
            gpu,
            quality: self.quality,
            tint: tint.map(f32::to_bits),
            visible: false,
            depth: None,
        })
    }

    fn upload_tool_view(
        &mut self,
        scene: &DynamicScene,
        previous: Option<WgpuDynamic>,
    ) -> WgpuDynamic {
        let probes: Vec<_> = self
            .reflection_targets
            .probes()
            .iter()
            .map(gpu::reflections::ProbeCube::view)
            .collect();
        let environment = self.level_environment();
        WgpuDynamic::upload(
            &mut DynamicUploadContext {
                device: &self.device,
                queue: &self.queue,
                cache: &mut self.textures,
                material_layout: &self.material_layout,
                environment_layout: &self.environment_layout,
                caustics: &self.caustics,
                lightmaps: &self.lightmaps,
                environment,
                probes: &probes,
                planar: &self.planar_fallback_view,
                probe_fallback: &self.probe_fallback_view,
                planar_fallback: &self.planar_fallback_view,
                level: self.quality,
                lighting_level: self.lighting_quality,
            },
            scene,
            previous,
        )
    }

    pub(super) fn prepare_tool_depth(&mut self) {
        let requested = self
            .post
            .as_ref()
            .and_then(gpu::postprocess::PostProcess::scene_size);
        if let (Some(view), Some(size)) = (self.tool_view.as_mut(), requested)
            && view.visible
            && view.depth.as_ref().is_none_or(|(old, _)| *old != size)
        {
            view.depth = Some((size, Self::create_depth(&self.device, size)));
        }
    }

    pub(super) fn encode_tool_view(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        post: &gpu::postprocess::PostProcess,
        frame: &WorldFrame,
    ) -> WorldDrawTotals {
        let (
            Some(view),
            Some(target),
            Some(pipeline),
            Some(world),
            Some(textures),
            Some(materials),
            Some(environment),
        ) = (
            self.tool_view.as_ref().filter(|view| view.visible),
            post.scene_view(),
            self.scene_pipeline.as_ref(),
            self.world.as_ref(),
            self.world_textures.as_ref(),
            self.world_materials.as_ref(),
            self.environment.as_ref(),
        )
        else {
            return WorldDrawTotals::default();
        };
        let Some((_, depth)) = view.depth.as_ref() else {
            return WorldDrawTotals::default();
        };
        let tool_frame = (*frame).with_occlusion(None, false);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("places-first-person-tool"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pipeline.encode_view_tool(
            &mut pass,
            WorldEncodeInputs {
                water: false,
                geometry: world,
                textures,
                materials,
                environment: environment.bind_group_for_probe(self.active_probe),
                props: None,
                dynamic: Some(&view.gpu),
                characters: None,
                capture_plane: None,
                filtering: self.filtering,
                frame: &tool_frame,
                cull: false,
            },
            &view.gpu,
        )
    }
}
