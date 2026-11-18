//! Cosmetic delivery fence. Epoch belongs to the emitting runtime, sequence to
//! the saved actor, and generation to one logical head waveform.
use super::Sound;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);
pub(super) fn allocate_epoch() -> Result<u64, String> {
    NEXT_EPOCH
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_exhausted| "Tall Guest audio epoch exhausted".into())
}
#[derive(Default)]
pub struct Fence {
    voices: BTreeMap<u64, Slot>,
}
struct Slot {
    epoch: u64,
    sequence: u64,
    generation: u64,
    stopped: bool,
}
impl Fence {
    pub fn sources(&self) -> impl Iterator<Item = u64> + '_ {
        self.voices.keys().copied()
    }

    pub fn clear(&mut self) {
        self.voices.clear();
    }
    /// Accept only commands emitted by the current actor. A newer update carries
    /// the desired waveform, so losing a start never requires replaying it.
    pub fn admit(&mut self, key: u64, expected: Option<u64>, sound: &Sound) -> bool {
        if expected != Some(sound.epoch) || sound.sequence == 0 {
            return false;
        }
        if !self.voices.contains_key(&key) && self.voices.len() >= 8 {
            return false;
        }
        let slot = self.voices.entry(key).or_insert(Slot {
            epoch: sound.epoch,
            sequence: 0,
            generation: 0,
            stopped: false,
        });
        if slot.epoch != sound.epoch {
            *slot = Slot {
                epoch: sound.epoch,
                sequence: 0,
                generation: 0,
                stopped: false,
            };
        }
        if sound.sequence <= slot.sequence
            || sound.generation < slot.generation
            || (sound.generation == slot.generation && slot.stopped && !sound.stop)
        {
            return false;
        }
        if sound.generation > slot.generation {
            slot.stopped = false;
        }
        slot.sequence = sound.sequence;
        slot.generation = sound.generation;
        if sound.stop {
            slot.stopped = true;
        }
        true
    }
}
