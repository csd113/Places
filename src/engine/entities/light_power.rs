//! Power gating composes existing prepared circuit contributions with flicker.
use super::{EntityHandle, EntityWorld, LevelDef};

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(super) enum Power {
    InitiallyOff,
    On,
    Off,
}

pub(super) struct CircuitPower {
    pub members: Vec<EntityHandle>,
    pub switches: Vec<EntityHandle>,
    pub power: Power,
    pub requested: bool,
    pub authored: bool,
}

impl EntityWorld {
    pub(super) fn reset_circuit_power(&mut self) {
        for circuit in &mut self.circuit_power {
            circuit.power = if circuit.authored {
                Power::On
            } else {
                Power::InitiallyOff
            };
            circuit.requested = circuit.authored;
        }
    }

    pub(super) fn resolve_circuit_power(
        &mut self,
        level: &LevelDef,
        circuits: &std::collections::BTreeMap<&str, Vec<EntityHandle>>,
    ) {
        self.circuit_power.clear();
        let ids = level.prop_instance_ids();
        for (name, members) in circuits {
            let switches: Vec<_> = level
                .props
                .iter()
                .enumerate()
                .filter(|(_, prop)| prop.light_switch.as_deref() == Some(*name))
                .filter_map(|(index, _)| ids.get(index).and_then(|id| self.handle_of(id)))
                .collect();
            if switches.is_empty() {
                continue;
            }
            let authored = members
                .first()
                .and_then(|handle| self.components.lights.get(*handle))
                .is_some_and(|light| light.enabled);
            self.circuit_power.push(CircuitPower {
                members: members.clone(),
                switches,
                power: if authored {
                    Power::On
                } else {
                    Power::InitiallyOff
                },
                requested: authored,
                authored,
            });
        }
    }

    pub(super) fn request_circuit_light(&mut self, handle: EntityHandle, on: bool) -> bool {
        let Some(circuit) = self
            .circuit_power
            .iter_mut()
            .find(|circuit| circuit.members.contains(&handle))
        else {
            return on;
        };
        circuit.requested = on;
        matches!(circuit.power, Power::On) && on
    }

    pub(super) fn toggle_circuit_power(&mut self, handle: EntityHandle) -> bool {
        let Some(circuit) = self
            .circuit_power
            .iter_mut()
            .find(|circuit| circuit.switches.contains(&handle))
        else {
            return false;
        };
        circuit.power = match circuit.power {
            Power::InitiallyOff => {
                circuit.requested = true;
                Power::On
            }
            Power::Off => Power::On,
            Power::On => Power::Off,
        };
        let on = matches!(circuit.power, Power::On) && circuit.requested;
        let members = circuit.members.clone();
        for member in members {
            let _changed = self.set_single_light(member, on, 0);
        }
        true
    }
}
