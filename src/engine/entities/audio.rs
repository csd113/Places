//! Independent authored sound entities and bounded source identity resolution.

use glam::Vec3;

use super::events::EventKind;
use super::volumes::TriggerVisit;
use super::{AudioEmitter, EntityHandle, EntityId, EntityWorld, LevelDef, Transform, WorldCommand};
use crate::engine::level::ActionDef;

impl EntityWorld {
    pub(super) fn resolve_sound_emitters(&mut self, level: &LevelDef) {
        for def in &level.sound_emitters {
            let id = def.id.trim();
            if id.is_empty() || self.names.contains(id) {
                continue;
            }
            let handle = self.store.insert();
            let _ = self.names.insert(EntityId::new(id), handle);
            let _ = self.components.transforms.insert(
                handle,
                Transform {
                    position: Vec3::new(def.x, def.y, def.z),
                    ..Transform::default()
                },
            );
            drop(self.components.audio.insert(
                handle,
                AudioEmitter {
                    attenuation: def.attenuation,
                    sound: def.sound.clone(),
                    gain: def.gain,
                    looped: def.looped,
                    enabled: def.enabled,
                    playing: def.playing,
                    time_domain: def.time_domain,
                    spatial: Some(def.spatial),
                },
            ));
            self.resolve_bindings_for(handle, &def.bindings);
        }
    }

    /// A stable non-zero key for a non-dynamic entity (audio emitters).
    pub(super) fn stable_key(&self, handle: EntityHandle) -> Option<u64> {
        self.store.contains(handle).then(|| {
            (u64::from(handle.index()) << 32_i32)
                | u64::from(handle.generation())
                | (1_u64 << 63_i32)
        })
    }

    /// Current world anchor for an audio source key, including routed emitters.
    #[must_use]
    pub fn audio_source_position(&self, key: u64) -> Option<Vec3> {
        let handle = self.audio_handle(key)?;
        if let Some(guest) = self
            .guests
            .iter()
            .find(|guest| self.handle_of(&guest.state.id) == Some(handle))
        {
            return Some(guest.acoustic_position());
        }
        if let Some(feltling) = self
            .feltlings
            .iter()
            .find(|actor| self.handle_of(&actor.state.id) == Some(handle))
        {
            return Some(feltling.acoustic_position());
        }
        self.components
            .transforms
            .get(handle)
            .map(|transform| transform.position)
    }

    /// Exact instance identity plus authored/routed anchor for a live audio source.
    /// The native loop substitutes a live physical body center when one exists.
    #[must_use]
    pub fn audio_source_anchor(&self, key: u64) -> Option<(&str, Vec3)> {
        let handle = self.audio_handle(key)?;
        Some((self.id_of(handle)?, self.audio_source_position(key)?))
    }

    /// Per-emitter near/far distance override, resolved by generation-safe source.
    #[must_use]
    pub fn audio_source_attenuation(&self, key: u64) -> Option<[f32; 2]> {
        self.components
            .audio
            .get(self.audio_handle(key)?)?
            .attenuation
    }

    /// Optional per-emitter spatial policy; attached sounds retain catalog defaults.
    #[must_use]
    pub fn audio_source_spatial(&self, key: u64) -> Option<bool> {
        self.components
            .audio
            .get(self.audio_handle(key)?)
            .and_then(|emitter| emitter.spatial)
    }

    // Stable authored keys encode the generation-safe slot. Runtime spawn
    // keys use the existing bounded live-spawn table. Active audio voices
    // never scan every authored emitter or prop each frame.
    fn audio_handle(&self, key: u64) -> Option<EntityHandle> {
        let handle = if key & (1_u64 << 63_i32) != 0 {
            let index = u32::try_from((key >> 32_i32) & 0x7fff_ffff).ok()?;
            let generation = u32::try_from(key & 0xffff_ffff).ok()?;
            EntityHandle::from_parts(index, generation)
        } else {
            self.handle_for_dynamic_key(key)?
        };
        let inactive_guest = self
            .guests
            .iter()
            .any(|guest| self.names.get(&guest.state.id) == Some(handle) && !guest.enabled());
        (self.store.contains(handle)
            && !inactive_guest
            && !self
                .feltlings
                .iter()
                .any(|actor| self.names.get(&actor.state.id) == Some(handle) && !actor.enabled())
            && (self.components.audio.get(handle).is_some()
                || self
                    .guests
                    .iter()
                    .any(|guest| self.handle_of(&guest.state.id) == Some(handle))
                || self
                    .feltlings
                    .iter()
                    .any(|actor| self.handle_of(&actor.state.id) == Some(handle))))
        .then_some(handle)
    }

    /// A trigger owns loops it starts that have an authored stop on departure.
    pub(super) fn volume_loop_visit(&self, handle: EntityHandle) -> Option<TriggerVisit> {
        let event = self.script_event.as_ref()?;
        if event.kind != EventKind::EnterVolume {
            return None;
        }
        let visit = event.trigger_visit.as_ref()?;
        let target_id = self.id_of(handle)?;
        let paired = self.bindings_by_owner.get(&event.subject)?.iter().any(|index| {
            self.bindings.get(*index).is_some_and(|binding| {
                binding.on == EventKind::ExitVolume
                    && binding.key.as_ref().is_none_or(|key| key == &event.key)
                    && binding.actions.iter().any(|action| {
                        matches!(action, ActionDef::StopSound { target }
                            if target.as_deref().map_or(handle == event.subject, |id| id.trim() == target_id))
                    })
            })
        });
        paired.then(|| visit.clone())
    }

    /// Reconciles only live playback owned by a visit whose exit could not be
    /// admitted. No binding, script, condition, cooldown or once flag runs here.
    pub(super) fn stop_dropped_trigger_loops(&mut self) {
        let stopped: Vec<_> = self
            .trigger_loops
            .extract_if(.., |_, visit| visit.exit_dropped())
            .map(|(handle, _)| handle)
            .collect();
        for handle in stopped {
            if let Some(emitter) = self.components.audio.get_mut(handle) {
                emitter.playing = false;
                self.queue_audio_stop(handle);
            }
        }
    }

    pub(super) fn queue_audio_start(&mut self, handle: EntityHandle) {
        let Some(emitter) = self
            .components
            .audio
            .get(handle)
            .filter(|emitter| emitter.enabled && emitter.playing)
        else {
            return;
        };
        let Some(key) = self.dynamic_key(handle).or_else(|| self.stable_key(handle)) else {
            return;
        };
        if self.audio_handle(key).is_none() {
            return;
        }
        let sound = emitter.sound.clone();
        let gain = emitter.gain;
        let looped = emitter.looped;
        let time_domain = emitter.time_domain;
        if !looped {
            let position = self
                .components
                .transforms
                .get(handle)
                .map(|pose| pose.position);
            let radius = emitter.attenuation.map_or(12.0, |[_, far]| far).min(30.0)
                * emitter.gain.clamp(0.0, 1.0);
            if let Some(anchor) = position {
                self.note_stimulus(anchor, radius, "sound", Some(handle));
            }
        }
        self.commands.push(WorldCommand::PlaySound {
            entity: key,
            sound,
            gain,
            looped,
            time_domain,
        });
    }

    pub(super) fn queue_audio_stop(&mut self, handle: EntityHandle) {
        if let Some(key) = self.dynamic_key(handle).or_else(|| self.stable_key(handle)) {
            self.commands.push(WorldCommand::StopSound { entity: key });
        }
    }
}
