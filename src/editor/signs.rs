//! The hub sign artwork is reusable through the existing decal collection.
use super::{
    App,
    actions::Popup,
    document::{ObjectId, signs::ART},
    ui::{self, Action, Hit},
};
use crate::engine::{level::DecalSurface, render::Vertex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Add,
    Open,
    Artwork(usize),
    Face,
    Size(bool),
    Mount,
}
impl App {
    pub(super) fn sign_placement(
        &self,
        point: [f32; 2],
    ) -> Result<(serde_json::Value, ObjectId), String> {
        let doc = self.document.as_ref().ok_or("Open a project")?;
        let (origin, direction) = self.viewport.ray(point).ok_or("Point at a wall")?;
        super::document::signs::placement(
            doc,
            origin,
            direction,
            self.view.snap.then_some(self.step),
            self.sign_artwork,
        )
    }
    pub(super) fn sign_action(&mut self, action: Action) -> Result<bool, String> {
        let Action::Sign(command) = action else {
            return Ok(false);
        };
        if command == Command::Add {
            self.start_placement(super::construction::Tool::WallSign)?;
            return Ok(true);
        }
        let id = self.selected.ok_or("Select a wall sign")?;
        if !matches!(id, ObjectId::Decal(_)) {
            return Err("Select a wall sign".into());
        }
        if !self.editable(id) {
            return Err("Show and unlock this sign in Layers before editing".into());
        }
        if command == Command::Open {
            self.popup = Some(Popup::WallSigns);
            return Ok(true);
        }
        let doc = self.document.as_mut().ok_or("Open a project")?;
        match command {
            Command::Artwork(index) => {
                doc.sign_artwork(id, index)?;
                self.sign_artwork = index;
            }
            Command::Mount => doc.mount_sign(id, self.view.snap.then_some(self.step))?,
            Command::Size(larger) => {
                let mut pose = doc.transform(id)?;
                let factor = if larger { 1.25 } else { 0.8 };
                pose.size[0] *= factor;
                pose.size[1] *= factor;
                doc.set_transform(id, pose)?;
            }
            Command::Face => {
                let ObjectId::Decal(index) = id else {
                    return Err("Select a sign".into());
                };
                let current = doc
                    .level()
                    .decals
                    .get(index)
                    .ok_or("Sign no longer exists")?
                    .surface;
                let next = match current {
                    DecalSurface::WallNorth => DecalSurface::WallEast,
                    DecalSurface::WallEast => DecalSurface::WallSouth,
                    DecalSurface::WallSouth => DecalSurface::WallWest,
                    DecalSurface::WallWest => DecalSurface::WallNorth,
                    DecalSurface::Floor | DecalSurface::Ceiling => {
                        return Err("Select a vertical wall sign".into());
                    }
                };
                doc.mount_sign_facing(id, self.view.snap.then_some(self.step), Some(next))?;
            }
            Command::Add | Command::Open => {}
        }
        self.changed();
        self.popup = Some(Popup::WallSigns);
        self.status = "Sign changed. Mount aligns to the chosen face; Ctrl Z undoes.".into();
        Ok(true)
    }
}
pub(super) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "WALL SIGN / ARTWORK & LETTERING",
        140.0,
        73.0,
        ui::INK,
        70,
    );
    ui::text(
        v,
        "Letters are in the PNG. Choose the printed sign:",
        140.0,
        84.0,
        ui::MUTED,
        74,
    );
    for (i, (_, name)) in ART.iter().enumerate() {
        let x = if i < 5 { 140.0 } else { 263.0 };
        let y = f32::from(u16::try_from(i % 5).unwrap_or(0)).mul_add(12.0, 96.0);
        let active = if let Some(ObjectId::Decal(index)) = app.selected {
            app.document
                .as_ref()
                .and_then(|d| d.level().decals.get(index))
                .is_some_and(|d| {
                    d.material == format!("collection:sign_{}", ART.get(i).map_or("", |a| a.0))
                })
        } else {
            false
        };
        ui::popup_button(
            v,
            hits,
            [x, y, 117.0, 10.0],
            &format!("{}{name}", if active { "> " } else { "" }),
            Action::Sign(Command::Artwork(i)),
        );
    }
    let detail = if let Some(ObjectId::Decal(index)) = app.selected {
        app.document
            .as_ref()
            .and_then(|d| d.level().decals.get(index))
            .map(|d| {
                format!(
                    "{:.2} x {:.2} m / {}",
                    d.width,
                    d.height,
                    match d.surface {
                        DecalSurface::WallNorth => "North (-Z)",
                        DecalSurface::WallSouth => "South (+Z)",
                        DecalSurface::WallWest => "West (-X)",
                        DecalSurface::WallEast => "East (+X)",
                        DecalSurface::Floor => "Floor",
                        DecalSurface::Ceiling => "Ceiling",
                    }
                )
            })
    } else {
        None
    };
    ui::text(
        v,
        detail.as_deref().unwrap_or("Select a sign"),
        140.0,
        158.0,
        ui::INK,
        75,
    );
    for (x, y, w, label, command) in [
        (140.0, 170.0, 76.0, "Smaller", Command::Size(false)),
        (222.0, 170.0, 76.0, "Larger", Command::Size(true)),
        (304.0, 170.0, 76.0, "Facing", Command::Face),
        (140.0, 185.0, 117.0, "Mount to wall", Command::Mount),
    ] {
        ui::popup_button(v, hits, [x, y, w, 11.0], label, Action::Sign(command));
    }
    ui::popup_button(
        v,
        hits,
        [263.0, 185.0, 117.0, 11.0],
        "Move / size / spin >",
        Action::Transform,
    );
    ui::popup_button(
        v,
        hits,
        [140.0, 200.0, 117.0, 11.0],
        "Add Wall Sign",
        Action::Sign(Command::Add),
    );
    ui::popup_button(
        v,
        hits,
        [263.0, 200.0, 117.0, 11.0],
        "Close",
        Action::ClosePopup,
    );
}
