//! World-owned reflection queries: physical poses never cross the sensing
//! boundary unless the requested known release is inside a valid clear beam.
use super::guest_acoustics::{Ambient, Layout, crosses, portal_evidence};
use super::{ai_runtime::PhysicalObservation, id::EntityHandle};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::physics::{ImpactRelease, ReleaseIdentity};
use crate::engine::tall_guest::hearing::{self, Heard, Request};
use crate::engine::tall_guest::prop_track::{Reflection, Scene};
use glam::Vec3;

pub(super) struct PropScene<'a> {
    pub items: &'a std::collections::HashMap<EntityHandle, PhysicalObservation>,
    pub layout: &'a Layout,
    pub ambient: &'a [Ambient],
    pub doors: &'a crate::engine::door::Doors,
    pub leaves: &'a [DoorCollider],
    pub solids: &'a [WallAabb],
}

impl PropScene<'_> {
    fn item(&self, identity: ReleaseIdentity) -> Option<&PhysicalObservation> {
        self.items.values().find(|item| {
            !item.held && item.release == Some(identity) && identity.kind == ImpactRelease::Thrown
        })
    }

    fn path(&self, body: &PhysicalObservation, from: Vec3, to: Vec3) -> bool {
        self.segment(from, to, false, Some(body.key)).is_some()
    }

    fn segment(
        &self,
        from: Vec3,
        to: Vec3,
        transmitting: bool,
        target: Option<u64>,
    ) -> Option<f32> {
        if !self.layout.geometry.static_clear(from, to) {
            return None;
        }
        let excluded = target
            .and_then(|key| self.items.values().find(|item| item.key == key))
            .and_then(|body| body.bounds);
        if self.solids.iter().any(|solid| {
            Some(*solid) != excluded && super::super::tall_guest::segment_box(from, to, *solid)
        }) {
            return None;
        }
        let mut transmission = 1.0;
        for leaf in self.leaves {
            if !crosses(from, to, leaf) {
                continue;
            }
            if !transmitting {
                return None;
            }
            let door = self.doors.iter().find(|door| door.collider() == *leaf)?;
            let factor = self
                .layout
                .transmissions
                .iter()
                .find(|(id, _)| id == &door.def.id)?
                .1;
            transmission *= factor;
        }
        Some(transmission)
    }

    fn noise(&self, listener: Vec3) -> f32 {
        let mut energy = 0.000_001_f32;
        for ambient in self.ambient.iter().take(128) {
            let distance = listener.distance(ambient.position);
            if ambient.spatial && distance >= ambient.attenuation[1] {
                continue;
            }
            let Some(transmission) = self.segment(listener, ambient.position, true, None) else {
                continue;
            };
            let attenuation = if ambient.spatial {
                ambient.attenuation[0] / distance.max(ambient.attenuation[0])
            } else {
                1.0
            };
            let amplitude = ambient.amplitude * attenuation * transmission;
            energy = amplitude.mul_add(amplitude, energy);
        }
        energy.sqrt()
    }

    fn candidate_portals(&self, request: &Request) -> Vec<&super::guest_acoustics::Portal> {
        let mut portals: Vec<_> = self.layout.portals.iter().collect();
        portals.sort_by(|a, b| {
            let score = |portal: &super::guest_acoustics::Portal| {
                request.origin.distance(portal.center) + request.source.distance(portal.center)
            };
            score(a).total_cmp(&score(b))
        });
        portals.truncate(8);
        portals
    }

    fn through_portals(&self, request: &Request, noise: f32) -> Option<Heard> {
        let portals = self.candidate_portals(request);
        let mut best: Option<Heard> = None;
        for first in &portals {
            let (near, far) = first.sides(request.origin);
            let Some(first_factor) = self
                .segment(request.origin, near, false, None)
                .zip(self.segment(near, far, true, None))
                .map(|(a, b)| a * b)
            else {
                continue;
            };
            for next_portal in std::iter::once(None).chain(
                portals
                    .iter()
                    .filter(|portal| portal.id != first.id)
                    .map(|portal| Some(*portal)),
            ) {
                let (distance, factor, ids) = if let Some(second) = next_portal {
                    let (second_near, second_far) = second.sides(far);
                    let Some(factor) = self
                        .segment(far, second_near, false, None)
                        .zip(self.segment(second_near, second_far, true, None))
                        .zip(self.segment(second_far, request.source, false, request.target_key))
                        .map(|((a, b), c)| a * b * c)
                    else {
                        continue;
                    };
                    (
                        request.origin.distance(near)
                            + near.distance(far)
                            + far.distance(second_near)
                            + second_near.distance(second_far)
                            + second_far.distance(request.source),
                        first_factor * factor,
                        vec![first.id, second.id],
                    )
                } else {
                    let Some(factor) = self.segment(far, request.source, false, request.target_key)
                    else {
                        continue;
                    };
                    (
                        request.origin.distance(near)
                            + near.distance(far)
                            + far.distance(request.source),
                        first_factor * factor,
                        vec![first.id],
                    )
                };
                if distance > request.radius {
                    continue;
                }
                if let Some(evidence) = hearing::resolve(request, distance, noise, factor)
                    && best
                        .as_ref()
                        .is_none_or(|old| evidence.quality > old.quality)
                {
                    best = Some(portal_evidence(request, evidence, near, far, ids));
                }
            }
        }
        best
    }
}

impl Scene for PropScene<'_> {
    fn clear(&self, from: Vec3, to: Vec3) -> bool {
        self.segment(from, to, false, None).is_some()
    }

    fn hear(&self, request: &Request) -> Option<Heard> {
        let eligibility_distance = request.listener.distance(request.source);
        let distance = request.origin.distance(request.source);
        if !distance.is_finite() || eligibility_distance > request.radius || request.radius <= 0.0 {
            return None;
        }
        let noise = self.noise(request.origin);
        if self
            .segment(request.origin, request.source, false, request.target_key)
            .is_some()
        {
            hearing::resolve(request, distance, noise, 1.0)
        } else {
            self.through_portals(request, noise)
        }
    }

    fn echo_quality(&self, origin: Vec3, hit: Vec3) -> f32 {
        let request = Request {
            listener: origin,
            origin,
            source: hit,
            radius: 14.0,
            loudness: 1.0,
            seed: 0,
            target_key: None,
        };
        hearing::resolve(&request, origin.distance(hit), self.noise(origin), 1.0)
            .map_or(0.0, |heard| heard.quality)
    }

    fn valid(&self, identity: ReleaseIdentity) -> bool {
        self.item(identity).is_some()
    }

    #[expect(
        clippy::arithmetic_side_effects,
        reason = "A finite normalized beam uses metre-space distances and dot products."
    )]
    fn reflect(&self, identity: ReleaseIdentity, origin: Vec3, axis: Vec3) -> Option<Reflection> {
        let body = self.item(identity)?;
        let delta = body.center - origin;
        let distance = delta.length();
        if !(0.01..=14.0).contains(&distance)
            || axis.dot(delta / distance) < 55.0_f32.to_radians().cos()
            || !self.path(body, origin, body.center)
            || self.echo_quality(origin, body.center) < 0.99
        {
            return None;
        }
        Some(Reflection {
            point: body.center,
            sleeping: body.sleeping,
        })
    }

    fn return_clear(&self, identity: ReleaseIdentity, from: Vec3, sampled: Vec3) -> bool {
        self.item(identity)
            .is_some_and(|body| self.path(body, from, sampled))
    }
}
