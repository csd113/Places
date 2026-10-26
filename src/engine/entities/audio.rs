//! Independent authored sound entities and bounded source identity resolution.

use glam::Vec3;

use super::{AudioEmitter, EntityHandle, EntityId, EntityWorld, LevelDef, Transform, WorldCommand};

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
        Some((
            self.id_of(handle)?,
            self.components.transforms.get(handle)?.position,
        ))
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
        self.components.audio.get(handle).map(|_| handle)
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
