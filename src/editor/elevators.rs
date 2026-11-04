//! Place two cabins and link them. Detailed shaft authoring remains optional.
mod links;
mod placement;
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
                self.status="Point at a wall or its doorway to fit the elevator automatically. R rotates free placement; Esc cancels.".into();
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
        let wall_distance =
            wall_under_pointer(self, origin, direction).map(|(_, distance)| distance);
        let picked = pick(doc, origin, direction)
            .filter(|(_, distance)| {
                wall_distance.is_none_or(|wall| *distance <= wall + 0.05)
                    && at.is_none_or(|floor| {
                        *distance <= origin.distance(Vec3::from_array(floor)) + 0.05
                    })
            })
            .map(|(index, _)| index);
        match self.elevators.mode {
            Some(Mode::Place) => {
                if let Some(index) = picked {
                    self.elevators.selected = Some(index);
                    self.elevators.mode = None;
                } else {
                    let fitted = placement_at(self, point)?;
                    let index = doc.level().elevators.len();
                    let raw = links::place(doc, &fitted)?;
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
                let Some(point) = convert(*x, *y).filter(|p| self.viewport.contains_ui(*p)) else {
                    return false;
                };
                match placement_at(self, point) {
                    Ok(fitted) => {
                        let def = fitted.definition();
                        let centre = def.point(0.0, fitted.at[1] + 1.315, 1.249);
                        let (sin, cos) = fitted.rotation.to_radians().sin_cos();
                        let dx = cos.abs().mul_add(1.45, sin.abs() * 1.231);
                        let dz = sin.abs().mul_add(1.45, cos.abs() * 1.231);
                        self.viewport.highlight(
                            Some(crate::engine::spatial::Aabb {
                                min: [centre.x - dx, fitted.at[1], centre.z - dz],
                                max: [centre.x + dx, fitted.at[1] + 2.63, centre.z + dz],
                            }),
                            &[],
                        );
                        self.status = if fitted.host.is_some() {
                            "Fits flush to this wall. Click to place the elevator and doorway."
                                .into()
                        } else {
                            "Space available. Click to place; R rotates; Esc cancels.".into()
                        };
                    }
                    Err(error) => {
                        self.viewport.highlight(None, &[]);
                        self.status = error;
                    }
                }
                true
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

// Include an opening's empty aperture in wall picking, so a horizontal ray
// through an existing doorway still fits its host rather than a distant wall.
fn wall_under_pointer(app: &App, origin: Vec3, direction: Vec3) -> Option<(usize, f32)> {
    let doc = app.document.as_ref()?;
    wall_under_ray(
        doc.level(),
        &app.focus,
        origin,
        direction,
        app.viewport.floor_distance(origin, direction),
    )
}

fn wall_under_ray(
    level: &crate::engine::level::LevelDef,
    focus: &super::focus::Focus,
    origin: Vec3,
    direction: Vec3,
    floor_distance: Option<f32>,
) -> Option<(usize, f32)> {
    // Use the viewport's emitted terrain triangles, including recesses/ramps.
    // The free-placement fallback plane is not geometry and cannot occlude walls.
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    level
        .walls
        .iter()
        .enumerate()
        .filter(|(index, _)| focus.visible(super::document::ObjectId::Wall(*index)))
        .filter_map(|(index, wall)| {
            let (x, z) = wall.min_corner();
            let (minimum, source, delta) = match wall.axis() {
                crate::engine::level::WallAxis::X => (z, origin.z, direction.z),
                crate::engine::level::WallAxis::Z => (x, origin.x, direction.x),
            };
            let face = if source <= wall.thickness().mul_add(0.5, minimum) {
                minimum
            } else {
                minimum + wall.thickness()
            };
            let distance = (face - source) / delta;
            if !distance.is_finite()
                || !(0.0..=500.0).contains(&distance)
                || floor_distance.is_some_and(|floor| distance > floor + 0.0005)
            {
                return None;
            }
            let hit = Vec3::new(
                direction.x.mul_add(distance, origin.x),
                direction.y.mul_add(distance, origin.y),
                direction.z.mul_add(distance, origin.z),
            );
            let along = match wall.axis() {
                crate::engine::level::WallAxis::X => hit.x - x,
                crate::engine::level::WallAxis::Z => hit.z - z,
            };
            let height = wall
                .height
                .unwrap_or_else(|| surfaces.clear_ceiling_height_along(wall, along));
            (along >= 0.0 && along <= wall.length() && hit.y >= wall.y && hit.y <= wall.y + height)
                .then_some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn placement_at(app: &App, point: [f32; 2]) -> Result<placement::Placement, String> {
    let doc = app.document.as_ref().ok_or("Open a project")?;
    let (origin, direction) = app.viewport.ray(point).ok_or("Point at a wall or floor")?;
    let hint = wall_under_pointer(app, origin, direction).map(|(index, _)| index);
    let at = if let Some(wall) = hint.and_then(|index| doc.level().walls.get(index)) {
        let (x, z) = wall.min_corner();
        let (cross, source, delta) = match wall.axis() {
            crate::engine::level::WallAxis::X => {
                (wall.thickness().mul_add(0.5, z), origin.z, direction.z)
            }
            crate::engine::level::WallAxis::Z => {
                (wall.thickness().mul_add(0.5, x), origin.x, direction.x)
            }
        };
        let distance = (cross - source) / delta;
        if distance.is_finite() && (0.0..=500.0).contains(&distance) {
            [
                direction.x.mul_add(distance, origin.x),
                wall.y,
                direction.z.mul_add(distance, origin.z),
            ]
        } else {
            return Err("Face the wall before placing the elevator".into());
        }
    } else {
        place_point(app, point).ok_or("Point at a wall or visible floor")?
    };
    let fitted = placement::resolve(
        doc,
        at,
        app.elevators.rotation,
        Some(origin),
        hint,
        app.view.snap.then_some(app.step),
    )?;
    let raw = links::place(doc, &fitted)?;
    let _level = doc
        .validate_candidate(&raw)
        .map_err(|error| format!("Not enough space for this elevator: {error}"))?;
    Ok(fitted)
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
    let at = choices
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

#[cfg(test)]
#[path = "tests/elevator_placement.rs"]
mod placement_tests;
