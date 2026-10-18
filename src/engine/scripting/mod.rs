//! Bounded project-local Luau.
//!
//! VMs own no OS capabilities; typed host calls enqueue the
//! existing entity actions. A failed call publishes no actions and disables its
//! module until the next fresh map run. See docs/SCRIPTING.md for the contract.

// Luau uses C++ exceptions for ordinary VM failures through mlua callbacks.
// Refuse incompatible profiles before a budget error can terminate the player.
#[cfg(panic = "abort")]
compile_error!(
    "Places Luau requires panic=unwind; panic=abort cannot cross its foreign error boundary safely"
);

mod api;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value, VmState};
use serde::{Deserialize, Serialize};

use crate::engine::entities::EntityWorld;
use crate::engine::level::{ActionDef, LevelDef};
use crate::engine::project::ProjectPath;

pub const MAX_SOURCE_BYTES: u64 = 65_536;
const MAX_MODULES: usize = 16;
const MAX_TOTAL_BYTES: usize = 524_288;
const VM_BYTES: usize = 2 * 1024 * 1024;
const MAX_HANDLERS: usize = 64;
const MAX_CALLS: usize = 64;
const MAX_ACTIONS: usize = 128;
const MAX_DIAGNOSTICS: usize = 32;

/// Compiler-owned UTF-8 snapshot, embedded in single-level semantics. No path
/// here is ever opened by the player; stage 7 owns complete project packaging.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptSource {
    pub path: ProjectPath,
    pub source: String,
}

/// # Errors
/// Rejects unsupported export names before any VM or editor mutation.
pub fn validate_handler(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(
            "Script handler must contain 1..=64 ASCII letters, digits or underscores".into(),
        );
    }
    Ok(())
}

/// # Errors
/// Validates path roles, uniqueness and aggregate memory before creating VMs.
pub fn validate_sources(level: &LevelDef) -> Result<(), String> {
    if level.scripts.len() > MAX_MODULES || level.script_sources.len() > MAX_MODULES {
        return Err("A map supports at most 16 Luau modules".into());
    }
    let mut seen = BTreeSet::new();
    for path in &level.scripts {
        path.require_role("scripts", "luau")?;
        if !seen.insert(path.as_str().to_ascii_lowercase()) {
            return Err("Duplicate script reference".into());
        }
    }
    seen.clear();
    let mut total = 0usize;
    for script in &level.script_sources {
        script.path.require_role("scripts", "luau")?;
        total = total.saturating_add(script.source.len());
        if script.source.is_empty()
            || u64::try_from(script.source.len()).unwrap_or(u64::MAX) > MAX_SOURCE_BYTES
            || total > MAX_TOTAL_BYTES
            || !seen.insert(script.path.as_str().to_ascii_lowercase())
        {
            return Err(
                "Luau sources require unique paths, 1..=64 KiB each and at most 512 KiB total"
                    .into(),
            );
        }
    }
    Ok(())
}

struct Module {
    path: ProjectPath,
    lua: Lua,
    exports: Table,
    disabled: bool,
    started: bool,
}

fn error(message: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}

fn deadline(lua: &Lua, duration: Duration) {
    let until = Instant::now().checked_add(duration);
    let interrupts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    lua.set_interrupt(move |_| {
        let count = interrupts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count >= 20_000 || until.is_none_or(|limit| Instant::now() >= limit) {
            return Err(error("Luau execution budget exhausted"));
        }
        Ok(VmState::Continue)
    });
}

impl Module {
    fn load(source: &ScriptSource) -> mlua::Result<Self> {
        if source.source.is_empty() || source.source.len() > 65_536 {
            return Err(error("Script source requires 1..=64 KiB"));
        }
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )?;
        let _previous = lua.set_memory_limit(VM_BYTES)?;
        // Protected calls/coroutines could catch an interrupt or retain suspended
        // host calls. Dynamic code loading and debugging are not map capabilities.
        for name in [
            "pcall",
            "xpcall",
            "coroutine",
            "require",
            "load",
            "loadstring",
            "loadfile",
            "dofile",
            "collectgarbage",
            "getfenv",
            "setfenv",
            "print",
            "warn",
            "os",
            "io",
            "debug",
            "package",
        ] {
            lua.globals().set(name, Value::Nil)?;
        }
        // Pattern matching may spend quadratic native work without VM
        // interrupts. Keep simple bounded string operations, omit patterns.
        let strings: Table = lua.globals().get("string")?;
        for name in ["find", "match", "gmatch", "gsub", "split"] {
            strings.set(name, Value::Nil)?;
        }
        lua.sandbox(true)?;
        deadline(&lua, Duration::from_millis(10));
        let exports: Table = lua
            .load(&source.source)
            .set_name(source.path.as_str())
            .set_mode(mlua::chunk::ChunkMode::Text)
            .eval()?;
        lua.remove_interrupt();
        if exports.metatable().is_some() {
            return Err(error("Export table cannot have a metatable"));
        }
        let mut count = 0usize;
        for pair in exports.pairs::<Value, Value>() {
            let (key, value) = pair?;
            let (Value::String(name), Value::Function(_)) = (key, value) else {
                return Err(error("Module exports require string names and functions"));
            };
            validate_handler(&name.to_str()?).map_err(error)?;
            count = count.saturating_add(1);
            if count > MAX_HANDLERS {
                return Err(error("Too many exported handlers (maximum 64)"));
            }
        }
        exports.set_readonly(true);
        Ok(Self {
            path: source.path.clone(),
            lua,
            exports,
            disabled: false,
            started: false,
        })
    }
}

/// Executes bounded module initialization, validates exports, then drops every
/// VM. This is a compiler/editor check, never a full game or map bake.
/// # Errors
/// Reports source path, line and bounded VM/contract diagnostics.
pub fn check(sources: &[ScriptSource]) -> Result<(), String> {
    if sources.len() > MAX_MODULES
        || sources
            .iter()
            .map(|s| s.source.len())
            .fold(0usize, usize::saturating_add)
            > MAX_TOTAL_BYTES
    {
        return Err("Script source budget exceeded".into());
    }
    for source in sources {
        let _module =
            Module::load(source).map_err(|e| diagnostic(source.path.as_str(), "load", &e))?;
    }
    Ok(())
}

/// # Errors
/// Returns bounded, sorted exports for editor linking without retaining a VM.
pub fn handlers(source: &ScriptSource) -> Result<Vec<String>, String> {
    let module = Module::load(source).map_err(|e| diagnostic(source.path.as_str(), "load", &e))?;
    let mut names = module
        .exports
        .pairs::<String, Function>()
        .map(|pair| pair.map(|(name, _)| name))
        .collect::<mlua::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    names.sort();
    Ok(names)
}

/// Validates a selected compiler snapshot and every linked export before baking.
/// # Errors
/// Reports unavailable modules/handlers or bounded initialization failures.
pub fn check_level(level: &LevelDef) -> Result<(), String> {
    validate_sources(level)?;
    let mut exports = std::collections::BTreeMap::new();
    for source in &level.script_sources {
        drop(exports.insert(source.path.as_str(), handlers(source)?));
    }
    for (path, handler) in crate::engine::loader::script_calls(level) {
        if !exports
            .get(path.as_str())
            .is_some_and(|names| names.iter().any(|name| name == handler))
        {
            return Err(format!(
                "{} / {handler}: linked handler unavailable",
                path.as_str()
            ));
        }
    }
    validate_compiled_sources(level)
}

/// # Errors
/// A player package must carry every referenced module; it cannot read local files.
pub fn validate_compiled_sources(level: &LevelDef) -> Result<(), String> {
    for path in level.scripts.iter().chain(
        crate::engine::loader::script_calls(level)
            .iter()
            .map(|(path, _)| *path),
    ) {
        if !level
            .script_sources
            .iter()
            .any(|source| source.path == *path)
        {
            return Err(format!("Missing compiled script {}", path.as_str()));
        }
    }
    Ok(())
}

/// Enforces the negotiated package capability before any module executes.
/// # Errors
/// Scripted semantics require `luau-v1` and complete embedded sources.
pub(crate) fn validate_package_sources(
    level: &LevelDef,
    capabilities: &[String],
) -> Result<(), String> {
    validate_compiled_sources(level)?;
    if !level.script_sources.is_empty()
        && !capabilities
            .iter()
            .any(|capability| capability == "luau-v1")
    {
        return Err("Scripted package must declare luau-v1".into());
    }
    Ok(())
}

fn diagnostic(path: &str, handler: &str, problem: &impl std::fmt::Display) -> String {
    // Error strings can contain script-controlled payloads; retain bounded,
    // printable diagnostics in the editor and native log.
    format!("{path} / {handler}: {problem}")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(512)
        .collect()
}

#[derive(Default)]
pub struct ScriptRuntime {
    sources: Vec<ScriptSource>,
    modules: Vec<Module>,
    diagnostics: Vec<String>,
    points: BTreeSet<String>,
    groups: BTreeSet<String>,
    calls: usize,
    actions: usize,
    elapsed: Duration,
}

impl ScriptRuntime {
    #[must_use]
    pub fn new(level: &LevelDef) -> Self {
        if let Err(problem) = validate_sources(level) {
            let mut invalid = Self::default();
            invalid.note(problem);
            return invalid;
        }
        let mut runtime = Self {
            sources: level.script_sources.clone(),
            points: level
                .spawn_points
                .iter()
                .map(|point| point.id.clone())
                .collect(),
            groups: level.physics_reset_groups().map(str::to_string).collect(),
            ..Self::default()
        };
        runtime.reload();
        runtime
    }

    fn note(&mut self, message: String) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            crate::engine::logging::warn(format!("[luau] {message}"));
            self.diagnostics.push(message);
        }
    }

    /// Recreates globals, closures, disabled status and diagnostics. No old
    /// callbacks, userdata or coroutine references survive a replay/reset.
    pub fn reload(&mut self) {
        self.modules.clear();
        self.diagnostics.clear();
        self.begin_tick();
        if self.sources.len() > MAX_MODULES {
            self.note("Luau module budget exceeded".into());
            return;
        }
        for source in self.sources.clone() {
            match Module::load(&source) {
                Ok(module) => self.modules.push(module),
                Err(problem) => self.note(diagnostic(source.path.as_str(), "load", &problem)),
            }
        }
    }

    pub const fn begin_tick(&mut self) {
        self.calls = 0;
        self.actions = 0;
        self.elapsed = Duration::ZERO;
    }

    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Named exported handlers. Host actions are staged atomically and only
    /// returned after successful execution; effects read pre-call world state.
    pub fn call(
        &mut self,
        path: &ProjectPath,
        handler: &str,
        world: &EntityWorld,
        subject: Option<&str>,
        origin: Option<glam::Vec3>,
        delta: f32,
    ) -> Vec<ActionDef> {
        let Some(index) = self.modules.iter().position(|module| module.path == *path) else {
            self.note(format!("{}: script unavailable", path.as_str()));
            return Vec::new();
        };
        let Some(module) = self.modules.get_mut(index) else {
            return Vec::new();
        };
        if module.disabled {
            return Vec::new();
        }
        if self.calls >= MAX_CALLS
            || self.actions >= MAX_ACTIONS
            || self.elapsed >= Duration::from_millis(8)
        {
            module.disabled = true;
            self.note(format!(
                "{} / {handler}: per-frame script budget exhausted; module disabled",
                path.as_str()
            ));
            return Vec::new();
        }
        self.calls = self.calls.saturating_add(1);
        let started = Instant::now();
        let queue = Mutex::new(Vec::new());
        let notices = Mutex::new(Vec::new());
        let remaining = MAX_ACTIONS.saturating_sub(self.actions).min(32);
        deadline(
            &module.lua,
            Duration::from_millis(2).min(Duration::from_millis(8).saturating_sub(self.elapsed)),
        );
        let result = module.lua.scope(|scope| {
            let host = api::host(
                &module.lua,
                scope,
                world,
                &queue,
                &notices,
                &self.points,
                &self.groups,
                remaining,
            )?;
            let event = module.lua.create_table()?;
            event.set("subject", subject)?;
            event.set("dt", delta)?;
            if let Some((kind, key, actor)) = world.script_event_context() {
                event.set("kind", kind)?;
                event.set("key", key)?;
                event.set("actor", actor)?;
            }
            if let Some(position) = origin {
                event.set("x", position.x)?;
                event.set("y", position.y)?;
                event.set("z", position.z)?;
            }
            event.set_readonly(true);
            let function: Function = module.exports.raw_get(handler)?;
            function.call::<()>((host, event))
        });
        module.lua.remove_interrupt();
        self.elapsed = self.elapsed.saturating_add(started.elapsed());
        if let Err(problem) = result {
            module.disabled = true;
            self.note(diagnostic(path.as_str(), handler, &problem));
            return Vec::new();
        }
        if let Ok(messages) = notices.into_inner() {
            for message in messages {
                self.note(message);
            }
        }
        let actions = queue.into_inner().unwrap_or_default();
        self.actions = self.actions.saturating_add(actions.len());
        actions
    }

    /// Automatic handlers run in declared source order on playing ticks only.
    pub fn automatic(&mut self) -> Vec<(ProjectPath, &'static str)> {
        let mut handlers = Vec::new();
        for module in &mut self.modules {
            if module.disabled {
                continue;
            }
            if !module.started {
                module.started = true;
                if module
                    .exports
                    .raw_get::<Value>("start")
                    .is_ok_and(|value| matches!(value, Value::Function(_)))
                {
                    handlers.push((module.path.clone(), "start"));
                }
            }
            if module
                .exports
                .raw_get::<Value>("update")
                .is_ok_and(|value| matches!(value, Value::Function(_)))
            {
                handlers.push((module.path.clone(), "update"));
            }
        }
        handlers
    }
}
