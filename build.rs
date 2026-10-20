//! Preparation identity follows source inputs, independent of binary packaging,
//! game/editor UI code, checkout location, debug symbols or build profile.
use sha2::{Digest as _, Sha256};
use std::{fs, io, path::Path};

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let child = entry?.path();
            if child.file_name().is_some_and(|name| name == "tests") {
                continue;
            }
            collect(&child, paths)?;
        }
    } else {
        paths.push(path.into());
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let mut paths = Vec::new();
    for directory in ["src/engine", "src/map_compiler"] {
        println!("cargo:rerun-if-changed={directory}");
        collect(Path::new(directory), &mut paths)?;
    }
    paths.retain(|path| {
        !matches!(
            path.to_str(),
            Some(
                "src/map_compiler/application.rs"
                    | "src/map_compiler/gui.rs"
                    | "src/map_compiler/events.rs"
                    | "src/map_compiler/progress.rs"
            )
        )
    });
    paths.extend(
        [
            "build.rs",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "assets/completed-models.json",
            "assets/shared/decals/validation_atlas_01.png",
            "assets/shared/textures/missing_01.png",
            "assets/shared/textures/white_fallback_01.png",
            "assets/shared/textures/white_01.png",
        ]
        .map(Into::into),
    );
    paths.sort();
    let mut hash = Sha256::new();
    for variable in [
        "TARGET",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_FEATURE_VISUAL_DIAGNOSTICS",
        "CARGO_FEATURE_PARALLEL_PHYSICS",
    ] {
        hash.update(variable.as_bytes());
        hash.update(std::env::var(variable).unwrap_or_default().as_bytes());
        hash.update([0]);
    }
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(fs::read(path)?);
        hash.update([0]);
    }
    let mut identity = String::with_capacity(64);
    for byte in hash.finalize() {
        use std::fmt::Write as _;
        write!(identity, "{byte:02x}").map_err(io::Error::other)?;
    }
    println!("cargo:rustc-env=PLACES_COMPILER_CODE_SHA256={identity}");
    Ok(())
}
