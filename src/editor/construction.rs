//! Direct viewport tools: previews are private; each click commits one edit.
use super::{
    App,
    document::{ObjectId, ObjectKind, Transform},
    ui::{self, Action, Hit},
};
use crate::engine::{
    level::{LevelSurfaces, WallAxis},
    render::{DrawableSize, Vertex},
};
use glam::Vec3;
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
    video::Window,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    Wall,
    Doorway,
    Door,
    Light,
}
impl Tool {
    const fn hint(self) -> &'static str {
        match self {
            Self::Wall => "Click start, then end. Walls snap to X/Z. Esc cancels.",
            Self::Doorway => "Click a wall: makes an opening only. Esc cancels.",
            Self::Door => "Click a wall or doorway: fits an interactive door. Esc cancels.",
            Self::Light => "Click a room floor: light mounts to its ceiling. Esc cancels.",
        }
    }
}
pub(super) struct Placement {
    pub tool: Tool,
    start: Option<[f32; 3]>,
    candidate: Option<(serde_json::Value, ObjectId)>,
    pub chain: bool,
    end: Option<[f32; 3]>,
}
impl Placement {
    pub(super) fn checkpoint(&self) -> serde_json::Value {
        serde_json::json!({"tool":format!("{:?}",self.tool),"start":self.start,"end":self.end,"candidate":self.candidate.is_some(),"chain":self.chain})
    }
}
pub(super) struct WallDrag {
    pub id: ObjectId,
    original: Transform,
    candidate: Option<serde_json::Value>,
    anchor: [f32; 3],
    endpoint: Option<bool>,
}
pub(super) fn floor_point(app: &App, point: [f32; 2], height: f32) -> Option<[f32; 3]> {
    let (origin, direction) = app.viewport.ray(point)?;
    if direction.y.abs() < 0.0001 {
        return None;
    }
    let distance = (height - origin.y) / direction.y;
    if !(0.0..=500.0).contains(&distance) {
        return None;
    }
    Some([
        direction.x.mul_add(distance, origin.x),
        height,
        direction.z.mul_add(distance, origin.z),
    ])
}
impl App {
    pub(super) fn start_placement(&mut self, tool: Tool) -> Result<(), String> {
        let _document = self
            .document
            .as_ref()
            .ok_or("Open or create a project first")?;
        self.popup = None;
        self.focus.isolated = None;
        let layer = if tool == Tool::Light { 2 } else { 0 };
        if let Some(hidden) = self.focus.hidden.get_mut(layer) {
            *hidden = false;
        }
        self.cancel_placement();
        self.placement = Some(Placement {
            tool,
            start: None,
            candidate: None,
            chain: false,
            end: None,
        });
        self.status = tool.hint().into();
        Ok(())
    }
    pub(super) fn cancel_placement(&mut self) {
        self.placement = None;
        self.wall_drag = None;
        self.viewport.highlight(None, &[]);
        self.changed();
    }
    fn placement_position(&self, point: [f32; 2]) -> Option<[f32; 3]> {
        let doc = self.document.as_ref()?;
        let floor = self
            .placement
            .as_ref()
            .and_then(|p| p.start)
            .map(|s| s[1])
            .or_else(|| {
                self.selected.and_then(|id| match id {
                    ObjectId::Room(i) => doc.level().rooms.get(i).map(|r| r.floor_y),
                    ObjectId::Wall(i) => doc.level().walls.get(i).map(|w| w.y),
                    ObjectId::Column(_)
                    | ObjectId::Pillar(_)
                    | ObjectId::ArcWall(_)
                    | ObjectId::Prop(_)
                    | ObjectId::Ramp(_)
                    | ObjectId::FloorRegion(_)
                    | ObjectId::Light(_)
                    | ObjectId::Audio(_)
                    | ObjectId::Door(_)
                    | ObjectId::Trigger(_)
                    | ObjectId::FogRegion(_)
                    | ObjectId::GroundFog(_)
                    | ObjectId::SpawnPoint(_) => None,
                })
            })
            .unwrap_or_else(|| {
                LevelSurfaces::new(doc.level())
                    .floor_y_at(self.viewport.target()[0], self.viewport.target()[2])
                    .unwrap_or(0.0)
            });
        let mut at = floor_point(self, point, floor)?;
        if self.view.snap {
            at[0] = (at[0] / self.step).round() * self.step;
            at[2] = (at[2] / self.step).round() * self.step;
            let mut nearest = None;
            for wall in &doc.level().walls {
                if (wall.y - floor).abs() > 0.05 {
                    continue;
                }
                for end in super::document::wall_edit::endpoints(wall) {
                    let d = (end[0] - at[0]).hypot(end[2] - at[2]);
                    if d < self.step.max(0.2) * 0.75 && nearest.is_none_or(|(_, best)| d < best) {
                        nearest = Some((end, d));
                    }
                }
            }
            if let Some((end, _)) = nearest {
                at = [end[0], floor, end[2]];
            }
        }
        Some(at)
    }
    fn update_placement(&mut self, point: [f32; 2]) {
        let Some(at) = self.placement_position(point) else {
            return;
        };
        let Some(placement) = &self.placement else {
            return;
        };
        let tool = placement.tool;
        let start_point = placement.start;
        let Some(doc) = &self.document else {
            return;
        };
        let result = match tool {
            Tool::Wall => {
                let Some(start) = start_point else {
                    self.viewport.highlight(
                        Some(crate::engine::spatial::Aabb {
                            min: [at[0] - 0.1, at[1], at[2] - 0.1],
                            max: [at[0] + 0.1, at[1] + 0.2, at[2] + 0.1],
                        }),
                        &[],
                    );
                    return;
                };
                let mut end = at;
                if (end[0] - start[0]).abs() >= (end[2] - start[2]).abs() {
                    end[2] = start[2];
                } else {
                    end[0] = start[0];
                }
                if let Some(p) = &mut self.placement {
                    p.end = Some(end);
                }
                doc.wall_segment_candidate(start, end)
            }
            Tool::Doorway | Tool::Door => {
                let wall = self.wall_at(point, at);
                wall.ok_or_else(|| "Point at a wall or its opening".to_string())
                    .and_then(|i| {
                        let w = doc.level().walls.get(i).ok_or("Missing wall")?;
                        let offset = self.door_offset(point, w)?;
                        doc.doorway_candidate(i, offset, tool == Tool::Door)
                    })
            }
            Tool::Light => light_candidate(doc, at),
        };
        match result {
            Ok((raw, id)) => {
                if self
                    .placement
                    .as_ref()
                    .and_then(|p| p.candidate.as_ref())
                    .is_some_and(|(previous, _)| previous == &raw)
                {
                    return;
                }
                let checked = self
                    .check_focused_edit(&raw)
                    .and_then(|()| doc.validate_candidate(&raw));
                match checked {
                    Ok(level) => {
                        self.viewport.rebuild(&level);
                        if let Some(p) = &mut self.placement {
                            p.candidate = Some((raw, id));
                        }
                        self.status = if tool == Tool::Wall {
                            format!(
                                "Wall {:.2}m | click to place | Esc cancels",
                                start_point.map_or(0.0, |s| (s[0] - at[0]).hypot(s[2] - at[2]))
                            )
                        } else {
                            "Preview ready. Click to place; Esc cancels.".into()
                        };
                    }
                    Err(e) => self.placement_error(&e),
                }
            }
            Err(e) => self.placement_error(&e),
        }
    }
    fn door_offset(
        &self,
        point: [f32; 2],
        w: &crate::engine::level::WallDef,
    ) -> Result<f32, String> {
        let (origin, direction) = self.viewport.ray(point).ok_or("Point at the wall face")?;
        let (distance, along) = if w.axis() == WallAxis::X {
            (
                (f32::mul_add(w.depth, 0.5, w.z) - origin.z) / direction.z,
                0_usize,
            )
        } else {
            (
                (f32::mul_add(w.width, 0.5, w.x) - origin.x) / direction.x,
                2_usize,
            )
        };
        if !distance.is_finite() || !(0.0..=500.0).contains(&distance) {
            return Err("Orbit to face the wall before placing a door".into());
        }
        let offset = if along == 0 {
            direction.x.mul_add(distance, origin.x) - w.x
        } else {
            direction.z.mul_add(distance, origin.z) - w.z
        };
        Ok(if self.view.snap {
            (offset / self.step).round() * self.step
        } else {
            offset
        })
    }
    fn placement_error(&mut self, error: &str) {
        if let Some(p) = &mut self.placement {
            p.candidate = None;
        }
        // Remove the last valid ghost when the next point is invalid.
        if let Some(doc) = &self.document {
            self.viewport.rebuild(doc.level());
        }
        self.status = format!("{error}. Esc cancels.");
    }
    fn wall_at(&self, point: [f32; 2], at: [f32; 3]) -> Option<usize> {
        if let Some(ObjectId::Wall(i)) = self.viewport.pick(point) {
            return Some(i);
        }
        self.document
            .as_ref()?
            .level()
            .walls
            .iter()
            .enumerate()
            .filter(|(i, _)| self.focus.visible(ObjectId::Wall(*i)))
            .filter_map(|(i, w)| {
                let cross = if w.axis() == WallAxis::X {
                    (at[2] - f32::mul_add(w.depth, 0.5, w.z)).abs()
                } else {
                    (at[0] - f32::mul_add(w.width, 0.5, w.x)).abs()
                };
                let offset = if w.axis() == WallAxis::X {
                    at[0] - w.x
                } else {
                    at[2] - w.z
                };
                (cross < 0.4 && offset >= 0.0 && offset <= w.length()).then_some((i, cross))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Viewport tools consume pointer, Escape and focus-loss events; other SDL metadata is unrelated."
    )]
    pub(super) fn construction_event(&mut self, event: &Event, window: &Window) -> bool {
        let size = window.size_in_pixels();
        let convert = |x, y| {
            super::layout::window_to_ui(DrawableSize::new(size.0, size.1), window.size(), [x, y])
        };
        if self.placement.is_some() {
            return self.placement_event(event, window);
        }
        if self.wall_drag.is_some() {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => {
                    self.cancel_placement();
                    self.status = "Wall drag cancelled.".into();
                }
                Event::MouseMotion { x, y, .. }
                | Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if let Some(p) = convert(*x, *y) {
                        self.update_wall_drag(p);
                    }
                    if matches!(event, Event::MouseButtonUp { .. })
                        && let Some(drag) = self.wall_drag.take()
                    {
                        if let Some(raw) = drag.candidate {
                            let result = self
                                .document
                                .as_mut()
                                .ok_or_else(|| "No project".to_string())
                                .and_then(|d| {
                                    let expected = d.raw().clone();
                                    d.apply_authoring_edit(&expected, raw)
                                });
                            self.report(result);
                        }
                        self.changed();
                    }
                }
                _ => {}
            }
            return true;
        }
        if self.popup.is_none()
            && !self.viewport.is_preparing()
            && let Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } = event
            && let Some(p) = convert(*x, *y).filter(|p| self.viewport.contains_ui(*p))
        {
            return self.begin_wall_drag(p);
        }
        false
    }
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Placement owns only pointer, Escape and focus-loss events."
    )]
    fn placement_event(&mut self, event: &Event, window: &Window) -> bool {
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
                self.cancel_placement();
                self.status =
                    "Placement cancelled; completed segments remain in undo history.".into();
            }
            Event::MouseMotion { x, y, .. } => {
                if let Some(p) = convert(*x, *y).filter(|p| self.viewport.contains_ui(*p)) {
                    self.update_placement(p);
                }
            }
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(point) = convert(*x, *y).filter(|p| self.viewport.contains_ui(*p)) {
                    let Some(at) = self.placement_position(point) else {
                        return true;
                    };
                    if let Some(p) = &mut self.placement
                        && p.tool == Tool::Wall
                        && p.start.is_none()
                    {
                        p.start = Some(at);
                        self.status =
                            "Start set. Move the pointer, then click the endpoint. Esc cancels."
                                .into();
                        return true;
                    }
                    self.update_placement(point);
                    let Some((raw, id)) = self.placement.as_mut().and_then(|p| p.candidate.take())
                    else {
                        return true;
                    };
                    let result = self
                        .document
                        .as_mut()
                        .ok_or_else(|| "No project".to_string())
                        .and_then(|d| {
                            let expected = d.raw().clone();
                            d.apply_authoring_edit(&expected, raw)
                        });
                    if result.is_ok() {
                        self.selected = Some(id);
                        let chain = self
                            .placement
                            .as_ref()
                            .is_some_and(|p| p.chain && p.tool == Tool::Wall);
                        if chain {
                            if let Some(p) = &mut self.placement {
                                p.start = p.end;
                            }
                        } else {
                            self.placement = None;
                        }
                        self.changed();
                        self.status = if chain {
                            "Wall placed. Click the next endpoint; Esc finishes the chain."
                        } else {
                            "Placed. Use Add again to repeat; Ctrl Z undoes this action."
                        }
                        .into();
                    }
                    self.report(result);
                    return true;
                }
                return false;
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => return convert(*x, *y).is_some_and(|p| self.viewport.contains_ui(p)),
            _ => return false,
        }
        true
    }
    fn begin_wall_drag(&mut self, point: [f32; 2]) -> bool {
        let Some(id @ ObjectId::Wall(i)) = self.selected else {
            return false;
        };
        if !self.editable(id) {
            self.status = "Wall is locked. Unlock it in Layers before editing.".into();
            return false;
        }
        let Some(doc) = &self.document else {
            return false;
        };
        let Some(wall) = doc.level().walls.get(i) else {
            return false;
        };
        let endpoint = super::document::wall_edit::endpoints(wall)
            .into_iter()
            .enumerate()
            .find_map(|(endpoint_index, p)| {
                let projected = self.viewport.project(Vec3::from_array(p))?;
                ((projected[0] - point[0]).hypot(projected[1] - point[1]) < 6.0)
                    .then_some(endpoint_index == 1)
            });
        if endpoint.is_none() && self.viewport.pick(point) != Some(id) {
            return false;
        }
        let Some(anchor) = floor_point(self, point, wall.y) else {
            return false;
        };
        let Ok(original) = doc.transform(id) else {
            return false;
        };
        self.wall_drag = Some(WallDrag {
            id,
            original,
            candidate: None,
            anchor,
            endpoint,
        });
        self.status = if endpoint.is_some() {
            "Drag endpoint to change wall length; Esc cancels."
        } else {
            "Drag wall across the floor; attachments follow. Esc cancels."
        }
        .into();
        true
    }
    fn update_wall_drag(&mut self, point: [f32; 2]) {
        let Some(drag) = &self.wall_drag else {
            return;
        };
        let Some(at) = floor_point(self, point, drag.original.position[1]) else {
            return;
        };
        let mut pose = drag.original;
        let snap = |v: f32| {
            if self.view.snap {
                (v / self.step).round() * self.step
            } else {
                v
            }
        };
        if let Some(end) = drag.endpoint {
            let axis = if pose.size[0] >= pose.size[2] { 0 } else { 2 };
            let delta = snap(
                at.get(axis).copied().unwrap_or(0.0)
                    - drag.anchor.get(axis).copied().unwrap_or(0.0),
            );
            if end {
                if let Some(length) = pose.size.get_mut(axis) {
                    *length += delta;
                }
            } else {
                if let Some(origin) = pose.position.get_mut(axis) {
                    *origin += delta;
                }
                if let Some(length) = pose.size.get_mut(axis) {
                    *length -= delta;
                }
            }
        } else {
            pose.position[0] += snap(at[0] - drag.anchor[0]);
            pose.position[2] += snap(at[2] - drag.anchor[2]);
        }
        let ObjectId::Wall(index) = drag.id else {
            return;
        };
        let result = self
            .document
            .as_ref()
            .ok_or_else(|| "No project".to_string())
            .and_then(|d| d.wall_edit_candidate(index, pose))
            .and_then(|raw| {
                self.check_focused_edit(&raw)?;
                Ok(raw)
            });
        match result {
            Ok(raw) => {
                if let Some(doc) = &self.document
                    && let Ok(level) = doc.validate_candidate(&raw)
                {
                    self.viewport.rebuild(&level);
                }
                if let Some(d) = &mut self.wall_drag {
                    d.candidate = Some(raw);
                }
                self.status = format!(
                    "Wall {:.2}m | release applies once | Esc cancels",
                    pose.size[0].max(pose.size[2])
                );
            }
            Err(e) => {
                if let Some(d) = &mut self.wall_drag {
                    d.candidate = None;
                }
                if let Some(doc) = &self.document {
                    self.viewport.rebuild(doc.level());
                }
                self.status = e;
            }
        }
    }
}
pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if let Some(p) = &app.placement {
        ui::rect(v, [118.0, 41.0, 354.0, 14.0], [0.06, 0.13, 0.17, 1.0]);
        ui::text(v, p.tool.hint(), 122.0, 44.0, ui::INK, 81);
        if p.tool == Tool::Wall {
            ui::button(
                v,
                hits,
                [120.0, 57.0, 87.0, 12.0],
                if p.chain { "Chain: On" } else { "Chain: Off" },
                Action::ChainWalls,
                true,
            );
        }
        ui::button(
            v,
            hits,
            [402.0, 57.0, 69.0, 12.0],
            "Cancel / Esc",
            Action::CancelPlacement,
            true,
        );
    }
}

fn light_candidate(
    doc: &super::Document,
    at: [f32; 3],
) -> Result<(serde_json::Value, ObjectId), String> {
    let mut raw = doc.raw().clone();
    let ceiling_room = doc.level().rooms.iter().find(|room| {
        let (left, right, front, back) = room.bounds();
        at[0] >= left
            && at[0] <= right
            && at[2] >= front
            && at[2] <= back
            && (room.floor_y - at[1]).abs() < 0.05
    });
    ceiling_room
        .ok_or_else(|| "Place the light inside the selected room floor".to_string())
        .and_then(|room| {
            let id =
                super::document::objects::add(&mut raw, doc.level(), ObjectKind::Light, at, None)?;
            let record = super::document::objects::object_mut(&mut raw, id)?;
            drop(record.insert(
                "y".into(),
                serde_json::json!(room.ceiling_y_at(at[0], at[2])),
            ));
            let _validated = doc.validate_candidate(&raw)?;
            Ok((raw, id))
        })
}
