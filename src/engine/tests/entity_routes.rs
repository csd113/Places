//! Tiny pose regressions for real transition and held-pose defects. A single
//! node and three two-key clips need no assets, map bake, renderer or GPU.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entity::{EntityRoute, PoseCue, RouteWorld};
use crate::engine::gltf::{
    AnimationInterpolation, AnimationPath, PropAnimation, PropAnimationChannel, PropModel, PropNode,
};
use crate::engine::level::{LevelDef, RouteStepDef, WalkableFloor};
use crate::engine::render::CharacterAnimator;

fn clip(name: &str, start: f32, end: f32) -> PropAnimation {
    PropAnimation {
        name: name.into(),
        duration: 1.0,
        channels: vec![PropAnimationChannel {
            node: 0,
            path: AnimationPath::Translation,
            interpolation: AnimationInterpolation::Linear,
            times: vec![0.0, 1.0],
            values: vec![start, 0.0, 0.0, end, 0.0, 0.0],
            values_per_key: 3,
        }],
        looped: true,
        reference_speed_mps: None,
        kind: None,
    }
}

fn animator() -> Result<CharacterAnimator, String> {
    CharacterAnimator::new(&PropModel {
        nodes: vec![PropNode {
            name: "body".into(),
            parent: None,
            children: vec![],
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }],
        animations: vec![
            clip("left", 10.0, 10.0),
            clip("right", -10.0, -10.0),
            clip("travel", 0.0, 1.0),
        ],
        ..PropModel::default()
    })
    .ok_or_else(|| "one-node rigid animator was rejected".into())
}

fn cue(name: &str, once: bool, paused: bool) -> PoseCue {
    PoseCue::Clip {
        name: name.into(),
        once,
        paused,
    }
}

fn position(animator: &CharacterAnimator) -> Result<f32, String> {
    animator
        .node_global("body")
        .map(|global| global.transform_point3(Vec3::ZERO).x)
        .ok_or_else(|| "body pose disappeared".into())
}

/// The first cue previously snapped, and an interrupted fade jumped to the
/// previous clip rather than the pose displayed. Four samples protect both.
#[test]
fn first_and_interrupted_cues_blend_from_the_displayed_pose() -> Result<(), String> {
    let mut animator = animator()?;
    let left = cue("LEFT", false, false);
    assert!(animator.update_cued(0.0, &left), "new cue must sample");
    assert!(
        position(&animator)?.abs() < 1.0e-6,
        "first cue at zero time must preserve the displayed rest pose"
    );
    assert!(animator.update_cued(0.18, &left), "fade must progress");
    let displayed = position(&animator)?;
    assert!(
        displayed > 6.3 && displayed < 6.4,
        "first cue must use the shared exponential fade"
    );
    let right = cue("right", false, false);
    assert!(
        animator.update_cued(0.0, &right),
        "interrupted cue must sample"
    );
    assert!(
        (position(&animator)? - displayed).abs() < 1.0e-6,
        "interrupting a fade must preserve the displayed blend"
    );
    assert!(animator.update_cued(0.18, &right), "new fade must progress");
    assert!(
        position(&animator)? > -4.1 && position(&animator)? < -3.9,
        "new cue must blend from the retained displayed pose"
    );
    Ok(())
}

/// Held cues previously re-sampled and requested vertex uploads every frame.
/// The same tiny rig checks pause/resume, completion, restart and a scrub's
/// arrival without turning these into asset or full-scene tests.
#[test]
fn stopped_and_completed_cues_hold_revision_until_the_pose_changes() -> Result<(), String> {
    let mut animator = animator()?;
    let playing = cue("travel", false, false);
    assert!(animator.update_cued(2.0, &playing), "settle the cue fade");
    assert!(animator.update_cued(0.25, &playing), "advance the clip");
    let before_pause = position(&animator)?;
    let revision = animator.revision();
    for delta in [0.0, f32::NAN, -1.0] {
        assert!(
            !animator.update_cued(delta, &playing),
            "zero or invalid elapsed time must preserve the pose"
        );
    }
    assert!(
        !animator.update_cued(0.25, &cue("travel", false, true)),
        "a settled paused clip must not request skinning"
    );
    assert_eq!(animator.revision(), revision, "pause must retain revision");
    assert!(
        (position(&animator)? - before_pause).abs() < 1.0e-6,
        "pause must retain clip time"
    );
    assert!(animator.update_cued(0.25, &playing), "resume must advance");
    assert!(
        (position(&animator)? - 0.5).abs() < 1.0e-6,
        "resume must continue from the paused clip time"
    );

    let once = cue("travel", true, false);
    assert!(animator.update_cued(2.0, &once), "one-shot must finish");
    assert!(animator.take_cue_finished(), "completion must be reported");
    let held_revision = animator.revision();
    assert!(
        !animator.update_cued(1.0, &once),
        "finished one-shot must not request skinning"
    );
    assert_eq!(
        animator.revision(),
        held_revision,
        "finished clip must retain revision"
    );
    assert!(
        !animator.take_cue_finished(),
        "held completion must be emitted only once"
    );
    animator.restart_cue();
    assert!(
        animator.update_cued(0.0, &once),
        "restart must sample even without elapsed time"
    );
    assert!(
        position(&animator)?.abs() < 1.0e-6,
        "restart must show first key"
    );
    assert!(
        animator.update_cued(0.5, &once),
        "restarted clip must advance"
    );

    let scrub = PoseCue::Scrub {
        name: "travel".into(),
        target: 0.0,
    };
    assert!(
        animator.update_cued(0.35, &scrub),
        "scrub must reach its target"
    );
    assert!(
        animator.take_cue_finished(),
        "scrub arrival must be reported"
    );
    let scrub_revision = animator.revision();
    assert!(
        !animator.update_cued(1.0, &scrub),
        "settled scrub must not request skinning"
    );
    assert_eq!(
        animator.revision(),
        scrub_revision,
        "settled scrub must retain revision"
    );
    Ok(())
}

/// A route that reached its endpoint still walked until the next positive
/// tick, so pausing there held a moving pose. One floor and one movement
/// substep protect immediate completion and its reset/pause boundary.
#[test]
fn arriving_route_finishes_in_the_same_update_and_holds_while_paused() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"format_version":3,"id":"route_endpoint","name":"Route endpoint",
        "spawn":{"x":0.0,"z":0.0},
        "rooms":[{"x":-1.0,"z":-1.0,"width":2.0,"depth":2.0}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let world = RouteWorld {
        walls: &[],
        floor: &floor,
        index: &index,
    };
    let route = EntityRoute {
        instance_id: "arrival".into(),
        looped: false,
        steps: vec![RouteStepDef::MoveTo {
            x: 0.06,
            z: 0.0,
            speed: 6.0,
        }],
        spawn_position: Vec3::ZERO,
        spawn_yaw_degrees: 90.0,
        radius: 0.1,
        body_height: 0.2,
        step_height: 0.3,
    };
    let mut state = route.new_state();
    let spawn = state.clone();
    route.advance(&mut state, 0.0, &world);
    assert_eq!(state, spawn, "paused route must preserve its spawn state");
    route.advance(&mut state, 0.01, &world);
    assert!(!state.blocked, "the tiny open floor must allow arrival");
    assert!(
        (state.position.x - 0.06).abs() < 1.0e-6,
        "the movement substep must reach its waypoint"
    );
    assert!(state.finished, "arrival must finish before another tick");
    assert!(
        matches!(state.cue, PoseCue::Idle),
        "arrival must stop walking"
    );
    let arrival = state.clone();
    for delta in [0.0, 0.01] {
        route.advance(&mut state, delta, &world);
        assert_eq!(state, arrival, "finished route must retain its final state");
    }
    assert_eq!(
        route.new_state(),
        spawn,
        "reset must restore the authored route"
    );
    Ok(())
}
