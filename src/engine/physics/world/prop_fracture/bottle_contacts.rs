//! Captures bottle contacts before a later CCD substep can overwrite their solved impulse.
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use glam::Vec3;
use rapier3d::prelude::{
    ColliderHandle, ColliderSet, CollisionEvent, ContactPair, EventHandler, Real, RigidBodySet,
    SoftBodySet, SoftBodyTearEvent,
};

use super::super::{BodyEntry, ImpactBudget, collider_key, impact_contact, incoming_velocity};
use super::BreakableBody;

pub(in crate::engine::physics::world) struct Witness {
    pub(in crate::engine::physics::world) key: u64,
    pub(in crate::engine::physics::world) pair: [ColliderHandle; 2],
    pub(in crate::engine::physics::world) position: Vec3,
    pub(in crate::engine::physics::world) speed: f32,
    pub(in crate::engine::physics::world) impulse: f32,
    pub(in crate::engine::physics::world) fracture: bool,
}

impl Witness {
    pub(in crate::engine::physics::world) fn pair_key(&self) -> [(u32, u32); 2] {
        let mut key = self.pair.map(ColliderHandle::into_raw_parts);
        key.sort_unstable();
        key
    }
}

pub(in crate::engine::physics::world) struct Step<'a> {
    entries: &'a BTreeMap<u64, BodyEntry>,
    breakables: &'a BTreeMap<u64, BreakableBody>,
    held: Option<u64>,
    player: (ColliderHandle, Vec3),
    first: Mutex<Vec<Witness>>,
}

impl<'a> Step<'a> {
    pub(in crate::engine::physics::world) const fn new(
        entries: &'a BTreeMap<u64, BodyEntry>,
        breakables: &'a BTreeMap<u64, BreakableBody>,
        held: Option<u64>,
        player: (ColliderHandle, Vec3),
    ) -> Self {
        Self {
            entries,
            breakables,
            held,
            player,
            first: Mutex::new(Vec::new()),
        }
    }

    pub(in crate::engine::physics::world) fn finish(self) -> Vec<Witness> {
        self.first
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn candidate(&self, colliders: &ColliderSet, handle: ColliderHandle) -> Option<(u64, bool)> {
        let key = collider_key(colliders, handle)?;
        let entry = self.entries.get(&key)?;
        let bottle = self.breakables.get(&key)?;
        let held = self.held == Some(key);
        (bottle.is_bottle() && !held && !entry.dormant && !entry.recovery.suspended)
            .then_some((key, bottle.can_test(key, entry, held)))
    }
}

impl EventHandler for Step<'_> {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
    }

    fn handle_contact_force_event(
        &self,
        _: Real,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        contact: &ContactPair,
        _: Real,
    ) {
        let impulse = contact.total_impulse_magnitude();
        if !impulse.is_finite() || impulse <= 0.001 {
            return;
        }
        let pair = [contact.collider1, contact.collider2];
        let keys = pair.map(|handle| self.candidate(colliders, handle));
        if keys == [None, None] {
            return;
        }
        let incoming = |handle, at| {
            incoming_velocity(self.entries, (colliders, bodies), self.player, handle, at)
        };
        let Some((position, relative_speed, normal)) = impact_contact(colliders, contact, incoming)
        else {
            return;
        };
        let mut first = self.first.lock().unwrap_or_else(PoisonError::into_inner);
        let directions = [normal, super::super::scaled(normal, -1.0)];
        for ((candidate, handle), direction) in keys.into_iter().zip(pair).zip(directions) {
            if let Some((key, can_fracture)) = candidate
                && !first.iter().any(|previous| previous.key == key)
            {
                // Relative speed alone cannot qualify a hit delivered by the other body.
                let speed = relative_speed.min(incoming(handle, position).dot(direction));
                // Resting or superficial contacts are not an audio fallback.
                // A thrown bottle still spends its own first solved contact.
                if can_fracture || ImpactBudget::meaningful(speed, impulse) {
                    first.push(Witness {
                        key,
                        pair,
                        position,
                        speed,
                        impulse,
                        fracture: false,
                    });
                }
            }
        }
    }

    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}
}

pub(in crate::engine::physics::world) fn commit_witnesses(
    first: &mut [Witness],
    breakables: &mut BTreeMap<u64, BreakableBody>,
    entries: &BTreeMap<u64, BodyEntry>,
) {
    for contact in first {
        if let Some(entry) = entries.get(&contact.key)
            && let Some(bottle) = breakables.get_mut(&contact.key)
        {
            contact.fracture =
                bottle.witness_contact(contact.key, entry, false, contact.speed, contact.impulse);
        }
    }
}
