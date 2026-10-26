//! Native forms compile to existing emission records and bounded editable Luau.
//! No renderer state or independent lighting scheduler belongs to the editor.
use super::{Document, ObjectId, objects, persistence};
use crate::engine::project::{ProjectPath, read_bounded_file};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flicker {
    pub enabled: bool,
    pub circuit: bool,
    pub autostart: bool,
    pub steady: f32,
    pub dark: f32,
    pub gap: f32,
    pub pulses: u8,
    pub delay: f32,
}
impl Default for Flicker {
    fn default() -> Self {
        Self {
            enabled: false,
            circuit: false,
            autostart: true,
            steady: 3.0,
            dark: 0.12,
            gap: 0.1,
            pulses: 1,
            delay: 0.0,
        }
    }
}
impl Flicker {
    pub fn validate(&self) -> Result<(), String> {
        for (label, value, minimum, maximum) in [
            ("Steady seconds", self.steady, 0.1, 300.0),
            ("Dark seconds", self.dark, 0.05, 300.0),
            ("Burst gap seconds", self.gap, 0.05, 30.0),
            ("Start delay seconds", self.delay, 0.0, 3600.0),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(format!("{label} must be {minimum} to {maximum}"));
            }
        }
        if !(1..=8).contains(&self.pulses) {
            return Err("Burst count must be 1 to 8".into());
        }
        Ok(())
    }
}

impl Document {
    pub(crate) fn flicker_owner(&self, id: ObjectId) -> Result<Option<(usize, Value)>, String> {
        let ObjectId::Light(index) = id else {
            return Err("Select a placed light fixture".into());
        };
        let ids = self.level().light_instance_ids();
        let identity = ids.get(index).ok_or("Selected fixture no longer exists")?;
        let mut active = None;
        let mut disabled = None;
        for (i, light) in self
            .raw()
            .get("ceiling_lights")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(meta) = light.get("editor_flicker") else {
                continue;
            };
            if meta.pointer("/settings/enabled").and_then(Value::as_bool) == Some(true) {
                if meta
                    .get("members")
                    .and_then(Value::as_array)
                    .is_some_and(|members| members.iter().any(|v| v.as_str() == Some(identity)))
                {
                    if active.is_some() {
                        return Err(
                            "Overlapping flicker controllers: disable the old controllers first"
                                .into(),
                        );
                    }
                    active = Some((i, meta.clone()));
                }
            } else if i == index {
                disabled = Some((i, meta.clone()));
            }
        }
        Ok(active.or(disabled))
    }
    pub(crate) fn flicker_config(&self, id: ObjectId) -> Result<Flicker, String> {
        if let Some((_, meta)) = self.flicker_owner(id)? {
            return serde_json::from_value(
                meta.get("settings")
                    .cloned()
                    .ok_or("Missing flicker settings")?,
            )
            .map_err(|e| format!("Flicker settings: {e}"));
        }
        let ObjectId::Light(index) = id else {
            return Err("Select a light fixture".into());
        };
        let light = self
            .level()
            .ceiling_lights
            .get(index)
            .ok_or("Missing fixture")?;
        Ok(Flicker {
            circuit: light.switch_group.is_some(),
            ..Flicker::default()
        })
    }
    pub(crate) fn flicker_script(&self, id: ObjectId) -> Result<Option<String>, String> {
        Ok(self.flicker_owner(id)?.and_then(|(_, meta)| {
            meta.get("script")
                .and_then(Value::as_str)
                .map(str::to_owned)
        }))
    }
    pub(crate) fn flicker_members(
        &self,
        id: ObjectId,
        circuit: bool,
    ) -> Result<Vec<usize>, String> {
        let ObjectId::Light(index) = id else {
            return Err("Select a placed light fixture".into());
        };
        members(self.level(), index, circuit)
    }
    pub(crate) fn apply_flicker(
        &mut self,
        id: ObjectId,
        settings: &Flicker,
        take_control: bool,
    ) -> Result<(), String> {
        settings.validate()?;
        let old = self.flicker_owner(id)?;
        let members = if settings.enabled {
            self.flicker_members(id, settings.circuit)?
        } else {
            Vec::new()
        };
        for member in &members {
            if self
                .flicker_owner(ObjectId::Light(*member))?
                .as_ref()
                .map(|v| v.0)
                != old.as_ref().map(|v| v.0)
                && self
                    .flicker_owner(ObjectId::Light(*member))?
                    .is_some_and(|(_, meta)| {
                        meta.pointer("/settings/enabled").and_then(Value::as_bool) == Some(true)
                    })
            {
                return Err("This circuit overlaps another flicker controller. Disable that controller first.".into());
            }
        }
        let mut raw = self.raw().clone();
        self.release_flicker(&mut raw, old, take_control)?;
        let ids = self.level().light_instance_ids();
        // Pin derived fixture IDs before edits, so later insert/delete operations cannot retarget scripts.
        for (i, identity) in ids.iter().enumerate() {
            drop(
                objects::object_mut(&mut raw, ObjectId::Light(i))?
                    .insert("id".into(), json!(identity)),
            );
        }
        let ObjectId::Light(index) = id else {
            return Err("Select a light fixture".into());
        };
        let identity = ids.get(index).ok_or("Missing stable fixture identity")?;
        let authored_on = self
            .level()
            .ceiling_lights
            .get(index)
            .ok_or("Missing fixture")?
            .enabled;
        let member_ids: Vec<_> = members
            .iter()
            .filter_map(|i| ids.get(*i))
            .cloned()
            .collect();
        if settings.enabled {
            for i in &members {
                let light = objects::object_mut(&mut raw, ObjectId::Light(*i))?;
                drop(light.insert("switchable".into(), json!(true)));
                // Full switches prepared layers. Existing opted-in Static preparation stays intact;
                // other Static worlds hold the authored state instead of flashing only the face.
                if light.get("static_switch").and_then(Value::as_str) != Some("prepared") {
                    drop(light.insert("hold_in_static".into(), json!(true)));
                }
            }
        }
        let metadata = json!({"settings":settings,"members":member_ids,"authored_on":authored_on});
        drop(objects::object_mut(&mut raw, id)?.insert("editor_flicker".into(), metadata));
        if !settings.enabled {
            return self.accept_edit(raw);
        }
        let source = flicker_source(identity, settings, authored_on);
        let hash = crate::engine::package::hash::digest_hex(&Sha256::digest(source.as_bytes()));
        let _created = self.create_script_edit(&raw, &source, |candidate, path| {
            let meta = objects::object_mut(candidate, id)?
                .get_mut("editor_flicker")
                .and_then(Value::as_object_mut)
                .ok_or("Missing flicker metadata")?;
            drop(meta.insert("script".into(), json!(path)));
            drop(meta.insert("generated_hash".into(), json!(hash)));
            Ok(())
        })?;
        Ok(())
    }
    fn release_flicker(
        &self,
        raw: &mut Value,
        old: Option<(usize, Value)>,
        take_control: bool,
    ) -> Result<(), String> {
        if let Some((owner, meta)) = old {
            if let Some(path) = meta.get("script").and_then(Value::as_str) {
                let text = if let Some(buffer) = self.script_text(path) {
                    buffer.to_string()
                } else {
                    let resolved = self.project.resolve(&ProjectPath::new(path.into())?)?;
                    String::from_utf8(read_bounded_file(
                        &resolved,
                        crate::engine::scripting::MAX_SOURCE_BYTES,
                    )?)
                    .map_err(|e| e.to_string())?
                };
                if meta.get("generated_hash").and_then(Value::as_str)
                    != Some(
                        crate::engine::package::hash::digest_hex(&Sha256::digest(text.as_bytes()))
                            .as_str(),
                    )
                    && !take_control
                {
                    return Err("Custom Luau owns this controller. Use 'Replace link / keep source' explicitly; edits and files are never overwritten.".into());
                }
                if self
                    .manifest()
                    .scripts
                    .iter()
                    .chain(self.manifest().levels.iter().flat_map(|l| &l.scripts))
                    .any(|p| p.as_str() == path)
                {
                    return Err("This module is shared by the project manifest; remove that shared link before replacing its controller".into());
                }
                super::behavior::unlink_owned_script(raw, path)?;
            }
            drop(objects::object_mut(raw, ObjectId::Light(owner))?.remove("editor_flicker"));
        }
        Ok(())
    }
    pub(crate) fn apply_glow(
        &mut self,
        asset: &str,
        enabled: bool,
        hz: f32,
        depth: f32,
        phase: f32,
    ) -> Result<(), String> {
        if asset.is_empty() || asset.len() > 96 || asset.chars().any(char::is_control) {
            return Err("Choose a material or fixture asset ID".into());
        }
        if !hz.is_finite() || !(0.01..=24.0).contains(&hz) {
            return Err("Glow frequency must be 0.01 to 24 Hz".into());
        }
        if !depth.is_finite() || !(0.01..=0.85).contains(&depth) {
            return Err("Glow depth must be 0.01 to 0.85".into());
        }
        if !phase.is_finite() || phase.abs() > 1000.0 {
            return Err("Glow phase must be finite, within 1000 cycles".into());
        }
        let mut raw = self.raw().clone();
        let rows = raw
            .as_object_mut()
            .ok_or("Invalid level")?
            .entry("animated_emissions")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid animated emissions")?;
        if rows
            .iter()
            .filter(|v| v.get("material").and_then(Value::as_str) == Some(asset))
            .count()
            > 1
        {
            return Err(
                "Duplicate emission entries for this asset; resolve them in source first".into(),
            );
        }
        if enabled {
            if let Some(row) = rows
                .iter_mut()
                .find(|v| v.get("material").and_then(Value::as_str) == Some(asset))
            {
                let object = row.as_object_mut().ok_or("Invalid emission entry")?;
                for (key, value) in [
                    ("effect", json!("flicker")),
                    ("hz", json!(hz)),
                    ("depth", json!(depth)),
                    ("phase", json!(phase)),
                ] {
                    drop(object.insert(key.into(), value));
                }
            } else {
                rows.push(json!({"material":asset,"effect":"flicker","hz":hz,"depth":depth,"phase":phase}));
            }
        } else {
            rows.retain(|v| v.get("material").and_then(Value::as_str) != Some(asset));
        }
        let _validated = persistence::validate_raw(&raw, self.level_id())?;
        self.accept_edit(raw)
    }
}

fn flicker_source(id: &str, settings: &Flicker, authored_on: bool) -> String {
    let target = super::behavior::luau(&json!(id), 0);
    format!(
        r#"--!strict
-- Editable light flicker. Native api.light selects existing prepared circuit layers.
-- Full lighting required; Static holds the authored state unless prepared Static was opted in.
-- The selected fixture OR its shared circuit is controlled by this module.
-- Link pause/resume/mute/steady/reset handlers in Events / scripts; no wait loops.
-- Direct set_light actions may be superseded by the next flicker edge. Use mute to keep it off.
local target = {target}
local steadySeconds, darkSeconds, gapSeconds = {steady}, {dark}, {gap}
local pulseCount, startDelay = {pulses}, {delay}
local clock = -startDelay
local paused, muted = {paused}, {muted}
local last = nil
local missing = false
local function write(api, on)
    if missing then return end
    if not api.exists(target) then
        missing = true
        api.notice("Flicker target missing: " .. target)
        return
    end
    if last ~= on then api.light(target, on); last = on end
end
local function state()
    if clock < 0 then return true end
    local phase = clock % (steadySeconds + pulseCount * (darkSeconds + gapSeconds))
    return phase < steadySeconds or (phase - steadySeconds) % (darkSeconds + gapSeconds) >= darkSeconds
end
return {{
    start = function(api, event)
        if muted then write(api, false) elseif paused then write(api, true) else write(api, state()) end
    end,
    update = function(api, event)
        if paused or muted or missing then return end
        clock += event.dt
        write(api, state())
    end,
    pause = function(api, event) paused = true end,
    resume = function(api, event) paused = false; muted = false; last = nil; write(api, state()) end,
    mute = function(api, event) muted = true; last = nil; write(api, false) end,
    steady = function(api, event) paused = true; muted = false; last = nil; write(api, true) end,
    reset = function(api, event)
        clock = -startDelay; paused = {paused}; muted = {muted}; missing = false; last = nil
        if muted then write(api, false) elseif paused then write(api, true) else write(api, state()) end
    end,
}}
"#,
        steady = settings.steady,
        dark = settings.dark,
        gap = settings.gap,
        pulses = settings.pulses,
        delay = settings.delay,
        paused = !settings.autostart,
        muted = !authored_on
    )
}

fn members(
    level: &crate::engine::level::LevelDef,
    index: usize,
    circuit: bool,
) -> Result<Vec<usize>, String> {
    let lights = &level.ceiling_lights;
    let selected = lights
        .get(index)
        .ok_or("Selected fixture no longer exists")?;
    if !circuit {
        if selected.switch_group.is_some() {
            return Err("This fixture belongs to a shared circuit. Choose Circuit, or clear Shared circuit in Configure > Components > Light controller first.".into());
        }
        return Ok(vec![index]);
    }
    let group = selected
        .switch_group
        .as_ref()
        .ok_or("Set Shared circuit in Configure > Components > Light controller first")?;
    if level
        .props
        .iter()
        .any(|p| p.switch_group.as_ref() == Some(group))
    {
        return Err("This circuit includes prop lights. Use a fixture-only circuit for prepared illumination flicker.".into());
    }
    let members: Vec<_> = lights
        .iter()
        .enumerate()
        .filter_map(|(i, light)| (light.switch_group.as_ref() == Some(group)).then_some(i))
        .collect();
    if members.len() > 8 {
        return Err("A flicker circuit supports at most eight fixtures".into());
    }
    if members
        .iter()
        .filter_map(|i| lights.get(*i))
        .any(|l| l.enabled != selected.enabled)
    {
        return Err("Circuit members must have the same authored Casts light state".into());
    }
    Ok(members)
}

pub(super) fn validate(raw: &Value, level: &crate::engine::level::LevelDef) -> Result<(), String> {
    let ids = level.light_instance_ids();
    let mut claimed = std::collections::BTreeSet::new();
    for (index, light) in raw
        .get("ceiling_lights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(meta) = light.get("editor_flicker") else {
            continue;
        };
        let settings: Flicker = serde_json::from_value(
            meta.get("settings")
                .cloned()
                .ok_or("Missing flicker settings")?,
        )
        .map_err(|e| format!("Flicker settings: {e}"))?;
        settings.validate()?;
        if !settings.enabled {
            continue;
        }
        if level.ceiling_lights.get(index).map(|l| l.enabled)
            != meta.get("authored_on").and_then(Value::as_bool)
        {
            return Err(
                "Disable illumination flicker before changing Casts light, then apply it again."
                    .into(),
            );
        }
        let current = members(level, index, settings.circuit)?;
        let expected: Vec<_> = current.iter().filter_map(|i| ids.get(*i)).collect();
        if meta.get("members") != Some(&json!(expected)) {
            return Err("Disable illumination flicker before changing its circuit members or fixture IDs, then apply it again.".into());
        }
        for member in current {
            if !claimed.insert(member) {
                return Err("Overlapping illumination flicker controllers".into());
            }
            if !level
                .ceiling_lights
                .get(member)
                .is_some_and(|l| l.switchable)
            {
                return Err("Disable illumination flicker before clearing Switchable".into());
            }
        }
        let path = meta
            .get("script")
            .and_then(Value::as_str)
            .ok_or("Enabled flicker requires its linked Luau module")?;
        if !raw
            .get("scripts")
            .and_then(Value::as_array)
            .is_some_and(|paths| paths.iter().any(|p| p.as_str() == Some(path)))
        {
            return Err("Disable illumination flicker before unlinking its module".into());
        }
    }
    Ok(())
}

pub(super) fn check_delete(
    raw: &Value,
    level: &crate::engine::level::LevelDef,
    id: ObjectId,
) -> Result<(), String> {
    let ObjectId::Light(index) = id else {
        return Ok(());
    };
    let ids = level.light_instance_ids();
    let identity = ids.get(index).ok_or("Missing fixture")?;
    for light in raw
        .get("ceiling_lights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if light
            .pointer("/editor_flicker/settings/enabled")
            .and_then(Value::as_bool)
            == Some(true)
            && light
                .pointer("/editor_flicker/members")
                .and_then(Value::as_array)
                .is_some_and(|members| members.iter().any(|m| m.as_str() == Some(identity)))
        {
            return Err("Disable illumination flicker before deleting one of its fixtures. Source files will be kept.".into());
        }
    }
    Ok(())
}
