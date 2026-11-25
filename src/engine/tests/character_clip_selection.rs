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

// A real skim can intersect the sloped seabed even if both placement centers
// are in water. Sample the actual 1.2 s swim and both installed skin poses at
// >=120 Hz using native transforms and surfaces. One small GLB, no bake/GPU.
#[test]
fn shipped_content_beach_fish_poses_clear_seabed_and_waterline() -> Result<(), String> {
    use super::CharacterAnimator;
    use crate::engine::level::{LevelDef, LevelSurfaces};
    use glam::Vec3;

    let beach = LevelDef::from_json(include_str!(
        "../../../projects/environments/levels/beach.json"
    ))
    .map_err(|error| error.to_string())?;
    let surfaces = LevelSurfaces::new(&beach);
    let model = crate::engine::gltf::parse_glb(include_bytes!(
        "../../../assets/entities/beach_fish/model/beach_fish.glb"
    ))
    .map_err(|error| error.to_string())?;
    let mut animator = CharacterAnimator::new(&model).ok_or("swim animator is missing")?;
    let frames = animator
        .clip_duration("swim")
        .ok_or("authored swim clip is missing")?
        .mul_add(120.0_f32, 0.0_f32)
        .ceil();
    assert!(
        (1.0..=480.0).contains(&frames),
        "the bounded fixture must sample an actual swim cycle"
    );
    let placements = beach
        .props
        .iter()
        .filter(|prop| prop.model == "beach:fish")
        .map(|prop| {
            let id = prop.id.as_deref().ok_or("authored fish lacks an ID")?;
            let origin = surfaces
                .floor_y_at(prop.x, prop.z)
                .ok_or_else(|| format!("{id} has no authored floor"))?;
            Ok((id, prop.placement_matrix(origin)))
        })
        .collect::<Result<Vec<_>, String>>()?;
    assert_eq!(
        placements.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        ["shallows_fish", "shallows_fish_pair"],
        "both authored fish are covered"
    );
    assert!(
        !model.vertices.is_empty(),
        "must sample the actual fish mesh"
    );
    assert_eq!(
        model.vertices.len(),
        model.joints.len(),
        "every vertex needs joint slots"
    );
    assert_eq!(
        model.vertices.len(),
        model.weights.len(),
        "every vertex needs skin weights"
    );
    for frame in (0_u16..=480)
        .map(f32::from)
        .take_while(|frame| *frame <= frames)
    {
        let _sample = animator.sample_fraction("swim", frame.mul_add(frames.recip(), 0.0));
        for ((vertex, joints), weights) in
            model.vertices.iter().zip(&model.joints).zip(&model.weights)
        {
            let pose = Vec3::from_array(animator.skin_position(*joints, *weights, vertex.pos));
            for (id, placement) in &placements {
                let point = placement.transform_point3(pose);
                let floor = surfaces
                    .floor_y_at(point.x, point.z)
                    .ok_or_else(|| format!("{id} has no supporting seabed at {point:?}"))?;
                let water = beach
                    .water
                    .iter()
                    .rev()
                    .find(|volume| volume.swimming && volume.contains(point.x, point.z))
                    .ok_or_else(|| format!("{id} swims out of water at {point:?}"))?;
                assert!(
                    point.y >= floor.mul_add(1.0, 0.02)
                        && point.y.mul_add(1.0, 0.02) <= water.surface_y,
                    "{id} swim frame {frame} clips the seabed or waterline: {point:?}"
                );
            }
        }
    }
    Ok(())
}
