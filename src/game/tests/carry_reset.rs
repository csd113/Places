//! Reproduces the same-map quest reset gap between `EntityWorld` and the native
//! item adapter. One real Game, three tiny physical bodies and the production
//! carry reconciliation helper verify reset custody, original dormant pose and
//! no duplicate/resurrected unrelated body. No assets, GPU, window or map bake.

use super::{Attachment, CarryRuntime, OwnedItemReset};
use crate::engine::level::LevelDef;
use crate::engine::physics::{BodyMotion, ItemPose, ItemWorld, PhysicsDef, ShapeDef};
use crate::game::{CollisionWorld, Game};
use glam::{Mat4, Quat, Vec3};

struct Fixture {
    game: Game,
    physics: ItemWorld,
    carry: CarryRuntime,
    original: ItemPose,
}

fn fixture() -> Result<Fixture, String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"carry_reset","name":"Carry reset","spawn":{"x":0,"z":0},"props":[
      {"id":"caretaker","model":"mannequin","x":0,"z":-2,"solid":false,"components":[
        {"component":"interactable","prompt":"Talk"},
        {"component":"nav_agent","radius":0.2,"speed_mps":1,"height":1.8},
        {"component":"ai","behavior":"idler"},
        {"component":"carry","item":"requested","socket":"hand.R","idle_clip":"carry_idle","walk_clip":"carry_walk"}
      ]},
      {"id":"requested","model":"core:crate","physics":{"motion":"dynamic","pickup":true,"reset_group":"quest"}}
    ]}"#).map_err(|error| error.to_string())?;
    let mut game = Game::new(
        Vec3::new(0.0, 1.5, 0.0),
        0.0,
        CollisionWorld::from_level(&level),
    );
    let mut physics = ItemWorld::from_level(&level, &[]);
    let definition = PhysicsDef {
        motion: BodyMotion::Dynamic,
        pickup: true,
        reset_group: Some("quest".into()),
        shape: Some(ShapeDef::Sphere { radius: 0.1 }),
        ..PhysicsDef::default()
    };
    let bounds = (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1));
    let original = ItemPose {
        position: Vec3::new(0.0, 1.4, -1.0),
        rotation: Quat::IDENTITY,
    };
    physics.spawn_authored_oriented(
        1,
        &definition,
        bounds,
        original.position,
        original.rotation,
        1.0,
    )?;
    physics.spawn_authored(2, &definition, bounds, Vec3::new(2.0, 1.4, -1.0), 0.0, 1.0)?;
    physics.spawn_authored(3, &definition, bounds, Vec3::new(4.0, 1.4, -1.0), 0.0, 1.0)?;
    assert!(
        physics.remove(3),
        "a deliberately deleted unrelated prop stays deleted"
    );
    assert!(
        physics.pickup(1, Vec3::new(0.0, 1.5, 0.0), Vec3::NEG_Z, 0.0),
        "requested fixture is reachable"
    );
    assert!(
        physics.transfer_held(1),
        "real handoff removes player/solver custody"
    );
    game.entities_mut()
        .confirm_handoff("caretaker", "requested");
    game.entities_mut().confirm_carry_retired("caretaker");
    let mut carry = CarryRuntime::default();
    let _previous_group = carry.item_groups.insert("requested".into(), "quest".into());
    let _previous_attachment = carry.attachments.insert(
        "caretaker".into(),
        Attachment {
            key: 1,
            item: "requested".into(),
            socket: "hand.R".into(),
            local: Mat4::IDENTITY,
            alignment: None,
            retirement_requested: true,
            retired: true,
            bounds: None,
            character_bounds: None,
            item_bounds: None,
            hidden: true,
            operation: None,
            reset: OwnedItemReset {
                definition,
                bounds,
                pose: original,
                scale: 1.0,
                fracture: None,
            },
        },
    );
    Ok(Fixture {
        game,
        physics,
        carry,
        original,
    })
}

#[test]
fn same_map_reset_restores_transferred_body_once_without_touching_other_items() -> Result<(), String>
{
    let Fixture {
        mut game,
        mut physics,
        mut carry,
        original,
    } = fixture()?;
    game.reset_to_spawn();
    assert!(
        !physics.contains_body(1) && carry.owns_group("quest"),
        "the native adapter still owns the transferred body after the engine reset boundary"
    );
    let restored = carry.restore_items(&mut physics)?;
    assert!(
        restored
            .first()
            .is_some_and(|item| item.actor == "caretaker" && item.key == 1),
        "reconciliation supplies the original visual key and actor for restoration"
    );
    assert_eq!(
        restored.len(),
        1,
        "one owned item produces one visual restoration"
    );
    let snapshots = physics.snapshots();
    let requested = snapshots
        .iter()
        .find(|body| body.key == 1)
        .ok_or("requested body not restored")?;
    assert!(
        requested.dormant
            && !requested.held
            && requested.pose.position.distance(original.position) < f32::EPSILON,
        "reset restores the original authored dormant pose without a stale player hold"
    );
    assert!(
        physics.contains_body(2) && !physics.contains_body(3),
        "unrelated existing/deleted bodies retain their identities"
    );
    assert_eq!(snapshots.len(), 2, "restoration creates no duplicate body");
    assert!(
        !carry.owns_group("quest") && carry.restore_items(&mut physics)?.is_empty(),
        "reset clears attachment/group custody and cannot restore twice"
    );
    Ok(())
}

// A tiny material-only source reproduces a real renderer rule: a later glass
// wall overrides the coincident opaque member's emitted face. Using the older
// member as an occluder could retire a fully visible model through the glass.
#[test]
fn translucent_coincident_wall_override_cannot_prove_an_actor_hidden() -> Result<(), String> {
    use crate::engine::{assets::AssetCatalog, materials::MaterialTable};
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"carry_glass","name":"Carry glass","spawn":{"x":0,"z":0},"defaults":{"wall":"core:solid"},"walls":[
      {"x":-1,"z":-3,"width":2,"depth":0.2,"height":3},
      {"x":-1,"z":-3,"width":2,"depth":0.2,"height":3,"material":"core:glass"},
      {"x":5,"z":-3,"width":2,"depth":0.2,"height":3}
    ]}"#).map_err(|error| error.to_string())?;
    let catalog = AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[
      {"id":"core:tex","display_name":"Texture","asset_class":"core","asset_type":"texture","source":"file","model":"shared/fixture.png"},
      {"id":"core:solid","display_name":"Solid","asset_class":"core","asset_type":"material","source":"definition","texture":"core:tex"},
      {"id":"core:glass","display_name":"Glass","asset_class":"core","asset_type":"material","source":"definition","texture":"core:tex","alpha_mode":"blend","opacity":0.5}
    ]}"#,
    )?;
    let materials = MaterialTable::logical(&level, &catalog, None);
    let carry = CarryRuntime::from_level(&level, &materials);
    assert_eq!(
        carry.occluders.len(),
        1,
        "only the separate unambiguous opaque wall is eligible"
    );
    assert!(
        carry.occluders.first().is_some_and(|wall| wall.min_x > 4.0),
        "an earlier opaque member cannot occlude through the later glass render owner"
    );
    Ok(())
}

// Essential regression: retain the original item at contact, settle to authored
// grip without a jump, and freeze that transition during pause. This isolates
// the floating-item bug in 24 matrix samples; no asset, GPU or map-bake cost.
#[test]
fn contact_grip_preserves_first_pose_pauses_and_reaches_authored_metadata() {
    let start = Mat4::from_translation(Vec3::new(0.0, -0.18, 0.0));
    let target = Mat4::from_rotation_translation(Quat::from_rotation_x(0.2), Vec3::ZERO);
    let mut alignment = super::GripAlignment {
        start,
        target,
        elapsed: 0.0,
        first_sample: true,
    };
    assert_eq!(alignment.advance(1.0 / 60.0), start);
    let mut previous = start;
    for _ in 0_u8..24 {
        let current = alignment.advance(1.0 / 60.0);
        assert!(current.w_axis.distance(previous.w_axis) < 0.025);
        assert_eq!(
            alignment.advance(0.0),
            current,
            "pause must freeze the item"
        );
        previous = current;
    }
    assert_eq!(
        previous, target,
        "the contact offset must not remain a held gap"
    );
}

// Exercises the real physics removal on handoff and the native carry adapter's
// shared reinstall for both put-down and reset. A normal later player drop
// must reach the fracture mechanism in either case. No render or asset needed.
#[test]
fn festive_item_still_breaks_after_npc_custody() -> Result<(), String> {
    use crate::engine::physics::{FIXED_STEP, PlayerContext, prop_fracture::Breakable};

    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"festive_return","name":"Festive return","spawn":{"x":1,"z":1},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":3}]}"#,
    ).map_err(|error| error.to_string())?;
    let position = Vec3::new(0.0, 1.4, -1.0);
    let player = PlayerContext {
        eye: Vec3::new(0.0, 1.5, 0.0),
        feet: Vec3::ZERO,
        yaw: 0.0,
        view: Vec3::NEG_Z,
        height: 1.8,
        half: 0.4,
        active: true,
    };
    let prop = OwnedItemReset {
        definition: PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.1 }),
            ..PhysicsDef::default()
        },
        bounds: (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        pose: ItemPose { position, rotation: Quat::IDENTITY },
        scale: 1.0,
        fracture: Some(Breakable::SpruceGlobe),
    };
    for live in [None, Some(prop.pose)] {
        let mut world = ItemWorld::from_level(&level, &[]);
        prop.restore(&mut world, 1, None)?;
        assert!(world.pickup(1, player.eye, player.view, player.yaw), "the original body is reachable");
        assert!(world.transfer_held(1), "handoff removes the original solver owner");
        prop.restore(&mut world, 1, live)?;
        assert!(world.pickup(1, player.eye, player.view, player.yaw), "the original body is reachable");
        assert!(world.drop(), "the player can drop the returned item");
        assert!(world.drain_break_plans().is_empty(), "release alone cannot break it");
        for _ in 0_u8..100 {
            world.step(FIXED_STEP, player, &[]);
        }
        let plans = world.drain_break_plans();
        let plan = plans.first().ok_or("returned item did not fracture on impact")?;
        assert_eq!(plans.len(), 1, "only the returned item qualifies");
        assert_eq!(plan.key, 1, "handoff keeps the authored key");
        assert_eq!(plan.kind.panels().len(), 5, "the restored kind is the original globe");
    }
    Ok(())
}
