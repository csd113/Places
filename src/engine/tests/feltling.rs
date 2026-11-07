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
