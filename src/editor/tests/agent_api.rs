use super::{Capability, ConnectEscalator, Error, PilotCommand, RailPath, ReplaceFixture, Request};
use crate::{
    editor::{Document, ObjectId, document::ObjectKind},
    engine::assets::AssetCatalog,
};
use glam::Vec3;
use std::{fs, path::PathBuf};

struct Project(PathBuf);

impl Project {
    fn new(suffix: &str) -> Result<(Self, Document), String> {
        let path = std::env::temp_dir().join(format!(
            "places-editor-agent-api-{}-{suffix}",
            std::process::id()
        ));
        let doc = Document::create(&path, "agent_api", "Agent API test")?;
        Ok((Self(path), doc))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

// One editor-authored starter scene (no assets, GPU or bake) catches stale
// indexed targets, unsupported versions and rejection changing edit history.
#[test]
fn preparation_rejects_stale_or_invalid_inputs_without_editing() -> Result<(), String> {
    let (project, mut doc) = Project::new("rejection")?;
    let before = doc.raw().clone();
    let original = doc.scene_stamp().clone();
    let request = |capability| Request {
        capability,
        scene: &original,
    };
    let catalog = AssetCatalog::from_json_str(include_str!("../../../assets/catalog.json"))?;
    let fixture = |asset| ReplaceFixture {
        light: 0,
        asset,
        catalog: &catalog,
    };
    let rail = || RailPath {
        start: [f32::NAN, 0.0, 0.0],
        middle: &[],
        end: [1.0, 0.0, 1.0],
    };
    let connect = ConnectEscalator {
        lower: Vec3::ZERO,
        upper: Vec3::X,
    };
    assert!(matches!(
        rail().prepare(&doc, request(RailPath::CAPABILITY)),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        fixture("core:chair").prepare(&doc, request(ReplaceFixture::CAPABILITY)),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        fixture("core:pool_light_wall").prepare(&doc, request(ReplaceFixture::CAPABILITY)),
        Err(Error::Rejected(message)) if message.contains("world height")
    ));
    assert!(matches!(
        connect.prepare(&doc, request(ConnectEscalator::CAPABILITY)),
        Err(Error::Rejected(_))
    ));
    let future = Capability {
        version: 2,
        ..RailPath::CAPABILITY
    };
    assert!(matches!(
        rail().prepare(&doc, request(future)),
        Err(Error::UnsupportedCapability)
    ));
    assert_eq!(doc.raw(), &before);
    assert!(!doc.is_dirty() && !doc.can_undo() && !doc.can_redo());

    let id = doc.add(ObjectKind::Light, [2.0, 1.0, 2.0], None)?;
    assert!(matches!(id, ObjectId::Light(_)));
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    let replace_wall = || fixture("core:pool_light_wall");
    assert_eq!(
        replace_wall().prepare(&doc, request(ReplaceFixture::CAPABILITY)),
        Err(Error::StaleRevision),
        "undo must not make an old indexed handle valid again"
    );
    assert!(!doc.is_dirty() && !doc.can_undo() && doc.can_redo());

    let reopened = Document::open(&project.0)?;
    assert_eq!(
        replace_wall().prepare(&reopened, request(ReplaceFixture::CAPABILITY)),
        Err(Error::StaleDocument),
        "even an identical reopened scene must not accept an old handle"
    );
    assert_eq!(reopened.raw(), &before);
    Ok(())
}

// The catalog path and native document action must choose the same carrier
// without moving the light. Preparing does not consume the previous state.
#[test]
fn fixture_preparation_matches_native_mount_and_one_step_undo() -> Result<(), String> {
    let (project, mut setup) = Project::new("fixture")?;
    let mut pose = setup.transform(ObjectId::Light(0))?;
    pose.position[1] = 1.6;
    setup.set_transform(ObjectId::Light(0), pose)?;
    setup.save_all()?;
    drop(setup);
    let mut doc = Document::open(&project.0)?;
    let before = doc.raw().clone();
    let catalog = AssetCatalog::from_json_str(include_str!("../../../assets/catalog.json"))?;
    let asset = "core:pool_light_wall";
    let prepared = ReplaceFixture {
        light: 0,
        asset,
        catalog: &catalog,
    }
    .prepare_current(&doc)?;
    assert_eq!(
        prepared
            .pointer("/ceiling_lights/0/mount")
            .and_then(serde_json::Value::as_str),
        Some("wall")
    );
    assert_eq!(
        prepared
            .pointer("/ceiling_lights/0/fixture")
            .and_then(serde_json::Value::as_str),
        Some(asset)
    );
    for axis in ["x", "y", "z"] {
        let pointer = format!("/ceiling_lights/0/{axis}");
        assert_eq!(prepared.pointer(&pointer), before.pointer(&pointer));
    }
    assert_eq!(doc.raw(), &before);
    assert!(!doc.can_undo() && !doc.is_dirty());

    doc.set_gameplay_asset(ObjectId::Light(0), asset, &catalog)?;
    assert_eq!(doc.raw(), &prepared);
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    assert!(!doc.can_undo());
    assert!(doc.redo());
    assert_eq!(doc.raw(), &prepared);
    Ok(())
}
