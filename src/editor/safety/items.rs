//! Dormant items are checked using their real authored rigid shapes, without
//! stepping physics or changing the source. This is an explicit Safety scan.
use super::{SafetyCategory, SafetyFinding, SafetyReport, SafetySeverity, marker_bounds};
use crate::editor::document::ObjectId;
use crate::engine::{
    level::{LevelDef, LevelSurfaces},
    loader::PropCatalog,
    physics::ItemWorld,
    props::PropAssets,
};
use glam::Vec3;
pub(super) fn check(level: &LevelDef, report: &mut SafetyReport) {
    if !level
        .props
        .iter()
        .any(crate::engine::level::PropDef::is_physics_movable)
    {
        return;
    }
    let catalog = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    let mut world = scenery(level, &catalog, &mut assets, report);
    let surfaces = LevelSurfaces::new(level);
    let mut admitted = Vec::new();
    for (index, prop) in level
        .props
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_physics_movable())
    {
        let key = u64::try_from(index).unwrap_or(u64::MAX);
        let position = Vec3::new(
            prop.x,
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
            prop.z,
        );
        let result = (|| {
            let path = catalog.get(&prop.model).model.ok_or("No installed model")?;
            let asset = assets.resolve(&path)?;
            let (min, max) = asset.model.bounds().ok_or("No model bounds")?;
            world.spawn_authored_oriented(
                key,
                prop.physics.as_ref().ok_or("No physics definition")?,
                (Vec3::from_array(min), Vec3::from_array(max)),
                position,
                prop.orientation(),
                prop.scale,
            )
        })();
        if let Err(error) = result {
            report.findings.push(SafetyFinding {
                check: "item-contact-unchecked",
                category: SafetyCategory::Review,
                severity: SafetySeverity::Warning,
                title: format!("Item {} contact not checked", index.saturating_add(1)),
                risk: error,
                position: position.to_array(),
                bounds: marker_bounds(position.to_array()),
                objects: vec![ObjectId::Prop(index)],
                surfaces: Vec::new(),
                fix: None,
                placement_ignore: None,
            });
        } else {
            admitted.push((index, key, position));
        }
    }
    for (index, key, position) in admitted {
        if let Some(depth) = world
            .body_penetration_depth(key)
            .filter(|depth| *depth > 0.002)
        {
            report.findings.push(SafetyFinding{check:"item-contact-penetration",category:SafetyCategory::Collision,severity:SafetySeverity::Warning,title:format!("Dormant item {} penetrates geometry",index.saturating_add(1)),risk:format!("Actual authored shape penetrates a world/item collider by {depth:.4} m. A dormant item may pick up yet remain trapped. Move it explicitly; verify lift/drop/throw in Play. No automatic placement correction."),position:position.to_array(),bounds:marker_bounds(position.to_array()),objects:vec![ObjectId::Prop(index)],surfaces:Vec::new(),fix:None,placement_ignore:None});
        }
    }
}

fn scenery(
    level: &LevelDef,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    report: &mut SafetyReport,
) -> ItemWorld {
    let mut world = ItemWorld::from_level(level, &level.item_collision_aabbs());
    let surfaces = LevelSurfaces::new(level);
    for prop in level
        .props
        .iter()
        .filter(|prop| prop.has_fixed_physics_shape())
    {
        let result = (|| {
            let path = catalog.get(&prop.model).model.ok_or("No fixed model")?;
            let asset = assets.resolve(&path)?;
            let (min, max) = asset.model.bounds().ok_or("No fixed model bounds")?;
            world.install_fixed_shape_oriented(
                prop.physics.as_ref().ok_or("Missing fixed definition")?,
                (Vec3::from_array(min), Vec3::from_array(max)),
                Vec3::new(
                    prop.x,
                    surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0) + prop.y,
                    prop.z,
                ),
                prop.orientation(),
                prop.scale,
            )
        })();
        if let Err(error) = result {
            report.findings.push(SafetyFinding {
                check: "item-scenery-unchecked",
                category: SafetyCategory::Review,
                severity: SafetySeverity::Warning,
                title: format!("Fixed item scenery {} not checked", prop.model),
                risk: error,
                position: [prop.x, prop.y, prop.z],
                bounds: marker_bounds([prop.x, prop.y, prop.z]),
                objects: Vec::new(),
                surfaces: Vec::new(),
                fix: None,
                placement_ignore: None,
            });
        }
    }
    world
}
