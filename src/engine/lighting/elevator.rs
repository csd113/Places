//! Light sources and sampling for the manufactured elevator assembly.

use crate::engine::elevator::{Elevator, door_leaf_x};
use glam::Vec3;

/// Analytic visibility of the manufactured opaque shell and paired door tracks.
/// It follows gameplay, including the eased leaf overlap, rather than the camera
/// or a lighting-quality setting. No static light is baked into a landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightEnclosure {
    pub origin: Vec3,
    pub rotation_degrees: f32,
    pub half_gap: f32,
    pub landing_doors: bool,
}

impl LightEnclosure {
    #[must_use]
    pub fn is_well_formed(self) -> bool {
        self.origin.is_finite()
            && self.rotation_degrees.is_finite()
            && self.half_gap.is_finite()
            && (-crate::engine::elevator::DOOR_LEAF_SEAL
                ..=crate::engine::elevator::DOOR_LEAF_TRAVEL)
                .contains(&self.half_gap)
    }
}

/// Elevator stock lies in a wall/shaft, where a geometric-centre probe has no
/// room and falls back to invented ambient. Receive the real landing's light
/// from clear hall air instead. Each fixed cabin samples its own landing even
/// while passengers are using another cabin in the same authored group.
#[must_use]
pub fn landing_light_probe(e: &Elevator, position: [f32; 3]) -> Vec3 {
    let local = e.def.local(Vec3::from_array(position));
    e.def.point(
        0.0,
        crate::engine::elevator::DOOR_HEIGHT.mul_add(0.5, local.y),
        -0.35,
    )
}

pub fn leaf_receiver_bounds(leaf_front: f32, leaf_back: f32) -> crate::engine::spatial::Aabb {
    // Concealed leaf overlaps lie inside the sill/header and cannot be used
    // as light receivers for the entire exposed panel. This is a sampling
    // subset only; drawing, culling and occluding geometry retain full stock.
    crate::engine::spatial::Aabb {
        min: [0.02, 0.03, leaf_front + 0.001],
        max: [
            crate::engine::elevator::DOOR_LEAF_WIDTH - 0.02,
            crate::engine::elevator::DOOR_HEIGHT - 0.03,
            leaf_back - 0.001,
        ],
    }
}

/// Every authored landing owns a fixed cabin practical as well as fixed geometry.
#[must_use]
pub fn cabin_lights(e: &Elevator) -> Vec<super::attached_lights::DynamicLight> {
    e.def
        .stops
        .iter()
        .enumerate()
        .filter(|(_, stop)| stop.is_landing())
        .map(|(index, stop)| {
            let opening = if index == e.at { e.opening } else { 0.0 };
            super::attached_lights::DynamicLight {
                key: format!("elevator:{}:{index}", e.def.id),
                position: e.def.point(0.0, stop.y + 2.30, 1.20),
                color: [1.0, 0.69, 0.38],
                intensity: 2.0,
                radius: 3.2,
                enclosure: Some(LightEnclosure {
                    origin: e.def.point(0.0, stop.y, 0.0),
                    rotation_degrees: e.def.rotation_degrees,
                    half_gap: door_leaf_x(1.0, opening) - crate::engine::elevator::DOOR_LEAF_SEAL,
                    landing_doors: true,
                }),
            }
        })
        .collect()
}
