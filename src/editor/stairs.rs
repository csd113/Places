//! Two-floor straight stairs with explicit shaft preview and existing editable
//! room/stair records. No hidden geometry, guessed collision or stairwell fill.
use super::{
    App,
    actions::Popup,
    document::{Document, ObjectId},
    ui::{self, Action, Hit},
};
use crate::engine::{
    level::{LevelDef, MIN_STRUCTURE_M},
    render::Vertex,
};
use serde_json::{Value, json};

pub(super) struct Wizard {
    lower: usize,
    upper: usize,
    width: String,
    expected: Value,
    candidate: Option<Value>,
    message: String,
}
impl Wizard {
    pub fn new(app: &App) -> Result<Self, String> {
        let doc = app.document.as_ref().ok_or("Open a project first")?;
        let lower = if let Some(ObjectId::Room(i)) = app.selected {
            i
        } else {
            0
        };
        let upper = doc
            .level()
            .rooms
            .iter()
            .enumerate()
            .find(|(_, r)| {
                doc.level()
                    .rooms
                    .get(lower)
                    .is_some_and(|l| r.floor_y > l.floor_y)
            })
            .map_or(lower, |(i, _)| i);
        Ok(Self {
            lower,
            upper,
            width: "1.2".into(),
            expected: doc.raw().clone(),
            candidate: None,
            message: "Choose floors and width. Preview reviews the shaft and landings.".into(),
        })
    }
}
fn split_room(room: &Value, cut: [f32; 4]) -> Result<Vec<Value>, String> {
    let typed: crate::engine::level::RoomDef =
        serde_json::from_value(room.clone()).map_err(|e| e.to_string())?;
    let (x0, x1, z0, z1) = typed.bounds();
    let [x, z, w, d] = cut;
    let mut result = Vec::new();
    for (left, near, width, depth) in [
        (x0, z0, x - x0, z1 - z0),
        (x + w, z0, x1 - x - w, z1 - z0),
        (x, z0, w, z - z0),
        (x, z + d, w, z1 - z - d),
    ] {
        if width <= 0.0001 || depth <= 0.0001 {
            continue;
        }
        let mut strip = room.clone();
        let object = strip.as_object_mut().ok_or("Invalid room")?;
        drop(object.remove("editor_instance_id"));
        for (key, value) in [("x", left), ("z", near), ("width", width), ("depth", depth)] {
            drop(object.insert(key.into(), json!(value)));
        }
        result.push(strip);
    }
    Ok(result)
}
/// A bounded source-only proposal. Returns an actionable refusal for ambiguous
/// floors, obstacles or insufficient run/standing clearance.
pub(super) fn candidate(
    doc: &Document,
    lower: usize,
    upper: usize,
    width: f32,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<Value, String> {
    let level = doc.level();
    let low = level.rooms.get(lower).ok_or("Choose a lower floor")?;
    let high = level.rooms.get(upper).ok_or("Choose an upper floor")?;
    let rise = high.floor_y - low.floor_y;
    if lower == upper || rise <= 0.0 {
        return Err("Upper floor must be above the lower floor.".into());
    }
    if !low.ceiling.is_flat() || !high.ceiling.is_flat() {
        return Err(
            "This straight shaft needs two flat rooms; review open/gabled floors manually.".into(),
        );
    }
    if !width.is_finite() || !(0.9..=4.0).contains(&width) {
        return Err("Width must be 0.9..4 metres; 1.2 m is the default.".into());
    }
    if high.floor_y - low.eave_y() < MIN_STRUCTURE_M - 0.0001 {
        return Err("Review the stacked-floor clearance in Safety first; stairs will not silently lower the ceiling.".into());
    }
    let (low_left, low_right, low_near, low_far) = low.bounds();
    let (high_left, high_right, high_near, high_far) = high.bounds();
    let (left, right, near, far) = (
        low_left.max(high_left),
        low_right.min(high_right),
        low_near.max(high_near),
        low_far.min(high_far),
    );
    let mut steps = 2_u16;
    while f32::from(steps) * 0.2 < rise && steps < 256 {
        steps = steps.saturating_add(1);
    }
    let run = f32::from(steps) * 0.28;
    let landing = width.max(1.2);
    let total = 2.0_f32.mul_add(landing, run);
    let along_x = right - left >= far - near;
    let (available, across) = if along_x {
        (right - left, far - near)
    } else {
        (far - near, right - left)
    };
    if available < total + 0.2 || across < width + 0.2 {
        return Err(format!(
            "Need a shared clear rectangle at least {total:.2} m long × {width:.2} m wide, plus 0.1 m margin; current overlap is {:.2} × {:.2} m. Extend a shaft or use the manual Staircase form.",
            right - left,
            far - near
        ));
    }
    let x = if along_x {
        left + 0.1
    } else {
        (right - left - width).mul_add(0.5, left)
    };
    let z = if along_x {
        (far - near - width).mul_add(0.5, near)
    } else {
        near + 0.1
    };
    let (shaft_width, shaft_depth, flight_x, flight_z, flight_width, flight_depth) = if along_x {
        (landing + run, width, x + landing, z, run, width)
    } else {
        (width, landing + run, x, z + landing, width, run)
    };
    let cut = [x, z, shaft_width, shaft_depth];
    // This is a full standing-volume check of the generated shaft envelope,
    // not a single tread/top-height sample.
    if high.eave_y() - high.floor_y < crate::engine::collision::PLAYER_HEIGHT + 0.1 {
        return Err("Upper landing has less than 1.9 m standing clearance.".into());
    }
    clear_envelope(level, lower, upper, cut, landing, catalog)?;
    assemble(
        doc,
        lower,
        upper,
        cut,
        [flight_x, flight_z, flight_width, flight_depth],
        steps,
    )
}
fn clear_envelope(
    level: &LevelDef,
    lower: usize,
    upper: usize,
    cut: [f32; 4],
    landing: f32,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<(), String> {
    let low = level.rooms.get(lower).ok_or("Missing lower floor")?;
    let high = level.rooms.get(upper).ok_or("Missing upper floor")?;
    let [x, z, width, depth] = cut;
    let (shaft_width, shaft_depth) = if width >= depth {
        (width + landing, depth)
    } else {
        (width, depth + landing)
    };
    let overlaps = |bounds: (f32, f32, f32, f32)| {
        bounds.0 < x + shaft_width && bounds.1 > x && bounds.2 < z + shaft_depth && bounds.3 > z
    };
    if level
        .walls
        .iter()
        .any(|w| overlaps((w.x, w.x + w.width, w.z, w.z + w.depth)) && w.y < high.floor_y + 1.9)
    {
        return Err("A wall intersects the proposed flight/landing envelope. Move it explicitly or choose another floor pair.".into());
    }
    // A ceiling fixture without authored Y still owns real geometry. Cutting
    // its supporting room would otherwise silently lift it into the shaft.
    if level.ceiling_lights.iter().any(|light| {
        let profile = crate::engine::lighting::fixture_profile(&light.fixture);
        let radius = profile.half_width.hypot(profile.half_depth);
        overlaps((
            light.x - radius,
            light.x + radius,
            light.z - radius,
            light.z + radius,
        ))
    }) {
        return Err("A fixture overlaps the proposed shaft/landing envelope. Move it explicitly before generating stairs.".into());
    }
    let surfaces = crate::engine::level::LevelSurfaces::new(level);
    if level.props.iter().any(|prop| {
        let size = prop
            .size
            .or_else(|| catalog.placeable(&prop.model).and_then(|entry| entry.size))
            .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE);
        let bounds =
            prop.placement_bounds(surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0), size);
        overlaps((bounds.min[0], bounds.max[0], bounds.min[2], bounds.max[2]))
            && bounds.min[1] < high.floor_y + 1.9
            && bounds.max[1] > low.floor_y
    }) {
        return Err("A prop's transformed bounds overlap the shaft or landing; review and move it before generating stairs.".into());
    }
    if level.doors.iter().any(|door| {
        overlaps((
            door.x - door.width,
            door.x + door.width,
            door.z - door.width,
            door.z + door.width,
        ))
    }) {
        return Err("A door's swing envelope overlaps the proposed shaft or landing. Move it explicitly before generating stairs.".into());
    }
    if level.rooms.iter().enumerate().any(|(i, r)| {
        i != lower
            && i != upper
            && overlaps(r.bounds())
            && r.floor_y > low.floor_y
            && r.floor_y < high.eave_y()
    }) {
        return Err("Another storey crosses this shaft. Review the room layout rather than cutting it implicitly.".into());
    }
    if !level.ramps.is_empty() || !level.stairs.is_empty() || !level.floor_regions.is_empty() {
        return Err("Existing ramps/stairs/global regions need a manual connection review. No geometry was changed.".into());
    }
    Ok(())
}
fn assemble(
    doc: &Document,
    lower: usize,
    upper: usize,
    cut: [f32; 4],
    flight: [f32; 4],
    steps: u16,
) -> Result<Value, String> {
    let level = doc.level();
    let low = level.rooms.get(lower).ok_or("Missing lower room")?;
    let high = level.rooms.get(upper).ok_or("Missing upper room")?;
    let rise = high.floor_y - low.floor_y;
    let [x, z, shaft_width, shaft_depth] = cut;
    let [flight_x, flight_z, flight_width, flight_depth] = flight;
    let mut raw = doc.raw().clone();
    let rooms = raw
        .get_mut("rooms")
        .and_then(Value::as_array_mut)
        .ok_or("Missing rooms")?;
    let lower_raw = rooms.get(lower).ok_or("Missing lower floor")?.clone();
    let upper_raw = rooms.get(upper).ok_or("Missing upper floor")?.clone();
    let mut replacements = Vec::new();
    for (i, room) in rooms.iter().enumerate() {
        if i == lower {
            replacements.extend(split_room(&lower_raw, cut)?);
        } else if i == upper {
            replacements.extend(split_room(&upper_raw, cut)?);
        } else {
            replacements.push(room.clone());
        }
    }
    let mut shaft = lower_raw;
    let object = shaft.as_object_mut().ok_or("Invalid room")?;
    drop(object.remove("editor_instance_id"));
    for (key, value) in [
        ("x", x),
        ("z", z),
        ("width", shaft_width),
        ("depth", shaft_depth),
        ("height", high.eave_y() - low.floor_y),
    ] {
        drop(object.insert(key.into(), json!(value)));
    }
    for key in [
        "ceiling_material",
        "ceiling_shine",
        "ceiling_tile_origin",
        "ceiling_tile_rotation_degrees",
    ] {
        if let Some(value) = upper_raw.get(key) {
            drop(object.insert(key.into(), value.clone()));
        } else {
            drop(object.remove(key));
        }
    }
    replacements.push(shaft);
    *rooms = replacements;
    super::document::record_array_mut(&mut raw,"stairs")?.push(json!({"x":flight_x,"z":flight_z,"width":flight_width,"depth":flight_depth,"offset_y":0.0_f32,"rise":rise,"steps":steps,"material":low.material.clone().unwrap_or_else(||level.defaults.floor.clone()),"side_material":level.defaults.wall}));
    let typed = LevelDef::from_json(&raw.to_string()).map_err(|error| error.to_string())?;
    crate::engine::loader::validate_level(&typed)?;
    Ok(raw)
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only stair generator commands are handled here."
    )]
    pub(super) fn stair_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::StairWizard => self.popup = Some(Popup::Stairs(Box::new(Wizard::new(self)?))),
            Action::StairFloor(upper) => {
                let count = self.document.as_ref().map_or(0, |d| d.level().rooms.len());
                if let Some(Popup::Stairs(w)) = &mut self.popup {
                    let selected = if upper { &mut w.upper } else { &mut w.lower };
                    *selected = selected
                        .saturating_add(1)
                        .checked_rem(count.max(1))
                        .unwrap_or(0);
                    w.candidate = None;
                }
            }
            Action::StairPreview | Action::StairApply => {
                let Some(Popup::Stairs(w)) = &mut self.popup else {
                    return Ok(false);
                };
                let doc = self.document.as_ref().ok_or("Open a project first")?;
                if doc.raw() != &w.expected {
                    return Err("Scene changed; reopen the stair generator.".into());
                }
                if action == Action::StairApply && w.candidate.is_none() {
                    return Err("Preview the proposed flight and shaft before applying.".into());
                }
                let proposed = candidate(
                    doc,
                    w.lower,
                    w.upper,
                    w.width
                        .parse()
                        .map_err(|_error| "Enter a width in metres")?,
                    &self.catalog,
                )?;
                if action == Action::StairApply {
                    let expected = w.expected.clone();
                    self.document
                        .as_mut()
                        .ok_or("Open a project first")?
                        .apply_checked_edit(&expected, proposed)?;
                    self.popup = None;
                    self.changed();
                    self.status="Stair flight, shaft and landings applied as one undo step. Generated rooms and stairs remain editable.".into();
                } else {
                    let typed = LevelDef::from_json(&proposed.to_string())
                        .map_err(|error| error.to_string())?;
                    self.viewport.rebuild(&typed);
                    w.candidate = Some(proposed);
                    w.message="PREVIEW: cuts a shaft through both rooms; lower ceiling opens here. Landings remain level; source unchanged. Apply or Cancel.".into();
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Width entry consumes only text and Backspace, independent of SDL metadata."
    )]
    pub(super) fn stair_event(&mut self, event: &sdl3::event::Event) -> bool {
        use sdl3::{event::Event, keyboard::Keycode};
        let Some(Popup::Stairs(w)) = &mut self.popup else {
            return false;
        };
        match event {
            Event::TextInput { text, .. } => {
                if w.width.len() < 16 {
                    w.width.push_str(text);
                }
                w.candidate = None;
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                ..
            } => {
                let _removed = w.width.pop();
                w.candidate = None;
            }
            _ => return false,
        }
        true
    }
}
pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>, w: &Wizard) {
    hits.clear();
    ui::rect(
        vertices,
        [122.0, 176.0, 344.0, 64.0],
        [0.075, 0.11, 0.14, 1.0],
    );
    for (x, upper, i) in [(128.0, false, w.lower), (294.0, true, w.upper)] {
        let label = app
            .document
            .as_ref()
            .and_then(|d| d.level().rooms.get(i))
            .map_or_else(
                || "Choose floor".into(),
                |r| {
                    format!(
                        "{} Room{} Y{:.2} >",
                        if upper { "Upper" } else { "Lower" },
                        i.saturating_add(1),
                        r.floor_y
                    )
                },
            );
        ui::popup_button(
            vertices,
            hits,
            [x, 181.0, 160.0, 12.0],
            &label,
            Action::StairFloor(upper),
        );
    }
    ui::text(
        vertices,
        &format!("Width m: {}_  (type / Backspace)", w.width),
        128.0,
        198.0,
        ui::INK,
        78,
    );
    ui::text(vertices, &w.message, 128.0, 209.0, ui::INK, 80);
    for (x, label, action) in [
        (128.0, "Preview", Action::StairPreview),
        (240.0, "Apply preview", Action::StairApply),
        (352.0, "Cancel", Action::ClosePopup),
    ] {
        ui::popup_button(vertices, hits, [x, 225.0, 104.0, 12.0], label, action);
    }
}
