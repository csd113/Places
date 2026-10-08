//! `places-compile`: the offline Places map compiler.
//!
//! Explicit workflow only: edit source -> compile changed content -> launch.
//! Building never happens inside `cargo run`, `build.rs`, `cargo check` or the
//! player. See `docs/MAP_AUTHORING_GUIDE.md` for the workflow and
//! `docs/PACKAGE_FORMAT.md` for the record contract.
//!
//! Exit codes: `0` success, `1` operation failure, `2` usage error.

// A command-line tool's contract is its stdout/stderr output.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the compiler CLI reports results and errors through standard output streams"
)]

use std::path::PathBuf;
use std::process::ExitCode;

use places::compiler::{self, BuildRequest};
use places::quality::LightmapQuality;

const USAGE: &str = "\
places-compile: build and inspect Places map packages (.placesmap)

USAGE:
  places-compile build <source.json> [--out <package>] [--variants off,medium,full]
                       [--asset-root <dir>] [--workers N] [--force] [--json]
  places-compile build-collection <dir> [--variants off,medium,full]
                       [--asset-root <dir>] [--workers N] [--force] [--json]
  places-compile validate <package> [--json]
  places-compile inspect <package> [--json]
  places-compile export-lighting <package> --out <new-directory>
                       [--variants off,medium,full] [--json]
  places-compile verify <source.json> --package <package> [--asset-root <dir>] [--json]
                       [--require-current]
  places-compile --help | --version

OPTIONS:
  --asset-root <dir>  Asset root holding catalog.json (default: PLACES_ASSET_ROOT
                      or the discovered assets directory).
  --require-current   verify only: exit 1 when the package is not current for the
                      source (the gate that keeps shipped packages from going
                      stale unnoticed).
  --workers N         Shared CPU budget for this run; 1 forces the serial path.
                      Capped at 12; defaults to available CPU parallelism.
                      Also read from PLACES_TOOL_WORKERS; the flag wins.
  --variants <list>   Comma-separated lightmap qualities to prepare
                      (default: off,medium,full).
  --out <path>        Build output package, or new export-lighting directory.
                      Build default: the source's sibling .placesmap.
  --force             Rebuild even when the existing package is current.
  --json              Machine-readable result on stdout.
";

fn main() -> ExitCode {
    let parsed_arguments: Result<Vec<String>, CliError> = std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument.into_string().map_err(|invalid_argument| {
                CliError::Usage(format!(
                    "a command-line argument is not valid UTF-8 ({} encoded bytes)",
                    invalid_argument.len()
                ))
            })
        })
        .collect();
    match parsed_arguments.and_then(|arguments| run(&arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("places-compile: {message}");
            eprintln!();
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failure(message)) => {
            eprintln!("places-compile: {message}");
            ExitCode::FAILURE
        }
    }
}

enum CliError {
    Usage(String),
    Failure(String),
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Failure(message)
    }
}

struct Options {
    asset_root: Option<PathBuf>,
    workers: usize,
    variants: Vec<LightmapQuality>,
    json: bool,
    require_current: bool,
}

fn parse_options(args: &[String]) -> Result<Options, CliError> {
    let mut options = Options {
        asset_root: None,
        workers: 0,
        variants: LightmapQuality::ALL.to_vec(),
        json: false,
        require_current: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--asset-root") => {
                let Some(next) = index.checked_add(1) else {
                    return Err(CliError::Usage(
                        "--asset-root needs a directory".to_string(),
                    ));
                };
                let value = args
                    .get(next)
                    .ok_or_else(|| CliError::Usage("--asset-root needs a directory".to_string()))?;
                options.asset_root = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            Some("--workers") => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| CliError::Usage("--workers needs a count".to_string()))?;
                options.workers = value.parse::<usize>().map_err(|error| {
                    CliError::Usage(format!("invalid --workers value '{value}': {error}"))
                })?;
                if options.workers == 0 {
                    return Err(CliError::Usage("--workers must be at least 1".to_string()));
                }
                index = index.saturating_add(2);
            }
            Some("--variants") => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| CliError::Usage("--variants needs a list".to_string()))?;
                options.variants = parse_variants(value)?;
                index = index.saturating_add(2);
            }
            Some("--json") => {
                options.json = true;
                index = index.saturating_add(1);
            }
            Some("--require-current") => {
                options.require_current = true;
                index = index.saturating_add(1);
            }
            Some("--force") => {
                index = index.saturating_add(1);
            }
            Some("--out" | "--package") => {
                if args.get(index.saturating_add(1)).is_none() {
                    return Err(CliError::Usage(format!(
                        "{} needs a value",
                        args.get(index).map_or("option", String::as_str)
                    )));
                }
                index = index.saturating_add(2);
            }
            Some(other) => return Err(CliError::Usage(format!("unexpected argument '{other}'"))),
            None => break,
        }
    }
    if options.workers == 0 {
        options.workers = std::env::var("PLACES_TOOL_WORKERS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
            });
    }
    options.workers = options.workers.min(compiler::MAX_WORKERS);
    Ok(options)
}

fn parse_variants(value: &str) -> Result<Vec<LightmapQuality>, CliError> {
    let mut variants = Vec::new();
    for raw_name in value.split(',') {
        let name = raw_name.trim();
        let quality = match name {
            "off" => LightmapQuality::Off,
            "medium" => LightmapQuality::Medium,
            "full" => LightmapQuality::Full,
            other => {
                return Err(CliError::Usage(format!(
                    "unknown lightmap variant '{other}' (off, medium, full)"
                )));
            }
        };
        if !variants.contains(&quality) {
            variants.push(quality);
        }
    }
    if variants.is_empty() {
        return Err(CliError::Usage("--variants is empty".to_string()));
    }
    Ok(variants)
}

#[expect(clippy::too_many_lines, reason = "one cohesive command dispatcher")] // one cohesive command dispatcher
fn run(args: &[String]) -> Result<(), CliError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage("no command given".to_string()));
    };
    match command {
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            return Ok(());
        }
        "--version" | "-V" => {
            println!("{}", compiler::COMPILER_NAME);
            return Ok(());
        }
        _ => {}
    }
    let rest = args.get(1..).unwrap_or_default();
    // Split positional operands from options in any order.
    let mut positional: Vec<&String> = Vec::new();
    let mut option_args: Vec<String> = Vec::new();
    let known_with_value = [
        "--asset-root",
        "--workers",
        "--variants",
        "--out",
        "--package",
    ];
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        if arg == "--json" || arg == "--force" || arg == "--require-current" {
            option_args.push(arg.clone());
        } else if known_with_value.contains(&arg.as_str()) {
            option_args.push(arg.clone());
            let value = it
                .next()
                .ok_or_else(|| CliError::Usage(format!("{arg} needs a value")))?;
            option_args.push(value.clone());
        } else if arg.starts_with("--") {
            return Err(CliError::Usage(format!("unexpected argument '{arg}'")));
        } else {
            positional.push(arg);
        }
    }
    let options = parse_options(&option_args)?;
    match command {
        "build" => {
            let source = positional
                .first()
                .ok_or_else(|| CliError::Usage("build needs a source path".to_string()))?;
            let out = option_value(&option_args, "--out").map_or_else(
                || compiler::package_path_for(std::path::Path::new(source.as_str())),
                PathBuf::from,
            );
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            pin_asset_root(&asset_root);
            let request = BuildRequest {
                source: PathBuf::from(source.as_str()),
                out,
                asset_root,
                variants: options.variants,
                workers: options.workers,
                force: option_args.iter().any(|arg| arg == "--force"),
                capture_probes: true,
            };
            let report = compiler::build(&request)?;
            if options.json {
                print_json(&report)?;
            } else {
                println!(
                    "{} {} -> {} ({} bytes, {:.0} ms)",
                    if report.rebuilt { "built" } else { "current" },
                    report.source,
                    report.out,
                    report.bytes,
                    report.millis
                );
                for decision in &report.cache_decisions {
                    println!(
                        "  cache {} {}: {}",
                        decision.stage,
                        decision.status,
                        decision.reasons.join("; ")
                    );
                }
                for variant in &report.variant_stats {
                    println!(
                        "  {}: {} range(s), {} vertices, {} prop batch(es), {} lightmap page(s)",
                        variant.lightmap_quality,
                        variant.mesh_ranges,
                        variant.mesh_vertices,
                        variant.prop_batches,
                        variant.lightmap_pages
                    );
                }
                for warning in &report.warnings {
                    println!("  note: {warning}");
                }
            }
            Ok(())
        }
        "build-collection" => {
            let directory = positional
                .first()
                .ok_or_else(|| CliError::Usage("build-collection needs a directory".to_string()))?;
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            pin_asset_root(&asset_root);
            let results = compiler::build_collection(
                &PathBuf::from(directory.as_str()),
                &asset_root,
                &options.variants,
                options.workers,
                option_args.iter().any(|arg| arg == "--force"),
            )?;
            let mut failures = 0_usize;
            for result in &results {
                match result {
                    Ok(report) => {
                        if options.json {
                            print_json(report)?;
                        } else {
                            println!(
                                "{} {} -> {}",
                                if report.rebuilt { "built" } else { "current" },
                                report.source,
                                report.out
                            );
                            for decision in &report.cache_decisions {
                                println!(
                                    "  cache {} {}: {}",
                                    decision.stage,
                                    decision.status,
                                    decision.reasons.join("; ")
                                );
                            }
                        }
                    }
                    Err((source, error)) => {
                        failures = failures.saturating_add(1);
                        eprintln!("failed {source}: {error}");
                    }
                }
            }
            if failures > 0 {
                return Err(CliError::Failure(format!(
                    "{failures} source(s) failed to build"
                )));
            }
            Ok(())
        }
        "validate" => {
            let package = positional
                .first()
                .ok_or_else(|| CliError::Usage("validate needs a package path".to_string()))?;
            let report = compiler::validate(&PathBuf::from(package.as_str()))?;
            if options.json {
                print_json(&report)?;
            } else {
                println!(
                    "valid {} ({}), variants: {}, {} entries, {} dependencies{}",
                    report.id,
                    report.name,
                    report.variants.join(","),
                    report.entries,
                    report.dependencies,
                    if report.dependencies_intact {
                        ""
                    } else {
                        " (some dependencies changed)"
                    }
                );
            }
            Ok(())
        }
        "inspect" => {
            let package = positional
                .first()
                .ok_or_else(|| CliError::Usage("inspect needs a package path".to_string()))?;
            let manifest = compiler::inspect(&PathBuf::from(package.as_str()))?;
            if options.json {
                print_json(&manifest)?;
            } else {
                println!("{} ({}) by {}", manifest.name, manifest.id, manifest.author);
                println!(
                    "format {}, created by {}",
                    manifest.package_format, manifest.created_by
                );
                println!("fingerprint {}", manifest.compiler_fingerprint);
                println!("capabilities {}", manifest.required_capabilities.join(","));
                for variant in &manifest.variants {
                    println!("variant {}", variant.lightmap_quality);
                }
                for entry in &manifest.entries {
                    println!("  {} {} ({} bytes)", entry.role, entry.name, entry.bytes);
                }
                for dependency in &manifest.dependencies {
                    println!(
                        "dependency {:?} {} ({} bytes)",
                        dependency.kind, dependency.path, dependency.bytes
                    );
                }
            }
            Ok(())
        }
        "export-lighting" => {
            if positional.len() != 1 {
                return Err(CliError::Usage(
                    "export-lighting needs exactly one package path".to_string(),
                ));
            }
            let package = positional.first().ok_or_else(|| {
                CliError::Usage("export-lighting needs a package path".to_string())
            })?;
            let output = option_value(&option_args, "--out").ok_or_else(|| {
                CliError::Usage("export-lighting needs --out <new-directory>".to_string())
            })?;
            let report = compiler::diagnostics::export_lighting(
                &PathBuf::from(package.as_str()),
                &PathBuf::from(output),
                &options.variants,
            )?;
            if options.json {
                print_json(&report)?;
            } else {
                println!("exported lighting {} to {}", report.package, report.out);
                for variant in &report.variants {
                    println!(
                        "  {}: {} charts, {} probes",
                        variant.quality, variant.charts, variant.probes
                    );
                }
            }
            Ok(())
        }
        "verify" => {
            let source = positional
                .first()
                .ok_or_else(|| CliError::Usage("verify needs a source path".to_string()))?;
            let package = option_value(&option_args, "--package")
                .ok_or_else(|| CliError::Usage("verify needs --package <package>".to_string()))?;
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            pin_asset_root(&asset_root);
            let report = compiler::verify(
                &PathBuf::from(source.as_str()),
                &PathBuf::from(&package),
                &asset_root,
            )?;
            if options.json {
                print_json(&report)?;
            } else if report.current {
                println!("current {}", report.package);
            } else {
                println!("stale {}", report.package);
                for difference in &report.differences {
                    println!("  {difference}");
                }
            }
            if options.require_current && !report.current {
                return Err(CliError::Failure(format!(
                    "{} is stale for {}: {}",
                    report.package,
                    report.source,
                    report.differences.join("; ")
                )));
            }
            Ok(())
        }
        other => Err(CliError::Usage(format!("unknown command '{other}'"))),
    }
}

/// Pins the process-wide asset root so every later resolver in this process
/// (fixture sheets, the headless renderer's own catalog load) agrees with the
/// explicitly selected `--asset-root`.
///
/// # Safety
///
/// Called once at the start of a single-threaded command-line process, before
/// any engine code has spawned a thread or read the environment.
fn pin_asset_root(root: &std::path::Path) {
    // SAFETY: command dispatch calls this before starting workers or engine
    // code, while the compiler process still has only its main thread.
    unsafe {
        std::env::set_var("PLACES_ASSET_ROOT", root);
    }
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.get(index.saturating_add(1)).cloned()
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Failure(format!("could not serialize result: {error}")))?;
    println!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_worker_budget_is_positive_and_bounded() {
        assert!(matches!(
            parse_options(&["--workers".into(), "0".into()]),
            Err(CliError::Usage(_))
        ));
        for (requested, expected) in [
            (1_usize, 1),
            (2_usize, 2),
            (4_usize, 4),
            (8_usize, 8),
            (12_usize, 12),
            (64_usize, 12),
        ] {
            assert!(
                parse_options(&["--workers".into(), requested.to_string()])
                    .is_ok_and(|options| options.workers == expected)
            );
        }
    }
}
