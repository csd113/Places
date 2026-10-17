//! Nonblocking compiler and player ownership. No subprocess changes cwd or
//! resolves resources independently of the shared installation/path policy.

mod child;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use crate::engine::paths;
use crate::engine::project::ProjectFolder;
use crate::map_compiler::events::{Outcome, Progress};
use crate::map_compiler::{BuildReport, project};

use child::{CapturedChild, StopPolicy};

#[derive(Clone, Debug)]
pub struct ToolPaths {
    pub compiler: PathBuf,
    pub game: PathBuf,
}

impl ToolPaths {
    /// Resolves sibling tools or explicit environment overrides once, without
    /// searching cwd or PATH. CLI overrides can replace either public path.
    ///
    /// # Errors
    /// Reports unavailable executable/launch paths or empty overrides.
    pub fn discover() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let parent = executable
            .parent()
            .ok_or("Editor executable has no parent directory")?;
        let sibling = |name: &str| {
            parent.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            })
        };
        Ok(Self {
            compiler: tool_override("PLACES_COMPILER", sibling("places-compile"))?,
            game: tool_override("PLACES_GAME", sibling("places"))?,
        })
    }

    /// # Errors
    /// Rejects empty paths or unavailable launch directories.
    pub fn set_compiler(&mut self, path: &Path) -> Result<(), String> {
        self.compiler = absolute(path)?;
        Ok(())
    }

    /// # Errors
    /// Rejects empty paths or unavailable launch directories.
    pub fn set_game(&mut self, path: &Path) -> Result<(), String> {
        self.game = absolute(path)?;
        Ok(())
    }
}

fn tool_override(name: &str, fallback: PathBuf) -> Result<PathBuf, String> {
    std::env::var_os(name).map_or(Ok(fallback), |value| absolute(Path::new(&value)))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err("An executable path must not be empty".into());
    }
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path))
    }
}

fn tool_command(path: &Path) -> Result<Command, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "Executable is missing or not an absolute file: {}",
            path.display()
        ));
    }
    let mut command = Command::new(path);
    let _assets = command.env(paths::ASSET_ROOT_ENV, paths::assets()?);
    let user = paths::user()?;
    let _state = command.env(paths::STATE_ROOT_ENV, &user.data);
    let _config = command.env(paths::CONFIG_ROOT_ENV, &user.config);
    let _cache = command.env(paths::CACHE_ROOT_ENV, &user.cache);
    Ok(command)
}

#[derive(Debug)]
pub enum BuildOutcome {
    Succeeded(Box<BuildReport>),
    Failed(String),
    Cancelled,
}

pub struct CompileJob {
    process: CapturedChild,
    output: PathBuf,
    progress: Option<Progress>,
    diagnostics: String,
    finished: bool,
}

impl CompileJob {
    /// Starts one build from already saved project documents. The caller owns
    /// the single job slot and must save/resolve dirty state before calling.
    ///
    /// # Errors
    /// Rejects undeclared levels, escaped/default output paths and launch errors.
    pub fn start(tools: &ToolPaths, folder: &ProjectFolder, level: &str) -> Result<Self, String> {
        let output = project::output_path(folder.root(), Some(level))?;
        let parent = output
            .parent()
            .ok_or("Build output has no parent directory")?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|error| error.to_string())?;
        if !canonical_parent.starts_with(folder.root()) {
            return Err("Build output must remain inside the chosen project".into());
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&output)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(format!(
                "Build output must be a regular package file: {}",
                output.display()
            ));
        }
        let mut command = tool_command(&tools.compiler)?;
        let _configured = command
            .arg("build")
            .arg(folder.root())
            .arg("--level")
            .arg(level)
            .arg("--out")
            .arg(&output)
            .arg("--asset-root")
            .arg(paths::assets()?)
            .arg("--events")
            .arg("--progress")
            .arg("off");
        Ok(Self {
            process: CapturedChild::spawn(&mut command, StopPolicy::Cooperative, true)?,
            output,
            progress: None,
            diagnostics: String::new(),
            finished: false,
        })
    }

    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
    pub const fn progress(&self) -> Option<&Progress> {
        self.progress.as_ref()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &str {
        &self.diagnostics
    }

    #[must_use]
    pub const fn cancelling(&self) -> bool {
        self.process.stopping()
    }

    pub fn cancel(&mut self) {
        self.process.stop();
    }

    /// Drains bounded feedback and checks process completion without waiting.
    pub fn poll(&mut self) -> Option<BuildOutcome> {
        if self.finished {
            return None;
        }
        let polled = self.process.poll();
        let capture = self.process.capture();
        self.progress.clone_from(&capture.progress);
        self.diagnostics.clone_from(&capture.diagnostics);
        let result = match polled {
            Ok(None) => return None,
            Ok(Some(status)) => completed_build(status.success(), &self.output, &capture),
            Err(error) => BuildOutcome::Failed(error),
        };
        self.finished = true;
        Some(result)
    }
}

fn completed_build(success: bool, output: &Path, capture: &child::Capture) -> BuildOutcome {
    if let Some(error) = &capture.protocol_error {
        return BuildOutcome::Failed(format!("Compiler event error: {error}"));
    }
    match capture.terminal.as_ref() {
        Some((Outcome::Success, Some(report), _))
            if success && capture.started && Path::new(&report.out) == output =>
        {
            BuildOutcome::Succeeded(report.clone())
        }
        Some((Outcome::Cancelled, None, _)) if !success => BuildOutcome::Cancelled,
        Some((Outcome::Failed, None, message)) if !success => BuildOutcome::Failed(
            message
                .clone()
                .unwrap_or_else(|| "Compiler reported failure".into()),
        ),
        Some(_) => BuildOutcome::Failed("Compiler result and exit status do not agree".into()),
        None => BuildOutcome::Failed(if capture.diagnostics.is_empty() {
            "Compiler exited without a final result event".into()
        } else {
            format!(
                "Compiler exited without a final result: {}",
                capture.diagnostics
            )
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayPhase {
    Importing,
    Playing,
}

#[derive(Debug)]
pub enum PlayOutcome {
    Returned,
    Stopped,
    Failed(String),
}

pub struct PlayJob {
    process: CapturedChild,
    tools: ToolPaths,
    level: String,
    state_root: PathBuf,
    phase: PlayPhase,
    stopping: bool,
    finished: bool,
    diagnostics: String,
}

impl PlayJob {
    /// Imports through the player's stable import boundary, then launches the
    /// direct player with the exact selected ID. Both steps remain asynchronous.
    ///
    /// # Errors
    /// Reports missing packages/tools or process creation errors.
    pub fn start(tools: &ToolPaths, package: &Path, level: &str) -> Result<Self, String> {
        let canonical_package = std::fs::canonicalize(package)
            .map_err(|error| format!("Package {}: {error}", package.display()))?;
        let state_root = play_state_root(&paths::user()?.cache, &canonical_package, level);
        let mut command = tool_command(&tools.game)?;
        let _configured = command.arg("--import").arg(canonical_package);
        configure_play(&mut command, &state_root);
        Ok(Self {
            process: CapturedChild::spawn(&mut command, StopPolicy::Terminate, false)?,
            tools: tools.clone(),
            level: level.into(),
            state_root,
            phase: PlayPhase::Importing,
            stopping: false,
            finished: false,
            diagnostics: String::new(),
        })
    }

    #[must_use]
    pub const fn phase(&self) -> PlayPhase {
        self.phase
    }

    #[must_use]
    pub fn diagnostics(&self) -> &str {
        &self.diagnostics
    }

    pub fn stop(&mut self) {
        self.stopping = true;
        self.process.stop();
    }

    pub fn poll(&mut self) -> Option<PlayOutcome> {
        if self.finished {
            return None;
        }
        let polled = self.process.poll();
        self.diagnostics
            .clone_from(&self.process.capture().diagnostics);
        let result = match polled {
            Ok(None) => return None,
            Ok(Some(_)) if self.stopping => PlayOutcome::Stopped,
            Ok(Some(status)) if !status.success() => PlayOutcome::Failed(self.failure(status)),
            Ok(Some(_)) if self.phase == PlayPhase::Importing => match self.launch_player() {
                Ok(()) => return None,
                Err(error) => PlayOutcome::Failed(error),
            },
            Ok(Some(_)) => PlayOutcome::Returned,
            Err(error) => PlayOutcome::Failed(error),
        };
        self.finished = true;
        Some(result)
    }

    fn failure(&self, status: ExitStatus) -> String {
        format!(
            "Player {:?} failed ({status}): {}",
            self.phase, self.diagnostics
        )
    }

    fn launch_player(&mut self) -> Result<(), String> {
        let mut command = tool_command(&self.tools.game)?;
        let _configured = command.env("PLACES_LEVEL", &self.level);
        configure_play(&mut command, &self.state_root);
        self.process = CapturedChild::spawn(&mut command, StopPolicy::Terminate, false)?;
        self.phase = PlayPhase::Playing;
        Ok(())
    }
}

fn play_state_root(cache: &Path, package: &Path, level: &str) -> PathBuf {
    // Canonical package identity separates projects that use the same source
    // filename, even when their level IDs also match. Encoded OS bytes preserve
    // non-UTF8 native paths; only the resulting portable hash enters the path.
    let mut identity = package.as_os_str().as_encoded_bytes().to_vec();
    identity.push(0);
    identity.extend_from_slice(level.as_bytes());
    cache
        .join("editor-play")
        .join(crate::engine::package::hash::sha256_hex(&identity))
}

fn configure_play(command: &mut Command, state_root: &Path) {
    // Editor test runs must never install packages or settings into the user's
    // normal player profile. Import and windowed play share this isolated root.
    let _state = command.env(paths::STATE_ROOT_ENV, state_root);
    let _config = command.env(paths::CONFIG_ROOT_ENV, state_root.join("config"));
    let _cache = command.env(paths::CACHE_ROOT_ENV, state_root.join("cache"));
}

#[cfg(test)]
#[path = "tests/process.rs"]
mod tests;
