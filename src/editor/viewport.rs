//! The editor's shared-engine viewport and its camera/input policy.

mod camera;
mod overlay;
mod picking;
mod preview;

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

enum PreviewMode {
    Inspect,
    Authored,
}

pub(super) struct Viewport {
    renderer: Renderer,
    worker: PreviewWorker,
    camera: Camera,
    objects: Vec<PickObject>,
    volume_guides: Vec<overlay::VolumeGuide>,
    selected: Option<ObjectId>,
    highlight_bounds: Option<Aabb>,
    highlight_surfaces: Vec<[[f32; 3]; 3]>,
    size: DrawableSize,
    window_size: (u32, u32),
    drag: Drag,
    movement: Vec<Keycode>,
    show_grid: bool,
    preview_mode: PreviewMode,
    uploading: bool,
    frame_next: bool,
    last_frame: Instant,
}

impl Viewport {
    pub fn new(window: &Window) -> Result<Self, String> {
        let mut renderer = Renderer::new(window)?;
        renderer.set_quality(QualityLevel::High);
        renderer.set_lighting_quality(QualityLevel::Low);
        renderer.set_lightmap_quality(LightmapQuality::Off);
        renderer.set_reflection_quality(ReflectionQuality::Off);
        renderer.set_bloom_enabled(false);
        let _swap_interval = renderer.set_swap_interval(true);
        let (width, height) = window.size_in_pixels();
        Ok(Self {
            renderer,
            worker: PreviewWorker::new()?,
            camera: Camera::default(),
            objects: Vec::new(),
            volume_guides: Vec::new(),
            selected: None,
            highlight_bounds: None,
            highlight_surfaces: Vec::new(),
            size: DrawableSize::new(width, height),
            window_size: window.size(),
            drag: Drag::Idle,
            movement: Vec::new(),
            show_grid: true,
            preview_mode: PreviewMode::Inspect,
            uploading: false,
            frame_next: true,
            last_frame: Instant::now(),
        })
    }

    pub fn rebuild(&mut self, level: &LevelDef) {
        self.volume_guides = overlay::volumes(level);
        self.worker.request(level, self.environment_preview());
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
        self.renderer.fatal_error().or_else(|| self.worker.error())
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

    fn selected_bounds(&self) -> Option<Aabb> {
        self.objects
            .iter()
            .find(|object| Some(object.id) == self.selected)
            .map(|object| object.bounds)
    }

    pub fn pick(&self, point: [f32; 2]) -> Option<ObjectId> {
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

    pub fn draw(&mut self, window: &Window, ui: &[Vertex]) -> bool {
        let (width, height) = window.size_in_pixels();
        self.size = DrawableSize::new(width, height);
        self.window_size = window.size();
        let _resized = self.renderer.set_drawable_size(self.size);
        if let Some(preview) = self.worker.poll() {
            self.objects = preview.objects;
            if self.frame_next {
                self.frame_next = false;
                self.frame_all();
            }
            self.renderer.install_prepared_precompiled(
                &preview.loaded,
                preview.build,
                preview.assets,
                preview.characters,
                false,
                crate::engine::package::world::ProbeCaptures::default(),
            );
            self.uploading = true;
        }
        if self.uploading && self.renderer.advance_prepared_install() {
            self.uploading = false;
        }
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.advance_camera(delta);
        self.renderer.render_scene(self.camera.render_camera());
        let mut guides = overlay::Guides::new(self.camera.matrix(self.size), self.size);
        if self.show_grid {
            guides.grid(self.camera.target);
        }
        for guide in &self.volume_guides {
            guides.volume(guide);
        }
        if let Some(bounds) = self.selected_bounds() {
            guides.selection(bounds);
        }
        if let Some(bounds) = self.highlight_bounds {
            guides.diagnostic(bounds, &self.highlight_surfaces);
        }
        let mut vertices = guides.finish();
        vertices.extend_from_slice(ui);
        self.renderer.render_ui(&vertices);
        self.renderer.present(window)
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
        self.size
            .window_to_ui(self.window_size, [x, y])
            .is_some_and(|point| {
                (112.0..480.0).contains(&point[0]) && (38.0..242.0).contains(&point[1])
            })
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
