//! Script buffers share project conflict checks and atomic publication. Invalid
//! source may be saved for later editing; Check/Build requires valid Luau.

use super::{Document, persistence};
use crate::engine::project::{ProjectPath, read_bounded_file};
use crate::engine::scripting::{MAX_SOURCE_BYTES, ScriptSource};
use std::fs;

pub(super) struct ScriptDocument {
    path: std::path::PathBuf,
    disk: Vec<u8>,
    pub text: String,
    undo: std::collections::VecDeque<String>,
    redo: Vec<String>,
}
impl ScriptDocument {
    pub(super) fn heap_charge(&self) -> usize {
        self.disk
            .capacity()
            .saturating_add(self.text.capacity())
            .saturating_add(
                self.undo
                    .iter()
                    .chain(&self.redo)
                    .map(String::capacity)
                    .fold(0_usize, usize::saturating_add),
            )
    }
    pub fn is_dirty(&self) -> bool {
        self.text.as_bytes() != self.disk
    }
}

impl Document {
    #[must_use]
    pub fn script_paths(&self) -> Vec<String> {
        let mut paths: Vec<_> = self
            .manifest()
            .scripts
            .iter()
            .chain(
                self.manifest()
                    .levels
                    .iter()
                    .filter(|level| level.id == self.level_id())
                    .flat_map(|level| &level.scripts),
            )
            .chain(&self.level().scripts)
            .map(|path| path.as_str().to_string())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// # Errors
    /// Creates a unique confined file, then links it to the current raw map.
    pub fn create_script(&mut self) -> Result<String, String> {
        self.create_script_edit(&self.raw().clone(),
            "--!strict\n-- Blank custom module. Add start, update, or named event handlers.\nreturn {}\n",
            |_raw, _path| Ok(()))
    }

    pub(super) fn create_script_edit(
        &mut self,
        base: &serde_json::Value,
        source: &str,
        configure: impl Fn(&mut serde_json::Value, &str) -> Result<(), String>,
    ) -> Result<String, String> {
        if source.len() > usize::try_from(MAX_SOURCE_BYTES).unwrap_or(0) {
            return Err("Script exceeds 64 KiB".into());
        }
        self.admit_script_buffer()?;
        let directory = self.root().join("scripts");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let canonical = fs::canonicalize(&directory).map_err(|e| e.to_string())?;
        if canonical != directory
            || fs::symlink_metadata(&directory)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("Scripts directory must be a real directory inside this project".into());
        }
        for number in 1..=1000u16 {
            let relative = format!("scripts/{}_{number}.luau", self.level_id());
            let path = self.root().join(&relative);
            if path.exists() || self.scripts.contains_key(&relative) {
                continue;
            }
            let mut raw = base.clone();
            let paths = raw
                .as_object_mut()
                .ok_or("Invalid level")?
                .entry("scripts")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .ok_or("Invalid scripts")?;
            if paths.len() >= 16 {
                return Err("Maximum 16 modules per map".into());
            }
            paths.push(serde_json::json!(relative));
            configure(&mut raw, &relative)?;
            let buffer = ScriptDocument {
                path: path.clone(),
                disk: source.as_bytes().to_vec(),
                text: source.into(),
                undo: std::collections::VecDeque::new(),
                redo: Vec::new(),
            };
            self.admit_document_bytes(
                buffer
                    .heap_charge()
                    .saturating_add(super::history::value_heap_charge(&raw))
                    .saturating_add(self.selected.byte_count.saturating_mul(2)),
            )?;
            let (level, byte_count) = self.prepare_edit(&raw)?;
            // The existing atomic publisher cleans failed staging writes and
            // never replaces another author's file. Installation below cannot
            // fail: no buffer reopen or second admission follows publication.
            persistence::atomic_create(&path, source.as_bytes())?;
            self.install_edit(raw, level, byte_count);
            drop(self.scripts.insert(relative.clone(), buffer));
            return Ok(relative);
        }
        Err("No unique script filename available".into())
    }

    fn admit_script_buffer(&self) -> Result<(), String> {
        if self.scripts.len() >= 64 {
            return Err(
                "At most 64 source buffers may be open; save and reopen project to release buffers"
                    .into(),
            );
        }
        Ok(())
    }

    /// # Errors
    /// Only the selected map's declared, confined source files are editable.
    pub fn open_script(&mut self, name: &str) -> Result<(), String> {
        if !self.script_paths().iter().any(|path| path == name) {
            return Err("Script is not linked to this level".into());
        }
        if self.scripts.contains_key(name) {
            return Ok(());
        }
        self.admit_script_buffer()?;
        self.admit_document_bytes(
            usize::try_from(MAX_SOURCE_BYTES)
                .unwrap_or(usize::MAX)
                .saturating_mul(2),
        )?;
        let path = self.project.resolve(&ProjectPath::new(name.into())?)?;
        let disk = read_bounded_file(&path, MAX_SOURCE_BYTES)?;
        let text = String::from_utf8(disk.clone()).map_err(|e| e.to_string())?;
        drop(self.scripts.insert(
            name.into(),
            ScriptDocument {
                path,
                disk,
                text,
                undo: std::collections::VecDeque::new(),
                redo: Vec::new(),
            },
        ));
        Ok(())
    }

    #[must_use]
    pub fn script_text(&self, name: &str) -> Option<&str> {
        self.scripts.get(name).map(|script| script.text.as_str())
    }

    /// # Errors
    /// Edits a bounded UTF-8 buffer without touching disk or the viewport.
    pub fn set_script_text(&mut self, name: &str, text: String) -> Result<(), String> {
        if text.len() > usize::try_from(MAX_SOURCE_BYTES).unwrap_or(0) {
            return Err("Script exceeds 64 KiB".into());
        }
        self.admit_document_bytes(text.capacity())?;
        let script = self.scripts.get_mut(name).ok_or("Open script first")?;
        if script.text != text {
            script
                .undo
                .push_back(std::mem::replace(&mut script.text, text));
            script.redo.clear();
            while script.undo.len() > 16 {
                drop(script.undo.pop_front());
            }
        }
        Ok(())
    }

    /// Restores a bounded source edit; scene undo remains independent.
    pub fn script_history(&mut self, name: &str, redo: bool) -> bool {
        let Some(script) = self.scripts.get_mut(name) else {
            return false;
        };
        let next = if redo {
            script.redo.pop()
        } else {
            script.undo.pop_back()
        };
        let Some(text) = next else {
            return false;
        };
        let previous = std::mem::replace(&mut script.text, text);
        if redo {
            script.undo.push_back(previous);
        } else {
            script.redo.push(previous);
        }
        true
    }

    /// # Errors
    /// Checks all selected source buffers and named action exports in a bounded VM.
    pub fn check_scripts(&self) -> Result<(), String> {
        let mut level = self.level().clone();
        self.project.attach_scripts(&mut level)?;
        for source in &mut level.script_sources {
            if let Some(text) = self.script_text(source.path.as_str()) {
                text.clone_into(&mut source.source);
            }
        }
        crate::engine::scripting::check_level(&level)
    }

    /// # Errors
    /// Lists exported handlers from the current bounded buffer for trigger linking.
    pub fn script_handlers(&self, name: &str) -> Result<Vec<String>, String> {
        let text = self.script_text(name).ok_or("Open script first")?;
        crate::engine::scripting::handlers(&ScriptSource {
            path: ProjectPath::new(name.into())?,
            source: text.into(),
        })
    }

    pub(super) fn preflight_scripts(&self) -> Result<(), String> {
        for (name, script) in &self.scripts {
            let resolved = self.project.resolve(&ProjectPath::new(name.clone())?)?;
            if resolved != script.path
                || read_bounded_file(&resolved, MAX_SOURCE_BYTES)? != script.disk
            {
                return Err(format!(
                    "Script changed outside editor; reopen project before saving: {name}"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn save_scripts(&mut self) -> Result<(), String> {
        for script in self.scripts.values_mut().filter(|script| script.is_dirty()) {
            persistence::atomic_replace(&script.path, script.text.as_bytes())?;
            script.disk = script.text.as_bytes().to_vec();
        }
        Ok(())
    }
}
