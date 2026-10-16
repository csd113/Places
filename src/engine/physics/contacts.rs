//! Bounded contact-onset bookkeeping, independent of future presentation.

use std::collections::HashMap;

use rapier3d::prelude::ColliderHandle;

use super::FIXED_STEP;

const MAX_CONTACTS: usize = super::MAX_BODIES * 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Pair([ColliderHandle; 2]);

impl Pair {
    fn new(first: ColliderHandle, second: ColliderHandle) -> Self {
        if first.into_raw_parts() <= second.into_raw_parts() {
            Self([first, second])
        } else {
            Self([second, first])
        }
    }
}

struct ContactState {
    touching: bool,
    seen: bool,
    cooldown: f32,
}

pub(super) struct ContactOnsets {
    pairs: HashMap<Pair, ContactState>,
}

impl Default for ContactOnsets {
    fn default() -> Self {
        Self {
            pairs: HashMap::with_capacity(MAX_CONTACTS),
        }
    }
}

impl ContactOnsets {
    pub fn begin_step(&mut self) {
        // Each pair changes only its own timers; iteration order cannot affect output.
        #[allow(
            clippy::iter_over_hash_type,
            reason = "Independent pair timers have no order-dependent output"
        )]
        for state in self.pairs.values_mut() {
            state.seen = false;
            state.cooldown = (state.cooldown - FIXED_STEP).max(0.0);
        }
    }

    pub fn observe(&mut self, first: ColliderHandle, second: ColliderHandle, impulse: f32) -> bool {
        // Speculative contact geometry alone supplies no physical onset.
        if !impulse.is_finite() || impulse <= 0.0 {
            return false;
        }
        let pair = Pair::new(first, second);
        if self.pairs.len() == MAX_CONTACTS && !self.pairs.contains_key(&pair) {
            return false;
        }
        let state = self.pairs.entry(pair).or_insert(ContactState {
            touching: false,
            seen: false,
            cooldown: 0.0,
        });
        state.seen = true;
        let onset = !state.touching && state.cooldown <= 0.0 && impulse >= 0.01;
        if onset {
            state.cooldown = 0.15;
        }
        onset
    }

    pub fn finish_step(&mut self) {
        self.pairs.retain(|_, state| {
            state.touching = state.seen;
            state.seen || state.cooldown > 0.0
        });
    }

    pub fn remove_collider(&mut self, handle: ColliderHandle) {
        self.pairs.retain(|pair, _| !pair.0.contains(&handle));
    }
}
