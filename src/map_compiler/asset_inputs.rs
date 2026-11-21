//! Asset identities shared across all members and cache paths of one export.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::engine::assets::{
    CATALOG_FILE_NAME, GAMEPLAY_FILE_NAME, MAX_CATALOG_JSON_BYTES, MAX_GAMEPLAY_JSON_BYTES,
    read_live_asset_bytes, reads,
};
use crate::engine::loader::PropCatalog;
use crate::engine::package::{DependencyKind, PackageDependency, dependencies, hash::sha256_hex};

type Dependencies = BTreeMap<(&'static str, String), ObservedDependency>;

struct ObservedDependency {
    kind: DependencyKind,
    identity: Result<PackageDependency, String>,
}

pub(super) struct AssetInputs {
    root: PathBuf,
    catalog: PropCatalog,
    bytes: Vec<u8>,
    catalog_hash: String,
    gameplay_hash: Option<String>,
    dependencies: RefCell<Dependencies>,
    reads: Arc<reads::AssetReads>,
}

impl AssetInputs {
    pub(super) fn read(requested_root: &Path) -> Result<Self, String> {
        let root = std::fs::canonicalize(requested_root).map_err(|error| {
            format!(
                "could not resolve asset root {}: {error}",
                requested_root.display()
            )
        })?;
        let path = root.join(CATALOG_FILE_NAME);
        let bytes = read_confined(&root, &path, MAX_CATALOG_JSON_BYTES)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| format!("catalog {} is not valid UTF-8: {error}", path.display()))?;
        let catalog = PropCatalog::from_json_str(text)
            .map_err(|error| format!("catalog {} is invalid: {error}", path.display()))?;
        let catalog_hash = sha256_hex(&bytes);
        // Renderer bootstrap may load this optional runtime-only sidecar.
        let gameplay_hash = read_confined(
            &root,
            &root.join(GAMEPLAY_FILE_NAME),
            MAX_GAMEPLAY_JSON_BYTES,
        )
        .ok()
        .map(|gameplay_bytes| sha256_hex(&gameplay_bytes));
        let reads = reads::AssetReads::new(root.clone());
        reads.remember(Path::new(CATALOG_FILE_NAME), Some(catalog_hash.clone()))?;
        reads.remember(Path::new(GAMEPLAY_FILE_NAME), gameplay_hash.clone())?;
        Ok(Self {
            root,
            catalog,
            bytes,
            catalog_hash,
            gameplay_hash,
            dependencies: RefCell::default(),
            reads,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) const fn catalog(&self) -> &PropCatalog {
        &self.catalog
    }

    pub(super) fn catalog_hash(&self) -> &str {
        &self.catalog_hash
    }

    pub(super) fn catalog_inputs(
        &self,
        level: &crate::engine::level::LevelDef,
    ) -> Result<crate::engine::package::catalog_inputs::CatalogInputs, String> {
        crate::engine::package::catalog_inputs::CatalogInputs::for_validated_catalog(
            level,
            &self.bytes,
            self.catalog.assets(),
        )
    }

    pub(super) fn scope(&self) -> reads::AssetReadScope {
        reads::enter(Some(Arc::clone(&self.reads)))
    }

    pub(super) fn dependency(
        &self,
        kind: DependencyKind,
        path: &str,
        warnings: &mut Vec<String>,
    ) -> Result<Option<PackageDependency>, String> {
        let key = (super::dependency_kind_name(kind), path.to_owned());
        let mut dependencies = self.dependencies.borrow_mut();
        let observed = dependencies.entry(key).or_insert_with(|| {
            let observed = dependencies::identity(&self.root, path, kind).map(|(bytes, sha256)| {
                PackageDependency {
                    kind,
                    path: path.into(),
                    bytes,
                    sha256,
                }
            });
            ObservedDependency {
                kind,
                identity: observed,
            }
        });
        match &observed.identity {
            Ok(dependency) => {
                self.reads
                    .remember(Path::new(path), Some(dependency.sha256.clone()))?;
                Ok(Some(dependency.clone()))
            }
            Err(error) => {
                self.reads.remember(Path::new(path), None)?;
                warnings.push(format!(
                    "dependency '{path}' could not be read ({error}); not recorded"
                ));
                Ok(None)
            }
        }
    }

    pub(super) fn check_reads(&self) -> Result<(), String> {
        self.reads.check()
    }

    pub(super) fn check(&self) -> Result<(), String> {
        self.check_reads()?;
        super::progress::check_cancelled()?;
        let catalog = read_confined(
            &self.root,
            &self.root.join(CATALOG_FILE_NAME),
            MAX_CATALOG_JSON_BYTES,
        )?;
        if catalog != self.bytes {
            return Err(changed(CATALOG_FILE_NAME));
        }
        let gameplay_hash = read_confined(
            &self.root,
            &self.root.join(GAMEPLAY_FILE_NAME),
            MAX_GAMEPLAY_JSON_BYTES,
        )
        .ok()
        .map(|gameplay_bytes| sha256_hex(&gameplay_bytes));
        if gameplay_hash != self.gameplay_hash {
            return Err(changed(GAMEPLAY_FILE_NAME));
        }
        for ((_, path), observed) in self.dependencies.borrow().iter() {
            super::progress::check_cancelled()?;
            let current = dependencies::identity(&self.root, path, observed.kind);
            match (&observed.identity, current) {
                (Ok(expected), Ok((bytes, sha256)))
                    if bytes == expected.bytes && sha256 == expected.sha256 => {}
                (Err(_), Err(_)) => {}
                _ => return Err(changed(path)),
            }
        }
        super::progress::check_cancelled()
    }
}

fn read_confined(root: &Path, path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let error = |error: std::io::Error| format!("could not read {}: {error}", path.display());
    let resolved = std::fs::canonicalize(path).map_err(error)?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "asset {} escapes the installed asset root",
            path.display()
        ));
    }
    read_live_asset_bytes(&resolved, limit).map_err(error)
}

fn changed(path: &str) -> String {
    format!("Asset '{path}' changed during compilation; previous output retained")
}
