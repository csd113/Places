//! Startup data is prepared by the same owned worker that later loads worlds.
//! SDL, the device and presentation always remain on the calling thread.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use super::{APP_VERSION, actions, loading, ui};
use crate::engine::audio::{AudioSystem, PreparedAudio};
use crate::engine::loader::LevelManager;
use crate::engine::perf::loading::{LoadTrace, StageTimer};
use crate::engine::render::{DrawableSize, RenderCamera, Renderer};
use glam::Vec3;
use sdl3::event::{Event, WindowEvent};
use sdl3::video::Window;

pub(super) struct Ready {
    pub manager: LevelManager,
    pub audio: Result<PreparedAudio, String>,
    pub ui: ui::UiState,
}

pub(super) struct Preparation {
    loader: Option<loading::Loader>,
    receiver: mpsc::Receiver<Result<Ready, String>>,
    cancelled: Arc<AtomicBool>,
    deferred: Vec<Event>,
    focus: Option<Event>,
}

impl Preparation {
    pub fn new() -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let loader = loading::Loader::new(move || {
            let result = prepare(&worker_cancelled);
            match result {
                Ok((manager, ready)) => {
                    sender
                        .send(Ok(ready))
                        .map_err(|_closed| "Startup receiver closed".to_string())?;
                    Ok(manager)
                }
                Err(error) => {
                    let _sent = sender.send(Err(error.clone()));
                    Err(error)
                }
            }
        })?;
        Ok(Self {
            loader: Some(loader),
            receiver,
            cancelled,
            deferred: Vec::new(),
            focus: None,
        })
    }

    /// Consume startup window controls; retain only the bounded native script
    /// events and the final focus event for ordinary frame-loop dispatch.
    pub fn pump(
        &mut self,
        pump: &mut sdl3::EventPump,
        window: &mut Window,
        actions: Option<&actions::Actions>,
        trace: &mut LoadTrace,
    ) -> Result<bool, String> {
        trace.record("event_pump", 0, "bootstrap");
        for event in pump.poll_iter() {
            if let Some(received) = actions.and_then(|script| script.receive_bootstrap(&event)) {
                trace.record("action", 0, &serde_json::json!({"id":received.id,"kind":format!("{:?}",received.action),"latency_ms":received.latency.as_secs_f64()*1_000.0_f64}).to_string());
                if matches!(received.action, actions::Action::Quit {}) {
                    return Ok(false);
                }
                if let actions::Action::Resize { width, height } = received.action {
                    window
                        .set_size(width, height)
                        .map_err(|error| error.to_string())?;
                }
            } else if matches!(event, Event::Quit { timestamp: _ }) {
                return Ok(false);
            } else if matches!(
                event,
                Event::Window {
                    win_event: WindowEvent::FocusGained | WindowEvent::FocusLost,
                    timestamp: _,
                    window_id: _
                }
            ) {
                self.focus = Some(event);
            } else if actions.is_some_and(|script| script.owns_event(&event)) {
                // Each of at most 128 script slots emits exactly one event.
                self.deferred.push(event);
            }
        }
        Ok(true)
    }

    pub fn poll(&mut self) -> Result<Option<Ready>, String> {
        // Retire the initialization request on the existing bounded scheduler.
        if let Some(loader) = &mut self.loader
            && let Some((_, Err(error))) = loader.poll()
        {
            return Err(error);
        }
        match self.receiver.try_recv() {
            Ok(result) => result.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Startup preparation stopped before completion".into())
            }
        }
    }

    pub fn finish(&mut self, sdl: &sdl3::Sdl) -> Result<loading::Loader, String> {
        let events = sdl.event().map_err(|error| error.to_string())?;
        if let Some(event) = self.focus.take() {
            events
                .push_event(event)
                .map_err(|error| error.to_string())?;
        }
        for event in self.deferred.drain(..) {
            events
                .push_event(event)
                .map_err(|error| error.to_string())?;
        }
        self.loader
            .take()
            .ok_or_else(|| "Startup loader already transferred".to_string())
    }

    pub fn shutdown(
        &mut self,
        pump: &mut sdl3::EventPump,
        trace: &mut LoadTrace,
    ) -> Result<(), String> {
        self.cancelled.store(true, Ordering::Release);
        let Some(loader) = &mut self.loader else {
            return Ok(());
        };
        loader.shutdown();
        trace.record("shutdown_requested", 0, "bootstrap");
        while !loader.is_finished() {
            trace.record("event_pump", 0, "bootstrap-shutdown");
            pump.pump_events();
            std::thread::sleep(Duration::from_millis(5));
        }
        let _joined = loader.join_finished()?;
        trace.record(
            "shutdown_complete",
            0,
            "bootstrap worker joined; device unopened",
        );
        Ok(())
    }
}

impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(loader) = &mut self.loader {
            loader.shutdown();
        }
    }
}

fn prepare(cancelled: &AtomicBool) -> Result<(LevelManager, Ready), String> {
    let mut timing = StageTimer::new("bootstrap-worker");
    let manager = LevelManager::new()?;
    timing.step("catalog-and-discovery");
    if cancelled.load(Ordering::Acquire) {
        return Err("Startup cancelled".into());
    }
    let menu = manager.fork_for_loading();
    timing.step("shared-manager-snapshot");
    let ui = super::new_ui_state(&menu);
    timing.step("menu-metadata");
    if cancelled.load(Ordering::Acquire) {
        return Err("Startup cancelled".into());
    }
    let root = crate::engine::assets::resolve_asset_root().unwrap_or_else(|| "assets".into());
    let audio = AudioSystem::prepare(&root, || cancelled.load(Ordering::Acquire));
    timing.step("audio-library");
    if cancelled.load(Ordering::Acquire) {
        return Err("Startup cancelled".into());
    }
    Ok((
        manager,
        Ready {
            manager: menu,
            audio,
            ui,
        },
    ))
}

pub(super) fn render(
    renderer: &mut Renderer,
    window: &Window,
    trace: &mut LoadTrace,
) -> Result<(), String> {
    let (width, height) = window.size_in_pixels();
    let drawable = DrawableSize::new(width, height);
    if drawable.is_empty() {
        return Ok(());
    }
    let _changed = renderer.set_drawable_size(drawable);
    let _prepared = renderer.prepare_startup_pipelines()?;
    renderer.render_scene(RenderCamera::new(Vec3::ZERO, 0.0, 0.0, 75.0));
    let state = ui::UiState::default();
    renderer.render_ui(&ui::loading_geometry(&state, APP_VERSION));
    let presented = renderer.present(window);
    trace.record("bootstrap_present", 0, &format!("{presented:?}"));
    Ok(())
}

pub(super) struct Started {
    pub ready: Ready,
    pub loader: loading::Loader,
    pub event_pump: sdl3::EventPump,
    pub actions: Option<actions::Actions>,
    pub renderer: Renderer,
}

pub(super) fn start(
    sdl: &sdl3::Sdl,
    window: &mut Window,
    settings: &super::Settings,
    bench: &mut super::Bench,
    trace: &mut LoadTrace,
) -> Result<Option<Started>, String> {
    let (mut event_pump, mut actions) = super::native_inputs(sdl, window)?;
    let mut preparation = Preparation::new()?;
    match preparation.pump(&mut event_pump, window, actions.as_ref(), trace) {
        Ok(true) => {}
        result => {
            stop(&mut preparation, &mut event_pump, &mut actions, trace)?;
            return result.map(|_quit| None);
        }
    }
    let mut renderer = match super::create_renderer(window, settings, bench) {
        Ok(renderer) => renderer,
        Err(error) => {
            stop(&mut preparation, &mut event_pump, &mut actions, trace)?;
            return Err(error);
        }
    };
    super::configure_vsync(&mut renderer, bench, settings);
    trace.record("window_ready", 0, "");
    let ready = match wait(
        &mut preparation,
        &mut event_pump,
        window,
        actions.as_ref(),
        &mut renderer,
        trace,
    ) {
        Ok(Some(ready)) => ready,
        result => {
            stop(&mut preparation, &mut event_pump, &mut actions, trace)?;
            return result.map(|_quit| None);
        }
    };
    let loader = match preparation.finish(sdl) {
        Ok(loader) => loader,
        Err(error) => {
            stop(&mut preparation, &mut event_pump, &mut actions, trace)?;
            return Err(error);
        }
    };
    Ok(Some(Started {
        ready,
        loader,
        event_pump,
        actions,
        renderer,
    }))
}

fn wait(
    preparation: &mut Preparation,
    pump: &mut sdl3::EventPump,
    window: &mut Window,
    actions: Option<&actions::Actions>,
    renderer: &mut Renderer,
    trace: &mut LoadTrace,
) -> Result<Option<Ready>, String> {
    loop {
        if !preparation.pump(pump, window, actions, trace)? {
            return Ok(None);
        }
        if let Some(ready) = preparation.poll()? {
            return Ok(Some(ready));
        }
        render(renderer, window, trace)?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn stop(
    preparation: &mut Preparation,
    pump: &mut sdl3::EventPump,
    actions: &mut Option<actions::Actions>,
    trace: &mut LoadTrace,
) -> Result<(), String> {
    if let Some(script) = actions {
        script.shutdown();
    }
    preparation.shutdown(pump, trace)?;
    if let Some(script) = actions {
        while !script.is_finished() {
            pump.pump_events();
            std::thread::sleep(Duration::from_millis(5));
        }
        let _joined = script.join_finished()?;
    }
    Ok(())
}
