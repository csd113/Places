//! Contextual geometric operations use the document's single edit history.

use super::App;
use super::actions::Popup;
use super::ui::Action;

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This handler owns only its contextual commands; other commands return to the exhaustive application dispatcher."
    )]
    pub(super) fn geometry_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Geometry => self.popup = Some(Popup::Geometry),
            Action::Shape => {
                let document = self.document.as_ref().ok_or("Open a project first")?;
                let transform = document.transform(self.selected.ok_or("Select a floor first")?)?;
                let [x, _, z] = transform.position;
                let [width, _, depth] = transform.size;
                let snapped = |value: f32| {
                    if self.view.snap {
                        (value / self.step).round() * self.step
                    } else {
                        value
                    }
                };
                self.opening_rect = Some([
                    snapped(x + width / 3.0),
                    snapped(z + depth / 3.0),
                    snapped(width / 3.0),
                    snapped(depth / 3.0),
                ]);
                self.popup = Some(Popup::Shape);
            }
            Action::UpperFloor | Action::CutOpening => {
                let id = self.selected.ok_or("Select a floor first")?;
                let document = self.document.as_mut().ok_or("Open a project first")?;
                self.selected = if action == Action::UpperFloor {
                    Some(document.add_upper_floor(id)?)
                } else {
                    document
                        .cut_floor_opening(
                            id,
                            self.opening_rect
                                .ok_or("Open floor shape tools to choose the opening rectangle")?,
                        )?
                        .first()
                        .copied()
                };
                self.popup = None;
                self.changed();
                self.viewport.frame_next_preview();
                self.status = if action == Action::UpperFloor {
                    "Added upper floor. Connect storeys through a separate side shaft; lower ceiling retained."
                } else {
                    "Split floor and ceiling around the opening. Check every storey's ceiling and access route."
                }.into();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
