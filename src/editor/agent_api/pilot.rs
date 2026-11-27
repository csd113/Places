//! Resolved domain inputs only. Native viewport picks, dialogs, focus and commit
//! dispatch stay in their existing owners. Preparation takes no live UI writer.

use glam::Vec3;
use serde_json::Value;

use super::SceneStamp;
use crate::{
    editor::{Document, ObjectId, escalators, railings},
    engine::{assets::AssetCatalog, escalator::Fit},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::editor) struct Capability {
    pub id: &'static str,
    pub version: u16,
}

const REGISTRY: [Capability; 3] = [
    RailPath::CAPABILITY,
    ConnectEscalator::CAPABILITY,
    ReplaceFixture::CAPABILITY,
];

/// Every object index in the payload is valid only in this exact scene stamp.
pub(in crate::editor) struct Request<'a> {
    pub capability: Capability,
    pub scene: &'a SceneStamp,
}

#[derive(Debug, Eq, PartialEq)]
pub(in crate::editor) enum Error {
    UnsupportedCapability,
    StaleDocument,
    StaleRevision,
    InvalidInput(&'static str),
    Rejected(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedCapability => f.write_str("Unsupported editor capability or version"),
            Self::StaleDocument => f.write_str("The editor is showing a different document"),
            Self::StaleRevision => f.write_str("The scene changed; refresh before trying again"),
            Self::InvalidInput(message) => f.write_str(message),
            Self::Rejected(message) => f.write_str(message),
        }
    }
}

pub(in crate::editor) trait PilotCommand: Sized {
    const CAPABILITY: Capability;
    type Prepared;

    fn stage(self, doc: &Document) -> Result<Self::Prepared, Error>;

    fn prepare(self, doc: &Document, request: Request<'_>) -> Result<Self::Prepared, Error> {
        if request.capability != Self::CAPABILITY || !REGISTRY.contains(&request.capability) {
            return Err(Error::UnsupportedCapability);
        }
        let live = doc.scene_stamp();
        if !live.same_epoch(request.scene) {
            return Err(Error::StaleDocument);
        }
        if !live.same_revision(request.scene) {
            return Err(Error::StaleRevision);
        }
        self.stage(doc)
    }

    fn prepare_current(self, doc: &Document) -> Result<Self::Prepared, String> {
        self.prepare(
            doc,
            Request {
                capability: Self::CAPABILITY,
                scene: doc.scene_stamp(),
            },
        )
        .map_err(|error| error.to_string())
    }
}

pub(in crate::editor) struct RailPath<'a> {
    pub start: [f32; 3],
    pub middle: &'a [[f32; 3]],
    pub end: [f32; 3],
}

impl PilotCommand for RailPath<'_> {
    const CAPABILITY: Capability = Capability {
        id: "railing.place_path",
        version: 1,
    };
    type Prepared = (Value, ObjectId);

    fn stage(self, doc: &Document) -> Result<Self::Prepared, Error> {
        if ![&self.start, &self.end]
            .into_iter()
            .chain(self.middle)
            .flatten()
            .all(|value| value.is_finite())
        {
            return Err(Error::InvalidInput(railings::FOOTING_CUE));
        }
        railings::candidate(doc, self.start, self.middle, self.end).map_err(Error::Rejected)
    }
}

pub(in crate::editor) struct ConnectEscalator {
    pub lower: Vec3,
    pub upper: Vec3,
}

impl PilotCommand for ConnectEscalator {
    const CAPABILITY: Capability = Capability {
        id: "escalator.connect_landings",
        version: 1,
    };
    type Prepared = Fit;

    fn stage(self, doc: &Document) -> Result<Self::Prepared, Error> {
        if !self.lower.is_finite() || !self.upper.is_finite() {
            return Err(Error::InvalidInput("Pick two valid floor positions"));
        }
        escalators::fit_connection(doc.level(), self.lower, self.upper).map_err(Error::Rejected)
    }
}

pub(in crate::editor) struct ReplaceFixture<'a> {
    pub light: usize,
    pub asset: &'a str,
    pub catalog: &'a AssetCatalog,
}

impl PilotCommand for ReplaceFixture<'_> {
    const CAPABILITY: Capability = Capability {
        id: "light.replace_fixture",
        version: 1,
    };
    type Prepared = Value;

    fn stage(self, doc: &Document) -> Result<Self::Prepared, Error> {
        if doc.level().ceiling_lights.get(self.light).is_none() {
            return Err(Error::InvalidInput(
                "Select a light first, then apply this fixture.",
            ));
        }
        if self
            .catalog
            .get(self.asset)
            .is_none_or(|entry| !entry.is_light())
        {
            return Err(Error::InvalidInput(
                "Choose a light fixture from the catalog",
            ));
        }
        let candidate = doc
            .fixture_candidate(self.light, self.asset)
            .map_err(Error::Rejected)?;
        doc.check_locked_edit(&candidate).map_err(Error::Rejected)?;
        drop(
            doc.validate_candidate(&candidate)
                .map_err(Error::Rejected)?,
        );
        Ok(candidate)
    }
}

#[cfg(test)]
#[path = "../tests/agent_api.rs"]
mod tests;
