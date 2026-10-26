//! One private entity namespace for components, actions and graph validation.

use super::geometry::rect_overlaps_room;
use crate::engine::entities::sequences::{
    MAX_LEVEL_SEQUENCES, MAX_SEQUENCE_ANIMATION_TIMEOUT_S, MAX_SEQUENCE_STEPS, MAX_SEQUENCE_WAIT_S,
    SequenceDef, SequenceStepDef,
};
use crate::engine::entities::spawn::{
    MAX_LEVEL_SPAWN_GROUPS, MAX_LEVEL_SPAWN_POINTS, MAX_LEVEL_SPAWN_TEMPLATES,
};
use crate::engine::level::{
    ActionDef, ComponentDef, ConditionDef, EventBindingDef, EventKindName, LevelDef,
    MAX_ACTIONS_PER_SOURCE, MAX_BINDINGS_PER_ENTITY, TriggerVolumeDef,
};
use crate::engine::loader::PropCatalog;
use glam::Vec3;
use std::collections::{HashMap, HashSet};

/// Largest authored lifetime a `lifetime` component may declare, in seconds.
///
/// A bound, not a tuning knob: a lifetime exists to remove a temporary or
/// spawned entity, and one that can outlive an hour-long session is a typo
/// (or an entity that never expires) that would pin its spawn slot for the
/// whole run. The runtime reaps the entity at expiry.
pub const MAX_LIFETIME_SECONDS: f32 = 3600.0;

/// Every authored id an action or condition can address, with the capability
/// facts those checks need.
///
/// One instance namespace covers props, doors, ceiling fixtures, trigger
/// volumes, timers and spawn points, exactly as the runtime addresses them;
/// sequences, spawn templates and spawn groups are separate resource
/// namespaces. See [`validate_instance_ids`].
#[derive(Default)]
pub(super) struct LevelIndex<'a> {
    transitions: HashSet<&'a str>,
    /// Entity instance ids, in authored order, mapped to their capability
    /// facts.
    entities: HashMap<String, EntityFacts<'a>>,
    /// Spawn template ids and the facts a template-born instance carries.
    spawn_templates: HashMap<String, EntityFacts<'a>>,
    /// Authored sequence ids.
    sequences: HashSet<String>,
    /// Authored spawn group ids.
    spawn_groups: HashSet<String>,
    /// Authored spawn point ids.
    spawn_points: HashSet<String>,
    /// Explicit runtime names reserved by authored spawn actions. They can be
    /// watched by an exact entity filter before that entity exists.
    named_spawns: HashSet<&'a str>,
    /// Groups declared by authored movable rigid bodies, separate from spawns.
    physics_reset_groups: HashSet<&'a str>,
    /// Sequence id -> every entity the sequence can run on, discovered from
    /// the authored `start_sequence` sites.
    pub(super) sequence_owners: HashMap<String, Vec<String>>,
    /// The world has two authored weather configurations to toggle between.
    weather_toggleable: bool,
    /// Continuous weather controls can reuse one seed and texture budget.
    weather_blendable: bool,
    weather_cycle: bool,
}

impl<'a> LevelIndex<'a> {
    /// The facts of one addressable entity: a placed record, or the instances
    /// a spawn template creates.
    fn facts_of(&self, id: &str) -> Option<&EntityFacts<'a>> {
        self.entities
            .get(id)
            .or_else(|| self.spawn_templates.get(id))
    }
}

/// What one authored record can do, derived from its kind and components.
///
/// The action, condition and binding checks read only these facts, so a target
/// that resolves always has a known capability set and every failure names the
/// record and the missing capability.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent capability bits, not mutually exclusive states"
)] // independent capability bits, not mutually exclusive states
#[derive(Default)]
struct EntityFacts<'a> {
    /// Diagnostic kind name, e.g. `prop` or `trigger volume`.
    kind: &'static str,
    /// True for a placed prop (the only record with a toggleable label).
    is_prop: bool,
    /// True for a door leaf.
    is_door: bool,
    /// True for a ceiling/wall fixture record.
    is_light_fixture: bool,
    /// True for an authored timer.
    is_timer: bool,
    /// True for an authored trigger volume.
    is_volume: bool,
    /// True for an authored spawn point.
    is_spawn_point: bool,
    /// True for a spawn template (its instances carry the template's facts).
    is_spawn_template: bool,
    /// The entity carries an `interactable` component.
    has_interactable: bool,
    /// The authored `interactable` starts enabled.
    interactable_enabled: bool,
    /// The entity carries an `animation` component.
    has_animation: bool,
    /// The entity carries a `light` component.
    has_light: bool,
    /// The entity's light (or fixture) can be switched at runtime.
    light_switchable: bool,
    /// The entity carries a `material` component.
    has_material: bool,
    /// The entity carries an `audio` component.
    has_audio: bool,
    /// Shared planner/mover is present.
    has_ai: bool,
    /// Authored item attachment contract.
    has_carry: bool,
    /// State names the entity authors.
    state_names: HashSet<&'a str>,
    /// Variant names of the entity's `material` component, in authored order.
    material_variants: Vec<&'a str>,
}

impl EntityFacts<'_> {
    /// A record of `kind` with no authored capabilities.
    fn of(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// True when an action can turn this entity's light on and off.
    const fn light_capable(&self) -> bool {
        self.is_light_fixture || self.has_light
    }
}

/// Stable instance identities, component values and resource namespaces.
///
/// Every placed record shares one instance-id namespace per level: props,
/// doors, ceiling fixtures, trigger volumes, timers and spawn points. An id is
/// authored or deterministically defaulted (see
/// [`crate::engine::level::LevelDef::prop_instance_ids`]); duplicates and malformed
/// values are named errors. Sequence, spawn-template and spawn-group ids are
/// separate resource namespaces with the same uniqueness contract within
/// their own kind.
///
/// The same pass validates every authored component on the records that carry
/// them and builds the capability index the binding checks read.
#[expect(clippy::too_many_lines, reason = "one cohesive id + capability pass")] // one cohesive id + capability pass
pub(super) fn validate_instance_ids(level: &LevelDef) -> Result<LevelIndex<'_>, String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut first: HashMap<&str, String> = HashMap::new();
    let mut index = LevelIndex {
        transitions: level.transitions.iter().map(String::as_str).collect(),
        named_spawns: named_spawn_ids(level),
        physics_reset_groups: level.physics_reset_groups().collect(),
        weather_toggleable: level.weather.is_some() && level.weather_alternate.is_some(),
        weather_blendable: level
            .weather
            .as_ref()
            .zip(level.weather_alternate.as_ref())
            .is_some_and(|(base, alternate)| {
                crate::engine::weather::strength_compatible(base, alternate)
            }),
        weather_cycle: level.weather_cycle.is_some(),
        ..LevelIndex::default()
    };

    let prop_ids = level.prop_instance_ids();
    for (i, id) in prop_ids.iter().enumerate() {
        validate_instance_id(&mut seen, &mut first, id, &format!("Prop {i}"), "instance")?;
    }
    let light_ids = level.light_instance_ids();
    for (i, id) in light_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Ceiling light {i}"),
            "instance",
        )?;
    }
    let door_ids = level.door_instance_ids();
    for (i, id) in door_ids.iter().enumerate() {
        validate_instance_id(&mut seen, &mut first, id, &format!("Door {i}"), "instance")?;
    }
    let volume_ids = volume_instance_ids(level);
    for (i, id) in volume_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Trigger volume {i}"),
            "instance",
        )?;
    }
    for (i, timer) in level.timers.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            timer.id.as_str(),
            &format!("Timer {i}"),
            "instance",
        )?;
    }
    for (i, emitter) in level.sound_emitters.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            &emitter.id,
            &format!("Sound emitter {i}"),
            "instance",
        )?;
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            point.id.as_str(),
            &format!("Spawn point {i}"),
            "instance",
        )?;
    }
    // Water volumes and effect emitters become entities with synthesized ids:
    // an authored id that shadows one would make the volume/emitter
    // unreachable to `enable`/`disable`.
    let water_ids = level.water_instance_ids();
    for (i, id) in water_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Water volume {i}"),
            "instance",
        )?;
    }
    let effect_ids: Vec<String> = level
        .effects
        .iter()
        .enumerate()
        .map(|(i, effect)| {
            effect
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map_or_else(|| format!("effect_{}", i.saturating_add(1)), str::to_string)
        })
        .collect();
    for (i, id) in effect_ids.iter().enumerate() {
        validate_instance_id(
            &mut seen,
            &mut first,
            id,
            &format!("Effect {i}"),
            "instance",
        )?;
    }

    // Resource namespaces: sequences, spawn templates and spawn groups are
    // addressed by resource name, so duplicates are errors within each kind
    // (an id shared across kinds is unambiguous).
    let mut sequence_seen: HashSet<&str> = HashSet::new();
    let mut sequence_first: HashMap<&str, String> = HashMap::new();
    for (i, sequence) in level.sequences.iter().enumerate() {
        validate_instance_id(
            &mut sequence_seen,
            &mut sequence_first,
            sequence.id.as_str(),
            &format!("Sequence {i}"),
            "sequence",
        )?;
        let _new_entry = index.sequences.insert(sequence.id.trim().to_string());
    }
    let mut template_seen: HashSet<&str> = HashSet::new();
    let mut template_first: HashMap<&str, String> = HashMap::new();
    for (i, template) in level.spawn_templates.iter().enumerate() {
        let context = format!("Spawn template {i}");
        validate_instance_id(
            &mut template_seen,
            &mut template_first,
            template.id.as_str(),
            &context,
            "spawn template",
        )?;
        let mut facts = EntityFacts::of("spawn template");
        facts.is_spawn_template = true;
        validate_components(&context, &template.components, &mut facts)?;
        validate_carry_items(&context, &template.components, level, &prop_ids)?;
        drop(
            index
                .spawn_templates
                .insert(template.id.trim().to_string(), facts),
        );
    }
    let mut group_seen: HashSet<&str> = HashSet::new();
    let mut group_first: HashMap<&str, String> = HashMap::new();
    for (i, group) in level.spawn_groups.iter().enumerate() {
        validate_instance_id(
            &mut group_seen,
            &mut group_first,
            group.id.as_str(),
            &format!("Spawn group {i}"),
            "spawn group",
        )?;
        let _new_entry_2 = index.spawn_groups.insert(group.id.trim().to_string());
    }

    // Capability facts for every placed record, in authored order.
    for (i, prop) in level.props.iter().enumerate() {
        let Some(id) = prop_ids.get(i) else {
            continue;
        };
        let context = format!("Prop {i} (`{id}`)");
        if let Some(name) = prop.display_name.as_deref()
            && name.trim().is_empty()
        {
            return Err(format!(
                "{context} display_name must not be blank when specified"
            ));
        }
        let mut facts = EntityFacts::of("prop");
        facts.is_prop = true;
        validate_components(&context, &prop.components, &mut facts)?;
        validate_carry_items(&context, &prop.components, level, &prop_ids)?;
        if prop.light_control().is_some()
            && (prop.float.is_some()
                || prop.is_physics_movable()
                || prop.components.iter().any(|component| {
                    matches!(
                        component,
                        ComponentDef::Animation {
                            clip: _,
                            speed: _,
                            looped: _,
                            playing: _
                        } | ComponentDef::Ai(_)
                            | ComponentDef::Balloon(_)
                    )
                }))
        {
            return Err(format!(
                "{context} a light controller requires a fixed prop"
            ));
        }
        if prop.emission_overrides.len() > crate::engine::level::MAX_PROP_MATERIALS
            || prop.emission_overrides.iter().any(|(slot, emission)| {
                usize::from(*slot) >= crate::engine::level::MAX_PROP_MATERIALS
                    || emission
                        .color
                        .iter()
                        .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
                    || !emission.intensity.is_finite()
                    || !(0.0..=8.0).contains(&emission.intensity)
            })
        {
            return Err(format!(
                "{context} emission_overrides need valid material slots, linear colours in 0..=1 and intensity in 0..=8"
            ));
        }
        if !prop.emission_overrides.is_empty() && !facts.has_light {
            return Err(format!(
                "{context} emission_overrides require a light controller"
            ));
        }
        if prop
            .components
            .iter()
            .any(|component| matches!(component, ComponentDef::Balloon(_)))
            && (prop.float.is_some() || level.routes.iter().any(|route| route.id.trim() == id))
        {
            return Err(format!(
                "{context} `balloon` cannot float or follow a route; its anchor is fixed"
            ));
        }
        drop(index.entities.insert(id.clone(), facts));
    }
    for (i, door) in level.doors.iter().enumerate() {
        let Some(id) = door_ids.get(i) else {
            continue;
        };
        let context = format!("Door {i} (`{id}`)");
        let mut facts = EntityFacts::of("door");
        facts.is_door = true;
        validate_components(&context, &door.components, &mut facts)?;
        drop(index.entities.insert(id.clone(), facts));
    }
    for (i, fixture) in level.ceiling_lights.iter().enumerate() {
        let Some(id) = light_ids.get(i) else {
            continue;
        };
        let mut facts = EntityFacts::of("ceiling light");
        facts.is_light_fixture = true;
        facts.light_switchable = fixture.switchable;
        drop(index.entities.insert(id.clone(), facts));
    }
    for (i, _volume) in level.volumes.iter().enumerate() {
        let Some(id) = volume_ids.get(i) else {
            continue;
        };
        let mut facts = EntityFacts::of("trigger volume");
        facts.is_volume = true;
        drop(index.entities.insert(id.clone(), facts));
    }
    // Water volumes and effect emitters become entities too (they carry no
    // components), so an `enable`/`disable` action can name one: the same
    // resolution the runtime performs when it creates their controllers.
    for id in &water_ids {
        drop(
            index
                .entities
                .insert(id.clone(), EntityFacts::of("water volume")),
        );
    }
    for id in &effect_ids {
        drop(index.entities.insert(id.clone(), EntityFacts::of("effect")));
    }
    for emitter in &level.sound_emitters {
        let mut facts = EntityFacts::of("sound emitter");
        facts.has_audio = true;
        drop(index.entities.insert(emitter.id.trim().to_string(), facts));
    }
    for timer in &level.timers {
        let id = timer.id.trim();
        let mut facts = EntityFacts::of("timer");
        facts.is_timer = true;
        drop(index.entities.insert(id.to_string(), facts));
    }
    for point in &level.spawn_points {
        let id = point.id.trim();
        let _new_entry_3 = index.spawn_points.insert(id.to_string());
        let mut facts = EntityFacts::of("spawn point");
        facts.is_spawn_point = true;
        drop(index.entities.insert(id.to_string(), facts));
    }
    Ok(index)
}

/// Validates one authored id and records it for duplicate detection.
///
/// `namespace` names the uniqueness contract the id belongs to (`instance`,
/// `sequence`, `spawn template`, `spawn group`), so a duplicate error says
/// exactly which set of names it collided with.
fn validate_instance_id<'a>(
    seen: &mut HashSet<&'a str>,
    first: &mut HashMap<&'a str, String>,
    id: &'a str,
    context: &str,
    namespace: &str,
) -> Result<(), String> {
    let trimmed = id.trim();
    if !crate::engine::assets::is_valid_asset_id(trimmed) {
        return Err(format!(
            "{context} id `{id}` must be a non-empty, well-formed identifier"
        ));
    }
    if !seen.insert(trimmed) {
        let first_context = first
            .get(trimmed)
            .map_or_else(|| "an earlier record".to_string(), String::clone);
        return Err(format!(
            "{context} id `{trimmed}` duplicates `{first_context}`; {namespace} ids must be unique \
             per level"
        ));
    }
    drop(first.insert(trimmed, context.to_string()));
    Ok(())
}

/// The stable instance id of one authored trigger volume: its own id when
/// authored, else `trigger_<n>` with `n` the 1-based authored position.
fn volume_id(index: usize, volume: &TriggerVolumeDef) -> String {
    volume
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map_or_else(
            || format!("trigger_{}", index.saturating_add(1)),
            str::to_string,
        )
}

/// Every trigger volume's stable instance id, in authored order.
fn volume_instance_ids(level: &LevelDef) -> Vec<String> {
    level
        .volumes
        .iter()
        .enumerate()
        .map(|(index, volume)| volume_id(index, volume))
        .collect()
}

/// Validates one record's components and records the capabilities they grant.
///
/// A component kind that only makes sense once per entity may appear at most
/// once; `state` is the one repeatable kind, and then only with a distinct,
/// non-empty name. Every authored value is checked here, so a component that
/// could never behave is a named error instead of a runtime no-op.
fn validate_components<'a>(
    context: &str,
    components: &'a [ComponentDef],
    facts: &mut EntityFacts<'a>,
) -> Result<(), String> {
    let mut singles: HashSet<&'static str> = HashSet::new();
    let mut state_names: HashSet<&'a str> = HashSet::new();
    for (i, component) in components.iter().enumerate() {
        if let ComponentDef::State { name, value: _ } = component {
            let state_name = name.trim();
            if state_name.is_empty() {
                return Err(format!(
                    "{context} `state` component {i} must name a non-empty state"
                ));
            }
            if !state_names.insert(state_name) {
                return Err(format!(
                    "{context} declares two `state` components named `{state_name}`; state names must be unique per entity"
                ));
            }
        } else {
            let kind = component.kind();
            if !singles.insert(kind) {
                return Err(format!(
                    "{context} declares more than one `{kind}` component; only `state` components may repeat, with distinct names"
                ));
            }
        }
        validate_component_value(context, i, component, facts)?;
    }
    validate_moving_components(context, components)?;
    facts.state_names = state_names;
    Ok(())
}

/// The carry reference is a placed item identity, for placed and spawned actors.
fn validate_carry_items(
    context: &str,
    components: &[ComponentDef],
    level: &LevelDef,
    prop_ids: &[String],
) -> Result<(), String> {
    for component in components {
        if let ComponentDef::Carry(carry) = component
            && !level.props.iter().zip(prop_ids).any(|(item, identity)| {
                identity == &carry.item
                    && item.is_physics_movable()
                    && item.physics.as_ref().is_some_and(|physics| physics.pickup)
            })
        {
            return Err(format!(
                "{context} carry item '{}' must name an authored movable pickup prop",
                carry.item
            ));
        }
    }
    Ok(())
}

/// Cross-component physical requirements for anchored or navigating actors.
fn validate_moving_components(context: &str, components: &[ComponentDef]) -> Result<(), String> {
    let has_ai = components
        .iter()
        .any(|component| matches!(component, ComponentDef::Ai(_)));
    let has_body = components.iter().any(|component| {
        matches!(
            component,
            ComponentDef::NavAgent {
                radius: _,
                speed_mps: _,
                height: _,
                step_height: _,
                max_slope: _
            }
        )
    });
    if !has_ai
        && components
            .iter()
            .any(|component| matches!(component, ComponentDef::Carry(_)))
    {
        return Err(format!(
            "{context} authors carry without an AI locomotion owner"
        ));
    }
    if has_ai && !has_body {
        return Err(format!(
            "{context} authors an `ai` component without a `nav_agent` body; an agent needs \
             the physical profile its baked navigation class is selected by"
        ));
    }
    if !has_ai
        && components
            .iter()
            .any(|component| matches!(component, ComponentDef::LightResponse(_)))
    {
        return Err(format!(
            "{context} authors `light_response` without an `ai` component; the response needs an AI locomotion owner"
        ));
    }
    if let Some(radius) = components.iter().find_map(|component| {
        if let ComponentDef::NavAgent {
            radius,
            speed_mps: _,
            height: _,
            step_height: _,
            max_slope: _,
        } = component
        {
            Some(*radius)
        } else {
            None
        }
    }) {
        for component in components {
            if let ComponentDef::Ai(def) = component
                && let Some(area) = def.home_bounds
            {
                let bounds = area.nav_bounds(radius);
                if area.width <= radius * 2.0
                    || area.depth <= radius * 2.0
                    || def
                        .hide_anchors
                        .iter()
                        .any(|anchor| !bounds.contains(Vec3::from_array(*anchor)))
                {
                    return Err(format!(
                        "{context} `ai` home_bounds must contain the body and every hide anchor with radius clearance"
                    ));
                }
            }
        }
    }
    // AI translates the placement-local anchor; its frame retains the same
    // analytic balloon pose. Routes/float remain excluded at the prop boundary.
    Ok(())
}

/// One component's authored values, and the capability it grants.
#[expect(clippy::too_many_lines, reason = "one match arm per component kind")] // one match arm per component kind
fn validate_component_value<'a>(
    context: &str,
    i: usize,
    component: &'a ComponentDef,
    facts: &mut EntityFacts<'a>,
) -> Result<(), String> {
    match component {
        ComponentDef::Interactable {
            prompt,
            reach,
            enabled,
            label,
        } => {
            if prompt
                .as_deref()
                .is_some_and(|prompt_text| prompt_text.trim().is_empty())
            {
                return Err(format!(
                    "{context} `interactable` component {i} prompt must not be blank when \
                     specified"
                ));
            }
            if label
                .as_deref()
                .is_some_and(|label_text| label_text.trim().is_empty())
            {
                return Err(format!(
                    "{context} `interactable` component {i} label must not be blank when \
                     specified"
                ));
            }
            if let Some(reach_m) = reach
                && !(reach_m.is_finite()
                    && *reach_m > 0.0
                    && *reach_m <= crate::engine::interact::MAX_INTERACTION_REACH_M)
            {
                return Err(format!(
                    "{context} `interactable` component {i} reach ({reach_m:?} m) must be \
                     between 0 and {} metres",
                    crate::engine::interact::MAX_INTERACTION_REACH_M
                ));
            }
            facts.has_interactable = true;
            facts.interactable_enabled = *enabled;
        }
        ComponentDef::Animation {
            clip,
            speed: _,
            looped: _,
            playing: _,
        } => {
            if clip.trim().is_empty() {
                return Err(format!(
                    "{context} `animation` component {i} clip must not be blank"
                ));
            }
            facts.has_animation = true;
        }
        ComponentDef::Audio {
            sound,
            gain,
            looped: _,
            enabled: _,
            playing: _,
            time_domain: _,
        } => {
            if sound.trim().is_empty() {
                return Err(format!(
                    "{context} `audio` component {i} sound must not be blank"
                ));
            }
            if !gain.is_finite() || !(0.0..=1.0).contains(gain) {
                return Err(format!(
                    "{context} `audio` component {i} gain must be finite and within 0..=1"
                ));
            }
            facts.has_audio = true;
        }
        ComponentDef::Light {
            emission_scale,
            switchable,
            enabled: _,
        } => {
            if !emission_scale.is_finite() || *emission_scale < 0.0 {
                return Err(format!(
                    "{context} `light` component {i} emission_scale ({emission_scale:?}) \
                     must be a finite number that is not negative"
                ));
            }
            if *switchable && !facts.is_prop {
                return Err(format!(
                    "{context} `light` component {i} switching requires a fixed placed prop"
                ));
            }
            facts.has_light = true;
            facts.light_switchable = *switchable;
        }
        ComponentDef::Material { variants, current } => {
            if variants.is_empty() {
                return Err(format!(
                    "{context} `material` component {i} must declare at least one variant"
                ));
            }
            let mut names: HashSet<&str> = HashSet::new();
            for (j, variant) in variants.iter().enumerate() {
                let name = variant.name.trim();
                if name.is_empty() {
                    return Err(format!(
                        "{context} `material` component {i} variant {j} must name a \
                         non-empty variant"
                    ));
                }
                if !names.insert(name) {
                    return Err(format!(
                        "{context} `material` component {i} declares variant `{name}` twice; \
                         variant names must be unique"
                    ));
                }
                if !variant.emission_scale.is_finite() || variant.emission_scale < 0.0 {
                    return Err(format!(
                        "{context} `material` component {i} variant `{name}` emission_scale \
                         ({:?}) must be a finite number that is not negative",
                        variant.emission_scale
                    ));
                }
                facts.material_variants.push(name);
            }
            if let Some(current_name) = current {
                let trimmed_current = current_name.trim();
                if trimmed_current.is_empty() {
                    return Err(format!(
                        "{context} `material` component {i} current must not be blank when \
                         specified"
                    ));
                }
                if !names.contains(trimmed_current) {
                    return Err(format!(
                        "{context} `material` component {i} current `{trimmed_current}` is not one \
                         of its variants"
                    ));
                }
            }
            facts.has_material = true;
        }
        ComponentDef::Lifetime { seconds } => {
            if !seconds.is_finite() || *seconds <= 0.0 || *seconds > MAX_LIFETIME_SECONDS {
                return Err(format!(
                    "{context} `lifetime` component {i} seconds ({seconds:?}) must be a \
                     finite number between 0 and {MAX_LIFETIME_SECONDS}"
                ));
            }
        }
        ComponentDef::NavAgent {
            radius,
            speed_mps,
            height,
            step_height,
            max_slope,
        } => {
            let valid = radius.is_finite()
                && *radius > 0.0
                && speed_mps.is_finite()
                && *speed_mps > 0.0
                && height.is_finite()
                && *height > 0.0
                && step_height.is_finite()
                && *step_height > 0.0
                && max_slope.is_finite()
                && *max_slope > 0.0;
            if !valid {
                return Err(format!(
                    "{context} `nav_agent` component {i} radius, speed_mps, height, \
                     step_height and max_slope must be finite and positive"
                ));
            }
        }
        ComponentDef::Ai(def) => {
            facts.has_ai = true;
            if !def.is_valid() {
                return Err(format!(
                    "{context} `ai` component {i} has an invalid behavior, speed, range, tag, home_bounds, shelter or clip"
                ));
            }
        }
        ComponentDef::Carry(def) => {
            facts.has_carry = true;
            if !def.is_valid() {
                return Err(format!(
                    "{context} carry component {i} requires bounded names, offsets within 2 m and rotations within 360 degrees"
                ));
            }
        }
        ComponentDef::LightResponse(def) => {
            if !def.is_valid() {
                return Err(format!(
                    "{context} `light_response` component {i} requires finite 0 < exit_intensity <= enter_intensity <= 8 and release_seconds in 0..=1"
                ));
            }
        }
        ComponentDef::Fade(def) => {
            if !def.period_seconds.is_finite()
                || def.period_seconds <= 0.0
                || def.period_seconds > crate::engine::level::MAX_FADE_PERIOD_SECONDS
            {
                return Err(format!(
                    "{context} `fade` component {i} period_seconds ({:?}) must be a finite \
                     number between 0 and {}",
                    def.period_seconds,
                    crate::engine::level::MAX_FADE_PERIOD_SECONDS
                ));
            }
            if let Some(phase) = def.phase
                && !(phase.is_finite() && (0.0..=1.0).contains(&phase))
            {
                return Err(format!(
                    "{context} `fade` component {i} phase ({phase:?}) must be a finite number \
                     between 0 and 1"
                ));
            }
            for (name, value) in [
                ("min_opacity", def.min_opacity),
                ("max_opacity", def.max_opacity),
            ] {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err(format!(
                        "{context} `fade` component {i} {name} ({value:?}) must be a finite \
                         number between 0 and 1"
                    ));
                }
            }
            if def.min_opacity > def.max_opacity {
                return Err(format!(
                    "{context} `fade` component {i} min_opacity ({:?}) must not exceed \
                     max_opacity ({:?})",
                    def.min_opacity, def.max_opacity
                ));
            }
            // The proximity contract is all-or-nothing: both radii, in order,
            // and the two fade times only alongside them.
            match (def.near_radius, def.far_radius) {
                (None, None) => {
                    if def.fade_out_seconds.is_some() || def.fade_in_seconds.is_some() {
                        return Err(format!(
                            "{context} `fade` component {i} authors fade_out_seconds/\
                             fade_in_seconds without near_radius and far_radius"
                        ));
                    }
                }
                (Some(near), Some(far)) => {
                    if !near.is_finite() || near <= 0.0 {
                        return Err(format!(
                            "{context} `fade` component {i} near_radius ({near:?}) must be a \
                             finite number greater than 0"
                        ));
                    }
                    if !far.is_finite() || far <= near {
                        return Err(format!(
                            "{context} `fade` component {i} far_radius ({far:?}) must be a \
                             finite number greater than near_radius ({near:?})"
                        ));
                    }
                    for (name, value) in [
                        ("fade_out_seconds", def.fade_out_seconds),
                        ("fade_in_seconds", def.fade_in_seconds),
                    ] {
                        if let Some(coefficient) = value
                            && (!coefficient.is_finite()
                                || coefficient <= 0.0
                                || coefficient > crate::engine::level::MAX_FADE_SECONDS)
                        {
                            return Err(format!(
                                "{context} `fade` component {i} {name} ({coefficient:?}) must be a \
                                 finite number between 0 and {}",
                                crate::engine::level::MAX_FADE_SECONDS
                            ));
                        }
                    }
                }
                (Some(_), None) => {
                    return Err(format!(
                        "{context} `fade` component {i} authors near_radius without far_radius"
                    ));
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "{context} `fade` component {i} authors far_radius without near_radius"
                    ));
                }
            }
        }
        ComponentDef::Balloon(def) => {
            if !facts.is_prop && !facts.is_spawn_template {
                return Err(format!(
                    "{context} `balloon` component {i} requires a prop or spawn template"
                ));
            }
            if let Some((field, bound)) = def.invalid_field() {
                return Err(format!(
                    "{context} `balloon` component {i} {field} must be finite within {bound}"
                ));
            }
        }
        ComponentDef::Glow(def) => {
            if !def
                .color
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
            {
                return Err(format!(
                    "{context} `glow` component {i} color ({:?}) must be three finite channels \
                     between 0 and 1",
                    def.color
                ));
            }
            if !def.intensity.is_finite()
                || !(0.0..=crate::engine::level::MAX_GLOW_INTENSITY).contains(&def.intensity)
            {
                return Err(format!(
                    "{context} `glow` component {i} intensity ({:?}) must be a finite number \
                     between 0 and {}",
                    def.intensity,
                    crate::engine::level::MAX_GLOW_INTENSITY
                ));
            }
            if !def.range.is_finite()
                || !(crate::engine::level::MIN_GLOW_RANGE_M
                    ..=crate::engine::level::MAX_GLOW_RANGE_M)
                    .contains(&def.range)
            {
                return Err(format!(
                    "{context} `glow` component {i} range ({:?}) must be a finite number \
                     between {} and {} metres",
                    def.range,
                    crate::engine::level::MIN_GLOW_RANGE_M,
                    crate::engine::level::MAX_GLOW_RANGE_M
                ));
            }
            if def
                .socket
                .as_deref()
                .is_some_and(|socket| socket.trim().is_empty())
            {
                return Err(format!(
                    "{context} `glow` component {i} socket must not be blank when specified"
                ));
            }
            if !def.offset.iter().all(|axis| {
                axis.is_finite() && axis.abs() <= crate::engine::level::MAX_GLOW_OFFSET_M
            }) {
                return Err(format!(
                    "{context} `glow` component {i} offset ({:?}) must be three finite numbers \
                     within +/-{} metres",
                    def.offset,
                    crate::engine::level::MAX_GLOW_OFFSET_M
                ));
            }
        }
        ComponentDef::State { name: _, value: _ }
        | ComponentDef::Steam { enabled: _ }
        | ComponentDef::Water { enabled: _ }
        | ComponentDef::NavObstacle {
            size: _,
            affects_nav: _,
        } => {}
    }
    Ok(())
}

/// One record's authored bindings and the label its diagnostics use.
struct AuthoredBindings<'a> {
    /// Stable instance id the record's own actions act on (a spawn template id
    /// for template-born instances).
    id: String,
    /// Authored record label, e.g. `Prop 3`.
    label: String,
    /// The bindings, in authored order.
    bindings: &'a [EventBindingDef],
}

/// Every record that carries bindings, in authored order: props, doors,
/// fixtures, volumes, timers, spawn points and spawn templates. Effects are
/// validated separately because an effect id is optional and not part of the
/// instance namespace.
fn authored_bindings(level: &LevelDef) -> Vec<AuthoredBindings<'_>> {
    let mut records = Vec::new();
    let prop_ids = level.prop_instance_ids();
    for (i, prop) in level.props.iter().enumerate() {
        if prop.bindings.is_empty() {
            continue;
        }
        if let Some(id) = prop_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Prop {i}"),
                bindings: &prop.bindings,
            });
        }
    }
    let light_ids = level.light_instance_ids();
    for (i, fixture) in level.ceiling_lights.iter().enumerate() {
        if fixture.bindings.is_empty() {
            continue;
        }
        if let Some(id) = light_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Ceiling light {i}"),
                bindings: &fixture.bindings,
            });
        }
    }
    let door_ids = level.door_instance_ids();
    for (i, door) in level.doors.iter().enumerate() {
        if door.bindings.is_empty() {
            continue;
        }
        if let Some(id) = door_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Door {i}"),
                bindings: &door.bindings,
            });
        }
    }
    let volume_ids = volume_instance_ids(level);
    for (i, volume) in level.volumes.iter().enumerate() {
        if volume.bindings.is_empty() {
            continue;
        }
        if let Some(id) = volume_ids.get(i) {
            records.push(AuthoredBindings {
                id: id.clone(),
                label: format!("Trigger volume {i}"),
                bindings: &volume.bindings,
            });
        }
    }
    for (i, timer) in level.timers.iter().enumerate() {
        if timer.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: timer.id.trim().to_string(),
            label: format!("Timer {i}"),
            bindings: &timer.bindings,
        });
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        if point.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: point.id.trim().to_string(),
            label: format!("Spawn point {i}"),
            bindings: &point.bindings,
        });
    }
    for (i, template) in level.spawn_templates.iter().enumerate() {
        if template.bindings.is_empty() {
            continue;
        }
        records.push(AuthoredBindings {
            id: template.id.trim().to_string(),
            label: format!("Spawn template {i}"),
            bindings: &template.bindings,
        });
    }
    for (i, emitter) in level.sound_emitters.iter().enumerate() {
        if !emitter.bindings.is_empty() {
            records.push(AuthoredBindings {
                id: emitter.id.trim().to_string(),
                label: format!("Sound emitter {i}"),
                bindings: &emitter.bindings,
            });
        }
    }
    records
}

/// Every authored binding, on every record that carries one.
pub(super) fn validate_bindings(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    for record in authored_bindings(level) {
        let Some(facts) = index.facts_of(&record.id) else {
            continue;
        };
        let context = format!("{} (`{}`)", record.label, record.id);
        validate_entity_bindings(&context, &record.id, record.bindings, facts, index)?;
    }
    for (i, effect) in level.effects.iter().enumerate() {
        if effect.bindings.is_empty() {
            continue;
        }
        let facts = EntityFacts::of("effect");
        let label = effect
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(
                || format!("Effect {i}"),
                |id| format!("Effect {i} (`{id}`)"),
            );
        validate_entity_bindings(&label, "the effect", &effect.bindings, &facts, index)?;
    }
    Ok(())
}

/// One record's bindings: bounded, and every one can actually fire on it.
fn validate_entity_bindings(
    context: &str,
    actor: &str,
    bindings: &[EventBindingDef],
    facts: &EntityFacts<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    if bindings.len() > MAX_BINDINGS_PER_ENTITY {
        return Err(format!(
            "{context} declares {} bindings; the limit is {MAX_BINDINGS_PER_ENTITY}",
            bindings.len()
        ));
    }
    for (i, binding) in bindings.iter().enumerate() {
        if let Some(id) = binding.id.as_deref()
            && id.trim().is_empty()
        {
            return Err(format!(
                "{context} binding {i} id must not be blank when specified"
            ));
        }
        let binding_context = binding
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map_or_else(
                || format!("{context} binding {i}"),
                |id| format!("{context} binding {i} (`{id}`)"),
            );
        if !binding.cooldown_seconds.is_finite() || binding.cooldown_seconds < 0.0 {
            return Err(format!(
                "{binding_context} cooldown_seconds must be a finite number that is not \
                 negative (found {:?})",
                binding.cooldown_seconds
            ));
        }
        match binding.on {
            EventKindName::EnterVolume | EventKindName::ExitVolume => {
                if !facts.is_volume {
                    return Err(format!(
                        "{binding_context} listens for `{}`, but {context} is not an \
                         authored trigger volume",
                        binding.on.name()
                    ));
                }
            }
            EventKindName::AnimationComplete => {
                if !facts.has_animation {
                    return Err(format!(
                        "{binding_context} listens for `animation_complete`, but {context} \
                         has no `animation` component"
                    ));
                }
            }
            EventKindName::Interact => {
                if !facts.has_interactable || !facts.interactable_enabled {
                    return Err(format!(
                        "{binding_context} listens for `interact`, but {context} has no \
                         enabled `interactable` component; a disabled interactable can \
                         never emit the event"
                    ));
                }
            }
            // A `timer` event may come from an authored timer *or* from a
            // sequence's `emit` step, which uses the same vocabulary as one
            // generic cue channel; any record may listen for it. The same
            // holds for the remaining kinds, which an `emit` step can publish.
            EventKindName::Timer
            | EventKindName::ObjectState
            | EventKindName::SequenceComplete
            | EventKindName::Spawn
            | EventKindName::AiState
            | EventKindName::Caught => {}
        }
        validate_conditions(&binding_context, &binding.when, index)?;
        validate_action_list(
            &binding_context,
            &binding.actions,
            &ImplicitTarget::Actor(actor, facts),
            index,
        )?;
    }
    Ok(())
}

/// Every condition in one binding: resolvable, and of the right kind.
fn validate_conditions(
    context: &str,
    conditions: &[ConditionDef],
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    for (i, condition) in conditions.iter().enumerate() {
        let condition_context = format!("{context} condition {i} (`{}`)", condition.kind());
        let target = condition.target().trim();
        if target.is_empty() {
            return Err(format!("{condition_context} target must not be blank"));
        }
        let Some(facts) = index.entities.get(target) else {
            return Err(format!(
                "{condition_context} targets unknown entity `{target}`"
            ));
        };
        match condition {
            ConditionDef::State {
                name,
                target: _,
                equals: _,
            } => {
                let state_name = name.trim();
                if state_name.is_empty() {
                    return Err(format!("{condition_context} names no state"));
                }
                if !facts.state_names.contains(state_name) {
                    return Err(format!(
                        "{condition_context} reads state `{state_name}` on `{target}`, which does \
                         not author that state"
                    ));
                }
            }
            ConditionDef::Locked { target: _ }
            | ConditionDef::Unlocked { target: _ }
            | ConditionDef::DoorOpen { target: _ }
            | ConditionDef::DoorClosed { target: _ } => {
                if !facts.is_door {
                    return Err(format!(
                        "{condition_context} targets `{target}`, a {}; locked/unlocked and \
                         door state conditions need a door",
                        facts.kind
                    ));
                }
            }
            ConditionDef::SequenceRunning { target: _ }
            | ConditionDef::SequenceIdle { target: _ }
            | ConditionDef::Enabled { target: _ }
            | ConditionDef::Disabled { target: _ } => {}
        }
    }
    Ok(())
}

/// The implicit target an action acts on when it omits `target`.
enum ImplicitTarget<'a> {
    /// A binding's own record; a spawn template binding passes the template's
    /// component facts, which are the facts its instances are born with.
    Actor(&'a str, &'a EntityFacts<'a>),
    /// Every entity a sequence step can run on, discovered statically; empty
    /// when no `start_sequence` names the sequence on an authored entity.
    Owners(&'a [String]),
}

/// One action's diagnostic identity: where it was authored and which action it
/// is.
struct ActionContext<'a> {
    /// Owning record context, e.g. ``Prop 3 (`switch`) binding 0``.
    context: &'a str,
    /// Position of the action in its binding or step list.
    index: usize,
    /// Serialized action tag, e.g. `open`.
    kind: &'a str,
}

/// The `context` action `index` (`kind`) prefix every action error starts with.
fn action_context_label(action_context: &ActionContext<'_>) -> String {
    format!(
        "{} action {} (`{}`)",
        action_context.context, action_context.index, action_context.kind
    )
}

/// One source's action list: bounded, non-empty and composed only of
/// implemented actions with resolvable, capability-compatible targets.
fn validate_action_list(
    context: &str,
    actions: &[ActionDef],
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    if actions.is_empty() {
        return Err(format!("{context} must declare at least one action"));
    }
    if actions.len() > MAX_ACTIONS_PER_SOURCE {
        return Err(format!(
            "{context} declares {} actions; the limit is {MAX_ACTIONS_PER_SOURCE}",
            actions.len()
        ));
    }
    for (i, action) in actions.iter().enumerate() {
        let action_context = ActionContext {
            context,
            index: i,
            kind: action.kind(),
        };
        validate_action(&action_context, action, implicit, index)?;
    }
    Ok(())
}

/// One action: every target resolves and the action fits the target's
/// capabilities.
#[expect(clippy::too_many_lines, reason = "one match arm per action kind")] // one match arm per action kind
fn validate_action(
    action_context: &ActionContext<'_>,
    action: &ActionDef,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    match action {
        ActionDef::RunScript { script, handler } => {
            script.require_role("scripts", "luau")?;
            crate::engine::scripting::validate_handler(handler)?;
        }
        ActionDef::ChangeLevel { level } => {
            if !index.transitions.contains(level.as_str()) {
                return Err(format!("Undeclared level transition '{level}'"));
            }
        }
        ActionDef::ShowMessage { title, lines } => {
            crate::engine::level::validate_message(title, lines)?;
        }
        ActionDef::HandoffItem { target }
        | ActionDef::ReleaseItem { target }
        | ActionDef::RetireHidden { target } => {
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "an authored carry contract",
                |facts| facts.has_carry,
            )?;
        }
        ActionDef::AiSenses { target, rules } => {
            if rules.len() > 16
                || !rules
                    .iter()
                    .all(crate::engine::ai::authoring::SenseRule::is_valid)
            {
                return Err("Invalid bounded AI sense rules".into());
            }
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "an AI body",
                |facts| facts.has_ai,
            )?;
        }
        ActionDef::AiPause {
            target,
            key,
            paused: _,
        } => {
            if !crate::engine::ai::authoring::name_valid(key) {
                return Err("Invalid bounded AI task key".into());
            }
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "an AI body",
                |facts| facts.has_ai,
            )?;
        }
        ActionDef::AiTask {
            target,
            key,
            request,
        } => {
            if !crate::engine::ai::authoring::name_valid(key)
                || request
                    .as_deref()
                    .is_some_and(|r| r.key != *key || !r.is_valid())
            {
                return Err("Invalid bounded AI task".into());
            }
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "an AI body",
                |facts| facts.has_ai,
            )?;
        }
        ActionDef::Navigate {
            target,
            position,
            speed_mps,
        } => {
            if speed_mps.is_some_and(|speed| !speed.is_finite() || !(0.05..=8.0).contains(&speed)) {
                return Err("Navigation speed must be finite within 0.05..=8 m/s".into());
            }
            if position
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 100_000.0)
            {
                return Err("Navigation goal must be finite within 100000 metres".into());
            }
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "an AI navigation body",
                |facts| facts.has_ai,
            )?;
        }
        ActionDef::EndRun {
            won: _,
            title,
            detail,
        } => {
            crate::engine::level::validate_run_feedback(title, detail)?;
        }
        ActionDef::Blood { emission } => emission.validate()?,
        ActionDef::Ragdoll {
            target: _,
            enabled: _,
            velocity,
        } => {
            let speed = glam::Vec3::from_array(*velocity);
            if !speed.is_finite() || speed.length() > 15.0 {
                return Err("ragdoll velocity must be finite and at most 15 m/s".into());
            }
        }
        ActionDef::MannequinPins {
            group,
            enabled: _,
            target,
        } => {
            crate::engine::physics::config::validate_reset_group(group)?;
            check_action_target(
                action_context,
                Some(target),
                implicit,
                index,
                "a declared miniatures state",
                |facts| facts.state_names.contains("miniatures"),
            )?;
            if !index.physics_reset_groups.contains(group.as_str()) {
                return Err(format!("Unknown mannequin pin group `{group}`"));
            }
        }
        ActionDef::PlayerForm { model, scale } => {
            if !scale.is_finite()
                || !(0.1..=4.0).contains(scale)
                || model
                    .as_ref()
                    .is_some_and(|id| !crate::game::representation::is_eligible(id))
            {
                return Err(
                    "player_form requires an eligible model and finite scale 0.1..=4".into(),
                );
            }
        }
        ActionDef::ResetToStart => {}
        ActionDef::ResetPhysicsGroup { group } | ActionDef::ActivatePhysicsGroup { group } => {
            crate::engine::physics::config::validate_reset_group(group)?;
            if !index.physics_reset_groups.contains(group.as_str()) {
                return Err(format!(
                    "{} references unknown movable physics group `{group}`",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::ToggleWeather => {
            if !index.weather_toggleable {
                return Err(format!(
                    "{} requires weather and weather_alternate",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::SetWeatherStrength {
            strength,
            transition_seconds,
        } => {
            crate::engine::weather::validate_strength(*strength, *transition_seconds)?;
            if !index.weather_blendable {
                return Err(format!(
                    "{} requires weather and weather_alternate with equal count and material",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::SetWeatherCycle { enabled: _ } => {
            if !index.weather_cycle {
                return Err(format!(
                    "{} requires weather_cycle",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::SetPrompt { target, prompt } => {
            if prompt.trim().is_empty() {
                return Err(format!(
                    "{} prompt must not be blank",
                    action_context_label(action_context)
                ));
            }
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a target with an `interactable` component",
                |facts| facts.has_interactable,
            )?;
        }
        ActionDef::Open { target }
        | ActionDef::Close { target }
        | ActionDef::Lock { target }
        | ActionDef::Unlock { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a door target",
            |facts| facts.is_door,
        )?,
        ActionDef::Toggle { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a door or a switchable fixed light",
            |facts| facts.is_door || (facts.light_capable() && facts.light_switchable),
        )?,
        ActionDef::Enable { target } | ActionDef::Disable { target } => {
            validate_explicit_target(action_context, target.as_deref(), index)?;
        }
        ActionDef::SetLight { target, on: _ } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a switchable light with `switchable: true`; a non-switchable light is baked once and cannot change",
            |facts| facts.light_capable() && facts.light_switchable,
        )?,
        ActionDef::PlayAnimation {
            target,
            clip,
            looped: _,
        }
        | ActionDef::ToggleAnimation { target, clip } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a target with an `animation` component",
                |facts| facts.has_animation,
            )?;
            if clip
                .as_deref()
                .is_some_and(|clip_name| clip_name.trim().is_empty())
            {
                return Err(format!(
                    "{} clip must not be blank when specified",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::PlaySound {
            target,
            sound,
            looped: _,
        } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a target with an `audio` component",
                |facts| facts.has_audio,
            )?;
            if sound
                .as_ref()
                .is_some_and(|sound_name| sound_name.trim().is_empty())
            {
                return Err(format!(
                    "{} `sound` must not be blank when specified",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::StopSound { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a target with an `audio` component",
            |facts| facts.has_audio,
        )?,
        ActionDef::ChangeMaterial { target, variant } => {
            let variant_name = variant.trim();
            if variant_name.is_empty() {
                return Err(format!(
                    "{} needs a non-empty material variant name",
                    action_context_label(action_context)
                ));
            }
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a runtime instance of a spawn template with a `material` component (a \
                 baked static prop's material is prepared geometry and cannot change)",
                |facts| facts.is_spawn_template && facts.has_material,
            )?;
            check_action_variant(
                action_context,
                target.as_deref(),
                implicit,
                index,
                variant_name,
            )?;
        }
        ActionDef::MoveObject {
            target,
            x,
            y,
            z,
            speed,
        } => {
            if !x.is_finite() || !z.is_finite() || y.is_some_and(|height| !height.is_finite()) {
                return Err(format!(
                    "{} coordinates must be finite numbers",
                    action_context_label(action_context)
                ));
            }
            if let Some(speed_mps) = speed
                && (!speed_mps.is_finite() || *speed_mps <= 0.0)
            {
                return Err(format!(
                    "{} speed must be a finite positive number of metres per second",
                    action_context_label(action_context)
                ));
            }
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a runtime instance of a spawn template (a baked static prop and a door \
                 cannot move; drive a door with `open`/`close`/`toggle`)",
                |facts| facts.is_spawn_template,
            )?;
        }
        ActionDef::SetState {
            target,
            name,
            value: _,
        } => {
            let state_name = name.trim();
            if state_name.is_empty() {
                return Err(format!(
                    "{} needs a non-empty state name",
                    action_context_label(action_context)
                ));
            }
            let requirement = format!(
                "an authored state named `{state_name}` (or a timer/trigger-volume target: the \
                 runtime owns those states); a `set_state` may only change a state the \
                 target already authors"
            );
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                &requirement,
                |facts| facts.state_names.contains(state_name) || facts.is_timer || facts.is_volume,
            )?;
        }
        ActionDef::ToggleLabel { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a placed prop target with an `interactable` label capability",
            |facts| facts.is_prop && facts.has_interactable,
        )?,
        ActionDef::StartSequence { sequence, target } => {
            let sequence_id = sequence.trim();
            if sequence_id.is_empty() {
                return Err(format!(
                    "{} needs a non-empty sequence id",
                    action_context_label(action_context)
                ));
            }
            if !index.sequences.contains(sequence_id) {
                return Err(format!(
                    "{} references unknown sequence `{sequence_id}`",
                    action_context_label(action_context)
                ));
            }
            for_each_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                |_subject, _facts| Ok(()),
            )?;
        }
        ActionDef::StopSequence { target } => {
            for_each_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                |_subject, _facts| Ok(()),
            )?;
        }
        ActionDef::StartTimer {
            target,
            seconds,
            repeat: _,
        } => {
            check_action_target(
                action_context,
                target.as_deref(),
                implicit,
                index,
                "a timer target",
                |facts| facts.is_timer,
            )?;
            if let Some(duration) = seconds
                && (!duration.is_finite() || *duration <= 0.0)
            {
                return Err(format!(
                    "{} seconds override must be a finite positive number of seconds",
                    action_context_label(action_context)
                ));
            }
        }
        ActionDef::StopTimer { target } => check_action_target(
            action_context,
            target.as_deref(),
            implicit,
            index,
            "a timer target",
            |facts| facts.is_timer,
        )?,
        ActionDef::SpawnEntity {
            template,
            point,
            group,
            name,
        } => {
            let spawn = SpawnAction {
                template: template.as_deref(),
                point: point.as_deref(),
                group: group.as_deref(),
                name: name.as_deref(),
            };
            validate_spawn_action(action_context, &spawn, implicit, index)?;
        }
        ActionDef::DespawnEntity { target } => {
            let target_id = target.trim();
            if target_id.is_empty() {
                return Err(format!(
                    "{} target must not be blank",
                    action_context_label(action_context)
                ));
            }
            if !index.entities.contains_key(target_id) && !index.spawn_groups.contains(target_id) {
                return Err(format!(
                    "{} targets `{target_id}`; `despawn_entity` needs an authored entity id \
                     or spawn group id",
                    action_context_label(action_context)
                ));
            }
        }
    }
    Ok(())
}

/// Resolves one action's optional target and runs `check` on every entity it
/// can act on.
///
/// An explicit target must resolve to an authored record or spawn template; an
/// omitted target is the acting record for a binding, or every owner a
/// sequence can run on for a sequence step. A sequence step whose sequence is
/// never started on any authored entity has nothing to act on and is rejected.
fn for_each_action_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    mut check: impl FnMut(&str, &EntityFacts<'_>) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(raw) = target {
        let resolved = raw.trim();
        if resolved.is_empty() {
            return Err(format!(
                "{} target must not be blank",
                action_context_label(action_context)
            ));
        }
        let Some(facts) = index.facts_of(resolved) else {
            return Err(format!(
                "{} targets unknown entity `{resolved}`",
                action_context_label(action_context)
            ));
        };
        return check(resolved, facts);
    }
    match implicit {
        ImplicitTarget::Actor(actor, facts) => check(actor, facts),
        ImplicitTarget::Owners(owners) => {
            if owners.is_empty() {
                return Err(format!(
                    "{} needs a `target`: the sequence is not started on any authored entity",
                    action_context_label(action_context)
                ));
            }
            for owner in *owners {
                let Some(facts) = index.facts_of(owner) else {
                    return Err(format!(
                        "{} runs on unknown entity `{owner}`",
                        action_context_label(action_context)
                    ));
                };
                check(owner, facts)?;
            }
            Ok(())
        }
    }
}

/// One action's target capability: every resolved target must satisfy
/// `satisfies`, and the failure names the record and what the action needs.
fn check_action_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    requirement: &str,
    satisfies: impl Fn(&EntityFacts<'_>) -> bool,
) -> Result<(), String> {
    for_each_action_target(action_context, target, implicit, index, |subject, facts| {
        if satisfies(facts) {
            return Ok(());
        }
        Err(format!(
            "{} targets `{subject}`, a {}; `{}` requires {requirement}",
            action_context_label(action_context),
            facts.kind,
            action_context.kind
        ))
    })
}

/// One `change_material` target's variant: the named variant must be one the
/// target's `material` component declares, or the runtime selection no-ops.
fn check_action_variant(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
    variant: &str,
) -> Result<(), String> {
    for_each_action_target(action_context, target, implicit, index, |subject, facts| {
        if facts.material_variants.contains(&variant) {
            return Ok(());
        }
        Err(format!(
            "{} targets `{subject}`, a {}; its `material` component declares no variant \
             `{variant}`",
            action_context_label(action_context),
            facts.kind
        ))
    })
}

/// An action whose omitted target is always legal: an explicit target, when
/// present, must still resolve.
fn validate_explicit_target(
    action_context: &ActionContext<'_>,
    target: Option<&str>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    let Some(raw) = target else {
        return Ok(());
    };
    let resolved = raw.trim();
    if resolved.is_empty() {
        return Err(format!(
            "{} target must not be blank",
            action_context_label(action_context)
        ));
    }
    if !index.entities.contains_key(resolved) && !index.spawn_templates.contains_key(resolved) {
        return Err(format!(
            "{} targets unknown entity `{resolved}`",
            action_context_label(action_context)
        ));
    }
    Ok(())
}

/// The authored fields of one `spawn_entity` action.
struct SpawnAction<'a> {
    /// Spawn template id override.
    template: Option<&'a str>,
    /// Spawn point to instantiate at.
    point: Option<&'a str>,
    /// Spawn group override.
    group: Option<&'a str>,
    /// Runtime name for the spawned instance.
    name: Option<&'a str>,
}

/// A `spawn_entity` action: the point resolves, the template exists and any
/// named group exists.
///
/// A template without a point can never resolve, because a template alone has
/// no world position; only the spawn point carries one. With neither field the
/// acting record must itself be a spawn point, whose authored template is
/// used.
fn validate_spawn_action(
    action_context: &ActionContext<'_>,
    spawn: &SpawnAction<'_>,
    implicit: &ImplicitTarget<'_>,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    let label = action_context_label(action_context);
    let template = spawn.template.map(str::trim).filter(|id| !id.is_empty());
    if spawn.template.is_some() && template.is_none() {
        return Err(format!(
            "{label} `template` must not be blank when specified"
        ));
    }
    if let Some(template_id) = template
        && !index.spawn_templates.contains_key(template_id)
    {
        return Err(format!(
            "{label} references unknown spawn template `{template_id}`"
        ));
    }
    if let Some(group) = spawn.group {
        let group_id = group.trim();
        if group_id.is_empty() {
            return Err(format!("{label} `group` must not be blank when specified"));
        }
        if !index.spawn_groups.contains(group_id) {
            return Err(format!(
                "{label} references unknown spawn group `{group_id}`"
            ));
        }
    }
    if let Some(name) = spawn.name
        && name.trim().is_empty()
    {
        return Err(format!(
            "{label} runtime `name` must not be blank when specified"
        ));
    }
    let point = spawn.point.map(str::trim).filter(|id| !id.is_empty());
    if spawn.point.is_some() && point.is_none() {
        return Err(format!("{label} `point` must not be blank when specified"));
    }
    if let Some(point_id) = point {
        if !index.spawn_points.contains(point_id) {
            return Err(format!("{label} targets unknown spawn point `{point_id}`"));
        }
        return Ok(());
    }
    if template.is_some() {
        return Err(format!(
            "{label} names a `template` without a `point`; a template alone has no world \
             position, so name the authored spawn point to spawn at"
        ));
    }
    for_each_action_target(action_context, None, implicit, index, |subject, facts| {
        if facts.is_spawn_point {
            return Ok(());
        }
        Err(format!(
            "{label} targets `{subject}`, a {}; a spawn without a `point` may only come \
             from a spawn point",
            facts.kind
        ))
    })
}

/// Adds `owner` to `sequence`'s owner list when both are known.
///
/// Returns true when the list changed, so the fixpoint can stop early.
fn record_sequence_owner(
    owners: &mut HashMap<String, Vec<String>>,
    sequence: &str,
    owner: &str,
) -> bool {
    let sequence_id = sequence.trim();
    let Some(list) = owners.get_mut(sequence_id) else {
        return false; // an unknown sequence is a named error elsewhere
    };
    if list.iter().any(|existing| existing == owner) {
        return false;
    }
    list.push(owner.to_string());
    true
}

/// Every entity each sequence can run on, gathered from the authored
/// `start_sequence` sites.
///
/// A binding starts a sequence on its own record when the action omits the
/// target; a sequence step starts one on the sequence's own owner. The sites
/// form a bounded fixpoint, so a sequence that starts another with an omitted
/// target propagates its own owners.
pub(super) fn collect_sequence_owners(level: &LevelDef) -> HashMap<String, Vec<String>> {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for sequence in &level.sequences {
        let _owner_slots = owners.entry(sequence.id.trim().to_string()).or_default();
    }
    let mut deferred: Vec<(String, String)> = Vec::new();
    for record in authored_bindings(level) {
        for binding in record.bindings {
            for action in &binding.actions {
                if let ActionDef::StartSequence { sequence, target } = action {
                    match target.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
                        Some(target_id) => {
                            let _record_sequence_owner_status =
                                record_sequence_owner(&mut owners, sequence, target_id);
                        }
                        None => {
                            let _record_sequence_owner_status_2 =
                                record_sequence_owner(&mut owners, sequence, &record.id);
                        }
                    }
                }
            }
        }
    }
    for sequence in &level.sequences {
        let from = sequence.id.trim().to_string();
        for step in &sequence.steps {
            if let SequenceStepDef::Action {
                action:
                    ActionDef::StartSequence {
                        sequence: to,
                        target,
                    },
            } = step
            {
                match target.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
                    Some(target_id) => {
                        let _record_sequence_owner_status_3 =
                            record_sequence_owner(&mut owners, to, target_id);
                    }
                    None => {
                        deferred.push((from.clone(), to.trim().to_string()));
                    }
                }
            }
        }
    }
    let mut passes = 0usize;
    while !deferred.is_empty() && passes <= owners.len() {
        let mut changed = false;
        for (from, to) in &deferred {
            let source = owners.get(from).cloned().unwrap_or_default();
            for owner in source {
                changed |= record_sequence_owner(&mut owners, to, &owner);
            }
        }
        if !changed {
            break;
        }
        passes = passes.saturating_add(1);
    }
    owners
}

/// Authored sequences: bounded steps, finite and ranged delays, and every step
/// action resolvable against the sequence's owners.
///
/// A `wait_animation` step's clip is only checked to be a non-empty name: a
/// model's clip set is not known to the loader, so the step's `timeout` is the
/// runtime bound that keeps a missing clip from stranding the sequence.
#[expect(clippy::too_many_lines, reason = "one match arm per step kind")] // one match arm per step kind
pub(super) fn validate_sequences(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    if level.sequences.len() > MAX_LEVEL_SEQUENCES {
        return Err(format!(
            "Level contains too many sequences: {} (limit: {MAX_LEVEL_SEQUENCES})",
            level.sequences.len()
        ));
    }
    for (i, sequence) in level.sequences.iter().enumerate() {
        let id = sequence.id.trim();
        let context = format!("Sequence {i} (`{id}`)");
        if sequence.steps.is_empty() {
            return Err(format!("{context} declares no steps"));
        }
        if sequence.steps.len() > MAX_SEQUENCE_STEPS {
            return Err(format!(
                "{context} declares {} steps; the limit is {MAX_SEQUENCE_STEPS}",
                sequence.steps.len()
            ));
        }
        let owners: &[String] = index.sequence_owners.get(id).map_or(&[], Vec::as_slice);
        for (j, step) in sequence.steps.iter().enumerate() {
            let step_context = format!("{context} step {j} (`{}`)", step.kind());
            match step {
                SequenceStepDef::Action { action } => {
                    let action_context = ActionContext {
                        context: &step_context,
                        index: 0,
                        kind: action.kind(),
                    };
                    validate_action(
                        &action_context,
                        action,
                        &ImplicitTarget::Owners(owners),
                        index,
                    )?;
                }
                SequenceStepDef::Wait { seconds } => {
                    if !seconds.is_finite() || *seconds < 0.0 || *seconds > MAX_SEQUENCE_WAIT_S {
                        return Err(format!(
                            "{step_context} seconds ({seconds:?}) must be a finite number \
                             between 0 and {MAX_SEQUENCE_WAIT_S}"
                        ));
                    }
                }
                SequenceStepDef::Move { x, y, z, speed } => {
                    if !x.is_finite()
                        || !z.is_finite()
                        || y.is_some_and(|height| !height.is_finite())
                        || !speed.is_finite()
                        || *speed <= 0.0
                    {
                        return Err(format!(
                            "{step_context} position and speed must be finite, and speed \
                             positive"
                        ));
                    }
                }
                SequenceStepDef::Face { yaw_degrees } => {
                    if !yaw_degrees.is_finite() {
                        return Err(format!("{step_context} yaw must be a finite number"));
                    }
                }
                SequenceStepDef::WaitAnimation { clip, timeout } => {
                    if clip
                        .as_deref()
                        .is_some_and(|clip_name| clip_name.trim().is_empty())
                    {
                        return Err(format!(
                            "{step_context} clip must not be blank when specified"
                        ));
                    }
                    if !timeout.is_finite()
                        || *timeout < 0.0
                        || *timeout > MAX_SEQUENCE_ANIMATION_TIMEOUT_S
                    {
                        return Err(format!(
                            "{step_context} timeout ({timeout:?}) must be a finite number \
                             between 0 and {MAX_SEQUENCE_ANIMATION_TIMEOUT_S}"
                        ));
                    }
                }
                SequenceStepDef::Emit { key, on: _ } => {
                    if key
                        .as_deref()
                        .is_some_and(|key_name| key_name.trim().is_empty())
                    {
                        return Err(format!(
                            "{step_context} key must not be blank when specified"
                        ));
                    }
                }
                SequenceStepDef::SetState { name, value: _ } => {
                    let state_name = name.trim();
                    if state_name.is_empty() {
                        return Err(format!("{step_context} must name a non-empty state"));
                    }
                    for owner in owners {
                        if let Some(facts) = index.facts_of(owner)
                            && !facts.state_names.contains(state_name)
                            && !facts.is_timer
                            && !facts.is_volume
                        {
                            return Err(format!(
                                "{step_context} writes state `{state_name}` on `{owner}`, a {}, \
                                 which does not author it; a sequence state write may only \
                                 change a state its owner already authors",
                                facts.kind
                            ));
                        }
                    }
                }
                SequenceStepDef::Stop => {}
            }
        }
    }
    Ok(())
}

/// Authored spawn templates, points and groups: resolvable, finite and
/// bounded.
///
/// An `at_most_one_active` group is deliberately allowed to be shared by
/// several spawn points: that is the encounter mechanism (several candidate
/// spots, one live member), and the runtime refuses a second live member, so
/// no check is needed here.
pub(super) fn validate_spawns(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    if level.spawn_templates.len() > MAX_LEVEL_SPAWN_TEMPLATES {
        return Err(format!(
            "Level contains too many spawn templates: {} (limit: {MAX_LEVEL_SPAWN_TEMPLATES})",
            level.spawn_templates.len()
        ));
    }
    if level.spawn_points.len() > MAX_LEVEL_SPAWN_POINTS {
        return Err(format!(
            "Level contains too many spawn points: {} (limit: {MAX_LEVEL_SPAWN_POINTS})",
            level.spawn_points.len()
        ));
    }
    if level.spawn_groups.len() > MAX_LEVEL_SPAWN_GROUPS {
        return Err(format!(
            "Level contains too many spawn groups: {} (limit: {MAX_LEVEL_SPAWN_GROUPS})",
            level.spawn_groups.len()
        ));
    }
    for (i, template) in level.spawn_templates.iter().enumerate() {
        let id = template.id.trim();
        let context = format!("Spawn template {i} (`{id}`)");
        if template.model.trim().is_empty() {
            return Err(format!("{context} must reference a non-empty model id"));
        }
        if !template.scale.is_finite() || template.scale <= 0.0 {
            return Err(format!("{context} scale must be a finite positive number"));
        }
        if let Some(seconds) = template.lifetime_seconds
            && (!seconds.is_finite() || seconds <= 0.0)
        {
            return Err(format!(
                "{context} lifetime_seconds must be a finite positive number when specified"
            ));
        }
    }
    for (i, point) in level.spawn_points.iter().enumerate() {
        let id = point.id.trim();
        let context = format!("Spawn point {i} (`{id}`)");
        if !point.x.is_finite()
            || !point.z.is_finite()
            || !point.yaw_degrees.is_finite()
            || point.y.is_some_and(|y| !y.is_finite())
        {
            return Err(format!("{context} position and yaw must be finite numbers"));
        }
        let template = point.template.trim();
        if template.is_empty() {
            return Err(format!(
                "{context} must reference a non-empty spawn template id"
            ));
        }
        if !index.spawn_templates.contains_key(template) {
            return Err(format!(
                "{context} references unknown spawn template `{template}`"
            ));
        }
        if let Some(group) = point.group.as_deref() {
            let group_id = group.trim();
            if group_id.is_empty() {
                return Err(format!("{context} group must not be blank when specified"));
            }
            if !index.spawn_groups.contains(group_id) {
                return Err(format!(
                    "{context} references unknown spawn group `{group_id}`"
                ));
            }
        }
    }
    Ok(())
}

/// Authored timers: a positive, finite period.
pub(super) fn validate_timers(level: &LevelDef) -> Result<(), String> {
    for (i, timer) in level.timers.iter().enumerate() {
        let id = timer.id.trim();
        let context = format!("Timer {i} (`{id}`)");
        if !timer.seconds.is_finite() || timer.seconds <= 0.0 {
            return Err(format!(
                "{context} seconds must be a finite positive number of seconds"
            ));
        }
    }
    Ok(())
}

/// Upper bound on the depth of the zero-delay cycle search.
///
/// A deeper chain is left to the runtime's chain budget rather than rejecting
/// a map the compiler cannot see whole.
const MAX_CYCLE_DEPTH: usize = 26;
/// Upper bound on the nodes one zero-delay cycle search may visit.
const MAX_CYCLE_VISITS: usize = 100_000;

/// One node of the zero-delay causality graph.
#[derive(Clone, PartialEq, Eq, Hash)]
enum CycleNode {
    /// One binding on one owner: its position in the record's list and the
    /// owner's instance id.
    Binding { owner: String, index: usize },
    /// An authored sequence.
    Sequence { id: String },
}

impl CycleNode {
    /// Diagnostic label naming the node.
    fn label(&self) -> String {
        match self {
            Self::Binding { owner, index } => format!("binding {index} on `{owner}`"),
            Self::Sequence { id } => format!("sequence `{id}`"),
        }
    }
}

/// One binding node's cycle-relevant fields.
struct CycleBinding<'a> {
    /// Instance id the binding lives on.
    owner: String,
    /// Position of the binding in its record's list.
    index: usize,
    /// Event kind the binding listens for.
    on: EventKindName,
    /// True when the binding itself may re-enter; a `once`, a cooldown or a
    /// condition can stop a repetition, so the search does not traverse it.
    traversable: bool,
    /// The binding's actions, in order.
    actions: &'a [ActionDef],
}

/// True when a sequence can complete without a delayed step and therefore
/// emits `sequence_complete` in the same immediate chain.
fn sequence_completes_immediately(sequence: &SequenceDef) -> bool {
    for step in &sequence.steps {
        if step.is_delayed() {
            return false;
        }
        if matches!(step, SequenceStepDef::Stop) {
            return true;
        }
    }
    !sequence.looped
}

/// Rejects the obvious zero-delay cycles: a binding that starts a sequence
/// whose first steps (before any wait, move, face, animation wait or timer)
/// start a sequence again, or complete and re-enter the same binding.
///
/// The search is deliberately conservative: a binding with a `when` condition,
/// `once` or a cooldown is not traversed (each of those can stop the
/// repetition), `emit`, `start_timer` and automatic sequence completion are
/// treated as consuming time, and the walk is bounded by [`MAX_CYCLE_DEPTH`]
/// and [`MAX_CYCLE_VISITS`], so a chain the compiler cannot see whole is left
/// to the runtime's own chain budget instead of rejecting a legitimate map.
#[expect(clippy::too_many_lines, reason = "one linear graph construction pass")] // one linear graph construction pass
pub(super) fn validate_zero_delay_cycles(
    level: &LevelDef,
    index: &LevelIndex<'_>,
) -> Result<(), String> {
    let sequence_by_id: HashMap<&str, &SequenceDef> = level
        .sequences
        .iter()
        .map(|sequence| (sequence.id.trim(), sequence))
        .collect();
    let records = authored_bindings(level);
    let mut infos: Vec<CycleBinding<'_>> = Vec::new();
    for record in &records {
        for (i, binding) in record.bindings.iter().enumerate() {
            infos.push(CycleBinding {
                owner: record.id.clone(),
                index: i,
                on: binding.on,
                traversable: !binding.once
                    && binding.cooldown_seconds <= 0.0
                    && binding.when.is_empty(),
                actions: &binding.actions,
            });
        }
    }
    let mut completion_bindings: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, info) in infos.iter().enumerate() {
        if info.on == EventKindName::SequenceComplete {
            completion_bindings
                .entry(info.owner.as_str())
                .or_default()
                .push(i);
        }
    }
    let mut edges: HashMap<CycleNode, Vec<CycleNode>> = HashMap::new();
    let mut order: Vec<CycleNode> = Vec::new();
    for info in &infos {
        let node = CycleNode::Binding {
            owner: info.owner.clone(),
            index: info.index,
        };
        order.push(node.clone());
        let mut out = Vec::new();
        if info.traversable {
            for action in info.actions {
                if let ActionDef::StartSequence {
                    sequence,
                    target: _,
                } = action
                {
                    let sequence_id = sequence.trim();
                    if sequence_by_id.contains_key(sequence_id) {
                        out.push(CycleNode::Sequence {
                            id: sequence_id.to_string(),
                        });
                    }
                }
            }
        }
        drop(edges.insert(node, out));
    }
    for sequence in &level.sequences {
        let id = sequence.id.trim();
        let node = CycleNode::Sequence { id: id.to_string() };
        order.push(node.clone());
        let mut out = Vec::new();
        for step in &sequence.steps {
            if step.is_delayed() || matches!(step, SequenceStepDef::Stop) {
                break;
            }
            if let SequenceStepDef::Action {
                action:
                    ActionDef::StartSequence {
                        sequence: to,
                        target: _,
                    },
            } = step
            {
                let destination_id = to.trim();
                if sequence_by_id.contains_key(destination_id) {
                    out.push(CycleNode::Sequence {
                        id: destination_id.to_string(),
                    });
                }
            }
        }
        if sequence_completes_immediately(sequence)
            && let Some(owners) = index.sequence_owners.get(id)
        {
            for owner in owners {
                if let Some(list) = completion_bindings.get(owner.as_str()) {
                    for binding_index in list {
                        if let Some(info) = infos.get(*binding_index)
                            && info.traversable
                        {
                            out.push(CycleNode::Binding {
                                owner: info.owner.clone(),
                                index: info.index,
                            });
                        }
                    }
                }
            }
        }
        drop(edges.insert(node, out));
    }
    let mut state: HashMap<CycleNode, u8> = HashMap::new();
    let mut visits = MAX_CYCLE_VISITS;
    for node in &order {
        if state.get(node).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<CycleNode> = Vec::new();
        if let Some(cycle) = cycle_dfs(node, 0, &edges, &mut state, &mut stack, &mut visits) {
            let path = cycle
                .iter()
                .map(CycleNode::label)
                .collect::<Vec<String>>()
                .join(" -> ");
            return Err(format!(
                "Zero-delay cycle: {path}. Each edge runs without consuming a wait, timer \
                 or animation, so the chain would recurse without end; add a delayed step, \
                 a timer, a `once`, a cooldown or a `when` condition."
            ));
        }
    }
    Ok(())
}

/// Depth-first cycle search over the zero-delay graph.
///
/// White nodes are unvisited, grey nodes are on the current path and black
/// nodes are finished. A grey neighbour is a cycle; the path from its first
/// occurrence to the current node is returned. The search stops without a
/// verdict once the depth or visit budget runs out, so it never rejects a map
/// it could not see whole.
fn cycle_dfs(
    node: &CycleNode,
    depth: usize,
    edges: &HashMap<CycleNode, Vec<CycleNode>>,
    state: &mut HashMap<CycleNode, u8>,
    stack: &mut Vec<CycleNode>,
    visits: &mut usize,
) -> Option<Vec<CycleNode>> {
    if *visits == 0 || depth > MAX_CYCLE_DEPTH {
        return None;
    }
    *visits = visits.saturating_sub(1);
    let _previous_value = state.insert(node.clone(), 1);
    stack.push(node.clone());
    if let Some(neighbours) = edges.get(node) {
        for next in neighbours {
            match state.get(next).copied().unwrap_or(0) {
                1 => {
                    let position = stack.iter().position(|item| item == next).unwrap_or(0);
                    let mut cycle = stack.get(position..).unwrap_or_default().to_vec();
                    if let Some(first) = cycle.first().cloned() {
                        cycle.push(first);
                    }
                    return Some(cycle);
                }
                0 => {
                    if let Some(cycle) =
                        cycle_dfs(next, depth.saturating_add(1), edges, state, stack, visits)
                    {
                        return Some(cycle);
                    }
                }
                _ => {}
            }
        }
    }
    drop(stack.pop());
    let _previous_value_2 = state.insert(node.clone(), 2);
    None
}

/// Resolves every authored sound request before compilation or package admission.
pub(super) fn validate_audio_references(level: &LevelDef) -> Result<(), String> {
    let components = level
        .props
        .iter()
        .flat_map(|prop| &prop.components)
        .chain(level.doors.iter().flat_map(|door| &door.components))
        .chain(
            level
                .spawn_templates
                .iter()
                .flat_map(|template| &template.components),
        );
    let mut sounds: Vec<&str> = components
        .filter_map(|component| {
            if let ComponentDef::Audio {
                sound,
                gain: _,
                looped: _,
                enabled: _,
                playing: _,
                time_domain: _,
            } = component
            {
                Some(sound.as_str())
            } else {
                None
            }
        })
        .collect();
    sounds.extend(
        level
            .sound_emitters
            .iter()
            .map(|emitter| emitter.sound.as_str()),
    );
    let binding_groups = level.all_bindings();
    let binding_actions = binding_groups
        .iter()
        .flat_map(|bindings| bindings.iter())
        .flat_map(|binding| &binding.actions);
    let sequence_actions = level
        .sequences
        .iter()
        .flat_map(|sequence| &sequence.steps)
        .filter_map(|step| {
            if let SequenceStepDef::Action { action } = step {
                Some(action)
            } else {
                None
            }
        });
    sounds.extend(
        binding_actions
            .chain(sequence_actions)
            .filter_map(|action| {
                if let ActionDef::PlaySound {
                    sound: Some(sound),
                    target: _,
                    looped: _,
                } = action
                {
                    Some(sound.as_str())
                } else {
                    None
                }
            }),
    );
    if sounds.is_empty() {
        return Ok(());
    }
    let root = crate::engine::assets::resolve_asset_root().ok_or_else(|| {
        "Authored audio requires an installed assets/audio/catalog.json".to_string()
    })?;
    let catalog = crate::engine::audio::AudioCatalog::load(&root)?;
    for sound in sounds {
        if !catalog.contains(sound) {
            return Err(format!(
                "Unknown authored audio cue `{sound}` in assets/audio/catalog.json"
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_sound_emitters(level: &LevelDef) -> Result<(), String> {
    const MAX_SOUND_EMITTERS: usize = 256;
    if level.sound_emitters.len() > MAX_SOUND_EMITTERS {
        return Err(format!(
            "Level contains too many sound emitters (limit: {MAX_SOUND_EMITTERS})"
        ));
    }
    for (index, emitter) in level.sound_emitters.iter().enumerate() {
        if emitter.attenuation.is_some_and(|[near, far]| {
            !near.is_finite() || !far.is_finite() || near < 0.0 || far <= near || far > 1_000.0
        }) {
            return Err(format!(
                "Sound emitter {index} attenuation requires 0 <= near < far <= 1000 metres"
            ));
        }
        if [emitter.x, emitter.y, emitter.z]
            .iter()
            .any(|value| !value.is_finite())
            || !emitter.gain.is_finite()
            || !(0.0..=1.0).contains(&emitter.gain)
            || emitter.sound.trim().is_empty()
        {
            return Err(format!(
                "Sound emitter {index} needs finite world coordinates, gain in 0..=1, and a sound cue"
            ));
        }
    }
    Ok(())
}

fn named_spawn_ids(level: &LevelDef) -> HashSet<&str> {
    let mut names = HashSet::new();
    for bindings in level.all_bindings() {
        for action in bindings.iter().flat_map(|binding| &binding.actions) {
            if let ActionDef::SpawnEntity {
                name: Some(name),
                template: _,
                point: _,
                group: _,
            } = action
            {
                let _ = names.insert(name.trim());
            }
        }
    }
    for step in level.sequences.iter().flat_map(|sequence| &sequence.steps) {
        if let SequenceStepDef::Action {
            action:
                ActionDef::SpawnEntity {
                    name: Some(name),
                    template: _,
                    point: _,
                    group: _,
                },
        } = step
        {
            let _ = names.insert(name.trim());
        }
    }
    names
}

fn validate_volume_filter(
    context: &str,
    level: &LevelDef,
    filter: &crate::engine::level::TriggerFilterDef,
    level_index: &LevelIndex<'_>,
) -> Result<(), String> {
    if let crate::engine::level::TriggerFilterDef::Entity { instance } = filter {
        if !level_index.named_spawns.contains(instance.as_str())
            && !level_index
                .entities
                .get(instance)
                .is_some_and(|facts| !facts.is_timer && !facts.is_volume)
        {
            return Err(format!(
                "{context} filter entity `{instance}` must name a placed entity with a world position or an authored named spawn"
            ));
        }
        return Ok(());
    }
    let crate::engine::level::TriggerFilterDef::Item {
        asset,
        instance,
        held: _,
    } = filter
    else {
        return Ok(());
    };
    if let Some(asset_id) = asset
        && (!crate::engine::assets::is_valid_asset_id(asset_id)
            || !PropCatalog::load_default().contains(asset_id))
    {
        return Err(format!(
            "{context} filter names unknown item asset `{asset_id}`"
        ));
    }
    if let Some(instance_id) = instance {
        let ids = level.prop_instance_ids();
        let prop = ids
            .iter()
            .position(|id| id == instance_id)
            .and_then(|index| level.props.get(index))
            .filter(|prop| prop.is_physics_movable())
            .ok_or_else(|| {
                format!(
                    "{context} filter instance `{instance_id}` must name a movable physical prop"
                )
            })?;
        if asset
            .as_deref()
            .is_some_and(|required| required != prop.model)
        {
            return Err(format!(
                "{context} filter asset and instance selectors refer to different item types"
            ));
        }
    }
    Ok(())
}

/// Authored trigger volumes: count, geometry, vertical bounds and room
/// containment.
///
/// A trigger is a real volume with an effect, so the checks mirror the water
/// volumes and ladders: the footprint must be finite and positive, the
/// resolved top strictly above the resolved bottom, and the volume must
/// overlap a room section. What each volume's `enter_volume`/`exit_volume`
/// bindings do is validated with every other binding.
pub(super) fn validate_volumes(level: &LevelDef, index: &LevelIndex<'_>) -> Result<(), String> {
    if level.timers.len() > crate::engine::level::MAX_LEVEL_TIMERS {
        return Err(format!(
            "Level contains too many timers: {} (limit: {})",
            level.timers.len(),
            crate::engine::level::MAX_LEVEL_TIMERS
        ));
    }
    if u64::try_from(level.volumes.len()).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_AREA_TRIGGERS
    {
        return Err(format!(
            "Level contains too many trigger volumes: {} (limit: {})",
            level.volumes.len(),
            crate::engine::level::MAX_LEVEL_AREA_TRIGGERS
        ));
    }
    for (i, volume) in level.volumes.iter().enumerate() {
        let context = format!("Trigger volume {i} (`{}`)", volume_id(i, volume));
        validate_volume_filter(&context, level, &volume.filter, index)?;
        if !volume.x.is_finite()
            || !volume.z.is_finite()
            || !volume.width.is_finite()
            || !volume.depth.is_finite()
        {
            return Err(format!(
                "{context} position and size must be finite numbers"
            ));
        }
        if volume.width <= 0.0 || volume.depth <= 0.0 {
            return Err(format!("{context} width and depth must be positive"));
        }
        for (name, value) in [("bottom_y", volume.bottom_y), ("top_y", volume.top_y)] {
            if value.is_some_and(|height| !height.is_finite()) {
                return Err(format!(
                    "{context} {name} must be a finite number when specified"
                ));
            }
        }
        let (bottom, top) = volume.resolved_y_bounds(level);
        if !bottom.is_finite() || !top.is_finite() || top <= bottom {
            return Err(format!(
                "{context} top_y ({top:.2} m) must be above its bottom_y ({bottom:.2} m)"
            ));
        }
        if !rect_overlaps_room(level, volume.bounds()) {
            return Err(format!("{context} lies outside every room section"));
        }
    }
    Ok(())
}

/// Effects: count, kind, bounds and material.
pub(super) fn validate_effects(level: &LevelDef) -> Result<(), String> {
    if level.effects.len() > crate::engine::level::MAX_LEVEL_EFFECTS {
        return Err(format!(
            "Level contains too many effects: {} (limit: {})",
            level.effects.len(),
            crate::engine::level::MAX_LEVEL_EFFECTS
        ));
    }
    for (i, effect) in level.effects.iter().enumerate() {
        if !effect
            .kind
            .eq_ignore_ascii_case(crate::engine::level::EFFECT_KIND_STEAM)
        {
            return Err(format!(
                "Effect {i} has unknown kind `{}`; expected `{}`",
                effect.kind,
                crate::engine::level::EFFECT_KIND_STEAM
            ));
        }
        if !effect.x.is_finite()
            || !effect.y.is_finite()
            || !effect.z.is_finite()
            || !effect.width.is_finite()
            || !effect.depth.is_finite()
            || !effect.height.is_finite()
            || !effect.size.is_finite()
            || !effect.drift.is_finite()
            || !effect.lifetime_seconds.is_finite()
        {
            return Err(format!("Effect {i} parameters must be finite numbers"));
        }
        if effect.width <= 0.0 || effect.depth <= 0.0 || effect.height <= 0.0 || effect.size <= 0.0
        {
            return Err(format!(
                "Effect {i} width, depth, height and size must be positive"
            ));
        }
        if effect.count == 0 || effect.count > crate::engine::level::MAX_EFFECT_PARTICLES {
            return Err(format!(
                "Effect {i} count must be between 1 and {}",
                crate::engine::level::MAX_EFFECT_PARTICLES
            ));
        }
        if effect.drift < 0.0 {
            return Err(format!("Effect {i} drift cannot be negative"));
        }
        if effect.lifetime_seconds <= 0.0 || effect.lifetime_seconds > 60.0 {
            return Err(format!(
                "Effect {i} lifetime_seconds must be between 0 and 60"
            ));
        }
        if let Some(material) = effect.material.as_deref()
            && material.trim().is_empty()
        {
            return Err(format!(
                "Effect {i} material must not be blank when specified"
            ));
        }
    }
    Ok(())
}

/// Existing authored action sites, shared with Luau export/reference validation.
pub fn script_calls(level: &LevelDef) -> Vec<(&crate::engine::project::ProjectPath, &str)> {
    let mut calls = Vec::new();
    for record in authored_bindings(level) {
        for binding in record.bindings {
            for action in &binding.actions {
                if let ActionDef::RunScript { script, handler } = action {
                    calls.push((script, handler.as_str()));
                }
            }
        }
    }
    for effect in &level.effects {
        for binding in &effect.bindings {
            for action in &binding.actions {
                if let ActionDef::RunScript { script, handler } = action {
                    calls.push((script, handler.as_str()));
                }
            }
        }
    }
    for sequence in &level.sequences {
        for step in &sequence.steps {
            if let crate::engine::entities::sequences::SequenceStepDef::Action {
                action: ActionDef::RunScript { script, handler },
            } = step
            {
                calls.push((script, handler.as_str()));
            }
        }
    }
    calls
}
