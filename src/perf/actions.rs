//! Explicit native acceptance actions, enqueued independently of the event loop.
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sdl3::event::Event;
use serde::Deserialize;

const MAX_ACTIONS: usize = 128;
const MAX_SCRIPT_BYTES: u64 = 65_536;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Load { level: String },
    Escape {},
    Resize { width: u32, height: u32 },
    Quality { level: String },
    Lightmaps { quality: String },
    LowLighting { enabled: bool },
    Focus { focused: bool },
    Quit {},
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    /// "start" or a request/upload/ready/failed level anchor; first occurrence.
    after: String,
    delay_ms: u64,
    action: Action,
}

fn parse(bytes: &[u8]) -> Result<Vec<Step>, String> {
    let steps: Vec<Step> = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if steps.is_empty() || steps.len() > MAX_ACTIONS {
        return Err("Native action script must contain 1..128 actions".to_string());
    }
    for step in &steps {
        let valid_anchor = step.after == "start"
            || ["request:", "upload:", "ready:", "failed:"]
                .iter()
                .any(|prefix| {
                    step.after
                        .strip_prefix(prefix)
                        .is_some_and(|id| !id.is_empty() && id.len() <= 256)
                });
        if !valid_anchor || step.delay_ms > 60_000 {
            return Err("Invalid action anchor or delay (maximum 60000 ms)".to_string());
        }
        match &step.action {
            Action::Load { level } if level.is_empty() || level.len() > 256 => {
                return Err("Invalid requested level id".to_string());
            }
            Action::Resize { width, height }
                if !(64..=8192).contains(width) || !(64..=8192).contains(height) =>
            {
                return Err("Native test window dimensions must be 64..8192".to_string());
            }
            Action::Quality { level } if crate::quality::QualityLevel::parse(level).is_none() => {
                return Err("Invalid quality".to_string());
            }
            Action::Lightmaps { quality }
                if crate::quality::LightmapQuality::parse(quality).is_none() =>
            {
                return Err("Invalid lightmap quality".to_string());
            }
            Action::Load { .. }
            | Action::Escape {}
            | Action::Resize { .. }
            | Action::Quit {}
            | Action::Quality { .. }
            | Action::Lightmaps { .. }
            | Action::LowLighting { .. }
            | Action::Focus { .. } => {}
        }
    }
    Ok(steps)
}

struct Slot {
    step: Step,
    enqueued: OnceLock<Instant>,
    handled: AtomicBool,
}

pub struct Received {
    pub id: usize,
    pub action: Action,
    pub latency: Duration,
}

pub struct Actions {
    event_type: u32,
    slots: Arc<Vec<Slot>>,
    anchors: Option<mpsc::SyncSender<(String, Instant)>>,
    handle: Option<JoinHandle<Result<(), String>>>,
    stop: Arc<AtomicBool>,
}
impl Actions {
    /// Inert unless `PLACES_BENCH_ACTIONS` names a script and `PLACES_BENCH=1`.
    pub fn from_env(events: &sdl3::EventSubsystem, window_id: u32) -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os("PLACES_BENCH_ACTIONS") else {
            return Ok(None);
        };
        if std::env::var("PLACES_BENCH").as_deref() != Ok("1") {
            return Err("PLACES_BENCH_ACTIONS requires PLACES_BENCH=1".to_string());
        }
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(MAX_SCRIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_SCRIPT_BYTES {
            return Err("Native action script exceeds 64 KiB".to_string());
        }
        let steps = parse(&bytes)?;
        // SAFETY: this id is used exclusively for null-pointer User events.
        // The integer code indexes owned slots; neither native pointer is dereferenced.
        let event_type = unsafe { events.register_event() }.map_err(|error| error.to_string())?;
        let slots = Arc::new(
            steps
                .into_iter()
                .map(|step| Slot {
                    step,
                    enqueued: OnceLock::new(),
                    handled: AtomicBool::new(false),
                })
                .collect::<Vec<_>>(),
        );
        let worker_slots = Arc::clone(&slots);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (anchors, notifications) = mpsc::sync_channel(MAX_ACTIONS);
        let sender = events.event_sender();
        let handle = std::thread::Builder::new()
            .name("places-native-actions".to_string())
            .spawn(move || {
                let mut times = HashMap::from([("start".to_string(), Instant::now())]);
                loop {
                    if worker_stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    let mut wait = Duration::from_millis(20);
                    let mut unfinished = false;
                    for (id, slot) in worker_slots.iter().enumerate() {
                        if slot.enqueued.get().is_some() {
                            continue;
                        }
                        unfinished = true;
                        let Some(anchor) = times.get(&slot.step.after) else {
                            continue;
                        };
                        let deadline = anchor
                            .checked_add(Duration::from_millis(slot.step.delay_ms))
                            .ok_or_else(|| "Action deadline overflow".to_string())?;
                        let now = Instant::now();
                        if now < deadline {
                            wait = wait.min(deadline.saturating_duration_since(now));
                            continue;
                        }
                        let _ = slot.enqueued.set(now);
                        sender
                            .push_event(Event::User {
                                timestamp: 0,
                                window_id,
                                type_: event_type,
                                code: i32::try_from(id).map_err(|error| error.to_string())?,
                                data1: std::ptr::null_mut(),
                                data2: std::ptr::null_mut(),
                            })
                            .map_err(|error| error.to_string())?;
                    }
                    if !unfinished {
                        return Ok(());
                    }
                    match notifications.recv_timeout(wait) {
                        Ok((key, time)) => {
                            times.entry(key).or_insert(time);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Some(Self {
            event_type,
            slots,
            anchors: Some(anchors),
            handle: Some(handle),
            stop,
        }))
    }

    /// The main loop publishes only lifecycle anchors used by this script.
    pub fn notify(&self, anchor: &str) -> Result<(), String> {
        if !self.slots.iter().any(|slot| slot.step.after == anchor) {
            return Ok(());
        }
        if let Some(sender) = &self.anchors {
            match sender.try_send((anchor.to_string(), Instant::now())) {
                Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    return Err("Native action notification queue full".to_string());
                }
            }
        }
        Ok(())
    }

    /// Call from `handle_event`, before ordinary gameplay/menu event handling.
    pub fn receive(&self, event: &Event) -> Option<Received> {
        let Event::User { type_, code, .. } = event else {
            return None;
        };
        if *type_ != self.event_type {
            return None;
        }
        let id = usize::try_from(*code).ok()?;
        let slot = self.slots.get(id)?;
        let enqueued = slot.enqueued.get()?;
        if slot.handled.swap(true, Ordering::Relaxed) {
            return None;
        }
        Some(Received {
            id,
            action: slot.step.action.clone(),
            latency: enqueued.elapsed(),
        })
    }

    /// Bench/capture limits must not terminate before all scripted events are handled.
    pub fn complete(&self) -> bool {
        self.slots
            .iter()
            .all(|slot| slot.handled.load(Ordering::Relaxed))
    }
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.anchors = None;
    }
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn join_finished(&mut self) -> Result<bool, String> {
        if !self.is_finished() {
            return Ok(false);
        }
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| "Native action injector panicked".to_string())??;
        }
        Ok(true)
    }
}
impl Drop for Actions {
    fn drop(&mut self) {
        self.shutdown();
        let _ = self.join_finished();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_validation_is_bounded_and_strict() {
        assert!(parse(br#"[{"after":"start","delay_ms":0,"action":{"kind":"quit"}}]"#).is_ok());
        assert!(
            parse(br#"[{"after":"upload:x","delay_ms":0,"action":{"kind":"escape"}}]"#).is_ok()
        );
        assert!(
            parse(br#"[{"after":"upload:","delay_ms":0,"action":{"kind":"escape"}}]"#).is_err()
        );
        assert!(
            parse(br#"[{"after":"start","delay_ms":0,"action":{"kind":"escape","extra":true}}]"#)
                .is_err()
        );
        assert!(parse(b"[]").is_err());
        assert!(
            parse(br#"[{"after":"start","delay_ms":60001,"action":{"kind":"quit"}}]"#).is_err()
        );
        assert!(parse(br#"[{"after":"ready:x","delay_ms":0,"action":{"kind":"resize","width":0,"height":360}}]"#).is_err());
        assert!(
            parse(br#"[{"after":"start","delay_ms":0,"action":{"kind":"load","level":""}}]"#)
                .is_err()
        );
        assert!(
            parse(br#"[{"after":"start","delay_ms":0,"action":{"kind":"shell","command":"x"}}]"#)
                .is_err()
        );
        assert!(
            parse(br#"[{"after":"start","delay_ms":0,"action":{"kind":"quit","extra":true}}]"#)
                .is_err()
        );
    }
}
