//! Axis-constrained viewport translation: private previews, one history commit.
use super::{
    App,
    document::{ObjectId, Transform},
};
use crate::engine::render::DrawableSize;
use glam::{Vec2, Vec3};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
    video::Window,
};

pub(super) struct Translation {
    id: ObjectId,
    original: Transform,
    proposed: Transform,
    axis: usize,
    edge: Option<super::document::room_resize::Edge>,
    start: Vec2,
    direction: Vec2,
}
impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Drag routing consumes only pointer movement, release, cancellation and focus loss; unrelated SDL metadata is irrelevant."
    )]
    pub(super) fn drag_event(&mut self, event: &Event, window: &Window) -> bool {
        let size = window.size_in_pixels();
        let ui_point = |x, y| {
            super::layout::window_to_ui(DrawableSize::new(size.0, size.1), window.size(), [x, y])
                .map(Vec2::from_array)
        };
        if self.translation.is_some() {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => {
                    self.translation = None;
                    self.changed();
                    self.status = "Drag cancelled; source unchanged.".into();
                }
                Event::MouseMotion { x, y, .. }
                | Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if let Some(point) = ui_point(*x, *y) {
                        self.update_drag(point);
                    }
                    if matches!(event, Event::MouseButtonUp { .. })
                        && let Some(drag) = self.translation.take()
                    {
                        let result = self
                            .document
                            .as_mut()
                            .ok_or_else(|| "Open a project first".to_string())
                            .and_then(|doc| {
                                if let Some(edge) = drag.edge {
                                    let delta = drag
                                        .proposed
                                        .position
                                        .get(drag.axis)
                                        .copied()
                                        .unwrap_or(0.0)
                                        - drag
                                            .original
                                            .position
                                            .get(drag.axis)
                                            .copied()
                                            .unwrap_or(0.0);
                                    doc.resize_room_edge(drag.id, edge, delta)
                                } else {
                                    doc.apply_drag(drag.id, drag.proposed, &self.catalog)
                                }
                            });
                        self.changed();
                        if result.is_ok() {
                            self.status = "Drag applied as one undo step.".into();
                        }
                        self.report(result);
                    }
                }
                _ => {}
            }
            return true;
        }
        if self.popup.is_some() || self.viewport.is_preparing() {
            return false;
        }
        if let Event::MouseButtonDown {
            mouse_btn: MouseButton::Left,
            x,
            y,
            ..
        } = event
        {
            let Some(point) = ui_point(*x, *y) else {
                return false;
            };
            return self.begin_drag(point);
        }
        false
    }
    fn begin_drag(&mut self, point: Vec2) -> bool {
        if self.selected.is_some_and(|id| !self.editable(id)) {
            return false;
        }
        if self.begin_edge_drag(point) {
            return true;
        }
        let Some(id) = self.selected else {
            return false;
        };
        let Some(bounds) = self.viewport.selected_bounds() else {
            return false;
        };
        let centre = Vec3::from_array(bounds.centre());
        let Some(start) = self.viewport.project(centre).map(Vec2::from_array) else {
            return false;
        };
        for (axis, vector) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            let Some(end) = self
                .viewport
                .project(Vec3::new(
                    centre.x + vector.x,
                    centre.y + vector.y,
                    centre.z + vector.z,
                ))
                .map(Vec2::from_array)
            else {
                continue;
            };
            let direction = Vec2::new(end.x - start.x, end.y - start.y);
            if direction.length_squared() < 9.0 {
                continue;
            }
            let offset = Vec2::new(point.x - start.x, point.y - start.y);
            let fraction = offset.dot(direction) / direction.length_squared();
            let distance = Vec2::new(
                direction.x.mul_add(-fraction, offset.x),
                direction.y.mul_add(-fraction, offset.y),
            )
            .length();
            if (0.15..=1.2).contains(&fraction) && distance < 4.0 {
                let Some(doc) = &self.document else {
                    return false;
                };
                match doc.transform(id).and_then(|pose| {
                    let _candidate = doc.drag_candidate(id, pose, &self.catalog)?;
                    Ok(pose)
                }) {
                    Ok(original) => {
                        self.translation = Some(Translation {
                            id,
                            original,
                            proposed: original,
                            axis,
                            edge: None,
                            start: point,
                            direction,
                        });
                        self.status="Drag axis. Snap follows toolbar; Esc cancels; release applies one undo step.".into();
                    }
                    Err(error) => self.status = error,
                }
                return true;
            }
        }
        false
    }
    fn begin_edge_drag(&mut self, point: Vec2) -> bool {
        use super::document::room_resize::Edge;
        let Some(ObjectId::Room(index)) = self.selected else {
            return false;
        };
        let Some(doc) = &self.document else {
            return false;
        };
        let Some(room) = doc.level().rooms.get(index) else {
            return false;
        };
        for edge in Edge::ALL {
            let handle = edge.handle(room);
            let Some(start) = self
                .viewport
                .project(Vec3::from_array(handle))
                .map(Vec2::from_array)
            else {
                continue;
            };
            if Vec2::new(point.x - start.x, point.y - start.y).length() > 5.0 {
                continue;
            }
            let mut end = handle;
            if let Some(value) = end.get_mut(edge.axis()) {
                *value += 1.0;
            }
            let Some(projected) = self.viewport.project(Vec3::from_array(end)) else {
                continue;
            };
            let direction = Vec2::new(projected[0] - start.x, projected[1] - start.y);
            if direction.length_squared() < 1.0 {
                self.status = "Orbit the camera to expose this edge's drag direction.".into();
                return true;
            }
            if let Ok(original) = doc.transform(ObjectId::Room(index)) {
                self.translation = Some(Translation {
                    id: ObjectId::Room(index),
                    original,
                    proposed: original,
                    axis: edge.axis(),
                    edge: Some(edge),
                    start: point,
                    direction,
                });
                self.status =
                    "Drag room edge; opposite edge stays fixed. Esc cancels; release commits once."
                        .into();
            }
            return true;
        }
        false
    }

    fn update_drag(&mut self, point: Vec2) {
        let Some(drag) = &mut self.translation else {
            return;
        };
        let offset = Vec2::new(point.x - drag.start.x, point.y - drag.start.y);
        let mut delta =
            (offset.dot(drag.direction) / drag.direction.length_squared()).clamp(-100.0, 100.0);
        if self.view.snap {
            delta = (delta / self.step).round() * self.step;
        }
        let mut proposed = drag.original;
        if let Some(value) = proposed.position.get_mut(drag.axis) {
            *value += delta;
        }
        if drag.proposed == proposed {
            return;
        }
        drag.proposed = proposed;
        let drag_id = drag.id;
        let drag_edge = drag.edge;
        if let Some(doc) = &self.document {
            let candidate = if let Some(edge) = drag_edge {
                doc.room_edge_candidate(drag_id, edge, delta)
            } else {
                doc.drag_candidate(drag_id, proposed, &self.catalog)
            };
            let checked_candidate = candidate.and_then(|raw| {
                self.check_focused_edit(&raw)?;
                Ok(raw)
            });
            match checked_candidate.and_then(|raw| {
                crate::engine::level::LevelDef::from_json(&raw.to_string())
                    .map_err(|error| error.to_string())
            }) {
                Ok(level) => {
                    if let ObjectId::Room(index) = drag_id
                        && let Some(room) = level.rooms.get(index)
                    {
                        self.status = format!(
                            "Room{}: {:.2} x {:.2} m | delta {delta:+.2}m | release applies; Esc cancels",
                            index.saturating_add(1),
                            room.width,
                            room.depth
                        );
                    }
                    self.viewport.rebuild(&level);
                }
                Err(error) => self.status = format!("Drag cannot fit: {error}; Esc cancels."),
            }
        }
    }
}
