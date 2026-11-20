//! One opt-in gameplay checkpoint per stable installed project/level/content.
use super::Game;
use super::campaign::{Installation, LockedReceipt};
use crate::engine::loader::{LevelEntry, LoadedLevel};
use crate::engine::package::hash::sha256_hex;
use crate::engine::package::world::{OpenedPackage, PackageRevision};
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
    fn scoped(
        installation: &Path,
        declared_project: Option<&str>,
        level: &str,
        content: &str,
    ) -> Self {
        let project = declared_project.map_or_else(|| format!("standalone:{level}"), str::to_owned);
        Self {
            project: format!(
                "{project}:{}",
                sha256_hex(installation.to_string_lossy().as_bytes())
            ),
            level: level.into(),
            content: content.into(),
        }
    }

    fn stem(&self) -> String {
        sha256_hex(format!("{}\n{}", self.project, self.level).as_bytes())
    }
    pub fn path(&self) -> Result<PathBuf, String> {
        crate::engine::paths::user().map(|paths| self.path_in(&paths.saves()))
    }

    pub fn path_in(&self, directory: &Path) -> PathBuf {
        directory.join(format!("checkpoint-{}.json", self.stem()))
    }
}

/// Retains the verified package handle through the complete world install.
pub(super) struct PackageGuard {
    _file: std::fs::File,
}

impl PackageGuard {
    pub fn through_commit<T>(guard: Option<Self>, commit: impl FnOnce() -> T) -> T {
        let result = commit();
        drop(guard);
        result
    }
}

/// The archive and collection owner authenticated together during selection.
pub(super) struct Target {
    pub identity: Identity,
    pub installation: Installation,
    pub revision: PackageRevision,
}

impl Target {
    pub fn for_entry(entry: &LevelEntry) -> Result<Option<Self>, String> {
        let canonical =
            std::fs::canonicalize(&entry.path).map_err(|problem| problem.to_string())?;
        let (target, opened) = Self::open(entry, canonical)?;
        Ok(opened.level.saving.then_some(target))
    }

    pub fn for_level(
        loaded: &LoadedLevel,
        installation: &Installation,
        revision: &PackageRevision,
    ) -> Result<Self, String> {
        if !loaded.level.saving {
            return Err("Saving is disabled for this level".into());
        }
        let target = Self::from_package(
            &loaded.entry,
            installation.canonical.clone(),
            loaded.project_id.as_deref(),
            revision.clone(),
        );
        if !target.installation.same_owner(installation) {
            return Err("The collection receipt belongs to a different installed package".into());
        }
        target.check_loaded(loaded, &installation.canonical, revision)?;
        Ok(target)
    }

    fn open(entry: &LevelEntry, canonical: PathBuf) -> Result<(Self, OpenedPackage), String> {
        let (revision, _) = PackageRevision::inspect(&canonical, &entry.id)?;
        let opened = revision.open_level(&canonical, &entry.id)?;
        let target = Self::from_package(entry, canonical, opened.project_id.as_deref(), revision);
        target.verify_current()?;
        Ok((target, opened))
    }

    fn from_package(
        entry: &LevelEntry,
        canonical: PathBuf,
        project: Option<&str>,
        revision: PackageRevision,
    ) -> Self {
        let identity = Identity::scoped(&canonical, project, &entry.id, revision.manifest_sha256());
        let installation = Installation::new(entry.path.clone(), canonical, project, &entry.id);
        Self {
            identity,
            installation,
            revision,
        }
    }

    pub fn verify_current(&self) -> Result<(), String> {
        self.installation.verify_path()?;
        self.revision.check_named_file(&self.installation.canonical)
    }

    pub fn hold_package(&self) -> Result<PackageGuard, String> {
        self.verify_current()?;
        let file = self
            .revision
            .hold(&self.installation.canonical, &self.identity.level)?;
        self.verify_current()?;
        Ok(PackageGuard { _file: file })
    }

    pub fn check_loaded(
        &self,
        loaded: &LoadedLevel,
        canonical: &Path,
        revision: &PackageRevision,
    ) -> Result<(), String> {
        let actual = Installation::new(
            loaded.entry.path.clone(),
            canonical.to_path_buf(),
            loaded.project_id.as_deref(),
            &loaded.level.id,
        );
        if !actual.same_owner(&self.installation)
            || loaded.level.id != self.identity.level
            || revision != &self.revision
            || loaded.package_identity.as_deref() != Some(&self.identity.content)
        {
            return Err("Checkpoint installation or content changed during loading. Select the level again.".into());
        }
        self.verify_current()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    health_rules: Option<crate::engine::health::HealthSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    health: Option<crate::engine::health::HealthState>,
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
            health_rules: Some(*self.health_settings()),
            health: self.health_settings().enabled.then_some(self.player_health),
            feet: [self.player_position.x, self.feet_y, self.player_position.z],
            yaw: self.player_yaw,
            pitch: self.player_pitch,
            crouched: self.stance == super::Stance::Crouched,
        })
    }
    /// # Errors
    /// Called on the candidate world before GPU admission; rejects unsafe poses.
    pub(super) fn restore_checkpoint_player(&mut self, player: &Player) -> Result<(), String> {
        if let Some(rules) = player.health_rules {
            self.set_health_settings(rules)?;
        }
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
        self.player_health = player
            .health
            .unwrap_or_else(|| super::health::initial_player_health(self.world.health_settings()));
        self.reseed_trigger_inside();
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    #[serde(default)]
    pub(super) elevator_cargo: super::app::elevator_cargo::Ledger,
    /// Older saves precede the first explicit collection reset.
    #[serde(default)]
    pub campaign_generation: u64,
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
    pub checksum: String,
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
        checksum: envelope.checksum,
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

pub(super) fn verify_selection(
    path: &Path,
    identity: &Identity,
    checksum: &str,
) -> Result<(), String> {
    let selected = read_at(path, identity)?.ok_or("The selected checkpoint was removed")?;
    if selected.checksum != checksum {
        return Err("The selected checkpoint changed. Select it again before resuming.".into());
    }
    Ok(())
}

pub fn read(identity: &Identity) -> Result<Option<Saved>, String> {
    read_at(&identity.path()?, identity)
}

fn atomic(
    path: &Path,
    bytes: &[u8],
    check: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().ok_or("Checkpoint path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
    let temporary = parent.join(format!(
        ".checkpoint-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|problem| problem.to_string())?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|problem| problem.to_string())?;
        drop(file);
        check()?;
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

#[derive(Debug, PartialEq, Eq)]
pub(super) enum WriteOutcome {
    Written,
    WrittenUnconfirmed(String),
}

#[cfg(test)]
fn write_at(
    path: &Path,
    identity: &Identity,
    name: &str,
    snapshot: &Snapshot,
) -> Result<WriteOutcome, String> {
    write_checked(path, identity, name, snapshot, || Ok(()))
}

fn write_checked(
    path: &Path,
    identity: &Identity,
    name: &str,
    snapshot: &Snapshot,
    check: impl Fn() -> Result<(), String>,
) -> Result<WriteOutcome, String> {
    let parent = path.parent().ok_or("Checkpoint path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
    let directory = open_checkpoint_directory(parent)?;
    let sync = || sync_checkpoint_directory(&directory);
    publish_checkpoint(path, identity, name, snapshot, check, sync)
}

fn publish_checkpoint(
    path: &Path,
    identity: &Identity,
    name: &str,
    snapshot: &Snapshot,
    check: impl Fn() -> Result<(), String>,
    sync: impl Fn() -> Result<(), String>,
) -> Result<WriteOutcome, String> {
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
    // Confirm that this filesystem can synchronize the parent before replacing
    // either file. Successful synchronization is a filesystem acknowledgement.
    sync()?;
    // Only a validated previous save is eligible for recovery. A failed backup or
    // replacement leaves the primary intact; stale temp files are never promoted.
    if let Ok(previous) = bounded_read(path)
        && decode(&previous, identity).is_ok()
    {
        atomic(&path.with_extension("previous"), &previous, &check)?;
    }
    atomic(path, &bytes, &check)?;
    Ok(sync().map_or_else(WriteOutcome::WrittenUnconfirmed, |()| WriteOutcome::Written))
}

fn open_checkpoint_directory(parent: &Path) -> Result<std::fs::File, String> {
    let mut options = std::fs::OpenOptions::new();
    let _access = options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FlushFileBuffers requires write access; BACKUP_SEMANTICS permits
        // opening a directory handle. A refusal is a checkpoint error.
        let _directory_access = options.write(true).custom_flags(0x0200_0000);
    }
    options.open(parent).map_err(|problem| {
        format!("Cannot open the checkpoint directory for synchronization: {problem}")
    })
}

fn sync_checkpoint_directory(directory: &std::fs::File) -> Result<(), String> {
    // macOS File::sync_all uses F_FULLFSYNC on ordinary files; fsync is the
    // directory operation. Windows reports whether FlushFileBuffers succeeded.
    #[cfg(unix)]
    let result = rustix::fs::fsync(directory);
    #[cfg(windows)]
    let result = directory.sync_all();
    result.map_err(|problem| format!("Cannot synchronize the checkpoint directory: {problem}"))
}

pub fn write(
    receipt: &LockedReceipt,
    identity: &Identity,
    name: &str,
    snapshot: &Snapshot,
    check: impl Fn() -> Result<(), String>,
) -> Result<WriteOutcome, String> {
    write_checked(
        &identity.path_in(receipt.directory()?),
        identity,
        name,
        snapshot,
        check,
    )
}
