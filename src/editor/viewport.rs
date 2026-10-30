//! The editor's shared-engine viewport and its camera/input policy.

mod camera;
mod overlay;
mod picking;
pub(super) mod preview;

use std::time::Instant;

use glam::Vec3;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use sdl3::mouse::{MouseButton, MouseWheelDirection};
use sdl3::video::Window;

use camera::Camera;
use picking::PickObject;
use preview::PreviewWorker;

use super::document::ObjectId;
use crate::engine::level::LevelDef;
use crate::engine::quality::{LightmapQuality, QualityLevel, ReflectionQuality};
use crate::engine::render::{DrawableSize, Renderer, Vertex};
use crate::engine::spatial::Aabb;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Drag {
    Idle,
    Orbit,
    Pan,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalPreview {
    Inactive,
    Pending,
    Ready,
    Invalidated,
}

enum PreviewMode {
    Inspect,
    Authored,
}

pub(super) struct Viewport {
    layout: super::layout::Layout,
    focus: super::focus::Focus,
    identity_map: Vec<(ObjectId, ObjectId)>,
    wall_handles: Vec<[[f32; 3]; 2]>,
    renderer: Renderer,
    worker: PreviewWorker,
    camera: Camera,
    objects: Vec<PickObject>,
    play_mesh: Option<std::sync::Arc<crate::engine::render::LevelMesh>>,
    play_floor: crate::engine::level::WalkableFloor,
    play_doors: Vec<crate::engine::collision::DoorCollider>,
    pending_objects: Option<Vec<PickObject>>,
    upload_samples: Vec<f64>,
    volume_guides: Vec<overlay::VolumeGuide>,
    duct_guides: Vec<super::ducts::Guide>,
    room_handles: Vec<[[f32; 3]; 4]>,
    navigation_coverage: Vec<super::navigation::Coverage>,
    navigation_route: Vec<Vec3>,
    navigation_start: Option<Vec3>,
    navigation_goal: Option<Vec3>,
    selected: Option<ObjectId>,
    highlight_bounds: Option<Aabb>,
    highlight_surfaces: Vec<[[f32; 3]; 3]>,
    size: DrawableSize,
    window_size: (u32, u32),
    drag: Drag,
    movement: Vec<Keycode>,
    show_grid: bool,
    preview_mode: PreviewMode,
    local_preview: LocalPreview,
    uploading: bool,
    frame_next: bool,
    last_frame: Instant,
    requested: Option<Instant>,
    preview_metrics: serde_json::Value,
}

impl Viewport {
    pub fn new(window: &Window, performance_test: bool) -> Result<Self, String> {
        let mut renderer = Renderer::new(window)?;
        renderer.set_editor_ui();
        renderer.set_quality(QualityLevel::High);
        renderer.set_lighting_quality(QualityLevel::Low);
        renderer.set_lightmap_quality(LightmapQuality::Off);
        renderer.set_reflection_quality(ReflectionQuality::Off);
        renderer.set_bloom_enabled(false);
        let interval = renderer.set_swap_interval(!performance_test);
        if performance_test && interval != 0_i32 {
            return Err("Editor performance testing requires Immediate presentation, unsupported by this surface".into());
        }
        let (width, height) = window.size_in_pixels();
        Ok(Self {
            layout: super::layout::Layout::for_window(window),
            focus: super::focus::Focus::default(),
            identity_map: Vec::new(),
            wall_handles: Vec::new(),
            renderer,
            worker: PreviewWorker::new()?,
            camera: Camera::default(),
            objects: Vec::new(),
            play_mesh: None,
            play_floor: crate::engine::level::WalkableFloor::default(),
            play_doors: Vec::new(),
            pending_objects: None,
            upload_samples: Vec::new(),
            volume_guides: Vec::new(),
            duct_guides: Vec::new(),
            room_handles: Vec::new(),
            navigation_coverage: Vec::new(),
            navigation_route: Vec::new(),
            navigation_start: None,
            navigation_goal: None,
            selected: None,
            highlight_bounds: None,
            highlight_surfaces: Vec::new(),
            size: DrawableSize::new(width, height),
            window_size: window.size(),
            drag: Drag::Idle,
            movement: Vec::new(),
            show_grid: true,
            preview_mode: PreviewMode::Inspect,
            local_preview: LocalPreview::Inactive,
            uploading: false,
            frame_next: true,
            last_frame: Instant::now(),
            requested: None,
            preview_metrics: serde_json::json!({}),
        })
    }

    pub fn sync_window(&mut self, window: &Window) {
        let (width, height) = window.size_in_pixels();
        self.size = DrawableSize::new(width, height);
        self.window_size = window.size();
        self.layout = super::layout::Layout::for_window(window);
    }
    pub const fn layout(&self) -> super::layout::Layout {
        self.layout
    }
    pub fn contains_ui(&self, point: [f32; 2]) -> bool {
        self.layout.contains(point)
    }
    pub fn set_ui_images(&mut self, images: Vec<crate::engine::render::UiImage>) {
        self.renderer.set_ui_images(images);
    }

    pub const fn set_focus(&mut self, focus: super::focus::Focus) {
        self.focus = focus;
    }
    pub fn rebuild(&mut self, level: &LevelDef) {
        if self.local_preview != LocalPreview::Inactive {
            self.local_preview = LocalPreview::Invalidated;
        }
        self.wall_handles = level
            .walls
            .iter()
            .map(super::document::wall_edit::endpoints)
            .collect();
        self.renderer.cancel_prepared_install();
        self.uploading = false;
        self.pending_objects = None;
        self.play_mesh = None;
        self.requested = Some(Instant::now());

        self.room_handles = level
            .rooms
            .iter()
            .map(|room| super::document::room_resize::Edge::ALL.map(|edge| edge.handle(room)))
            .collect();
        let (preview, identities) = self.focus.preview(level);
        self.volume_guides = overlay::volumes(&preview);
        self.duct_guides = super::ducts::guides(level)
            .into_iter()
            .filter(|guide| self.focus.included(level, guide.id))
            .collect();
        self.identity_map = identities;
        self.worker.request(&preview, self.environment_preview());
    }

    pub fn start_local_preview(&mut self, level: &LevelDef, scope: [f32; 4]) {
        // Scope never inherits isolation: outside lights and blockers remain visible to transport.
        self.set_environment_preview(true);
        self.rebuild(level);
        self.local_preview = LocalPreview::Pending;
        self.worker.request_scoped(level, true, Some(scope));
    }

    pub fn local_progress(&self) -> Option<String> {
        match self.local_preview {
            LocalPreview::Inactive => None,
            LocalPreview::Pending => Some(if self.worker.is_preparing() {
                self.worker.progress.label()
            } else if self.uploading {
                "Uploading preview".into()
            } else {
                "Preview failed / clear and retry / see status".into()
            }),
            LocalPreview::Ready => Some(if self.uploading {
                "Uploading local preview".into()
            } else {
                "Ready / edit invalidates / no final export or cache writes".into()
            }),
            LocalPreview::Invalidated => {
                Some("Stale preview invalidated / refreshing authoring view".into())
            }
        }
    }

    pub const fn environment_preview(&self) -> bool {
        matches!(self.preview_mode, PreviewMode::Authored)
    }

    pub const fn set_environment_preview(&mut self, enabled: bool) {
        self.preview_mode = if enabled {
            PreviewMode::Authored
        } else {
            PreviewMode::Inspect
        };
    }

    pub fn highlight(&mut self, bounds: Option<Aabb>, surfaces: &[[[f32; 3]; 3]]) {
        self.highlight_bounds = bounds;
        self.highlight_surfaces = surfaces.to_vec();
    }

    pub fn navigation_guides(
        &mut self,
        coverage: Vec<super::navigation::Coverage>,
        route: &[Vec3],
        start: Option<Vec3>,
        goal: Option<Vec3>,
    ) {
        self.navigation_coverage = coverage;
        self.navigation_route.clear();
        self.navigation_route.extend_from_slice(route);
        self.navigation_start = start;
        self.navigation_goal = goal;
    }

    pub fn ray(&self, point: [f32; 2]) -> Option<(Vec3, Vec3)> {
        self.camera.ray(point, self.size)
    }

    /// Native verification still dispatches an ordinary screen click.
    pub fn project(&self, position: Vec3) -> Option<[f32; 2]> {
        let guide = overlay::Guides::new(
            self.camera.matrix(self.size),
            self.size,
            self.layout.viewport(),
        );
        guide.project(position)
    }

    pub fn frame_bounds(&mut self, bounds: Aabb) {
        self.camera.frame(bounds);
    }

    pub const fn select(&mut self, selected: Option<ObjectId>) {
        self.selected = selected;
    }

    pub const fn set_grid(&mut self, enabled: bool) {
        self.show_grid = enabled;
    }

    pub const fn target(&self) -> [f32; 3] {
        self.camera.target.to_array()
    }

    pub const fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn status(&self) -> Option<&str> {
        self.renderer
            .fatal_error()
            .or_else(|| self.worker.error())
            .or_else(|| {
                self.preview_metrics
                    .get("admission_error")
                    .and_then(serde_json::Value::as_str)
            })
    }

    pub const fn is_preparing(&self) -> bool {
        self.uploading || self.worker.is_preparing()
    }

    pub fn capture(&mut self, path: &std::path::Path) -> Result<(), String> {
        use std::io::Write;
        let image = self.renderer.capture_default_framebuffer()?;
        let bytes = crate::engine::materials::encode_png(&image)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("Cannot create capture {}: {error}", path.display()))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Cannot write capture {}: {error}", path.display()))
    }

    pub fn frame_selected(&mut self) {
        if let Some(bounds) = self.selected_bounds() {
            self.camera.frame(bounds);
        } else {
            self.frame_all();
        }
    }

    pub fn frame_all(&mut self) {
        if self.worker.is_preparing() {
            self.frame_next = true;
            return;
        }
        let bounds = self
            .objects
            .iter()
            .fold(Aabb::EMPTY, |bounds, object| bounds.union(&object.bounds));
        self.camera.frame(bounds);
    }

    pub const fn frame_next_preview(&mut self) {
        self.frame_next = true;
    }

    pub fn cancel_navigation(&mut self) {
        self.drag = Drag::Idle;
        self.movement.clear();
    }

    pub(super) fn selected_bounds(&self) -> Option<Aabb> {
        self.objects
            .iter()
            .find(|object| Some(object.id) == self.selected)
            .map(|object| object.bounds)
    }

    pub fn verification_view(&mut self, eye: [f32; 3], yaw: f32, pitch: f32) -> Result<(), String> {
        self.camera.set_pose(crate::game::play_spawn::Pose::new(
            Vec3::from_array(eye),
            yaw.to_radians(),
            pitch.to_radians(),
        )?);
        Ok(())
    }

    pub fn play_camera(&self) -> Result<crate::game::play_spawn::Pose, String> {
        let view = self.camera.render_camera();
        crate::game::play_spawn::Pose::new(view.position, self.camera.yaw, self.camera.pitch)
    }

    pub fn play_floor(&self, point: [f32; 2]) -> Result<Vec3, String> {
        if self.is_preparing() {
            return Err("Play here: wait for the current scene preview".into());
        }
        let ray = self.ray(point).ok_or("Play here: no scene ray")?;
        picking::play_floor_hit(
            self.play_mesh
                .as_deref()
                .ok_or("Play here: scene is not ready")?,
            &self.objects,
            &self.play_floor,
            &self.play_doors,
            ray,
        )
        .ok_or_else(|| {
            "Play here: click a visible floor; walls, doors and props block the pick".into()
        })
    }

    pub fn pick(&self, point: [f32; 2]) -> Option<ObjectId> {
        if self.is_preparing() {
            return None;
        }
        let (origin, direction) = self.camera.ray(point, self.size)?;
        self.objects
            .iter()
            .filter_map(|object| {
                object
                    .distance(origin, direction)
                    .map(|distance| (object.id, distance))
            })
            .min_by(|first, second| first.1.total_cmp(&second.1))
            .map(|(id, _)| id)
    }

    pub fn room_edge_handles(&self) -> Vec<[[f32; 2]; 2]> {
        let Some(ObjectId::Room(index)) = self.selected else {
            return Vec::new();
        };
        let Some(handles) = self.room_handles.get(index) else {
            return Vec::new();
        };
        handles
            .iter()
            .zip(super::document::room_resize::Edge::ALL)
            .filter_map(|(point, edge)| {
                let mut end = *point;
                *end.get_mut(edge.axis())? += 1.0;
                Some([
                    self.project(Vec3::from_array(*point))?,
                    self.project(Vec3::from_array(end))?,
                ])
            })
            .collect()
    }
    pub fn wall_endpoint_handles(&self) -> Vec<[f32; 2]> {
        let Some(ObjectId::Wall(i)) = self.selected else {
            return Vec::new();
        };
        self.wall_handles
            .get(i)
            .into_iter()
            .flatten()
            .filter_map(|p| self.project(Vec3::from_array(*p)))
            .collect()
    }
    pub fn axis_handles(&self) -> Vec<[[f32; 2]; 2]> {
        let Some(bounds) = self.selected_bounds() else {
            return Vec::new();
        };
        let centre = Vec3::from_array(bounds.centre());
        [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter()
            .filter_map(|axis| {
                Some([
                    self.project(centre)?,
                    self.project(Vec3::new(
                        centre.x + axis.x,
                        centre.y + axis.y,
                        centre.z + axis.z,
                    ))?,
                ])
            })
            .collect()
    }

    pub const fn presentation_mode(&self) -> &'static str {
        self.renderer.presentation_mode()
    }

    pub fn worker_snapshot(&self) -> serde_json::Value {
        self.worker.resource_snapshot()
    }

    pub const fn preview_metrics(&self) -> &serde_json::Value {
        &self.preview_metrics
    }

    fn start_preview(&mut self, preview: preview::PreparedPreview) -> bool {
        self.renderer
            .set_lightmap_quality(if preview.build.lightmaps.is_some() {
                LightmapQuality::Full
            } else {
                LightmapQuality::Off
            });
        let admission_started = Instant::now();
        if let Err(error) = self.renderer.admit_prepared(
            &preview.loaded,
            &preview.build,
            &preview.assets,
            &preview.geometry,
        ) {
            if let Some(metrics) = self.preview_metrics.as_object_mut() {
                drop(metrics.insert(
                    "admission_error".into(),
                    serde_json::json!(error.to_string()),
                ));
            }
            self.uploading = false;
            return false;
        }
        self.local_preview = if preview
            .metrics
            .get("local_preview")
            .is_some_and(serde_json::Value::is_array)
        {
            LocalPreview::Ready
        } else {
            LocalPreview::Inactive
        };
        let admission_ms = admission_started.elapsed().as_secs_f64() * 1_000.0_f64;
        self.preview_metrics = preview.metrics;
        let setup_started = Instant::now();
        self.play_mesh = Some(std::sync::Arc::clone(&preview.build.mesh));
        self.play_floor = crate::engine::level::WalkableFloor::from_level(&preview.loaded.level);
        self.play_doors = crate::engine::door::Doors::from_level(&preview.loaded.level).colliders();
        self.pending_objects = Some(
            preview
                .objects
                .into_iter()
                .filter_map(|mut object| {
                    object.id = self
                        .identity_map
                        .iter()
                        .find(|(filtered, _)| *filtered == object.id)?
                        .1;
                    Some(object)
                })
                .collect(),
        );
        self.upload_samples.clear();
        self.renderer.install_prepared_precompiled(
            &preview.loaded,
            preview.build,
            preview.assets,
            preview.characters,
            false,
            crate::engine::package::world::ProbeCaptures::default(),
        );
        self.renderer.set_prepared_geometry(preview.geometry);
        self.renderer.set_prepared_caustics(preview.caustics);
        if let Some(metrics) = self.preview_metrics.as_object_mut() {
            drop(metrics.insert("admission_ms".into(), serde_json::json!(admission_ms)));
            drop(metrics.insert(
                "install_setup_ms".into(),
                serde_json::json!(setup_started.elapsed().as_secs_f64() * 1_000.0_f64),
            ));
        }
        self.uploading = true;
        true
    }

    pub fn draw(&mut self, window: &Window, ui: &[Vertex]) -> bool {
        let (width, height) = window.size_in_pixels();
        self.size = DrawableSize::new(width, height);
        self.window_size = window.size();
        self.layout = super::layout::Layout::for_window(window);
        let _resized = self.renderer.set_drawable_size(self.size);
        if let Some(preview) = self.worker.poll()
            && !self.start_preview(preview)
        {
            return false;
        }
        if self.uploading {
            let upload_started = Instant::now();
            let finished = self.renderer.advance_prepared_install();
            let blocked_ms = upload_started.elapsed().as_secs_f64() * 1_000.0_f64;
            if self.upload_samples.len() < 4096 {
                self.upload_samples.push(blocked_ms);
            }
            let old = self
                .preview_metrics
                .get("max_upload_handler_ms")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0_f64);
            if let Some(metrics) = self.preview_metrics.as_object_mut() {
                drop(metrics.insert(
                    "max_upload_handler_ms".into(),
                    serde_json::json!(old.max(blocked_ms)),
                ));
            }
            if finished {
                self.uploading = false;
                if let Some(objects) = self.pending_objects.take() {
                    self.objects = objects;
                }
                if self.frame_next {
                    self.frame_next = false;
                    self.frame_all();
                }
                self.upload_samples.sort_by(f64::total_cmp);
                if let Some(metrics) = self.preview_metrics.as_object_mut() {
                    let index = self
                        .upload_samples
                        .len()
                        .saturating_sub(1)
                        .saturating_mul(95)
                        / 100;
                    drop(metrics.insert(
                        "p95_upload_handler_ms".into(),
                        serde_json::json!(self.upload_samples.get(index)),
                    ));
                    drop(metrics.insert(
                        "upload_handler_samples".into(),
                        serde_json::json!(self.upload_samples.len()),
                    ));
                }
                if let Some(requested) = self.requested.take()
                    && let Some(metrics) = self.preview_metrics.as_object_mut()
                {
                    drop(metrics.insert(
                        "request_to_ready_ms".into(),
                        serde_json::json!(requested.elapsed().as_secs_f64() * 1_000.0_f64),
                    ));
                }
            }
        }
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.advance_camera(delta);
        self.renderer.render_scene(self.camera.render_camera());
        let mut guides = overlay::Guides::new(
            self.camera.matrix(self.size),
            self.size,
            self.layout.viewport(),
        );
        if self.show_grid {
            guides.grid(self.camera.target);
        }
        guides.navigation(
            &self.navigation_coverage,
            &self.navigation_route,
            self.navigation_start,
            self.navigation_goal,
        );
        for guide in &self.volume_guides {
            guides.volume(guide);
        }
        if let Some(bounds) = self.selected_bounds() {
            guides.selection(bounds);
        }
        self.draw_handles(&mut guides);
        if let Some(bounds) = self.highlight_bounds {
            guides.diagnostic(bounds, &self.highlight_surfaces);
        }
        let mut vertices = guides.finish();
        vertices.extend_from_slice(ui);
        self.renderer.render_ui(&vertices);
        self.renderer.present(window)
    }

    fn draw_handles(&self, guides: &mut overlay::Guides) {
        for duct in &self.duct_guides {
            if Some(duct.id) == self.selected {
                guides.duct_ports(&duct.ports);
            }
        }

        if let Some(ObjectId::Room(index)) = self.selected
            && let Some(handles) = self.room_handles.get(index)
        {
            guides.room_edges(handles);
        }
        if let Some(ObjectId::Wall(index)) = self.selected
            && let Some(handles) = self.wall_handles.get(index)
        {
            guides.wall_ends(handles);
        }
    }
    fn advance_camera(&mut self, seconds: f32) {
        let value = |positive, negative| {
            f32::from(u8::from(self.movement.contains(&positive)))
                - f32::from(u8::from(self.movement.contains(&negative)))
        };
        self.camera.fly(
            Vec3::new(
                value(Keycode::D, Keycode::A),
                value(Keycode::E, Keycode::Q),
                value(Keycode::W, Keycode::S),
            ),
            seconds,
        );
    }

    fn within(&self, x: f32, y: f32) -> bool {
        super::layout::window_to_ui(self.size, self.window_size, [x, y])
            .is_some_and(|point| self.layout.contains(point))
    }

    #[expect(
        clippy::rest_pattern_accessible_field,
        reason = "SDL event timestamps, device identities and unused coordinates do not affect editor navigation."
    )]
    pub fn handle_event(&mut self, event: &Event) -> bool {
        if let Event::KeyDown {
            keycode: Some(Keycode::F),
            repeat: false,
            ..
        } = event
        {
            self.frame_selected();
            return true;
        }
        if let Event::MouseButtonDown {
            mouse_btn, x, y, ..
        } = event
            && self.within(*x, *y)
        {
            if *mouse_btn == MouseButton::Right {
                self.drag = Drag::Orbit;
                return true;
            }
            if *mouse_btn == MouseButton::Middle {
                self.drag = Drag::Pan;
                return true;
            }
        }
        if let Event::MouseButtonUp { mouse_btn, .. } = event {
            if *mouse_btn == MouseButton::Right {
                let handled = self.drag == Drag::Orbit;
                if handled {
                    self.drag = Drag::Idle;
                }
                self.movement.clear();
                return handled;
            }
            if *mouse_btn == MouseButton::Middle {
                let handled = self.drag == Drag::Pan;
                if handled {
                    self.drag = Drag::Idle;
                }
                return handled;
            }
        }
        if let Event::MouseMotion { xrel, yrel, .. } = event {
            if self.drag == Drag::Orbit {
                self.camera.orbit([*xrel, *yrel]);
                return true;
            }
            if self.drag == Drag::Pan {
                self.camera.pan([*xrel, *yrel]);
                return true;
            }
        }
        if let Event::MouseWheel {
            y,
            direction,
            mouse_x,
            mouse_y,
            ..
        } = event
            && self.within(*mouse_x, *mouse_y)
        {
            self.camera
                .zoom(if *direction == MouseWheelDirection::Flipped {
                    -*y
                } else {
                    *y
                });
            return true;
        }
        if let Event::KeyDown {
            keycode: Some(key), ..
        } = event
            && self.drag == Drag::Orbit
            && movement_key(*key)
        {
            if !self.movement.contains(key) {
                self.movement.push(*key);
            }
            return true;
        }
        if let Event::KeyUp {
            keycode: Some(key), ..
        } = event
            && self.movement.contains(key)
        {
            self.movement.retain(|pressed| pressed != key);
            return true;
        }
        if let Event::Window {
            win_event: sdl3::event::WindowEvent::FocusLost,
            ..
        } = event
        {
            self.cancel_navigation();
        }
        false
    }
}

const fn movement_key(key: Keycode) -> bool {
    matches!(
        key,
        Keycode::W | Keycode::A | Keycode::S | Keycode::D | Keycode::Q | Keycode::E
    )
}
