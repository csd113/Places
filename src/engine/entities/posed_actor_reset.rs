//! Publish the native controllers' authored start to the entity tables.
use super::{EntityWorld, WorldCommand};

impl EntityWorld {
    pub(super) fn reset_posed_actors(&mut self) {
        if self.guests.is_empty() && self.feltlings.is_empty() {
            return;
        }
        let handles: Vec<_> = self
            .guests
            .iter()
            .map(|actor| actor.state.id.as_str())
            .chain(self.feltlings.iter().map(|actor| actor.state.id.as_str()))
            .filter_map(|id| self.handle_of(id))
            .collect();
        let sources: Vec<_> = handles
            .iter()
            .filter_map(|&handle| self.stable_key(handle))
            .collect();
        self.commands.retain(|command| {
            !matches!(command, WorldCommand::PlaySound { entity, sound: _, gain: _, looped: _, time_domain: _ } if sources.contains(entity))
        });
        for handle in handles {
            if let Some(glow) = self.components.glows.get_mut(handle) {
                glow.enabled = glow.authored_enabled;
            }
            if !self.components.audio.contains(handle) {
                self.queue_audio_stop(handle);
            }
            if !self
                .authored_states
                .iter()
                .any(|(authored, _)| *authored == handle)
            {
                drop(self.components.states.remove(handle));
            }
        }
        // A full world reset replaces both ends of capture at once. Live
        // interruptions still use the capture controller's swept release.
        for guest in &mut self.guests {
            guest.reset_to_spawn();
        }
        for feltling in &mut self.feltlings {
            feltling.reset_to_spawn();
        }
        self.sync_posed_actor_transforms();
    }

    pub(super) fn sync_posed_actor_transforms(&mut self) {
        let guests = self.guests.iter().map(|guest| {
            let state = &guest.state;
            (state.id.as_str(), state.position, state.yaw)
        });
        let feltlings = self.feltlings.iter().map(|feltling| {
            let state = &feltling.state;
            (state.id.as_str(), state.position, state.yaw)
        });
        for (id, position, yaw) in guests.chain(feltlings) {
            if let Some(handle) = self.names.get(id)
                && let Some(transform) = self.components.transforms.get_mut(handle)
            {
                transform.position = position;
                transform.yaw_degrees = yaw.to_degrees();
            }
        }
    }
}
