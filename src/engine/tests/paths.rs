//! Three compact regressions protect platform/override precedence, authoritative
//! resource discovery and first-run persistence. Only tiny temp files/directories;
//! no environment mutation, native resources, map build or checkout assets.
//! The first-run fixture also protects save-file custody and a config-file
//! obstruction: every target must pass preflight before the data root is created.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::paths::{PathConfig, Platform, UserPaths};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "places-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(
            fs::canonicalize(root).map_err(|error| error.to_string())?,
        ))
    }
    fn config(&self) -> PathConfig {
        PathConfig {
            platform: Platform::Mac,
            cwd: self.0.join("arbitrary"),
            executable: self.0.join("installed/places"),
            asset_override: None,
            state_override: None,
            config_override: None,
            cache_override: None,
            home: Some(self.0.join("home")),
            local_app_data: Some(self.0.join("local")),
            xdg_data: None,
            xdg_config: None,
            xdg_cache: None,
            checkout: Some(self.0.join("checkout")),
        }
    }
    fn assets(&self, parent: &str) -> Result<PathBuf, String> {
        let path = self.0.join(parent).join("assets");
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        fs::write(path.join("catalog.json"), b"{}").map_err(|error| error.to_string())?;
        Ok(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn platform_storage_and_explicit_override_precedence() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut config = fixture.config();
    let mac = config.resolve_user()?;
    assert_eq!(
        mac.data,
        fixture.0.join("home/Library/Application Support/Places")
    );
    assert_eq!(mac.config, mac.data);
    assert_eq!(mac.cache, fixture.0.join("home/Library/Caches/Places"));
    config.platform = Platform::Windows;
    assert_eq!(config.resolve_user()?.data, fixture.0.join("local/Places"));
    config.platform = Platform::Unix;
    assert_eq!(
        config.resolve_user()?.config,
        fixture.0.join("home/.config/places")
    );
    config.xdg_data = Some(fixture.0.join("xdg-data"));
    config.xdg_config = Some(fixture.0.join("xdg-config"));
    config.xdg_cache = Some(fixture.0.join("xdg-cache"));
    assert_eq!(
        config.resolve_user()?.data,
        fixture.0.join("xdg-data/places")
    );
    assert_eq!(
        config.resolve_user()?.cache,
        fixture.0.join("xdg-cache/places")
    );
    config.state_override = Some("portable ü".into());
    let portable = config.resolve_user()?;
    assert_eq!(portable.data, config.cwd.join("portable ü"));
    assert_eq!(portable.settings(), portable.data.join("settings.json"));
    assert_eq!(portable.cache, portable.data.join("cache"));
    config.config_override = Some(fixture.0.join("settings"));
    config.cache_override = Some(fixture.0.join("cache"));
    assert_eq!(config.resolve_user()?.config, fixture.0.join("settings"));
    config.state_override = None;
    config.xdg_data = Some("relative-xdg".into());
    assert!(
        config.resolve_user().is_err(),
        "relative XDG must not retarget storage to cwd"
    );
    config.platform = Platform::Windows;
    config.local_app_data = None;
    assert!(
        config.resolve_user().is_err(),
        "missing platform storage must not fall back to install/cwd"
    );
    Ok(())
}

#[test]
fn resources_are_authoritative_and_independent_of_launch_directory() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let installed = fixture.assets("installed")?;
    let override_assets = fixture.assets("chosen ü")?;
    let _decoy = fixture.assets("arbitrary")?;
    let checkout_assets = fixture.assets("checkout")?;
    let mut config = fixture.config();
    assert_eq!(config.resolve_assets(None)?, installed);
    config.asset_override = Some(fixture.0.join("chosen ü"));
    assert_eq!(
        config.resolve_assets(None)?,
        override_assets,
        "legacy package-root override"
    );
    config.asset_override = Some(override_assets.clone());
    assert_eq!(
        config.resolve_assets(None)?,
        override_assets,
        "canonical assets-directory override"
    );
    assert_eq!(
        config.resolve_assets(Some(&installed))?,
        installed,
        "explicit API/CLI choice wins"
    );
    config.asset_override = Some(fixture.0.join("missing"));
    assert!(
        config.resolve_assets(None).is_err(),
        "bad explicit root cannot silently select installed assets"
    );
    config.asset_override = None;
    config.executable = fixture.0.join("foreign/places");
    assert!(
        config.resolve_assets(None).is_err(),
        "release copies cannot read cwd or compile-time checkout"
    );
    config.executable = fixture.0.join("checkout/target/release/places");
    assert_eq!(config.resolve_assets(None)?, checkout_assets);
    config.executable = fixture.0.join("Places.app/Contents/MacOS/places");
    fs::create_dir_all(fixture.0.join("Places.app/Contents/MacOS"))
        .map_err(|error| error.to_string())?;
    assert!(
        config.resolve_assets(None).is_err(),
        "empty bundle has no payload"
    );
    let bundle = fixture.assets("Places.app/Contents/Resources")?;
    assert_eq!(config.resolve_assets(None)?, bundle);
    config.platform = Platform::Unix;
    config.executable = fixture.0.join("prefix/bin/places");
    fs::create_dir_all(fixture.0.join("prefix/bin")).map_err(|error| error.to_string())?;
    let prefix_assets = fixture.assets("prefix/share/places")?;
    assert_eq!(
        config.resolve_assets(None)?,
        prefix_assets,
        "Unix bin/share prefix layout"
    );
    config.asset_override = Some(PathBuf::new());
    assert!(
        config.resolve_assets(None).is_err(),
        "empty overrides must fail"
    );
    assert_eq!(
        config.resolve_assets(Some(&installed))?,
        installed,
        "explicit choice also wins over an empty environment override"
    );
    Ok(())
}

#[test]
fn first_run_creates_only_user_folders_and_preserves_existing_files() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let assets = fixture.assets("read-only-install")?;
    let user = UserPaths {
        data: fixture.0.join("user ü"),
        config: fixture.0.join("config"),
        cache: fixture.0.join("cache"),
    };
    user.initialize(&assets)?;
    for folder in [
        user.levels(),
        user.imports(),
        user.projects(),
        user.saves(),
        user.diagnostics(),
        user.cache.clone(),
    ] {
        assert!(
            folder.is_dir(),
            "required first-run folder {}",
            folder.display()
        );
    }
    fs::write(user.settings(), b"existing settings").map_err(|error| error.to_string())?;
    let package = user.levels().join("existing.placesmap");
    fs::write(&package, b"existing map").map_err(|error| error.to_string())?;
    let save = user.saves().join("existing.save");
    fs::write(&save, b"existing save").map_err(|error| error.to_string())?;
    user.initialize(&assets)?;
    assert_eq!(
        fs::read(user.settings()).map_err(|error| error.to_string())?,
        b"existing settings"
    );
    assert_eq!(
        fs::read(&package).map_err(|error| error.to_string())?,
        b"existing map"
    );
    assert_eq!(
        fs::read(save).map_err(|error| error.to_string())?,
        b"existing save"
    );
    assert_eq!(
        fs::read_dir(&assets)
            .map_err(|error| error.to_string())?
            .count(),
        1,
        "resources are never storage"
    );
    #[cfg(unix)]
    check_storage_symlink_preflight(&fixture, &user, &assets)?;
    let obstruction = fixture.0.join("not-a-directory");
    fs::write(&obstruction, b"keep me").map_err(|error| error.to_string())?;
    let invalid = UserPaths {
        data: fixture.0.join("must-stay-absent"),
        config: obstruction.clone(),
        ..user
    };
    let storage_error = invalid
        .initialize(&assets)
        .err()
        .ok_or("file accepted as user root")?;
    assert!(
        storage_error.contains("not-a-directory") && storage_error.contains("PLACES_STATE_ROOT"),
        "actionable error: {storage_error}"
    );
    assert_eq!(
        fs::read(obstruction).map_err(|error| error.to_string())?,
        b"keep me"
    );
    assert!(
        !invalid.data.exists(),
        "preflight every directory before creating the first one"
    );
    Ok(())
}

#[cfg(unix)]
fn check_storage_symlink_preflight(
    fixture: &Fixture,
    user: &UserPaths,
    assets: &std::path::Path,
) -> Result<(), String> {
    let redirected = UserPaths {
        data: fixture.0.join("redirected"),
        ..user.clone()
    };
    fs::create_dir(&redirected.data).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(assets, redirected.levels()).map_err(|error| error.to_string())?;
    assert!(
        redirected.initialize(assets).is_err(),
        "derived symlink cannot redirect writes into assets"
    );
    assert!(
        !redirected.imports().exists(),
        "all targets checked before any first-run writes"
    );
    let dangling = UserPaths {
        data: fixture.0.join("dangling-must-stay-absent"),
        config: fixture.0.join("dangling-config"),
        ..user.clone()
    };
    std::os::unix::fs::symlink(fixture.0.join("missing-target"), &dangling.config)
        .map_err(|error| error.to_string())?;
    assert!(
        dangling.initialize(assets).is_err(),
        "dangling storage symlinks must fail preflight"
    );
    assert!(
        !dangling.data.exists(),
        "dangling config cannot leave a partially initialized data root"
    );
    Ok(())
}
