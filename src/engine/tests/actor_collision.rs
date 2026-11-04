//! The former reverse handle scan defines the pose-order oracle. Compare its
//! resolved targets with indexed poses across duplicate/mixed-generation moves;
//! also retain collision, caught-body, vertical separation and overlap escape.
//! A handful of tiny CPU sweeps adds no benchmark or map cost to ordinary tests.
use super::{AiBehavior, AiDef, AiOutcome, AiTarget, AiTickContext, AiWorld, actor_segment_clear};
use crate::engine::{
    collision_index::CollisionIndex,
    door::Doors,
    entities::id::EntityHandle,
    level::{WalkableCeiling, WalkableFloor},
    nav::NavAgentProfile,
};
use glam::Vec3;
use std::collections::HashMap;

#[test]
fn indexed_actor_poses_match_reverse_scan_with_generation_and_order_controls() -> Result<(), String>
{
    let mut world = AiWorld::new();
    let owner = EntityHandle::from_parts(1, 0);
    world.register(
        owner,
        "walker",
        AiDef::default(),
        NavAgentProfile::reference(),
        Vec3::ZERO,
        0.0,
        false,
    );
    let agent = world.agent("walker").ok_or("walker")?;
    let targets: Vec<_> = (0_u16..6)
        .map(|slot| AiTarget {
            handle: EntityHandle::from_parts(u32::from(slot).saturating_add(1), 0),
            instance_id: format!("body_{slot}"),
            role: None,
            behavior: AiBehavior::Idler,
            position: Vec3::new(f32::from(slot), 0.0, 0.0),
            radius: 0.25,
            height: 1.8,
            caught: slot == 5,
        })
        .collect();
    let doors = Doors::default();
    let index = CollisionIndex::empty();
    let floor = WalkableFloor::default();
    let ceiling = WalkableCeiling::default();
    let ctx = AiTickContext {
        delta: 0.05,
        sim_time: 0.0,
        nav: None,
        doors: &doors,
        door_version: 0,
        walls: &[],
        leaves: &[],
        index: &index,
        floor: &floor,
        ceiling: &ceiling,
        targets: &targets,
        stimuli: &[],
        scripted: &[],
        obstacles: &[],
        objects: &[],
        player_visible: &[],
        use_acks: &[],
    };
    let moves = [
        (
            EntityHandle::from_parts(2, 0),
            Vec3::new(0.4, 0.0, 0.0),
            0.0,
            false,
        ),
        (
            EntityHandle::from_parts(3, 0),
            Vec3::new(0.0, 3.0, 0.0),
            0.0,
            false,
        ),
        (EntityHandle::from_parts(2, 1), Vec3::ZERO, 0.0, false),
        (
            EntityHandle::from_parts(2, 0),
            Vec3::new(1.0, 0.0, 1.0),
            0.0,
            false,
        ),
        (
            EntityHandle::from_parts(4, 0),
            Vec3::new(0.0, 0.0, -0.4),
            0.0,
            false,
        ),
    ];
    for reverse in [false, true] {
        let mut out = AiOutcome::default();
        let mut indexed = HashMap::new();
        for update in if reverse {
            moves.into_iter().rev().collect::<Vec<_>>()
        } else {
            moves.to_vec()
        } {
            out.moved.push(update);
            let _old = indexed.insert(update.0, update.1);
            let mut reference = targets.clone();
            for target in &mut reference {
                target.position = out
                    .moved
                    .iter()
                    .rev()
                    .find(|(handle, _, _, _)| *handle == target.handle)
                    .map_or(target.position, |(_, position, _, _)| *position);
            }
            for from in [Vec3::ZERO, Vec3::new(0.3, 0.0, 0.0), Vec3::Y * 3.0] {
                for to in [
                    Vec3::X,
                    Vec3::NEG_X,
                    Vec3::Z,
                    Vec3::NEG_Z,
                    Vec3::new(1.0, 0.0, 1.0),
                ] {
                    assert_eq!(
                        actor_segment_clear(agent, from, to, &ctx, &indexed),
                        actor_segment_clear(
                            agent,
                            from,
                            to,
                            &AiTickContext {
                                targets: &reference,
                                ..ctx
                            },
                            &HashMap::new()
                        ),
                        "pose resolution must preserve last-publication order and full generations",
                    );
                }
            }
        }
    }
    assert!(!actor_segment_clear(
        agent,
        Vec3::ZERO,
        Vec3::X,
        &ctx,
        &HashMap::new()
    ));
    assert!(actor_segment_clear(
        agent,
        Vec3::ZERO,
        Vec3::NEG_X,
        &ctx,
        &HashMap::new()
    ));
    let overlapping = HashMap::from([(EntityHandle::from_parts(2, 0), Vec3::new(0.2, 0.0, 0.0))]);
    assert!(actor_segment_clear(
        agent,
        Vec3::ZERO,
        Vec3::NEG_X,
        &ctx,
        &overlapping
    ));
    assert!(!actor_segment_clear(
        agent,
        Vec3::ZERO,
        Vec3::new(0.1, 0.0, 0.0),
        &ctx,
        &overlapping
    ));
    Ok(())
}
