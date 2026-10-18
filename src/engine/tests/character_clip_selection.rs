//! Reordered pose-library regression: no assets, GPU or map bake.
//! Carry clips must not replace the original neutral idle/walk interfaces.

use super::{ClipPlayer, CueState, IDLE, WALKING, neutral_idle_clip};
use crate::engine::gltf::PropAnimation;

#[test]
fn task_clips_do_not_replace_neutral_standing_and_walking() -> Result<(), String> {
    let animations: Vec<_> = ["carry_idle", "carry_walk", "pose_stand", "run", "walk"]
        .into_iter()
        .map(|name| PropAnimation {
            name: name.to_string(),
            duration: 1.0,
            channels: Vec::new(),
            looped: true,
            reference_speed_mps: None,
            kind: None,
        })
        .collect();
    let player = ClipPlayer::new(&animations, 0).ok_or("missing clip player")?;
    assert_eq!(player.state_clip.get(IDLE), Some(&2));
    assert_eq!(player.state_clip.get(WALKING), Some(&4));
    assert_eq!(
        neutral_idle_clip(
            animations
                .iter()
                .enumerate()
                .map(|(index, clip)| (index, clip.name.as_str()))
        ),
        Some(2),
        "the explicit Idle cue uses the same neutral standing fallback"
    );
    assert_eq!(
        neutral_idle_clip([(2, "pose_stand"), (9, "IDLE")].into_iter()),
        Some(9),
        "a literal idle keeps its established case-insensitive precedence"
    );
    let partial = animations.get(..2).ok_or("missing partial fixture")?;
    let legacy = ClipPlayer::new(partial, 0).ok_or("missing partial clip player")?;
    assert_eq!(legacy.state_clip.get(IDLE), Some(&0));
    assert_eq!(legacy.state_clip.get(WALKING), Some(&1));
    Ok(())
}

#[test]
fn zero_duration_pose_and_one_shot_clocks_remain_bounded() {
    // A looped HOLD pose must not accumulate an unbounded unused clock;
    // a real one-shot must finish exactly once and retain its final time.
    let mut cue = CueState {
        clip: 0,
        time: 0.0,
        once: false,
        paused: false,
        finished: false,
        scrub: None,
    };
    assert!(!cue.advance(100_000.0, 2.0, 0.0));
    assert_eq!(cue.time, 0.0);
    cue.once = true;
    assert!(cue.advance(10.0, 1.5, 4.0));
    assert_eq!(cue.time, 4.0);
    assert!(!cue.advance(10.0, 1.5, 4.0));
    assert_eq!(cue.time, 4.0);
    cue.once = false;
    assert!(!cue.advance(0.5, 1.0, 4.0));
    assert_eq!(cue.time, 0.5);
}
