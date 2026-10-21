//! Explicit, bounded native acceptance scripts drive the ordinary editor input.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod};
use sdl3::mouse::{MouseButton, MouseState};
use sdl3::video::Window;
use serde::Deserialize;

use super::{App, ui};
use crate::engine::project::read_bounded_file;
use crate::engine::render::DrawableSize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    output: PathBuf,
    actions: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    #[serde(default)]
    delay_ms: u64,
    action: Action,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    /// Clicks the centre of an actually visible hit region, identified by its
    /// diagnostic action name. Hidden controls cannot be invoked this way.
    Control {
        action: String,
    },
    ScrollView {
        lines: i16,
    },
    ScrollScene {
        lines: i16,
    },
    Click {
        x: f32,
        y: f32,
    },
    /// Projects a world point, then sends the same click as a user.
    WorldClick {
        position: [f32; 3],
    },
    Key {
        key: String,
        #[serde(default)]
        command: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        pressed: Option<bool>,
    },
    Mouse {
        button: String,
        pressed: bool,
        x: f32,
        y: f32,
    },
    Motion {
        x: f32,
        y: f32,
        xrel: f32,
        yrel: f32,
    },
    Text {
        text: String,
    },
    Capture {
        name: String,
    },
    Checkpoint {
        name: String,
    },
    WaitIdle,
    Focus,
    Resize {
        width: u16,
        height: u16,
    },
    Close,
}

pub(super) struct Verification {
    script: Script,
    next: usize,
    completed: Instant,
    waiting: Option<Instant>,
    failure: Option<String>,
}

impl Verification {
    pub(super) fn from_path(path: &Path) -> Result<Self, String> {
        let bytes = read_bounded_file(path, 65_536)?;
        let mut script: Script =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if !script.output.is_absolute() || script.actions.is_empty() || script.actions.len() > 256 {
            return Err(
                "Verification needs an absolute output directory and 1..=256 actions".into(),
            );
        }
        for step in &script.actions {
            if step.delay_ms > 30_000 {
                return Err("Verification delay exceeds 30 seconds".into());
            }
            validate(&step.action)?;
        }
        fs::create_dir_all(&script.output).map_err(|error| error.to_string())?;
        script.output = fs::canonicalize(&script.output).map_err(|error| error.to_string())?;
        Ok(Self {
            script,
            next: 0,
            completed: Instant::now(),
            waiting: None,
            failure: None,
        })
    }

    /// Call after polling jobs and successfully drawing the frame. Failures
    /// first cancel/reap owned tools through the ordinary application job loop.
    pub(super) fn advance(&mut self, app: &mut App, window: &mut Window) -> Result<(), String> {
        if self.failure.is_some() {
            if let Some(job) = &mut app.compile {
                job.cancel();
            }
            if let Some(job) = &mut app.play {
                job.stop();
            }
            if app.compile.is_none() && app.play.is_none() {
                return Err(self
                    .failure
                    .take()
                    .unwrap_or_else(|| "Verification failed".into()));
            }
            return Ok(());
        }
        let Some(step) = self.script.actions.get(self.next) else {
            return Ok(());
        };
        if self.completed.elapsed() < Duration::from_millis(step.delay_ms) {
            return Ok(());
        }
        if matches!(step.action, Action::WaitIdle) && busy(app) {
            let began = self.waiting.get_or_insert_with(Instant::now);
            if began.elapsed() >= Duration::from_secs(120) {
                self.failure = Some(format!(
                    "Verification action {} timed out waiting for idle",
                    self.next
                ));
            }
            return Ok(());
        }
        let started = Instant::now();
        if let Err(error) = self.apply(app, window) {
            self.failure = Some(format!("Verification action {}: {error}", self.next));
            return Ok(());
        }
        let timing = serde_json::json!({"step":self.next,"handler_ms":started.elapsed().as_secs_f64()*1_000.0_f64,"wait_ms":self.waiting.map(|start|start.elapsed().as_secs_f64()*1_000.0_f64)});
        let mut timings = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.script.output.join("timings.jsonl"))
            .map_err(|error| error.to_string())?;
        writeln!(timings, "{timing}").map_err(|error| error.to_string())?;
        self.next = self.next.saturating_add(1);
        self.completed = Instant::now();
        self.waiting = None;
        Ok(())
    }

    fn apply(&self, app: &mut App, window: &mut Window) -> Result<(), String> {
        let step = self
            .script
            .actions
            .get(self.next)
            .ok_or("Missing verification action")?;
        match &step.action {
            Action::WorldClick { position } => {
                let point = app
                    .viewport
                    .project(glam::Vec3::from_array(*position))
                    .ok_or("World click is outside the visible scene viewport")?;
                mouse(app, window, MouseButton::Left, false, point)?;
            }
            Action::ScrollScene { lines } => scroll_scene(app, window, *lines, false)?,
            Action::ScrollView { lines } => scroll_scene(app, window, *lines, true)?,
            Action::Control { action } => {
                let (_, controls) = ui::build(app);
                let control = controls
                    .iter()
                    .find(|hit| format!("{:?}", hit.action) == *action)
                    .ok_or_else(|| format!("Visible control {action:?} not found"))?;
                mouse(app, window, MouseButton::Left, false, control.center())?;
            }
            Action::Click { x, y } => mouse(app, window, MouseButton::Left, false, [*x, *y])?,
            Action::Mouse {
                button,
                pressed,
                x,
                y,
            } => mouse(app, window, mouse_button(button)?, *pressed, [*x, *y])?,
            Action::Motion { x, y, xrel, yrel } => {
                let [logical_x, logical_y] = window_point(window, [*x, *y])?;
                let [origin_x, origin_y] = window_point(window, [0.0, 0.0])?;
                let [relative_x, relative_y] = window_point(window, [*xrel, *yrel])?;
                dispatch(
                    app,
                    window,
                    &Event::MouseMotion {
                        timestamp: 0,
                        window_id: window.id(),
                        which: 0,
                        mousestate: MouseState::from_sdl_state(0),
                        x: logical_x,
                        y: logical_y,
                        xrel: relative_x - origin_x,
                        yrel: relative_y - origin_y,
                    },
                );
            }
            Action::Key {
                key,
                command,
                shift,
                pressed,
            } => {
                let code = Keycode::from_name(key).ok_or("Unknown verification key")?;
                let mut modifiers = Mod::NOMOD;
                if *command {
                    modifiers |= Mod::LCTRLMOD;
                }
                if *shift {
                    modifiers |= Mod::LSHIFTMOD;
                }
                if pressed.is_none_or(|edge| edge) {
                    key_event(app, window, code, modifiers, true);
                }
                if pressed.is_none_or(|edge| !edge) {
                    key_event(app, window, code, modifiers, false);
                }
            }
            Action::Text { text } => dispatch(
                app,
                window,
                &Event::TextInput {
                    timestamp: 0,
                    window_id: window.id(),
                    text: text.clone(),
                },
            ),
            Action::Capture { name } => {
                app.viewport
                    .capture(&self.script.output.join(format!("{name}.png")))?;
            }
            Action::Checkpoint { name } => self.checkpoint(app, window, name)?,
            Action::Resize { width, height } => window
                .set_size(u32::from(*width), u32::from(*height))
                .map_err(|error| error.to_string())?,
            Action::Focus => {
                if !window.raise() {
                    return Err("SDL window raise failed".into());
                }
            }
            Action::WaitIdle => {
                if let Some(error) = app.viewport.status() {
                    return Err(error.into());
                }
            }
            Action::Close => dispatch(app, window, &Event::Quit { timestamp: 0 }),
        }
        Ok(())
    }

    fn checkpoint(&self, app: &App, window: &Window, name: &str) -> Result<(), String> {
        let (_, controls) = ui::build(app);
        let value = serde_json::json!({
            "step": self.next, "document": app.document.as_ref().map(super::Document::raw),
            "dirty": app.document.as_ref().is_some_and(super::Document::is_dirty),
            "selected": app.selected.map(|selected| format!("{selected:?}")),
            "status": app.status, "compile": app.compile.is_some(), "play": app.play.is_some(),
            "preparing": app.viewport.is_preparing(), "window": window.size(),
            "viewport_objects": app.viewport.object_count(),
            "environment_preview": app.viewport.environment_preview(),
            "navigation": app.navigation.checkpoint(),
            "visible_controls": controls.iter().map(|hit| serde_json::json!({"action":format!("{:?}",hit.action),"center":hit.center()})).collect::<Vec<_>>(),
            "input_focus": window.has_input_focus(),
            "ui_focus": app.ui_focus.map(|action| format!("{action:?}")),
            "audio_preview": app.preview_audio.stats(),
            "editor_resources":app.document.as_ref().map(super::Document::resource_snapshot),
            "preview_metrics":app.viewport.preview_metrics(),
            "axis_handles":app.viewport.axis_handles(),
            "thumbnail_cache_tiles":app.browser.cached_tiles(),
            "bake_rows":app.bakes.rows.iter().map(|(id,result)|serde_json::json!({"id":id,"report":result.as_ref().ok(),"error":result.as_ref().err()})).collect::<Vec<_>>(),
            "drawable": window.size_in_pixels(), "camera_target": app.viewport.target(),
            "safety": app.safety_report.as_ref().map(|report| report.findings.iter().map(|finding| serde_json::json!({
                "check":finding.check,"title":finding.title,"category":finding.category.label(),
                "severity":finding.severity.label(),"risk":finding.risk,"position":finding.position,"bounds":{"min":finding.bounds.min,"max":finding.bounds.max},
                "objects":finding.objects.iter().map(|object|format!("{object:?}")).collect::<Vec<_>>(),
                "placement_ignore":finding.placement_ignore.as_ref().map(|state|serde_json::json!({"key":state.key,"active":state.active})),
                "surface_count":finding.surfaces.len(),"fix":finding.fix.as_ref().map(|fix|&fix.title)
            })).collect::<Vec<_>>()),
        });
        let bytes = serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?;
        let path = self.script.output.join(format!("{name}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())
    }
}

const fn busy(app: &App) -> bool {
    app.compile.is_some()
        || app.play.is_some()
        || app.viewport.is_preparing()
        || app.bakes.busy()
        || app.safety_job.is_some()
        || app.navigation.is_baking()
}

fn validate(action: &Action) -> Result<(), String> {
    let valid = match action {
        Action::Control { action: name } => {
            // Named fields include bounded target/type descriptors. The complete
            // recipe retains its 64 KiB and 256-action admission limits.
            !name.is_empty() && name.len() <= 512 && !name.chars().any(char::is_control)
        }
        Action::ScrollScene { lines } | Action::ScrollView { lines } => {
            (1..=64).contains(&lines.unsigned_abs())
        }
        Action::Click { x, y } => valid_point(*x, *y),
        Action::WorldClick { position } => position
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 100_000.0),
        Action::Key {
            key,
            command: _,
            shift: _,
            pressed: _,
        } => key.len() <= 32 && Keycode::from_name(key).is_some(),
        Action::Mouse {
            button,
            pressed: _,
            x,
            y,
        } => mouse_button(button).is_ok() && valid_point(*x, *y),
        Action::Motion { x, y, xrel, yrel } => {
            valid_point(*x, *y) && xrel.abs() <= 480.0 && yrel.abs() <= 272.0
        }
        Action::Text { text } => text.len() <= 256,
        Action::Capture { name } | Action::Checkpoint { name } => {
            !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        }
        Action::WaitIdle | Action::Focus | Action::Close => true,
        Action::Resize { width, height } => {
            (960..=2560).contains(width) && (544..=1600).contains(height)
        }
    };
    valid
        .then_some(())
        .ok_or_else(|| "Invalid verification coordinates, key, text or evidence basename".into())
}

fn valid_point(x: f32, y: f32) -> bool {
    (0.0..480.0).contains(&x) && (0.0..272.0).contains(&y)
}

fn scroll_scene(app: &mut App, window: &Window, lines: i16, viewport: bool) -> Result<(), String> {
    let [x, y] = window_point(
        window,
        if viewport {
            [300.0, 140.0]
        } else {
            [50.0, 90.0]
        },
    )?;
    let direction = f32::from(lines.signum());
    for _ in 0..lines.unsigned_abs() {
        dispatch(
            app,
            window,
            &Event::MouseWheel {
                timestamp: 0,
                window_id: window.id(),
                which: 0,
                x: 0.0,
                y: direction,
                direction: sdl3::mouse::MouseWheelDirection::Normal,
                mouse_x: x,
                mouse_y: y,
                integer_x: 0_i32,
                integer_y: i32::from(lines.signum()),
            },
        );
    }
    Ok(())
}

fn dispatch(app: &mut App, window: &Window, event: &Event) {
    let (_, hits) = ui::build(app);
    app.event(event, window, &hits);
}

fn mouse_button(name: &str) -> Result<MouseButton, String> {
    match name {
        "left" => Ok(MouseButton::Left),
        "middle" => Ok(MouseButton::Middle),
        "right" => Ok(MouseButton::Right),
        _ => Err("Unknown verification mouse button".into()),
    }
}

fn mouse(
    app: &mut App,
    window: &Window,
    button: MouseButton,
    pressed: bool,
    point: [f32; 2],
) -> Result<(), String> {
    let [x, y] = window_point(window, point)?;
    let event = if pressed {
        Event::MouseButtonDown {
            timestamp: 0,
            window_id: window.id(),
            which: 0,
            mouse_btn: button,
            clicks: 1,
            x,
            y,
        }
    } else {
        Event::MouseButtonUp {
            timestamp: 0,
            window_id: window.id(),
            which: 0,
            mouse_btn: button,
            clicks: 1,
            x,
            y,
        }
    };
    dispatch(app, window, &event);
    Ok(())
}

fn key_event(app: &mut App, window: &Window, code: Keycode, modifiers: Mod, pressed: bool) {
    let event = if pressed {
        Event::KeyDown {
            timestamp: 0,
            window_id: window.id(),
            keycode: Some(code),
            scancode: None,
            keymod: modifiers,
            repeat: false,
            which: 0,
            raw: 0,
        }
    } else {
        Event::KeyUp {
            timestamp: 0,
            window_id: window.id(),
            keycode: Some(code),
            scancode: None,
            keymod: modifiers,
            repeat: false,
            which: 0,
            raw: 0,
        }
    };
    dispatch(app, window, &event);
}

fn window_point(window: &Window, [x, y]: [f32; 2]) -> Result<[f32; 2], String> {
    let (width, height) = window.size_in_pixels();
    let (logical_width, logical_height) = window.size();
    if width == 0 || height == 0 || logical_width == 0 || logical_height == 0 {
        return Err("Cannot verify clicks on an empty window".into());
    }
    let viewport = DrawableSize::new(width, height).ui_viewport();
    let float = |value: u32| -> Result<f32, String> {
        u16::try_from(value)
            .map(f32::from)
            .map_err(|error| error.to_string())
    };
    let origin_x = float(u32::try_from(viewport.x).map_err(|error| error.to_string())?)?;
    let origin_y = float(u32::try_from(viewport.y).map_err(|error| error.to_string())?)?;
    let fitted_width = float(u32::try_from(viewport.width).map_err(|error| error.to_string())?)?;
    let fitted_height = float(u32::try_from(viewport.height).map_err(|error| error.to_string())?)?;
    Ok([
        (origin_x + x * fitted_width / 480.0) * float(logical_width)? / float(width)?,
        (origin_y + y * fitted_height / 272.0) * float(logical_height)? / float(height)?,
    ])
}
