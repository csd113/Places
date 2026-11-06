//! Articulated guests have runtime geometry even though their authored prop is
//! not a generic spawned dynamic. Retire render ownership with a tombstone.
use super::{EntityHandle, EntityWorld, WorldCommand};

impl EntityWorld {
    /// # Errors
    /// Loading must fail rather than install a static render without its controller.
    pub fn validate_guest_capability(&self) -> Result<(), String> {
        self.guest_error
            .as_ref()
            .map_or(Ok(()), |error| Err(error.clone()))
    }

    #[must_use]
    pub fn guest_voice_cue(&self, key: u64) -> Option<&str> {
        self.guests
            .iter()
            .find(|guest| {
                self.handle_of(&guest.state.id)
                    .and_then(|handle| self.stable_key(handle))
                    == Some(key)
            })
            .and_then(crate::engine::tall_guest::Guest::voice_cue)
    }

    #[must_use]
    pub fn guest_audio_epoch(&self, id: &str) -> Option<u64> {
        self.guests
            .iter()
            .find(|guest| guest.state.id == id && guest.enabled())
            .map(crate::engine::tall_guest::Guest::audio_epoch)
    }

    pub(super) fn set_guest_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let key = self
            .handle_of(id)
            .and_then(|handle| self.stable_key(handle));
        let Some(guest) = self.guests.iter_mut().find(|guest| guest.state.id == id) else {
            return false;
        };
        let changed = guest.set_enabled(enabled);
        if !enabled && let Some(entity) = key {
            self.commands.push(WorldCommand::StopSound { entity });
        }
        self.refresh_guest_colliders();
        self.rebuild_entity_frames();
        changed
    }

    pub(super) fn retire_guest(&mut self, handle: EntityHandle) -> bool {
        let Some(id) = self.id_of(handle).map(str::to_string) else {
            return false;
        };
        let Some(index) = self.guests.iter().position(|guest| guest.state.id == id) else {
            return false;
        };
        if self.retired_guest_frames.len() >= 8 {
            return false;
        }
        if let Some(entity) = self.stable_key(handle) {
            self.commands.push(WorldCommand::StopSound { entity });
        }
        let mut frame = self.guests.remove(index).frame();
        frame.opacity = 0.0;
        frame.shadow_only = false;
        frame.glow = None;
        self.retired_guest_frames.push(frame);
        self.refresh_guest_colliders();
        true
    }
}

impl EntityWorld {
    pub(super) fn restore_checkpoint_guests(
        &mut self,
        records: &[crate::engine::tall_guest::Record],
    ) -> Result<(), String> {
        if records.len() != self.guests.len() {
            return Err("Checkpoint Tall Guest membership mismatch".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for record in records {
            if !seen.insert(&record.id) {
                return Err("Duplicate checkpoint Tall Guest".into());
            }
            let guest = self
                .guests
                .iter_mut()
                .find(|guest| guest.state.id == record.id)
                .ok_or("Unknown checkpoint Tall Guest")?;
            guest.restore(record)?;
        }
        self.refresh_guest_colliders();
        Ok(())
    }

    /// # Errors
    /// Call on a private candidate after installing restored physical state.
    pub fn validate_restored_guests(
        &mut self,
        ctx: &super::WorldContext<'_>,
    ) -> Result<(), String> {
        self.validate_restored_feltlings(ctx)?;
        let bodies: Vec<_> = self
            .guests
            .iter()
            .map(crate::engine::tall_guest::Guest::body_colliders)
            .collect();
        let base = self
            .door_colliders
            .get(..self.guest_collider_start)
            .unwrap_or(&[]);
        for (index, guest) in self.guests.iter_mut().enumerate() {
            let mut leaves = base.to_vec();
            leaves.extend(
                self.feltlings
                    .iter()
                    .filter_map(crate::engine::feltling::Feltling::collider),
            );
            for (other, body) in bodies.iter().enumerate() {
                if other != index {
                    leaves.extend_from_slice(body);
                }
            }
            guest.validate_restored_pose(ctx, &leaves, &self.navigation_obstacles)?;
        }
        self.refresh_guest_colliders();
        self.rebuild_entity_frames();
        Ok(())
    }
}
