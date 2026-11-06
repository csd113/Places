//! Explicit native diagnostic delivery faults. Never changes authoritative state.
use crate::engine::tall_guest::Sound;

pub(super) struct Delivery {
    enabled: bool,
    delayed: Option<(u64, Sound)>,
    delayed_once: bool,
    dropped_stop: bool,
}
impl Delivery {
    pub(super) fn new() -> Self {
        Self {
            enabled: std::env::var_os("PLACES_BENCH").is_some()
                && std::env::var_os("PLACES_TALL_GUEST_AUDIO_FAULT").is_some(),
            delayed: None,
            delayed_once: false,
            dropped_stop: false,
        }
    }
    pub(super) fn apply(&mut self, sounds: Vec<(u64, Sound)>) -> Vec<(u64, Sound)> {
        if !self.enabled {
            return sounds;
        }
        let stale = self.delayed.take();
        let mut result = Vec::new();
        for (key, sound) in sounds {
            if sound.forward.is_none() {
                result.push((key, sound));
                continue;
            }
            if !self.delayed_once
                && sound.cue == "tg_pursuit_shriek_loop"
                && !sound.update
                && !sound.stop
            {
                self.delayed_once = true;
                self.delayed = Some((key, sound));
                crate::engine::logging::info(
                    "[guest-audio-fault] delayed first start past its desired-state update",
                );
            } else if !self.dropped_stop && sound.cue == "tg_pursuit_shriek_loop" && sound.stop {
                self.dropped_stop = true;
                crate::engine::logging::info(
                    "[guest-audio-fault] dropped first stop; authoritative silence must retire it",
                );
            } else {
                // Duplicate commands exercise the same native consumer fence.
                result.push((key, sound.clone()));
                result.push((key, sound));
            }
        }
        if let Some(old) = stale {
            crate::engine::logging::info(
                "[guest-audio-fault] delivered stale start after newer sequence",
            );
            result.push(old);
        }
        result
    }
}
