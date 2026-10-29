//! One opt-in gameplay checkpoint per stable installed project/level/content.
use super::Game;
use crate::engine::loader::LoadedLevel;
use crate::engine::package::hash::sha256_hex;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "tests/checkpoints.rs"]
mod tests;

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub project: String,
    pub level: String,
    pub content: String,
}

impl Identity {
    pub fn for_entry(entry: &crate::engine::loader::LevelEntry) -> Result<Option<Self>, String> {
        let opened = crate::engine::package::world::open_level(&entry.path, &entry.id)?;
        if !opened.level.saving {
            return Ok(None);
        }
        let project = opened
            .project_id
            .unwrap_or_else(|| format!("standalone:{}", entry.id));
        let installation =
            std::fs::canonicalize(&entry.path).map_err(|problem| problem.to_string())?;
        Ok(Some(Self {
            project: format!(
                "{project}:{}",
                sha256_hex(installation.to_string_lossy().as_bytes())
            ),
            level: entry.id.clone(),
            content: crate::engine::package::world::manifest_identity(&opened.manifest)?,
        }))
    }

    pub fn for_level(loaded: &LoadedLevel) -> Result<Self, String> {
        if !loaded.level.saving {
            return Err("Saving is disabled for this level".into());
        }
        let project = loaded
            .project_id
            .clone()
            .unwrap_or_else(|| format!("standalone:{}", loaded.level.id));
        let content = loaded
            .package_identity
            .clone()
            .ok_or("Gameplay checkpoints need a verified installed package")?;
        let current_project = crate::engine::package::project::inspect(&loaded.entry.path)?
            .map_or_else(
                || format!("standalone:{}", loaded.level.id),
                |package| package.project.id,
            );
        if current_project != project
            || crate::engine::package::world::package_identity(&loaded.entry)? != content
        {
            return Err(
                "Level package changed during this visit. Reload or start new before saving."
                    .into(),
            );
        }
        // Installation scope prevents an unrelated same-named project adopting another save.
        let installation =
            std::fs::canonicalize(&loaded.entry.path).map_err(|problem| problem.to_string())?;
        let scoped_project = format!(
            "{project}:{}",
            sha256_hex(installation.to_string_lossy().as_bytes())
        );
        Ok(Self {
            project: scoped_project,
            level: loaded.level.id.clone(),
            content,
        })
    }
    fn stem(&self) -> String {
        sha256_hex(format!("{}\n{}", self.project, self.level).as_bytes())
    }
    pub fn path(&self) -> Result<PathBuf, String> {
        crate::engine::paths::user().map(|paths| {
            paths
                .saves()
                .join(format!("checkpoint-{}.json", self.stem()))
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    feet: [f32; 3],
    yaw: f32,
    pitch: f32,
    crouched: bool,
}

impl Game {
    pub(super) fn checkpoint_player(&self) -> Result<Player, String> {
        if !self.grounded
            || self.swimming
            || self.climbing.is_some()
            || self.water_exit.is_some()
            || self.noclip.is_some()
            || self.form_model.is_some()
            || (self.world_time_scale - 1.0).abs() > f32::EPSILON
        {
            return Err("Checkpoint needs a grounded human player outside swimming, climbing or developer modes".into());
        }
        for value in self.player_position.to_array() {
            crate::engine::entities::checkpoint::number(value, 100_000.0)?;
        }
        crate::engine::entities::checkpoint::number(self.player_yaw, super::TWO_PI)?;
        crate::engine::entities::checkpoint::number(self.player_pitch, super::MAX_PITCH)?;
        let current_feet =
            glam::Vec3::new(self.player_position.x, self.feet_y, self.player_position.z);
        if !self.body_fits_for(current_feet, self.stance.height(), super::PLAYER_RADIUS)
            || self.player_is_out_of_map()
        {
            return Err("Checkpoint needs a clear supported player pose".into());
        }
        Ok(Player {
            feet: [self.player_position.x, self.feet_y, self.player_position.z],
            yaw: self.player_yaw,
            pitch: self.player_pitch,
            crouched: self.stance == super::Stance::Crouched,
        })
    }
    /// # Errors
    /// Called on the candidate world before GPU admission; rejects unsafe poses.
    pub(super) fn restore_checkpoint_player(&mut self, player: &Player) -> Result<(), String> {
        for value in player.feet {
            crate::engine::entities::checkpoint::number(value, 100_000.0)?;
        }
        crate::engine::entities::checkpoint::number(player.yaw, super::TWO_PI)?;
        crate::engine::entities::checkpoint::number(player.pitch, super::MAX_PITCH)?;
        self.stance = if player.crouched {
            super::Stance::Crouched
        } else {
            super::Stance::Standing
        };
        self.eye_offset_current = self.stance.eye_offset();
        self.feet_y = player.feet.get(1).copied().ok_or("Invalid player feet")?;
        self.player_position = {
            let [x, _, z] = player.feet;
            glam::Vec3::new(x, self.feet_y + self.eye_offset_current, z)
        };
        self.player_yaw = player.yaw;
        self.player_pitch = player.pitch;
        self.horizontal_velocity = glam::Vec2::ZERO;
        self.vertical_velocity = 0.0;
        self.grounded = true;
        self.player_floor_y = self.feet_y;
        self.duck_fraction = if player.crouched { 1.0 } else { 0.0 };
        let feet = glam::Vec3::from_array(player.feet);
        if !self.body_fits_for(feet, self.stance.height(), super::PLAYER_RADIUS)
            || self.player_is_out_of_map()
        {
            return Err("Checkpoint player pose is obstructed or outside this level. Start new to play from the beginning.".into());
        }
        self.reseed_trigger_inside();
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub player: Player,
    pub world: crate::engine::entities::checkpoint::Snapshot,
    pub items: Vec<(String, crate::engine::physics::checkpoint::Item)>,
    pub flashlight: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    identity: Identity,
    name: String,
    checksum: String,
    payload: String,
}

pub struct Saved {
    pub snapshot: Snapshot,
    pub name: String,
    pub recovered: bool,
}

fn decode(bytes: &[u8], identity: &Identity) -> Result<Saved, String> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(
        |_problem| "Checkpoint is truncated or corrupt. Start new to play from the beginning.",
    )?;
    if envelope.version != VERSION || &envelope.identity != identity {
        return Err("Checkpoint belongs to different content or a different save version. Start new to play from the beginning.".into());
    }
    if envelope.name.is_empty()
        || envelope.name.len() > 80
        || envelope.name.chars().any(char::is_control)
        || envelope.checksum != sha256_hex(envelope.payload.as_bytes())
    {
        return Err(
            "Checkpoint integrity check failed. Start new to play from the beginning.".into(),
        );
    }
    let snapshot = serde_json::from_str(&envelope.payload)
        .map_err(|_problem| "Checkpoint state is invalid. Start new to play from the beginning.")?;
    Ok(Saved {
        snapshot,
        name: envelope.name,
        recovered: false,
    })
}

fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|problem| problem.to_string())?;
    let mut bytes = Vec::new();
    let _count = file
        .take(MAX_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|problem| problem.to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err("Checkpoint exceeds 4 MiB".into());
    }
    Ok(bytes)
}

pub fn read_at(path: &Path, identity: &Identity) -> Result<Option<Saved>, String> {
    if !path.exists() && !path.with_extension("previous").exists() {
        return Ok(None);
    }
    match bounded_read(path).and_then(|bytes| decode(&bytes, identity)) {
        Ok(saved) => Ok(Some(saved)),
        Err(problem) => match bounded_read(&path.with_extension("previous"))
            .and_then(|bytes| decode(&bytes, identity))
        {
            Ok(mut saved) => {
                saved.recovered = true;
                Ok(Some(saved))
            }
            Err(_) => Err(problem),
        },
    }
}

pub fn read(identity: &Identity) -> Result<Option<Saved>, String> {
    read_at(&identity.path()?, identity)
}

fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().ok_or("Checkpoint path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
    let temporary = parent.join(format!(
        ".checkpoint-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|problem| problem.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|problem| problem.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|problem| problem.to_string())
    })();
    if result.is_err()
        && let Err(problem) = std::fs::remove_file(&temporary)
        && problem.kind() != std::io::ErrorKind::NotFound
    {
        crate::engine::logging::warn(format!("Checkpoint temporary cleanup: {problem}"));
    }
    result
}

pub fn write_at(
    path: &Path,
    identity: &Identity,
    name: &str,
    snapshot: &Snapshot,
) -> Result<(), String> {
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err("Invalid checkpoint name".into());
    }
    let payload = serde_json::to_string(snapshot).map_err(|problem| problem.to_string())?;
    let envelope = Envelope {
        version: VERSION,
        identity: identity.clone(),
        name: name.into(),
        checksum: sha256_hex(payload.as_bytes()),
        payload,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|problem| problem.to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err("Checkpoint exceeds 4 MiB".into());
    }
    // Only a validated previous save is eligible for recovery. A failed backup or
    // replacement leaves the primary intact; stale temp files are never promoted.
    if let Ok(previous) = bounded_read(path)
        && decode(&previous, identity).is_ok()
    {
        atomic(&path.with_extension("previous"), &previous)?;
    }
    atomic(path, &bytes)
}

pub fn write(identity: &Identity, name: &str, snapshot: &Snapshot) -> Result<(), String> {
    write_at(&identity.path()?, identity, name, snapshot)
}
