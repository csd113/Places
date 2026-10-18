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

use crate::engine::quality::LightmapQuality;
use crate::map_compiler::{self as compiler, BuildRequest};
use compiler::progress::{Mode, Session};

use super::gui;

const USAGE: &str = "\
places-compile: build and inspect Places map packages (.placesmap)

USAGE:
  places-compile gui [source.json] [--out <package>] [--asset-root <dir>]
                       [--workers N]
  places-compile build <source.json|project> [--level <id>] [--out <package>] [--variants static,full]
                       [--asset-root <dir>] [--workers N] [--force] [--json]
                       [--progress auto|plain|off] [--events]
  places-compile build-collection <dir> [--variants static,full]
                       [--asset-root <dir>] [--workers N] [--force] [--json]
                       [--progress auto|plain|off]
  places-compile build-project <project> --out <existing-directory>
                       [--asset-root <dir>] [--variants static,full] [--workers N]
                       [--force] [--json] [--progress auto|plain|off]
  places-compile export-project <project> --out <project.placesmap> [build options]
  places-compile import-project <project.placesmap> --out <NEW-project-folder> [--events]
  places-compile check-project <project> [--events] [--asset-root <dir>]
  places-compile migrate-project <project> [--asset-root <dir>]
  places-compile validate <package> [--asset-root <dir>] [--json]
  places-compile inspect <package> [--json]
  places-compile export-lighting <package> --out <new-directory>
                       [--variants static,full] [--json]
  places-compile verify <source.json|project> [--level <id>] --package <package> [--asset-root <dir>] [--json]
                       [--require-current]
  places-compile --help | --version

OPTIONS:
  --level <id>        Select a declared project level (default: entry_level).
  --asset-root <dir>  Asset root holding catalog.json (default: PLACES_ASSET_ROOT
                      or the discovered assets directory).
  --require-current   verify only: exit 1 when the package is not current for the
                      source (the gate that keeps shipped packages from going
                      stale unnoticed).
  --workers N         Shared CPU budget for this run; 1 forces the serial path.
                      Capped at 12; defaults to available CPU parallelism.
                      Also read from PLACES_TOOL_WORKERS; the flag wins.
  --variants <list>   Comma-separated lightmap qualities to prepare
                      (default: static,full).
  --out <path>        Build output package, or new export-lighting directory.
                      Build default: the source's sibling .placesmap.
  --force             Rebuild even when the existing package is current.
  --json              Machine-readable result on stdout.
  --events            build/export/import/check: versioned NDJSON progress/result on stdout.
                      Stdin accepts {\"command\":\"cancel\"}; EOF cancels safely.
  --progress <mode>   Build feedback on stderr (default: auto). Auto redraws on
                      a capable terminal; redirected/CI output is plain text.
                      Plain always logs stages; off suppresses progress.
";

/// Runs Map Compiler's CLI or native GUI using this process's arguments.
///
/// Call at process startup before creating worker threads, so asset-root
/// selection is pinned before shared engine asset discovery begins.
#[must_use]
pub fn run() -> ExitCode {
    crate::engine::logging::diagnostics_to_stderr();
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
    let machine = parsed_arguments
        .as_ref()
        .is_ok_and(|arguments| arguments.iter().any(|argument| argument == "--events"));
    match parsed_arguments.and_then(|arguments| dispatch(&arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            emit_failure(machine, compiler::events::Outcome::Failed, &message);
            eprintln!("places-compile: {message}");
            eprintln!();
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failure(message)) => {
            emit_failure(machine, compiler::events::Outcome::Failed, &message);
            eprintln!("places-compile: {message}");
            ExitCode::FAILURE
        }
        Err(CliError::Interrupted) => {
            emit_failure(
                machine,
                compiler::events::Outcome::Cancelled,
                "build cancelled",
            );
            eprintln!("places-compile: build interrupted");
            ExitCode::from(130)
        }
    }
}

fn emit_failure(machine: bool, outcome: compiler::events::Outcome, message: &str) {
    if machine {
        let _emitted = compiler::events::emit(&compiler::events::Event::Finished {
            outcome,
            report: None,
            message: Some(message.into()),
        });
    }
}

enum CliError {
    Usage(String),
    Failure(String),
    Interrupted,
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Failure(message)
    }
}

struct Options {
    level: Option<String>,
    asset_root: Option<PathBuf>,
    workers: usize,
    variants: Vec<LightmapQuality>,
    json: bool,
    events: bool,
    require_current: bool,
    progress: Mode,
}

fn parse_options(args: &[String]) -> Result<Options, CliError> {
    let mut options = Options {
        level: None,
        asset_root: None,
        workers: 0,
        variants: LightmapQuality::ALL.to_vec(),
        json: false,
        events: false,
        require_current: false,
        progress: Mode::Auto,
    };
    let mut index = 0;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--level") => {
                options.level = Some(
                    args.get(index.saturating_add(1))
                        .ok_or_else(|| CliError::Usage("--level needs an ID".into()))?
                        .clone(),
                );
                index = index.saturating_add(2);
            }
            Some("--progress") => {
                options.progress = progress_mode(args.get(index.saturating_add(1)))?;
                index = index.saturating_add(2);
            }
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
            Some("--events") => {
                options.events = true;
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

fn progress_mode(value: Option<&String>) -> Result<Mode, CliError> {
    match value.map(String::as_str) {
        Some("auto") => Ok(Mode::Auto),
        Some("plain") => Ok(Mode::Plain),
        Some("off") => Ok(Mode::Off),
        _ => Err(CliError::Usage(
            "--progress needs auto, plain or off".into(),
        )),
    }
}

fn parse_variants(value: &str) -> Result<Vec<LightmapQuality>, CliError> {
    let mut variants = Vec::new();
    for raw_name in value.split(',') {
        let name = raw_name.trim();
        let quality = match name {
            "off" | "static" => LightmapQuality::Off,
            "full" => LightmapQuality::Full,
            other => {
                return Err(CliError::Usage(format!(
                    "unknown lightmap variant '{other}' (static, full)"
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
fn dispatch(args: &[String]) -> Result<(), CliError> {
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
        "--progress",
        "--level",
    ];
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        if matches!(
            arg.as_str(),
            "--json" | "--force" | "--require-current" | "--events"
        ) {
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
    if options.events
        && (!matches!(
            command,
            "build" | "export-project" | "import-project" | "check-project"
        ) || options.json)
    {
        return Err(CliError::Usage(
            "--events supports build/export/import/check and cannot be combined with --json".into(),
        ));
    }
    if options.level.is_some() && !matches!(command, "build" | "verify") {
        return Err(CliError::Usage(
            "--level is only supported by build and verify".into(),
        ));
    }
    match command {
        "gui" => {
            if positional.len() > 1
                || option_args.iter().any(|arg| {
                    matches!(
                        arg.as_str(),
                        "--json"
                            | "--progress"
                            | "--force"
                            | "--variants"
                            | "--package"
                            | "--require-current"
                    )
                })
            {
                return Err(CliError::Usage(
                    "gui accepts one source and only --out, --asset-root and --workers".into(),
                ));
            }
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            crate::engine::paths::configure_assets(&asset_root)?;
            gui::run(
                positional
                    .first()
                    .map(|source| PathBuf::from(source.as_str())),
                option_value(&option_args, "--out").map(PathBuf::from),
                asset_root,
                options.workers,
            )?;
            Ok(())
        }
        "build" | "export-project" | "import-project" => {
            if positional.len() != 1
                || (command != "build" && option_value(&option_args, "--out").is_none())
            {
                return Err(CliError::Usage(format!(
                    "{command} needs one source and export/import requires explicit --out"
                )));
            }
            let source = positional
                .first()
                .ok_or_else(|| CliError::Usage("build needs a source path".to_string()))?;
            let out = if let Some(output) = option_value(&option_args, "--out") {
                PathBuf::from(output)
            } else {
                compiler::project::output_path(
                    std::path::Path::new(source.as_str()),
                    options.level.as_deref(),
                )?
            };
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            crate::engine::paths::configure_assets(&asset_root)?;
            let request = BuildRequest {
                source: PathBuf::from(source.as_str()),
                project_level: options.level,
                out,
                asset_root,
                variants: options.variants,
                workers: options.workers,
                force: option_args.iter().any(|arg| arg == "--force"),
                capture_probes: true,
            };
            if options.events {
                let report =
                    compiler::events::build(&request, command).map_err(
                        |failure| match failure {
                            compiler::events::Failure::Failed(error) => CliError::Failure(error),
                            compiler::events::Failure::Cancelled => CliError::Interrupted,
                        },
                    )?;
                compiler::events::emit(&compiler::events::Event::Finished {
                    outcome: compiler::events::Outcome::Success,
                    report: Some(Box::new(report)),
                    message: None,
                })?;
                return Ok(());
            }
            let session = Session::start(options.progress)?;
            let result = match command {
                "export-project" => compiler::distribution::export(&request),
                "import-project" => compiler::distribution::recover(&request),
                _ => compiler::build(&request),
            };
            if session.interrupted() {
                return Err(CliError::Interrupted);
            }
            let report = result?;
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
        "check-project" | "migrate-project" => {
            let source = positional
                .first()
                .ok_or_else(|| CliError::Usage("check-project needs a project".into()))?;
            let folder = crate::engine::project::ProjectFolder::open(std::path::Path::new(source))?;
            if options.events {
                let request = BuildRequest {
                    source: folder.root().into(),
                    project_level: None,
                    out: folder.root().into(),
                    asset_root: compiler::resolve_asset_root(options.asset_root.as_deref())?,
                    variants: Vec::new(),
                    workers: 0,
                    force: false,
                    capture_probes: false,
                };
                let report =
                    compiler::events::build(&request, "check-project").map_err(|failure| {
                        match failure {
                            compiler::events::Failure::Failed(error) => CliError::Failure(error),
                            compiler::events::Failure::Cancelled => CliError::Interrupted,
                        }
                    })?;
                compiler::events::emit(&compiler::events::Event::Finished {
                    outcome: compiler::events::Outcome::Success,
                    report: Some(Box::new(report)),
                    message: None,
                })?;
                return Ok(());
            }
            let session = Session::start(options.progress)?;
            drop(crate::engine::package::project::snapshot(
                &folder,
                &compiler::progress::check_cancelled,
            )?);
            if session.interrupted() {
                return Err(CliError::Interrupted);
            }
            println!(
                "Supported project/level schema version 1; validated all sources. No conversion needed; no files changed."
            );
            Ok(())
        }
        "build-project" => {
            if positional.len() != 1 {
                return Err(CliError::Usage(
                    "build-project needs one project folder or manifest".into(),
                ));
            }
            let source = positional
                .first()
                .ok_or_else(|| CliError::Usage("build-project needs a project".into()))?;
            let out = option_value(&option_args, "--out").ok_or_else(|| {
                CliError::Usage("build-project needs --out <existing-directory>".into())
            })?;
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            crate::engine::paths::configure_assets(&asset_root)?;
            let template = BuildRequest {
                source: PathBuf::from(source.as_str()),
                project_level: None,
                out: PathBuf::from(out),
                asset_root,
                variants: options.variants,
                workers: options.workers,
                force: option_args.iter().any(|arg| arg == "--force"),
                capture_probes: true,
            };
            let plan = compiler::project::build_plan(&template)?;
            let session = Session::start(options.progress)?;
            for request in plan {
                let result = compiler::build(&request);
                if session.interrupted() {
                    return Err(CliError::Interrupted);
                }
                let report = result?;
                if options.json {
                    print_json(&report)?;
                } else {
                    println!(
                        "{} {}",
                        if report.rebuilt { "built" } else { "current" },
                        report.out
                    );
                }
            }
            Ok(())
        }
        "build-collection" => {
            let directory = positional
                .first()
                .ok_or_else(|| CliError::Usage("build-collection needs a directory".to_string()))?;
            let asset_root = compiler::resolve_asset_root(options.asset_root.as_deref())?;
            crate::engine::paths::configure_assets(&asset_root)?;
            let session = Session::start(options.progress)?;
            let collection_result = compiler::build_collection(
                &PathBuf::from(directory.as_str()),
                &asset_root,
                &options.variants,
                options.workers,
                option_args.iter().any(|arg| arg == "--force"),
            );
            if session.interrupted() {
                return Err(CliError::Interrupted);
            }
            let results = collection_result?;
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
            if let Some(root) = options.asset_root.as_deref() {
                crate::engine::paths::configure_assets(root)?;
            }
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
            crate::engine::paths::configure_assets(&asset_root)?;
            let report = compiler::verify_selected(
                &PathBuf::from(source.as_str()),
                options.level.as_deref(),
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
