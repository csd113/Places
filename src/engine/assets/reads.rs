//! Per-operation verification for encoded asset consumers used by the compiler.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::engine::package::{DependencyKind, PackageDependency, dependencies, hash::sha256_hex};

#[derive(Debug)]
pub struct AssetReads {
    root: PathBuf,
    expected: Mutex<BTreeMap<PathBuf, Option<String>>>,
    failure: Mutex<Option<String>>,
    validation_models: Mutex<BTreeMap<String, PackageDependency>>,
}

impl AssetReads {
    pub fn new(root: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            root,
            expected: Mutex::default(),
            failure: Mutex::default(),
            validation_models: Mutex::default(),
        })
    }

    pub fn remember(&self, relative: &Path, sha256: Option<String>) -> Result<(), String> {
        let mut expected = self.expected.lock().map_err(|error| error.to_string())?;
        if let Some(previous) = expected.get(relative) {
            if *previous != sha256 {
                return Err(format!(
                    "Conflicting asset identities for {}",
                    relative.display()
                ));
            }
        } else {
            drop(expected.insert(relative.into(), sha256));
        }
        drop(expected);
        Ok(())
    }

    /// Rig validation runs while editable sources are read, before the
    /// compiler can enumerate the completed level's normal dependencies.
    pub(crate) fn capture_validation_model(&self, path: &str) -> Result<(), String> {
        let mut models = self
            .validation_models
            .lock()
            .map_err(|error| error.to_string())?;
        if models.contains_key(path) {
            return Ok(());
        }
        let asset_path = self.root.join(path);
        let kind = DependencyKind::Model;
        let (bytes, sha256) = dependencies::identity(&self.root, path, kind)
            .map_err(|error| self.reject(&asset_path, &error).to_string())?;
        self.remember(Path::new(path), Some(sha256.clone()))
            .map_err(|error| self.reject(&asset_path, &error).to_string())?;
        drop(models.insert(
            path.into(),
            PackageDependency {
                kind,
                path: path.into(),
                bytes,
                sha256,
            },
        ));
        drop(models);
        Ok(())
    }

    /// Project source snapshots may validate rigs outside the selected member.
    pub(crate) fn check_validation_models(&self) -> Result<(), String> {
        let models = self
            .validation_models
            .lock()
            .map_err(|error| error.to_string())?;
        for model in models.values() {
            dependencies::validate(&self.root, model).map_err(|error| {
                self.reject(&self.root.join(&model.path), &error)
                    .to_string()
            })?;
        }
        drop(models);
        Ok(())
    }

    pub fn check(&self) -> Result<(), String> {
        self.failure
            .lock()
            .map_err(|error| error.to_string())?
            .clone()
            .map_or(Ok(()), Err)
    }

    pub(super) fn read(&self, path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Err(self.reject(path, "read outside the captured asset root"));
        };
        let expected = self
            .expected
            .lock()
            .map_err(|error| io::Error::other(error.to_string()))?
            .get(relative)
            .cloned()
            .ok_or_else(|| self.reject(path, "read outside the captured asset set"))?;
        match (expected, self.read_confined(path, max_bytes)) {
            (Some(sha), Ok(bytes)) if sha == sha256_hex(&bytes) => Ok(bytes),
            (None, Err(error)) => Err(error),
            _ => Err(self.reject(path, "content or availability changed during compilation")),
        }
    }

    fn read_confined(&self, path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
        let resolved = std::fs::canonicalize(path)?;
        if !resolved.starts_with(&self.root) {
            return Err(self.reject(path, "path escapes the captured asset root"));
        }
        super::read_live_asset_bytes(&resolved, max_bytes)
    }

    fn reject(&self, path: &Path, reason: &str) -> io::Error {
        let message = format!(
            "Asset {}: {reason}; previous output retained",
            path.display()
        );
        if let Ok(mut failure) = self.failure.lock() {
            // Some installed asset readers are deliberately allowed to fall
            // back. A read drift must still reject publication on that path.
            let _first = failure.get_or_insert_with(|| message.clone());
        }
        io::Error::other(message)
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<AssetReads>>> = const { RefCell::new(None) };
}

pub struct AssetReadScope(Option<Arc<AssetReads>>);

impl Drop for AssetReadScope {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            drop(current.replace(self.0.take()));
        });
    }
}

pub fn current() -> Option<Arc<AssetReads>> {
    CURRENT.with(|current| current.borrow().clone())
}

pub fn enter(reads: Option<Arc<AssetReads>>) -> AssetReadScope {
    AssetReadScope(CURRENT.with(|current| current.replace(reads)))
}
