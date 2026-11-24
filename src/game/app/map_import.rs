//! SDL file selection on the frame loop, installation on the bounded CPU worker.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use sdl3::dialog::DialogError;

use super::{FrameLoop, loading::Worker};
use crate::engine::loader::{self, LevelManager};

type Selection = Result<Vec<PathBuf>, DialogError>;

struct Report {
    message: String,
    archive: bool,
}

#[derive(Default)]
pub(super) struct MapImport {
    dialog: Option<Receiver<Selection>>,
    worker: Option<Worker<PathBuf, Report>>,
    running: bool,
}

impl MapImport {
    pub(super) const fn busy(&self) -> bool {
        self.dialog.is_some() || self.running
    }

    pub(super) fn shutdown(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.shutdown();
        }
        self.dialog = None;
    }

    pub(super) fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(Worker::is_finished)
    }

    pub(super) fn join_finished(&mut self) -> Result<(), String> {
        if let Some(worker) = &mut self.worker {
            let _joined = worker.join_finished()?;
        }
        Ok(())
    }

    fn start(&mut self, path: PathBuf, manager: &LevelManager) -> Result<(), String> {
        if self.worker.is_none() {
            let mut owner = manager.fork_for_loading();
            self.worker = Some(Worker::spawn(move |source: PathBuf, control| {
                let _budget = control.enter_budget();
                let archive = loader::is_map_zip(&source);
                let message = if archive {
                    owner.import_zip(&source)?.to_string()
                } else {
                    let installed = owner.import_file(&source)?;
                    format!("{} - {}", installed.entry.name, installed.outcome.label())
                };
                Ok(Some(Report { message, archive }))
            })?);
        }
        let worker = self.worker.as_mut().ok_or("Missing import worker")?;
        let _request = worker.request(path)?;
        self.running = true;
        Ok(())
    }

    fn selection(&mut self) -> Option<Result<Option<PathBuf>, String>> {
        let selection = match self.dialog.as_ref()?.try_recv() {
            Ok(Ok(paths)) => Ok(paths.into_iter().next()),
            Ok(Err(DialogError::Canceled)) => Ok(None),
            Ok(Err(error)) => Err(error.to_string()),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("The file picker stopped unexpectedly".into()),
        };
        self.dialog = None;
        Some(selection)
    }
}

impl FrameLoop<'_> {
    pub(super) fn choose_import_file(&mut self) {
        if self.map_import.busy() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let result = sdl3::dialog::show_open_file_dialog(
            &[sdl3::dialog::DialogFileFilter {
                name: "Compiled map or map ZIP",
                pattern: "placesmap;zip",
            }],
            None::<&std::path::Path>,
            false,
            &*self.window,
            Box::new(move |selection, _filter| {
                let _sent = sender.send(selection);
            }),
        );
        match result {
            Ok(()) => self.map_import.dialog = Some(receiver),
            Err(error) => self
                .ui_state
                .set_status(format!("Could not open import file picker: {error}"), true),
        }
    }

    pub(super) fn poll_map_import(&mut self) {
        if let Some(selection) = self.map_import.selection() {
            let started = selection.and_then(|path| {
                path.map(|selected| self.map_import.start(selected, self.level_manager))
                    .transpose()
            });
            match started {
                Ok(Some(())) => self
                    .ui_state
                    .set_status("Importing selected file...", false),
                Ok(None) => {}
                Err(error) => self
                    .ui_state
                    .set_status(format!("Import failed: {error}"), true),
            }
        }
        let result = self.map_import.worker.as_mut().and_then(Worker::poll);
        if let Some((_, outcome)) = result {
            self.map_import.running = false;
            self.level_manager.refresh();
            self.refresh_level_entries();
            match outcome {
                Ok(report) if report.archive => self.ui_state.set_import_report(report.message),
                Ok(report) => self.ui_state.set_status(report.message, false),
                Err(error) => self
                    .ui_state
                    .set_status(format!("Import failed: {error}"), true),
            }
            self.ui_state.pointer.invalidate();
        }
    }
}
