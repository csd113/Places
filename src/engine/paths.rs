//! Installation resources and per-user storage shared by player and tools.
//! Resolution is explicit/testable; only the process adapters snapshot the environment.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const ASSET_ROOT_ENV: &str = "PLACES_ASSET_ROOT";
pub const STATE_ROOT_ENV: &str = "PLACES_STATE_ROOT";
pub const CONFIG_ROOT_ENV: &str = "PLACES_CONFIG_ROOT";
pub const CACHE_ROOT_ENV: &str = "PLACES_CACHE_ROOT";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Windows,
    Unix,
}

/// Inputs can be constructed without changing process globals or the working directory.
#[derive(Clone, Debug)]
pub struct PathConfig {
    pub platform: Platform,
    pub cwd: PathBuf,
    pub executable: PathBuf,
    pub asset_override: Option<PathBuf>,
    pub state_override: Option<PathBuf>,
    pub config_override: Option<PathBuf>,
    pub cache_override: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub xdg_cache: Option<PathBuf>,
    /// Used only when the executable actually resides below this checkout's target/.
    pub checkout: Option<PathBuf>,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

impl PathConfig {
    /// # Errors
    /// Returns an unavailable launch directory or executable path.
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            platform: if cfg!(target_os = "macos") {
                Platform::Mac
            } else if cfg!(target_os = "windows") {
                Platform::Windows
            } else {
                Platform::Unix
            },
            cwd: std::env::current_dir()
                .map_err(|error| format!("Cannot read launch directory: {error}"))?,
            executable: std::env::current_exe()
                .map_err(|error| format!("Cannot locate executable: {error}"))?,
            asset_override: env_path(ASSET_ROOT_ENV),
            state_override: env_path(STATE_ROOT_ENV),
            config_override: env_path(CONFIG_ROOT_ENV),
            cache_override: env_path(CACHE_ROOT_ENV),
            home: env_path(if cfg!(target_os = "windows") {
                "USERPROFILE"
            } else {
                "HOME"
            }),
            local_app_data: env_path("LOCALAPPDATA"),
            xdg_data: env_path("XDG_DATA_HOME"),
            xdg_config: env_path("XDG_CONFIG_HOME"),
            xdg_cache: env_path("XDG_CACHE_HOME"),
            checkout: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
        })
    }

    fn absolute(&self, path: &Path) -> Result<PathBuf, String> {
        if path.as_os_str().is_empty() || !self.cwd.is_absolute() {
            return Err(
                "path resolution needs a nonempty path and absolute launch directory".into(),
            );
        }
        Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        })
    }

    fn absolute_override(&self, path: &Path, name: &str) -> Result<PathBuf, String> {
        self.absolute(path)
            .map_err(|error| format!("Invalid {name}: {error}; unset it or supply a directory"))
    }

    /// Candidate asset directories. Launch-directory contents are never searched.
    #[must_use]
    pub fn asset_candidates(&self) -> Vec<PathBuf> {
        if let Some(explicit) = &self.asset_override {
            return self
                .absolute(explicit)
                .map_or_else(|_| Vec::new(), |path| vec![path]);
        }
        let mut candidates = Vec::new();
        if let Some(directory) = self.executable.parent() {
            if self.platform == Platform::Mac
                && directory.file_name().is_some_and(|name| name == "MacOS")
            {
                candidates.push(directory.join("../Resources/assets"));
            }
            candidates.push(directory.join("assets"));
            if self.platform == Platform::Unix {
                candidates.push(directory.join("../share/places/assets"));
            }
        }
        if let Some(checkout) = &self.checkout
            && self.executable.starts_with(checkout.join("target"))
        {
            candidates.push(checkout.join("assets"));
        }
        candidates
    }

    /// `explicit` (CLI/API) > environment > executable layout > actual Cargo checkout.
    /// Both assets/ itself and the historical directory containing assets/ are accepted.
    /// # Errors
    /// Explicit invalid overrides and broken discovered installations never fall through.
    pub fn resolve_assets(&self, explicit: Option<&Path>) -> Result<PathBuf, String> {
        if let Some(path) = explicit.or(self.asset_override.as_deref()) {
            return asset_directory(
                &self.absolute_override(path, "PLACES_ASSET_ROOT/--asset-root")?,
            );
        }
        let candidates = self.asset_candidates();
        for candidate in &candidates {
            match fs::symlink_metadata(candidate) {
                Ok(_) => return asset_directory(candidate),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Cannot inspect assets {}: {error}",
                        candidate.display()
                    ));
                }
            }
        }
        Err(format!(
            "Required Places assets missing. Expected assets/catalog.json beside the executable, \
             Contents/Resources/assets in a Mac bundle, or share/places/assets in a Unix install. \
             Checked: {}. Install the complete resource payload or set {ASSET_ROOT_ENV}=<assets directory>.",
            candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    /// # Errors
    /// Missing platform homes, relative XDG homes and invalid overrides are actionable errors.
    pub fn resolve_user(&self) -> Result<UserPaths, String> {
        let data = if let Some(path) = &self.state_override {
            self.absolute_override(path, STATE_ROOT_ENV)?
        } else {
            match self.platform {
                Platform::Mac => self.home_dir()?.join("Library/Application Support/Places"),
                Platform::Windows => self.local_app_data.as_deref()
                    .filter(|path| path.is_absolute())
                    .ok_or("LOCALAPPDATA is unavailable or relative; set PLACES_STATE_ROOT to writable storage")?
                    .join("Places"),
                Platform::Unix => self.xdg_home(self.xdg_data.as_deref(), ".local/share")?.join("places"),
            }
        };
        let config = if let Some(path) = &self.config_override {
            self.absolute_override(path, CONFIG_ROOT_ENV)?
        } else if self.state_override.is_some() || self.platform != Platform::Unix {
            data.clone()
        } else {
            self.xdg_home(self.xdg_config.as_deref(), ".config")?
                .join("places")
        };
        let cache = if let Some(path) = &self.cache_override {
            self.absolute_override(path, CACHE_ROOT_ENV)?
        } else if self.state_override.is_some() || self.platform == Platform::Windows {
            data.join("cache")
        } else if self.platform == Platform::Mac {
            self.home_dir()?.join("Library/Caches/Places")
        } else {
            self.xdg_home(self.xdg_cache.as_deref(), ".cache")?
                .join("places")
        };
        Ok(UserPaths {
            data,
            config,
            cache,
        })
    }

    fn home_dir(&self) -> Result<&Path, String> {
        self.home
            .as_deref()
            .filter(|path| path.is_absolute())
            .ok_or_else(|| {
                "User home is unavailable or relative; set PLACES_STATE_ROOT to writable storage"
                    .into()
            })
    }

    fn xdg_home(&self, supplied: Option<&Path>, fallback: &str) -> Result<PathBuf, String> {
        match supplied {
            Some(path) if path.as_os_str().is_empty() => Ok(self.home_dir()?.join(fallback)),
            Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
            Some(path) => Err(format!(
                "XDG directory {} must be absolute; unset it or correct it",
                path.display()
            )),
            None => Ok(self.home_dir()?.join(fallback)),
        }
    }
}

fn asset_directory(path: &Path) -> Result<PathBuf, String> {
    let selected = if path.join("catalog.json").is_file() {
        path.to_path_buf()
    } else {
        path.join("assets")
    };
    if !selected.join("catalog.json").is_file() {
        return Err(format!(
            "Required asset catalog missing at {} (or {}/catalog.json). Correct {ASSET_ROOT_ENV}/--asset-root or reinstall the resource payload",
            selected.join("catalog.json").display(),
            path.display()
        ));
    }
    fs::canonicalize(&selected)
        .map_err(|error| format!("Cannot resolve assets {}: {error}", selected.display()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserPaths {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
}

impl UserPaths {
    #[must_use]
    pub fn levels(&self) -> PathBuf {
        self.data.join("levels")
    }
    #[must_use]
    pub fn imports(&self) -> PathBuf {
        self.data.join("import")
    }
    #[must_use]
    pub fn projects(&self) -> PathBuf {
        self.data.join("projects")
    }
    #[must_use]
    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    #[must_use]
    pub fn diagnostics(&self) -> PathBuf {
        self.data.join("diagnostics")
    }

    /// Create required folders without touching existing contents. Probe write access now,
    /// so a read-only directory is reported before settings/imports appear to succeed.
    /// # Errors
    /// Names the failed directory and the override used for recovery.
    pub fn initialize(&self, resources: &Path) -> Result<(), String> {
        let directories = [
            self.data.clone(),
            self.config.clone(),
            self.cache.clone(),
            self.levels(),
            self.imports(),
            self.projects(),
            self.diagnostics(),
        ];
        // Validate every path before creating or probing any of them. Existing
        // user-directory symlinks must never redirect writes into installed assets.
        for path in &directories {
            check_writable_target(path, resources)?;
        }
        for path in &directories {
            initialize_directory(path).map_err(|error| format!(
                "Cannot initialize writable Places directory {}: {error}. Choose writable storage with {STATE_ROOT_ENV}, {CONFIG_ROOT_ENV} or {CACHE_ROOT_ENV}.", path.display()
            ))?;
        }
        Ok(())
    }
}

fn check_writable_target(root: &Path, resources: &Path) -> Result<(), String> {
    let mut existing = root;
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or("User directory has no existing ancestor")?;
    }
    let resolved = fs::canonicalize(existing)
        .map_err(|error| format!("Cannot resolve {}: {error}", root.display()))?;
    let tail = root
        .strip_prefix(existing)
        .map_err(|error| error.to_string())?;
    if tail
        .components()
        .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(format!(
            "Uncreated user directory {} contains '..'; choose an unambiguous path",
            root.display()
        ));
    }
    if resolved.join(tail).starts_with(resources) {
        return Err(format!(
            "Writable directory {} is inside immutable assets {}; correct the Places storage overrides",
            root.display(),
            resources.display()
        ));
    }
    Ok(())
}

fn initialize_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    for attempt in 0_u8..16 {
        let probe = path.join(format!(
            ".places-write-check-{}-{attempt}",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
        {
            Ok(file) => {
                drop(file);
                return fs::remove_file(probe);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("all write-check names are occupied"))
}

static ASSETS: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static USER: OnceLock<Result<UserPaths, String>> = OnceLock::new();
static CONFIG: OnceLock<Result<PathConfig, String>> = OnceLock::new();

fn process_config() -> Result<&'static PathConfig, String> {
    CONFIG
        .get_or_init(PathConfig::from_env)
        .as_ref()
        .map_err(Clone::clone)
}

/// Pin a CLI/API resource choice before using engine resources; no environment mutation.
/// # Errors
/// Rejects an invalid root or changing a process's already selected resource tree.
pub fn configure_assets(explicit: &Path) -> Result<(), String> {
    let root = process_config()?.resolve_assets(Some(explicit))?;
    let selected = ASSETS.get_or_init(|| Ok(root.clone()));
    match selected {
        Ok(existing) if existing == &root => Ok(()),
        _ => Err(
            "Assets already resolved for this process; configure them before loading resources"
                .into(),
        ),
    }
}

/// # Errors
/// Returns the cached installation discovery error, without fallback to a different tree.
pub fn assets() -> Result<&'static Path, String> {
    ASSETS
        .get_or_init(|| process_config()?.resolve_assets(None))
        .as_ref()
        .map(PathBuf::as_path)
        .map_err(Clone::clone)
}

/// # Errors
/// Returns the cached platform/override configuration error.
pub fn user() -> Result<&'static UserPaths, String> {
    USER.get_or_init(|| process_config()?.resolve_user())
        .as_ref()
        .map_err(Clone::clone)
}

/// Prepare shared writable storage, refusing a configured root inside immutable assets.
/// # Errors
/// Returns resource/storage errors before the player opens a window.
pub fn initialize_user() -> Result<&'static UserPaths, String> {
    let selected = user()?;
    let resources = assets()?;
    selected.initialize(resources)?;
    Ok(selected)
}
