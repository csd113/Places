//! Place two cabins and link them. Detailed shaft authoring remains optional.
mod links;
use super::{
    App,
    actions::Popup,
    document::Document,
    ui::{self, Action, Hit},
};
use crate::engine::{
    elevator::ElevatorDef,
    render::{DrawableSize, Vertex},
};
use glam::Vec3;
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
    mouse::MouseButton,
    video::Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Place,
    Select(usize),
    Link,
    Level,
    NextLevel,
    NextArrival,
    ApplyLevel,
    Name,
    Advanced,
}
#[derive(Clone, Copy)]
pub(super) enum Mode {
    Place,
    Link(usize),
}
#[derive(Default)]
pub(super) struct State {
    pub mode: Option<Mode>,
    selected: Option<usize>,
    rotation: f32,
    external: bool,
    level: usize,
    arrival: usize,
    name: Option<String>,
    all: bool,
}
fn title(lift: &ElevatorDef) -> &str {
    lift.stops
        .get(lift.initial_stop)
        .map_or(&lift.id, |stop| stop.label.as_str())
}

impl App {
    pub(super) fn elevator_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Elevator(command) = action else {
            self.elevators.name = None;
            if self.elevators.mode.take().is_some() {
                self.viewport.highlight(None, &[]);
            }
            return Ok(false);
        };
        let _document = self.document.as_ref().ok_or("Open a project first")?;
        if command != Command::Name {
            self.elevators.name = None;
        }
        match command {
            Command::Place => {
                self.cancel_placement();
                self.popup = None;
                self.elevators.mode = Some(Mode::Place);
                self.status="Click the floor where the elevator doors should go. R rotates; Esc cancels. Click a cabin to select it.".into();
            }
            Command::Select(index) => {
                self.elevators.selected = Some(index);
                self.elevators.external = false;
                self.popup = Some(Popup::Elevators);
            }
            Command::Link => {
                let source = self.elevators.selected.ok_or("Select an elevator")?;
                self.popup = None;
                self.elevators.mode = Some(Mode::Link(source));
                self.status="Click the other elevator to link them. Existing groups stay separate until you link them. Esc cancels.".into();
            }
            Command::Level => {
                self.elevators.external = true;
                self.elevators.level = 0;
                self.elevators.arrival = 0;
            }
            Command::NextLevel => {
                self.elevators.level = self.elevators.level.saturating_add(1);
                self.elevators.arrival = 0;
            }
            Command::NextArrival => {
                self.elevators.arrival = self.elevators.arrival.saturating_add(1);
            }
            Command::ApplyLevel => {
                let doc = self.document.as_ref().ok_or("Open a project")?;
                let index = self.elevators.selected.ok_or("Select an elevator")?;
                let (level, target) = external_target(doc, &self.elevators)?;
                let raw = links::external(doc, index, &level, &target)?;
                self.apply_elevator_edit(raw)?;
                self.elevators.external = false;
                self.popup = Some(Popup::Elevators);
                self.status =
                    "Level destination added. Save and export the complete project to play it."
                        .into();
            }
            Command::Name => {
                let lift = self
                    .document
                    .as_ref()
                    .and_then(|d| {
                        d.level()
                            .elevators
                            .get(self.elevators.selected.unwrap_or(usize::MAX))
                    })
                    .ok_or("Select an elevator")?;
                self.elevators.name = Some(title(lift).into());
                self.elevators.all = true;
            }
            Command::Advanced => {
                let index = self.elevators.selected.ok_or("Select an elevator")?;
                let _handled = self.authoring_action(Action::Author(
                    super::authoring::Command::EditElevator(index),
                ))?;
            }
        }
        Ok(true)
    }
    fn apply_elevator_edit(&mut self, raw: serde_json::Value) -> Result<(), String> {
        self.check_focused_edit(&raw)?;
        let doc = self.document.as_mut().ok_or("Open a project")?;
        let expected = doc.raw().clone();
        doc.apply_authoring_edit(&expected, raw)?;
        self.elevators.mode = None;
        self.viewport.highlight(None, &[]);
        self.changed();
        Ok(())
    }
    fn elevator_click(&mut self, point: [f32; 2]) -> Result<(), String> {
        let doc = self.document.as_ref().ok_or("Open a project")?;
        let (origin, direction) = self
            .viewport
            .ray(point)
            .ok_or("Click a floor or elevator")?;
        let at = place_point(self, point);
        let picked = pick(doc, origin, direction)
            .filter(|(_, distance)| {
                at.is_none_or(|floor| *distance <= origin.distance(Vec3::from_array(floor)) + 0.05)
            })
            .map(|(index, _)| index);
        match self.elevators.mode {
            Some(Mode::Place) => {
                if let Some(index) = picked {
                    self.elevators.selected = Some(index);
                    self.elevators.mode = None;
                } else {
                    let position = at.ok_or("Click a visible floor")?;
                    let index = doc.level().elevators.len();
                    let raw = links::place(doc, position, self.elevators.rotation)?;
                    self.apply_elevator_edit(raw)?;
                    self.elevators.selected = Some(index);
                    self.status =
                        "Elevator placed. Place another, then Link them. Save with Cmd/Ctrl+S."
                            .into();
                }
            }
            Some(Mode::Link(source)) => {
                let target = picked.ok_or("Click an elevator to link")?;
                let raw = links::connect(doc, source, target)?;
                self.apply_elevator_edit(raw)?;
                self.status="Elevators linked both ways. Each independent group keeps its own destinations.".into();
            }
            None => return Ok(()),
        }
        self.elevators.external = false;
        self.popup = Some(Popup::Elevators);
        Ok(())
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "This tool owns only pointer, rotate, Escape and focus-loss events."
    )]
    pub(super) fn elevator_event(&mut self, event: &Event, window: &Window) -> bool {
        if self.elevators.name.is_some() && matches!(self.popup, Some(Popup::Elevators)) {
            return self.elevator_name_event(event);
        }
        if self.elevators.mode.is_none() || self.popup.is_some() {
            return false;
        }
        let size = window.size_in_pixels();
        let convert = |x, y| {
            super::layout::window_to_ui(DrawableSize::new(size.0, size.1), window.size(), [x, y])
        };
        match event {
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            }
            | Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.elevators.mode = None;
                self.viewport.highlight(None, &[]);
                self.status = "Elevator placement cancelled; completed edits are kept.".into();
                true
            }
            Event::KeyDown {
                keycode: Some(Keycode::R),
                repeat: false,
                ..
            } => {
                self.elevators.rotation = (self.elevators.rotation + 90.0).rem_euclid(360.0);
                true
            }
            Event::MouseMotion { x, y, .. } => {
                if let Some(point) = convert(*x, *y).filter(|p| self.viewport.contains_ui(*p))
                    && let Some(at) = place_point(self, point)
                {
                    let def = ElevatorDef {
                        id: String::new(),
                        x: at[0],
                        z: at[2],
                        rotation_degrees: self.elevators.rotation,
                        initial_stop: 0,
                        speed_mps: 1.5,
                        stops: Vec::new(),
                    };
                    let centre = def.point(0.0, at[1] + 1.3, 1.25);
                    self.viewport.highlight(
                        Some(crate::engine::spatial::Aabb {
                            min: [centre.x - 1.5, centre.y - 1.3, centre.z - 1.5],
                            max: [centre.x + 1.5, centre.y + 1.3, centre.z + 1.5],
                        }),
                        &[],
                    );
                    return true;
                }
                false
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => convert(*x, *y).is_some_and(|p| self.viewport.contains_ui(p)),
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => convert(*x, *y)
                .filter(|p| self.viewport.contains_ui(*p))
                .is_some_and(|point| {
                    let result = self.elevator_click(point);
                    self.report(result);
                    true
                }),
            _ => false,
        }
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Name editing consumes only text and editing keys; other SDL events pass through."
    )]
    fn elevator_name_event(&mut self, event: &Event) -> bool {
        match event {
            Event::TextInput { text, .. } => {
                if let Some(name) = &mut self.elevators.name {
                    if self.elevators.all {
                        name.clear();
                        self.elevators.all = false;
                    }
                    if name.len().saturating_add(text.len()) <= 48
                        && !text.chars().any(char::is_control)
                    {
                        name.push_str(text);
                    }
                }
                true
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                ..
            } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD) => {
                self.elevators.all = true;
                true
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                ..
            } => {
                if let Some(name) = &mut self.elevators.name {
                    if self.elevators.all {
                        name.clear();
                        self.elevators.all = false;
                    } else {
                        let _removed = name.pop();
                    }
                }
                true
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return),
                ..
            } => {
                let result = self
                    .document
                    .as_ref()
                    .ok_or_else(|| "Open a project".to_string())
                    .and_then(|d| {
                        links::rename(
                            d,
                            self.elevators.selected.ok_or("Select a lift")?,
                            self.elevators.name.as_deref().unwrap_or(""),
                        )
                    })
                    .and_then(|raw| self.apply_elevator_edit(raw));
                self.elevators.name = None;
                self.report(result);
                true
            }
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => {
                self.elevators.name = None;
                true
            }
            _ => false,
        }
    }
}

fn place_point(app: &App, point: [f32; 2]) -> Option<[f32; 3]> {
    let doc = app.document.as_ref()?;
    let (origin, direction) = app.viewport.ray(point)?;
    let mut choices = Vec::new();
    for room in &doc.level().rooms {
        let Some(at) = super::construction::floor_point(app, point, room.floor_y) else {
            continue;
        };
        if at[0] >= room.x - 0.01
            && at[0] <= room.x + room.width + 0.01
            && at[2] >= room.z - 0.01
            && at[2] <= room.z + room.depth + 0.01
        {
            choices.push((Vec3::from_array(at).distance_squared(origin), at));
        }
    }
    let mut at = choices
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, at)| at)
        .or_else(|| {
            super::construction::floor_point(
                app,
                point,
                app.selected
                    .and_then(|id| {
                        if let super::document::ObjectId::Room(i) = id {
                            doc.level().rooms.get(i).map(|r| r.floor_y)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0.0),
            )
        })?;
    if direction.y.abs() < 0.0001 {
        return None;
    }
    if app.view.snap {
        at[0] = (at[0] / app.step).round() * app.step;
        at[2] = (at[2] / app.step).round() * app.step;
    }
    Some(at)
}
fn pick(doc: &Document, origin: Vec3, direction: Vec3) -> Option<(usize, f32)> {
    doc.level()
        .elevators
        .iter()
        .enumerate()
        .filter_map(|(index, lift)| {
            let local = lift.local(origin);
            let end = lift.local(Vec3::new(
                origin.x + direction.x,
                origin.y + direction.y,
                origin.z + direction.z,
            ));
            let step = Vec3::new(end.x - local.x, end.y - local.y, end.z - local.z);
            let y = lift.stops.get(lift.initial_stop)?.y;
            let low = Vec3::new(-1.45, y, -0.17);
            let high = Vec3::new(1.45, y + 2.64, 2.48);
            let mut near = 0.0_f32;
            let mut far = 500.0_f32;
            for ((value, delta), (a, b)) in local
                .to_array()
                .into_iter()
                .zip(step.to_array())
                .zip(low.to_array().into_iter().zip(high.to_array()))
            {
                if delta.abs() < 1e-6 {
                    if value < a || value > b {
                        return None;
                    }
                } else {
                    let first = (a - value) / delta;
                    let last = (b - value) / delta;
                    near = near.max(first.min(last));
                    far = far.min(first.max(last));
                }
            }
            (near <= far).then_some((index, near))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}
fn external_target(doc: &Document, state: &State) -> Result<(String, ElevatorDef), String> {
    let levels: Vec<_> = doc.level_ids().filter(|id| *id != doc.level_id()).collect();
    let level = levels
        .get(state.level.checked_rem(levels.len()).unwrap_or(0))
        .ok_or("Add another level to the project first")?;
    let (_, _, target) = doc.project().read_level(Some(level))?;
    let lift = target
        .elevators
        .get(
            state
                .arrival
                .checked_rem(target.elevators.len())
                .unwrap_or(0),
        )
        .ok_or("Place and save an elevator in that level first")?;
    Ok(((*level).into(), lift.clone()))
}

pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.65]);
    ui::rect(v, [116.0, 40.0, 354.0, 197.0], [0.075, 0.11, 0.14, 1.0]);
    ui::text(v, "ELEVATORS / PLACE AND LINK", 126.0, 49.0, ui::INK, 85);
    if app.elevators.external {
        draw_external(app, v, hits);
    } else {
        draw_main(app, v, hits);
    }
    ui::popup_button(
        v,
        hits,
        [350.0, 218.0, 105.0, 13.0],
        "Close",
        Action::ClosePopup,
    );
}
fn draw_main(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (y, label, command) in [
        (70.0, "Place another elevator", Command::Place),
        (92.0, "Link selected elevator...", Command::Link),
        (114.0, "Load another level...", Command::Level),
    ] {
        ui::popup_button(
            v,
            hits,
            [126.0, y, 157.0, 16.0],
            label,
            Action::Elevator(command),
        );
    }
    let Some(doc) = &app.document else { return };
    ui::text(v, "SELECT AN ELEVATOR", 296.0, 65.0, ui::INK, 40);
    for (index, lift) in doc.level().elevators.iter().enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(15.0, 78.0);
        ui::popup_button(
            v,
            hits,
            [296.0, y, 160.0, 13.0],
            &format!(
                "{}{}",
                if Some(index) == app.elevators.selected {
                    "> "
                } else {
                    ""
                },
                title(lift)
            ),
            Action::Elevator(Command::Select(index)),
        );
    }
    if let Some(lift) = app
        .elevators
        .selected
        .and_then(|i| doc.level().elevators.get(i))
    {
        ui::text(v, "Name (optional)", 126.0, 142.0, ui::INK, 40);
        ui::popup_button(
            v,
            hits,
            [126.0, 153.0, 157.0, 16.0],
            app.elevators.name.as_deref().unwrap_or_else(|| title(lift)),
            Action::Elevator(Command::Name),
        );
    }
    if let Some(lift) = app
        .elevators
        .selected
        .and_then(|index| doc.level().elevators.get(index))
    {
        for (row, (_, stop)) in lift
            .stops
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != lift.initial_stop)
            .take(2)
            .enumerate()
        {
            ui::text(
                v,
                &format!("To: {}", stop.label),
                126.0,
                f32::from(u16::try_from(row).unwrap_or(0)).mul_add(12.0, 176.0),
                ui::INK,
                43,
            );
        }
    }
    ui::popup_button(
        v,
        hits,
        [126.0, 218.0, 157.0, 13.0],
        "Advanced settings...",
        Action::Elevator(Command::Advanced),
    );
    ui::text(
        v,
        "Place cabins, then Link. Max 3 stops / 2 destination buttons.",
        126.0,
        204.0,
        ui::INK,
        90,
    );
}
fn draw_external(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "1. Place and save a cabin in the other level first.",
        126.0,
        70.0,
        ui::INK,
        85,
    );
    let target = app
        .document
        .as_ref()
        .ok_or_else(|| "Open a project".to_string())
        .and_then(|d| external_target(d, &app.elevators));
    let (level, arrival) = target.as_ref().map_or(
        ("Choose project level", "Choose arrival elevator"),
        |(id, lift)| (id.as_str(), title(lift)),
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 90.0, 330.0, 17.0],
        &format!("Level: {level}  / click to change"),
        Action::Elevator(Command::NextLevel),
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 114.0, 330.0, 17.0],
        &format!("Arrival: {arrival}  / click to change"),
        Action::Elevator(Command::NextArrival),
    );
    ui::popup_button(
        v,
        hits,
        [126.0, 147.0, 330.0, 18.0],
        "Add level destination",
        Action::Elevator(Command::ApplyLevel),
    );
    ui::text(
        v,
        target.as_ref().err().map_or(
            "Save, then export the complete project to play.",
            String::as_str,
        ),
        126.0,
        183.0,
        ui::INK,
        90,
    );
}

pub(super) fn remove_candidate(doc: &Document, index: usize) -> Result<serde_json::Value, String> {
    links::remove(doc, index)
}

#[cfg(test)]
#[path = "tests/elevator_links.rs"]
mod tests;
