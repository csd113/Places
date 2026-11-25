//! Native floor picks and a discardable, surface-aligned placement preview.
use glam::Vec3;
use sdl3::{event::Event, keyboard::Keycode, mouse::MouseButton, video::Window};

use super::{
    App,
    actions::Popup,
    ui::{self, Action, Hit},
};
use crate::engine::{
    escalator::{Dimensions, Fit},
    render::Vertex,
    spatial::Aabb,
};

pub(super) const LENGTH_FIELD: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Connect,
    Free,
    Rotate,
    Length,
}

#[derive(Clone, Copy)]
pub(super) enum Draft {
    Connect(Option<Vec3>),
    Free {
        dimensions: Dimensions,
        turn: u8,
        hover: Option<Vec3>,
    },
}

impl App {
    pub(super) fn start_escalator(&mut self, free: bool) -> Result<(), String> {
        if self.document.is_none() {
            return Err("Open a project first".into());
        }
        self.retire_construction();
        self.translation = None;
        self.popup = None;
        self.play_floor_pick = false;
        self.viewport.highlight(None, &[]);
        self.escalator = Some(if free {
            Draft::Free {
                dimensions: Dimensions::new(3.0)?,
                turn: 0,
                hover: None,
            }
        } else {
            Draft::Connect(None)
        });
        self.status = if free {
            "Free escalator: set length, rotate if needed, then pick a base. Esc cancels."
        } else {
            "Escalator: click the lower landing near its edge. Esc cancels."
        }
        .into();
        Ok(())
    }

    pub(super) fn escalator_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Escalator(command) = action else {
            return Ok(false);
        };
        match command {
            Command::Connect => self.start_escalator(false)?,
            Command::Free => self.start_escalator(true)?,
            Command::Rotate => {
                if let Some(Draft::Free {
                    dimensions: _,
                    turn,
                    hover,
                }) = &mut self.escalator
                {
                    *turn = turn.saturating_add(1) % 4;
                    if let Some(point) = *hover {
                        self.update_escalator_preview(point);
                    }
                }
            }
            Command::Length => {
                if let Some(Draft::Free {
                    dimensions,
                    turn: _,
                    hover: _,
                }) = self.escalator
                {
                    self.popup = Some(Popup::Number {
                        field: LENGTH_FIELD,
                        value: format!("{:.2}", dimensions.length),
                    });
                }
            }
        }
        Ok(true)
    }

    pub(super) fn set_escalator_length(&mut self, length: f32) -> Result<(), String> {
        let Some(Draft::Free {
            dimensions,
            turn: _,
            hover,
        }) = &mut self.escalator
        else {
            return Err("Start Free Placement first".into());
        };
        *dimensions = Dimensions::from_length(length)?;
        if let Some(point) = *hover {
            self.update_escalator_preview(point);
        }
        Ok(())
    }

    fn fitted_escalator(&self, point: Vec3) -> Result<Fit, String> {
        let level = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .level();
        match self.escalator {
            Some(Draft::Connect(Some(lower))) => Fit::connect(level, lower, point),
            Some(Draft::Free {
                dimensions,
                turn,
                hover: _,
            }) => Fit::free(level, point, turn, dimensions),
            Some(Draft::Connect(None)) | None => Err("Pick the lower landing first".into()),
        }
    }

    fn update_escalator_preview(&mut self, point: Vec3) {
        if let Some(Draft::Free {
            dimensions: _,
            turn: _,
            hover,
        }) = &mut self.escalator
        {
            *hover = Some(point);
        }
        match self.fitted_escalator(point) {
            Ok(fit) => {
                let triangles = fit.lane.preview();
                let mut bounds = Aabb::EMPTY;
                for &vertex in triangles.iter().flatten() {
                    bounds.expand(vertex);
                }
                self.viewport.highlight(Some(bounds), &triangles);
                self.status = format!(
                    "Click to place. {:.2} m long, {:.2} m rise.{}",
                    fit.dimensions.length,
                    fit.dimensions.rise,
                    if fit.open_crest {
                        " Upper end opens into space."
                    } else if fit.lane.is_free() {
                        " No building floors added."
                    } else {
                        " Both landings fit."
                    }
                );
            }
            Err(problem) => {
                self.viewport.highlight(None, &[]);
                self.status = problem;
            }
        }
    }

    fn escalator_click(&mut self, point: [f32; 2]) -> Result<(), String> {
        let floor = self
            .viewport
            .play_floor(point)
            .map_err(|error| error.replace("Play here:", "Escalator:"))?;
        if matches!(self.escalator, Some(Draft::Connect(None))) {
            self.escalator = Some(Draft::Connect(Some(floor)));
            self.viewport
                .highlight(Some(Aabb::from_point(floor.to_array())), &[]);
            self.status = "Now click the upper landing near the open edge. Esc cancels.".into();
            return Ok(());
        }
        let fit = self.fitted_escalator(floor)?;
        let selected = self
            .document
            .as_mut()
            .ok_or("Open a project first")?
            .add_escalator(fit)?;
        self.selected = Some(selected);
        self.changed();
        self.status = if fit.open_crest {
            "Free escalator placed. Its upper end opens into space."
        } else if fit.lane.is_free() {
            "Free escalator placed. No building floors were changed."
        } else {
            "Escalator connected. Direction is in Properties."
        }
        .into();
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        clippy::rest_pattern_accessible_field,
        reason = "Placement owns only the pointer and Escape, not every SDL event field."
    )]
    pub(super) fn escalator_event(&mut self, event: &Event, window: &Window) -> bool {
        if self.escalator.is_none() {
            return false;
        }
        match event {
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => {
                self.escalator = None;
                self.viewport.highlight(None, &[]);
                self.status = "Escalator placement cancelled.".into();
                true
            }
            Event::MouseMotion { x, y, .. } => {
                if !matches!(self.escalator, Some(Draft::Connect(None)))
                    && let Some(point) = super::input::ui_point(window, [*x, *y])
                        .filter(|p| self.viewport.contains_ui(*p))
                {
                    if let Ok(floor) = self.viewport.play_floor(point) {
                        self.update_escalator_preview(floor);
                    } else {
                        self.viewport.highlight(None, &[]);
                    }
                }
                false
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => super::input::ui_point(window, [*x, *y])
                .is_some_and(|p| self.viewport.contains_ui(p)),
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let Some(point) = super::input::ui_point(window, [*x, *y])
                    .filter(|p| self.viewport.contains_ui(*p))
                else {
                    return false;
                };
                let result = self.escalator_click(point);
                self.report(result);
                true
            }
            _ => false,
        }
    }
}

pub(super) fn draw_options(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if let Some(Draft::Free {
        dimensions,
        turn: _,
        hover: _,
    }) = app.escalator
    {
        ui::button(
            vertices,
            hits,
            [5.0, 143.0, 94.0, 11.0],
            &format!("Length: {:.2} m", dimensions.length),
            Action::Escalator(Command::Length),
            true,
        );
        ui::button(
            vertices,
            hits,
            [5.0, 157.0, 94.0, 11.0],
            "Rotate 90 deg",
            Action::Escalator(Command::Rotate),
            true,
        );
    } else if !app.view.simple || app.dock.advanced {
        ui::button(
            vertices,
            hits,
            [5.0, 143.0, 94.0, 11.0],
            "Free escalator >",
            Action::Escalator(Command::Free),
            app.document.is_some(),
        );
    }
}
