//! Opt-in controller replay; ordinary Cargo tests never bake a map.
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(
        args.next()
            .ok_or("Usage: places-movement-check <gym-level.json> <new-output-directory>")?,
    );
    let out = PathBuf::from(args.next().ok_or("Missing output directory")?);
    if args.next().is_some() || out.exists() {
        return Err("Use exactly a source path and a new output directory".into());
    }
    places::game::movement_probe::run(&source, &out)
}
