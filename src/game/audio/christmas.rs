//! Original Christmas prop audio. Only authored bells and active boxes are ticked.
use crate::engine::audio::{AudioBus, AudioSystem, MAX_VOICES, PlayRequest, SourceId};
use crate::engine::entities::EntityWorld;
use crate::engine::level::LevelDef;
use crate::engine::physics::ItemWorld;
use glam::Vec3;

const MUSIC_BOX: &str = "christmas:windup_music_box";
const BELLS: &str = "christmas:brass_sleigh_bells";
const WIND: &str = "christmas_music_box_wind";
const TUNE: &str = "christmas_music_box_run_down";
const SOFT: &str = "christmas_sleigh_jingle_soft";
const FIRM: &str = "christmas_sleigh_jingle_firm";

pub fn music_box_preset(model: &str) -> Option<serde_json::Value> {
    (model == MUSIC_BOX).then(|| {
        serde_json::json!({
            "display_name": "Wind-up Music Box",
            "components": [
                {"component":"interactable","reach":1.8_f64,"prompt":"Wind music box"},
                {"component":"audio","sound":WIND}
            ],
            "bindings": [{"on":"interact","actions":[{"action":"play_sound"}]}]
        })
    })
}

#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct Witness {
    pub wind_started: u64,
    pub tune_started: u64,
    pub tune_finished: u64,
    pub rewind_while_playing: u64,
    pub soft: u64,
    pub firm: u64,
    pub last_jingle: Option<Jingle>,
}

#[derive(Clone, Copy, serde::Serialize)]
pub struct Jingle {
    key: u64,
    held: bool,
    cue: &'static str,
    linear: f32,
    angular: f32,
    gain: f32,
    interval: f32,
}

#[derive(Default)]
pub(super) struct Props {
    boxes: Vec<BoxVoice>,
    bells: Vec<Bell>,
    pub(super) witness: Witness,
}

struct BoxVoice {
    source: SourceId,
    request: PlayRequest,
    tuning: bool,
}

struct Bell {
    key: u64,
    cooldown: f32,
    moving: bool,
}

impl Props {
    pub(super) fn prepare(&mut self, level: &LevelDef) {
        self.bells = level
            .props
            .iter()
            .enumerate()
            .filter(|(_, prop)| prop.model == BELLS && prop.is_physics_movable())
            .map(|(index, _)| Bell {
                key: crate::engine::physics::prop_fracture::authored_item_key(index),
                cooldown: 0.0,
                moving: false,
            })
            .collect();
    }

    pub(super) fn play(
        &mut self,
        system: &mut AudioSystem,
        cue: &str,
        request: PlayRequest,
    ) -> bool {
        if cue != WIND {
            // The newer authored command owns this source from here on. The
            // mixer may stop the wind before admitting its replacement, so a
            // missing wind on the next tick cannot advance the old sequence.
            if let Some(source) = request.source {
                self.boxes.retain(|voice| voice.source != source);
            }
            return system.play(cue, request);
        }
        let Some(source) = request.source else {
            return false;
        };
        if self.boxes.iter().any(|voice| voice.source == source)
            || system.source_playing(source, WIND)
            || system.source_playing(source, TUNE)
        {
            self.witness.rewind_while_playing = self.witness.rewind_while_playing.saturating_add(1);
            return false;
        }
        if self.boxes.len() >= MAX_VOICES || request.position.is_none() {
            return false;
        }
        let wind_request = PlayRequest {
            looping: Some(false),
            spatial: Some(true),
            bus: AudioBus::Prop,
            ..request
        };
        if !system.play(WIND, wind_request) {
            return false;
        }
        self.boxes.push(BoxVoice {
            source,
            request: wind_request,
            tuning: false,
        });
        self.witness.wind_started = self.witness.wind_started.saturating_add(1);
        true
    }

    pub(super) fn stop(&mut self, system: &mut AudioSystem, source: SourceId) {
        self.boxes.retain(|voice| voice.source != source);
        system.stop_source(source);
    }

    pub(super) fn update(
        &mut self,
        entities: &EntityWorld,
        items: &ItemWorld,
        delta: f32,
        system: &mut AudioSystem,
    ) {
        let witness = &mut self.witness;
        self.boxes.retain_mut(|voice| {
            let Some(position) = entities.audio_source_position(voice.source.0) else {
                system.stop_source(voice.source);
                return false;
            };
            if system.source_playing(voice.source, if voice.tuning { TUNE } else { WIND }) {
                return true;
            }
            if voice.tuning {
                witness.tune_finished = witness.tune_finished.saturating_add(1);
                return false;
            }
            voice.tuning = system.play(
                TUNE,
                PlayRequest {
                    position: Some(position),
                    ..voice.request
                },
            );
            if voice.tuning {
                witness.tune_started = witness.tune_started.saturating_add(1);
            }
            voice.tuning
        });
        if delta <= 0.0 {
            return;
        }
        for bell in &mut self.bells {
            let Some((position, linear, angular)) = items.resolved_body_motion(bell.key) else {
                bell.moving = false;
                continue;
            };
            if let Some(jingle) =
                bell.advance(linear, angular, items.held() == Some(bell.key), delta)
                && system.play(
                    jingle.cue,
                    PlayRequest {
                        position: Some(position),
                        gain: jingle.gain,
                        bus: AudioBus::Prop,
                        ..PlayRequest::default()
                    },
                )
            {
                if jingle.cue == FIRM {
                    witness.firm = witness.firm.saturating_add(1);
                } else {
                    witness.soft = witness.soft.saturating_add(1);
                }
                witness.last_jingle = Some(jingle);
            }
        }
    }
}

impl Bell {
    fn advance(&mut self, linear: Vec3, angular: Vec3, held: bool, delta: f32) -> Option<Jingle> {
        self.cooldown = (self.cooldown - delta).max(0.0);
        let linear_speed = linear.length();
        let angular_speed = angular.length();
        // Convert swivel and roll to an approximate tangential speed at the bells.
        let strength = linear_speed.max(0.14 * angular_speed);
        self.moving = strength >= if self.moving { 0.12 } else { 0.19 };
        if !self.moving || self.cooldown > 0.0 {
            return None;
        }
        let energy = ((strength - 0.19) / 1.15).clamp(0.0, 1.0);
        let interval = energy.mul_add(-0.28, 0.39);
        self.cooldown = interval;
        Some(Jingle {
            key: self.key,
            held,
            cue: if strength >= 0.75 { FIRM } else { SOFT },
            linear: linear_speed,
            angular: angular_speed,
            gain: energy.mul_add(0.64, 0.36),
            interval,
        })
    }
}

#[cfg(test)]
#[path = "../tests/christmas_audio.rs"]
mod tests;
