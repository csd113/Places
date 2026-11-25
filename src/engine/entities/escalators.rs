//! Narrow pose publication and one frame snapshot for all powered treads.

use crate::engine::entity::{EntityFrame, PoseCue};
use crate::engine::escalator::Escalator;

impl super::EntityWorld {
    pub(crate) fn begin_escalator_frame(&mut self, seconds: f32) {
        let mechanisms = &mut self.fixed_mechanisms;
        if mechanisms.escalators.is_empty() {
            return;
        }
        mechanisms.belt_seconds = seconds;
        let first = mechanisms.escalator_frame_start;
        for (index, escalator) in mechanisms.escalators.iter_mut().enumerate() {
            if let Some(phase) = escalator.begin_frame(seconds)
                && let Some(frame) = self.entity_frames.get_mut(first.saturating_add(index))
                && let Some(PoseCue::Sample { name: _, progress }) = frame.cue.as_mut()
            {
                *progress = phase;
            }
        }
    }

    #[must_use]
    pub const fn escalator_frame_seconds(&self) -> f32 {
        self.fixed_mechanisms.belt_seconds
    }

    #[must_use]
    pub const fn escalators(&self) -> &[Escalator] {
        self.fixed_mechanisms.escalators.as_slice()
    }

    pub(super) fn append_escalator_frames(&mut self) {
        let mechanisms = &mut self.fixed_mechanisms;
        if mechanisms.escalators.is_empty() {
            return;
        }
        self.entity_frames
            .retain(|frame| !mechanisms.escalator_slots.contains_key(&frame.instance_id));
        mechanisms.escalator_frame_start = self.entity_frames.len();
        self.entity_frames
            .extend(mechanisms.escalators.iter().map(|escalator| EntityFrame {
                rigid_transform: None,
                instance_id: escalator.id.clone(),
                transform: None,
                cue: Some(escalator.cue()),
                opacity: 1.0,
                shadow_only: false,
                glow: None,
                balloon: None,
            }));
    }
}

pub(super) fn interaction_geometry(
    prop: &crate::engine::level::PropDef,
    floor_y: f32,
    size: [f32; 3],
) -> (crate::engine::spatial::Aabb, [f32; 3]) {
    prop.escalator
        .as_ref()
        .and_then(|_| crate::engine::escalator::Lane::new(prop, floor_y + prop.y))
        .map_or_else(
            || {
                (
                    prop.placement_bounds(floor_y, prop.size.unwrap_or(super::PROP_FALLBACK_SIZE)),
                    size,
                )
            },
            crate::engine::escalator::Lane::control_proxy,
        )
}
