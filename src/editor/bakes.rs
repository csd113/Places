//! Saved-source validity uses the installed compiler's exact dependency and
//! integrity contract, on a read-only worker. Builds keep the existing job slot.
use super::{
    App,
    actions::Popup,
    ui::{self, Action, Hit},
};
use crate::{engine::render::Vertex, map_compiler::VerifyReport};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

struct Update {
    id: String,
    report: Result<VerifyReport, String>,
    size: Vec<String>,
}

#[derive(Default)]
pub(super) struct Planner {
    pub rows: Vec<(String, Result<VerifyReport, String>)>,
    receiver: Option<mpsc::Receiver<Update>>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    pub size: Vec<String>,
    pub page: usize,
}
impl Planner {
    pub fn invalidate(&mut self) {
        self.rows.clear();
        self.size.clear();
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub const fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn poll(&mut self) {
        let mut done = false;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(row) => {
                        if !self.cancel.load(Ordering::Relaxed) {
                            self.rows.push((row.id, row.report));
                            if !row.size.is_empty() {
                                self.size = row.size;
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
        }
        if done {
            self.receiver = None;
            if let Some(worker) = self.worker.take() {
                let _joined = worker.join();
            }
        }
    }
}
impl Drop for Planner {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _joined = worker.join();
        }
    }
}
impl App {
    pub(super) fn check_bakes(&mut self) -> Result<(), String> {
        self.popup = Some(Popup::Bakes);
        if self.bakes.busy() {
            return Ok(());
        }
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        if doc.is_dirty() {
            return Err(
                "Bake status is unknown for unsaved edits. Save, then Check saved levels.".into(),
            );
        }
        let source = doc.root().to_path_buf();
        let exported = source.join(format!("{}.placesmap", doc.manifest().id));
        let targets: Vec<_> = doc
            .level_ids()
            .map(|id| {
                let mut packages = vec![exported.clone()];
                if let Ok(single) = crate::map_compiler::project::output_path(doc.root(), Some(id))
                {
                    packages.push(single);
                }
                (id.to_string(), packages)
            })
            .collect();
        self.bakes.size = vec!["Checking export size...".into()];
        let compiler = self.tools.compiler.clone();
        let assets = crate::engine::paths::assets()?.to_path_buf();
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::clone(&cancel);
        let worker = thread::Builder::new()
            .name("places-editor-validity".into())
            .spawn(move || {
                let mut size = package_size(&exported);
                for (id, packages) in targets {
                    let mut result: Result<VerifyReport, String> =
                        Err("No output exists. Build this level to establish a valid bake.".into());
                    for package in packages.into_iter().filter(|p| p.is_file()) {
                        if cancellation.load(Ordering::Relaxed) {
                            break;
                        }
                        let checked = verify(&compiler, &source, &id, &package, &assets);
                        let rank = |value: &Result<VerifyReport, String>| {
                            value
                                .as_ref()
                                .map_or(4_u8, |report| match report.status.as_str() {
                                    "current" => 0,
                                    "refresh" => 1,
                                    "bake" => 2,
                                    _ => 3,
                                })
                        };
                        if rank(&checked) < rank(&result) {
                            result = checked;
                        }
                        if result.as_ref().is_ok_and(|report| report.current) {
                            break;
                        }
                    }
                    if cancellation.load(Ordering::Relaxed)
                        || sender
                            .send(Update {
                                id,
                                report: result,
                                size: std::mem::take(&mut size),
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        self.bakes.rows.clear();
        self.bakes.page = 0;
        self.bakes.cancel = cancel;
        self.bakes.receiver = Some(receiver);
        self.bakes.worker = Some(worker);
        Ok(())
    }
    pub(super) fn build_bake_row(&mut self, index: usize) -> Result<(), String> {
        let id = self
            .bakes
            .rows
            .get(index)
            .ok_or("Check saved levels first")?
            .0
            .clone();
        self.document
            .as_mut()
            .ok_or("Open a project first")?
            .select_level(&id)?;
        self.changed();
        self.popup = None;
        self.start_build(false)
    }
}
fn verify(
    compiler: &std::path::Path,
    source: &std::path::Path,
    id: &str,
    package: &std::path::Path,
    assets: &std::path::Path,
) -> Result<VerifyReport, String> {
    let output = std::process::Command::new(compiler)
        .arg("verify")
        .arg(source)
        .arg("--level")
        .arg(id)
        .arg("--package")
        .arg(package)
        .arg("--asset-root")
        .arg(assets)
        .arg("--json")
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(300)
            .collect())
    }
}

fn package_size(path: &PathBuf) -> Vec<String> {
    let result = (|| {
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let archive_bytes = file.metadata().map_err(|error| error.to_string())?.len();
        let archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
        let entries = archive.decompressed_size().unwrap_or(0);
        // Project ZIPs deduplicate content into opaque .record blobs. Classify
        // each physical record once using its virtual member name, rather than
        // pretending every blob is an unidentifiable metadata record.
        let mut names = std::collections::BTreeMap::new();
        if let Some(project) = crate::engine::package::project::inspect(path)? {
            for level in project.levels {
                for (logical, record) in level.records {
                    let _name = names.entry(record).or_insert(logical);
                }
            }
        }
        let mut categories = std::collections::BTreeMap::<String, u64>::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index_data(index)
                .map_err(|error| error.to_string())?;
            let physical = entry.name().map_err(|error| error.to_string())?;
            let name = names
                .get(physical.as_ref())
                .map_or_else(|| physical.as_ref(), String::as_str);
            let category = if name.contains("lightmaps") {
                "lightmaps"
            } else if name.contains(".props") {
                "models"
            } else if std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mesh"))
            {
                "mesh"
            } else if std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("irradiance"))
            {
                "probes"
            } else {
                "other records"
            };
            let bytes = categories.entry(category.into()).or_default();
            *bytes = bytes.saturating_add(entry.size());
        }
        let mut sorted: Vec<_> = categories.into_iter().collect();
        sorted.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
        let mut lines = vec![
            format!("Export archive: {archive_bytes} bytes on disk (ZIP compressed)"),
            format!("Archive entries: {entries} bytes after ZIP decoding"),
        ];
        for chunk in sorted.chunks(3) {
            lines.push(
                chunk
                    .iter()
                    .map(|(name, bytes)| format!("{name}: {bytes} B"))
                    .collect::<Vec<_>>()
                    .join(" / "),
            );
        }
        Ok::<_, String>(lines)
    })();
    result.unwrap_or_else(|_| {
        vec!["Complete-project export size: unavailable until exported.".into()]
    })
}
pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    hits.clear();
    ui::rect(
        vertices,
        [15.0, 28.0, 450.0, 211.0],
        [0.075, 0.11, 0.14, 1.0],
    );
    ui::text(
        vertices,
        "LIGHTING / SAVED LEVEL VALIDITY",
        25.0,
        36.0,
        ui::INK,
        100,
    );
    ui::text(
        vertices,
        if app.bakes.busy() {
            "Checking hashes/integrity. Cancel stops after the current level."
        } else if app.bakes.rows.is_empty() {
            "Unknown until checked. Save edits, then check saved levels."
        } else {
            "Select a row to build/refresh only that level; other outputs are kept."
        },
        25.0,
        48.0,
        ui::INK,
        106,
    );
    draw_rows(app, vertices, hits);
    for (x, label, action) in [
        (25.0, "< Prev", Action::Page(false)),
        (105.0, "Next >", Action::Page(true)),
    ] {
        ui::popup_button(vertices, hits, [x, 161.0, 70.0, 10.0], label, action);
    }
    ui::text(
        vertices,
        &format!(
            "Page {} / {}",
            app.bakes.page.saturating_add(1),
            app.bakes
                .rows
                .len()
                .saturating_sub(1)
                .checked_div(4)
                .unwrap_or(0)
                .saturating_add(1)
        ),
        210.0,
        163.0,
        ui::MUTED,
        40,
    );
    ui::text(
        vertices,
        "Entry bytes precede codecs; decoded data, installed cache and RAM differ.",
        25.0,
        178.0,
        ui::MUTED,
        106,
    );
    for (index, line) in app.bakes.size.iter().enumerate() {
        ui::text(
            vertices,
            line,
            25.0,
            f32::from(u16::try_from(index).unwrap_or(0)).mul_add(8.0, 188.0),
            ui::INK,
            106,
        );
    }
    for (x, label, action) in [
        (25.0, "Check saved", Action::BakeStatus),
        (175.0, "Cancel check", Action::BakeCancel),
        (385.0, "Close", Action::ClosePopup),
    ] {
        ui::popup_button(vertices, hits, [x, 225.0, 72.0, 11.0], label, action);
    }
}

fn draw_rows(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (index, (id, result)) in app
        .bakes
        .rows
        .iter()
        .enumerate()
        .skip(app.bakes.page.saturating_mul(4))
        .take(4)
    {
        let y = f32::from(u16::try_from(index % 4).unwrap_or(0)).mul_add(23.0, 63.0);
        let (label, reason) = match result {
            Ok(report) => (
                format!(
                    "{id}: {} / {}",
                    match report.status.as_str() {
                        "current" => "valid",
                        "bake" => "stale",
                        "refresh" => "metadata refresh",
                        _ => "invalid",
                    },
                    if report.bake_required {
                        "rebake required"
                    } else {
                        "reuse prepared lighting"
                    }
                ),
                report.differences.first().map_or(
                    "Dependency hashes and output integrity match.",
                    String::as_str,
                ),
            ),
            Err(error) => (format!("{id}: unknown"), error.as_str()),
        };
        ui::button(
            vertices,
            hits,
            [25.0, y, 428.0, 10.0],
            &label,
            Action::BakeLevel(index),
            !result.as_ref().is_ok_and(|report| report.current),
        );
        ui::text(vertices, reason, 27.0, y + 12.0, ui::MUTED, 105);
    }
}
