//! Essential numerical/continuation regressions, with no asset, bake or native
//! window. These protect the user's strict attention boundary, interrupted
//! gestures and retained guide progress for negligible deterministic CPU cost.
use crate::engine::feltling::policy::{Mode, Policy, Stage, attention_allowed};

#[test]
fn attention_requires_strictly_more_than_twenty_metres() {
    assert!(
        !attention_allowed(400.0),
        "exactly 20 m must not trigger attention"
    );
    assert!(
        !attention_allowed(f32::from_bits(400.0_f32.to_bits().saturating_sub(1))),
        "the immediately nearer float must not trigger"
    );
    assert!(
        attention_allowed(f32::from_bits(400.0_f32.to_bits().saturating_add(1))),
        "the immediately farther float must trigger"
    );
    assert!(
        !attention_allowed(f32::NAN),
        "malformed observations cannot authorize attention"
    );
    assert!(
        !attention_allowed(f32::INFINITY),
        "unbounded observations cannot authorize attention"
    );
    let mut policy = Policy::new(Mode::Guide);
    policy.advance(0.05, 401.0, false);
    assert_eq!(
        policy.stage,
        Stage::AttentionJump,
        "a distant guide calls for attention"
    );
    policy.advance(0.05, 400.0, false);
    assert_eq!(
        policy.stage,
        Stage::Greeting,
        "crossing back to exactly 20 m cancels attention immediately"
    );
    policy.advance(0.05, 401.0, false);
    assert_eq!(
        policy.stage,
        Stage::Greeting,
        "a canceled attention call retains its cooldown across boundary oscillation"
    );
}

#[test]
fn guide_wait_resume_and_serialized_interruption_retain_continuation()
-> Result<(), serde_json::Error> {
    let mut policy = Policy::new(Mode::Guide);
    policy.advance(0.1, 4.0, false);
    for _ in 0_u8..21 {
        policy.advance(0.1, 4.0, false);
    }
    assert_eq!(
        policy.stage,
        Stage::Leading,
        "approach completes the pointing invitation"
    );
    policy.advance(0.1, 121.0, false);
    assert_eq!(
        policy.stage,
        Stage::Waiting,
        "guide stops before abandoning a lagging player"
    );
    policy.advance(0.1, 441.0, false);
    let bytes = serde_json::to_vec(&policy)?;
    let mut restored: Policy = serde_json::from_slice(&bytes)?;
    let elapsed = restored.elapsed.to_bits();
    restored.advance(0.0, 441.0, false);
    assert_eq!(
        restored.elapsed.to_bits(),
        elapsed,
        "pause must not advance or restart a gesture"
    );
    assert_eq!(
        restored.stage,
        Stage::AttentionJump,
        "gesture survives semantic serialization"
    );
    restored.advance(0.1, 36.0, false);
    assert_eq!(
        restored.stage,
        Stage::Leading,
        "close player resumes the existing guide continuation"
    );
    assert!(
        restored.begun,
        "resume must not restart the introductory invitation"
    );
    restored.set_mode(Mode::Talk);
    restored.advance(0.1, 4.0, false);
    restored.set_mode(Mode::Guide);
    restored.advance(0.1, 4.0, false);
    assert_eq!(
        restored.stage,
        Stage::Leading,
        "authored conversation interruption preserves the continuation"
    );
    restored.advance(0.1, 4.0, true);
    assert_eq!(
        restored.stage,
        Stage::Arrived,
        "completed route does not loop or fabricate another destination"
    );
    Ok(())
}

// Escape must never inherit guide greetings or player-distance waiting. This
// protects the real chase from an invisible pause, using no assets or map bake.
#[test]
fn fleeing_route_never_waits_for_player_proximity() {
    let mut policy = Policy::new(Mode::Flee);
    for distance in [0.0_f32, 25.0, 400.0, 10_000.0] {
        policy.advance(0.01, distance, false);
        assert_eq!(
            policy.stage,
            Stage::Leading,
            "escape remains live at every player distance"
        );
    }
    policy.enter(Stage::Blocked);
    policy.advance(0.01, 10_000.0, false);
    assert_eq!(
        policy.stage,
        Stage::Leading,
        "physical blockage retries without a player trigger"
    );
    policy.advance(0.01, 10_000.0, true);
    assert_eq!(
        policy.stage,
        Stage::Arrived,
        "only actual route completion stops escape"
    );
}

// Unique value: distance hysteresis was previously hardcoded independently of
// attention. Protect map-local 20/20, serialized compatibility and malformed
// configuration using only deterministic scalar policy calls.
#[test]
fn map_local_recovery_strict_boundary_and_legacy_defaults() -> Result<(), serde_json::Error> {
    use crate::engine::feltling::policy::Recovery;
    let mut p = Policy::new(Mode::Guide);
    p.begun = true;
    p.stage = Stage::Leading;
    p.recovery = Recovery {
        stop_metres: 20.0,
        resume_metres: 20.0,
    };
    for d in [399.99, 400.0] {
        p.advance(0.1, d, false);
        assert!(!p.waiting);
        assert_eq!(p.stage, Stage::Leading);
    }
    p.advance(0.1, 400.01, false);
    assert!(p.waiting);
    assert_eq!(p.stage, Stage::AttentionJump);
    let mut restored: Policy = serde_json::from_slice(&serde_json::to_vec(&p)?)?;
    restored.advance(0.1, 400.0, false);
    assert!(!restored.waiting);
    assert_eq!(restored.stage, Stage::Leading);
    let mut raw = serde_json::to_value(Policy::new(Mode::Guide))?;
    if let Some(object) = raw.as_object_mut() {
        drop(object.remove("recovery"));
    }
    let mut old: Policy = serde_json::from_value(raw)?;
    old.begun = true;
    old.stage = Stage::Leading;
    old.advance(0.1, 100.01, false);
    assert!(old.waiting);
    old.advance(0.1, 36.01, false);
    assert!(old.waiting);
    old.advance(0.1, 36.0, false);
    assert!(!old.waiting);
    for recovery in [
        Recovery {
            stop_metres: f32::INFINITY,
            resume_metres: 1.0,
        },
        Recovery {
            stop_metres: 20.0,
            resume_metres: 21.0,
        },
        Recovery {
            stop_metres: 20.0,
            resume_metres: 0.0,
        },
    ] {
        assert!(!recovery.is_valid());
    }
    Ok(())
}
