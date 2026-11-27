use super::{DisplayEvidence, VisibilityDenial, replace_power};
use std::cell::RefCell;

#[test]
fn stale_off_cannot_interrupt_new_generation() -> Result<(), &'static str> {
    let evidence = DisplayEvidence::new();
    let old = evidence.generation().ok_or("Missing initial generation")?;
    assert!(evidence.publish(old, false));

    // An OFF accepted before retirement cannot leave a separate late latch,
    // and an in-flight OFF delivered after the fresh ON is also rejected.
    evidence.invalidate();
    let current = evidence.generation().ok_or("Missing refresh generation")?;
    assert!(evidence.publish(current, true));
    assert!(!evidence.publish(old, false));
    assert!(evidence.powered());
    assert!(!evidence.take_interruption());

    // Conversely an ON never clears a genuine interruption of this generation.
    assert!(evidence.publish(current, false));
    assert_eq!(evidence.check(), Err(VisibilityDenial::DisplayNotOn));
    assert!(evidence.publish(current, true));
    assert_eq!(evidence.check(), Err(VisibilityDenial::Interrupted));
    assert!(evidence.take_interruption());
    assert!(!evidence.take_interruption());
    assert!(evidence.powered());
    Ok(())
}

#[test]
fn power_cycle_after_initial_admission_blocks_the_reply() -> Result<(), &'static str> {
    let evidence = DisplayEvidence::new();
    let current = evidence.generation().ok_or("Missing initial generation")?;
    assert!(evidence.publish(current, true));
    assert_eq!(evidence.check(), Ok(()), "before polling the pipe");

    // Reproduce the former gap: a clear latch followed by OFF then ON before
    // the final power read. The old power-only predicate would allow the reply.
    assert!(!evidence.take_interruption());
    assert!(evidence.publish(current, false));
    assert!(evidence.publish(current, true));
    assert_eq!(evidence.check(), Err(VisibilityDenial::Interrupted));
    assert_eq!(
        evidence.check(),
        Err(VisibilityDenial::Interrupted),
        "a verification attempt never acknowledges the interruption"
    );
    assert!(
        evidence.take_interruption(),
        "only deliberate opt-in can acknowledge it"
    );
    assert_eq!(evidence.check(), Ok(()));
    Ok(())
}

#[test]
fn new_display_evidence_cannot_use_old_callbacks() -> Result<(), &'static str> {
    let evidence = DisplayEvidence::new();
    let old = evidence.generation().ok_or("Missing initial generation")?;
    assert!(
        evidence.publish(old, true) && evidence.powered(),
        "initial sample is on"
    );

    evidence.invalidate();
    let next = evidence.generation().ok_or("Missing refresh generation")?;
    assert_eq!(evidence.check(), Err(VisibilityDenial::NoPowerReport));
    assert_ne!(old, next, "display change retires the callback generation");
    assert!(
        !evidence.powered(),
        "display change fails closed while waiting"
    );
    assert!(
        !evidence.publish(old, true),
        "late old on callback is ignored"
    );
    assert!(
        !evidence.powered(),
        "an obsolete callback cannot establish visibility"
    );

    assert!(
        evidence.publish(next, true) && evidence.powered(),
        "fresh on evidence is accepted"
    );
    assert!(
        !evidence.publish(old, false),
        "retired off callback cannot change the new sample"
    );
    assert!(
        evidence.powered(),
        "only the active registration may update power evidence"
    );
    assert!(
        evidence.publish(next, false) && !evidence.powered(),
        "fresh off fails closed"
    );
    Ok(())
}

#[test]
fn refresh_does_not_duplicate_or_reuse_an_owned_registration() {
    let sequence = RefCell::new(Vec::new());
    let mut subscription = Some(7_u8);
    replace_power(
        &mut subscription,
        |handle| {
            sequence.borrow_mut().push(("unregister refused", handle));
            false
        },
        || {
            sequence.borrow_mut().push(("unexpected registration", 8));
            Some(8)
        },
    );
    assert_eq!(
        subscription,
        Some(7),
        "failure retains ownership for Drop or retry"
    );
    assert_eq!(
        *sequence.borrow(),
        [("unregister refused", 7)],
        "no second subscription while old one lives"
    );

    replace_power(
        &mut subscription,
        |handle| {
            sequence.borrow_mut().push(("unregistered", handle));
            true
        },
        || {
            sequence.borrow_mut().push(("register failed", 8));
            None
        },
    );
    assert_eq!(
        subscription, None,
        "registration failure cannot keep a dead handle"
    );
    replace_power(
        &mut subscription,
        |_| false,
        || {
            sequence.borrow_mut().push(("fresh registration", 9));
            Some(9)
        },
    );
    assert_eq!(
        subscription,
        Some(9),
        "later refresh may obtain a new handle"
    );
    assert_eq!(
        *sequence.borrow(),
        [
            ("unregister refused", 7),
            ("unregistered", 7),
            ("register failed", 8),
            ("fresh registration", 9),
        ],
        "refresh always releases the existing handle before registering"
    );
}

use crate::editor::{
    App, ToolPaths,
    agent_api::session_windows::client,
    create_pumped_app,
    performance::FrameTrace,
    retire_app,
    ui::{self, Action},
};
use sdl3::{
    EventPump,
    event::Event,
    keyboard::{Keycode, Mod},
    video::{Window, WindowPos::Positioned},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

struct NativeProject(PathBuf);

impl Drop for NativeProject {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

// This is an explicit desktop smoke, never a default test run. It protects
// keyboard footer dispatch and the clipboard boundary with real frames, native
// monitorability and live pipes. No other test covers that complete path.
#[test]
#[ignore = "Opens two disposable windows on an idle, unlocked Windows console"]
fn native_footer_revokes_and_requires_fresh_code_after_real_cover() -> Result<(), String> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    crate::engine::paths::configure_assets(&assets)?;
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let work = video
        .get_primary_display()
        .and_then(|display| display.get_usable_bounds())
        .map_err(|error| error.to_string())?;
    if work.width() < 1150 || work.height() < 600 {
        return Err("Not enough usable desktop area for two tiled QA windows".into());
    }

    let x = work.x().saturating_add(20);
    let y = work.y().saturating_add(24);
    let adjacent = x.saturating_add(1000);
    let mut window = video
        .window("Places QA editor", 960, 544)
        .position(x, y)
        .build()
        .map_err(|error| error.to_string())?;
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    let mut trace = FrameTrace::new(false);
    let mut app = create_pumped_app(
        &window,
        &sdl,
        ToolPaths::discover()?,
        None,
        false,
        &mut events,
        &mut trace,
    )?
    .ok_or("QA editor startup cancelled")?;
    let result = (|| {
        let path = std::env::temp_dir().join(format!("places-footer-qa-{}", std::process::id()));
        if path.exists() {
            return Err("Disposable footer project already exists".into());
        }
        let project = NativeProject(path);
        app.create_project(&project.0)?;
        app.agent_clipboard_capture = Some(String::new());
        app.install_agent(&window);
        assert_eq!(app.agent.label(), Some("Agent Off"));
        if !window.raise() {
            return Err("Could not show the owned QA editor above ordinary windows".into());
        }

        let lower_y = work
            .y()
            .saturating_add(i32::try_from(work.height()).map_err(|error| error.to_string())?)
            .saturating_sub(560);
        let far_x = work
            .x()
            .saturating_add(i32::try_from(work.width()).map_err(|error| error.to_string())?)
            .saturating_sub(980);
        let mut picked = None;
        for (test_x, test_y, cover_x) in [
            (x, y, adjacent),
            (far_x, y, x),
            (x, lower_y, adjacent),
            (far_x, lower_y, x),
        ] {
            if !window.set_position(Positioned(test_x), Positioned(test_y)) {
                continue;
            }
            match qa_wait_ready(&mut app, &window, &mut events, Duration::from_secs(75)) {
                Ok(()) => {
                    picked = Some((test_x, test_y, cover_x));
                    break;
                }
                Err(_) if qa_visibility(&app)? == Err(VisibilityDenial::Covered) => {}
                Err(reason) => return Err(reason),
            }
        }
        let (viewport_x, viewport_y, cover_x) =
            picked.ok_or("No uncovered desktop slot for the disposable editor")?;
        let mut cover = video
            .window("Places QA ordinary cover", 120, 120)
            .position(cover_x, viewport_y)
            .build()
            .map_err(|error| error.to_string())?;
        qa_exercise_footer(
            &mut app,
            &window,
            &mut events,
            &mut cover,
            (viewport_x, viewport_y, cover_x),
        )
    })();
    app.pause_agent();
    retire_app(app, &mut events, &mut trace);
    result
}

fn qa_visibility(app: &App) -> Result<Result<(), VisibilityDenial>, String> {
    app.agent
        .monitor_for_qa()
        .map(super::Monitor::verified)
        .ok_or_else(|| "Missing native viewport monitor".into())
}

fn qa_dispatch(app: &mut App, window: &Window, event: &Event) {
    app.agent_before_event(event, window);
    let (_, hits) = ui::build(app);
    app.event(event, window, &hits);
    app.agent_after_event();
}

fn qa_frame(app: &mut App, window: &Window, events: &mut EventPump) -> bool {
    for event in events.poll_iter() {
        qa_dispatch(app, window, &event);
    }
    app.poll_background();
    app.viewport.sync_window(window);
    let presented = app.draw(window);
    app.agent_tick(presented);
    presented
}

fn qa_wait_ready(
    app: &mut App,
    window: &Window,
    events: &mut EventPump,
    timeout: Duration,
) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let presented = qa_frame(app, window, events);
        let visible = qa_visibility(app)?;
        let preparing = app.viewport.is_preparing();
        let local_preview = app.viewport.local_progress().is_some();
        if !preparing
            && !local_preview
            && ((presented && visible.is_ok()) || visible == Err(VisibilityDenial::Covered))
        {
            return visible.map_err(|reason| format!("Native editor visibility: {reason:?}"));
        }
        if let Some(error) = app.viewport.status() {
            return Err(format!("Native QA viewport startup failed: {error}"));
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "Native editor was not ready. Visibility={visible:?}, presented={presented}, preparing={preparing}, local_preview={local_preview}"
            ));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn qa_key(app: &mut App, window: &Window, code: Keycode, keymod: Mod) {
    qa_dispatch(
        app,
        window,
        &Event::KeyDown {
            timestamp: 0,
            window_id: window.id(),
            keycode: Some(code),
            scancode: None,
            keymod,
            repeat: false,
            which: 0,
            raw: 0,
        },
    );
}

fn qa_press_footer(app: &mut App, window: &Window) -> Result<(), String> {
    let (_, hits) = ui::build(app);
    for _ in 0..hits.len() {
        if app.focused_action() == Some(Action::AgentSession) {
            qa_key(app, window, Keycode::Return, Mod::NOMOD);
            return Ok(());
        }
        qa_key(app, window, Keycode::Tab, Mod::LSHIFTMOD);
    }
    Err("Agent footer could not be reached by keyboard".into())
}

fn qa_grant(app: &mut App, window: &Window) -> Result<String, String> {
    qa_press_footer(app, window)?;
    if app.agent.label() != Some("Take Over") {
        return Err(format!(
            "Native footer did not offer Take Over: {}",
            app.status
        ));
    }
    let captured = app
        .agent_clipboard_capture
        .as_mut()
        .ok_or("QA clipboard capture missing")?;
    let code = captured
        .strip_prefix("places-editor --agent status ")
        .filter(|code| !code.is_empty())
        .ok_or("Native footer did not publish the status command")?
        .to_owned();
    captured.clear();
    Ok(code)
}

fn qa_reply_while_visible(
    app: &mut App,
    window: &Window,
    events: &mut EventPump,
    code: &str,
) -> Result<(), String> {
    let query = ["status".into(), code.into()];
    let request = std::thread::spawn(move || {
        client(&query)
            .is_ok_and(|reply| reply == "Places Editor agent session is active (status only).")
    });
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) && !request.is_finished() {
        let _presented = qa_frame(app, window, events);
        std::thread::sleep(Duration::from_millis(5));
    }
    if !request.join().unwrap_or(false) {
        return Err("The visible QA endpoint did not answer through the editor loop".into());
    }
    Ok(())
}

fn qa_denied(code: &str) -> bool {
    client(&["status".into(), code.into()])
        .is_err_and(|reason| reason.starts_with("Editor session unavailable:"))
}

#[expect(
    clippy::print_stdout,
    reason = "Explicitly invoked native QA prints sanitized receipts."
)]
fn qa_exercise_footer(
    app: &mut App,
    window: &Window,
    events: &mut EventPump,
    cover: &mut Window,
    (x, y, adjacent): (i32, i32, i32),
) -> Result<(), String> {
    qa_wait_ready(app, window, events, Duration::from_secs(5))?;
    let first = qa_grant(app, window)?;
    qa_reply_while_visible(app, window, events, &first)?;
    println!("NATIVE_FOOTER_VISIBLE=ready");

    if !cover.set_position(
        Positioned(x.saturating_sub(12)),
        Positioned(y.saturating_sub(12)),
    ) {
        return Err("Could not place the disposable cover".into());
    }
    cover
        .set_size(1000, 580)
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        let _presented = qa_frame(app, window, events);
        let visible = qa_visibility(app)?;
        if visible == Err(VisibilityDenial::Covered) && app.agent.label() == Some("Resume Agent") {
            break;
        }
        if started.elapsed() >= Duration::from_secs(2) {
            return Err(format!(
                "Ordinary window did not cause native cover and revoke: {visible:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    assert!(qa_denied(&first));
    qa_press_footer(app, window)?;
    assert_eq!(app.agent.label(), Some("Resume Agent"));
    assert!(
        app.agent_clipboard_capture
            .as_deref()
            .is_some_and(str::is_empty)
    );
    println!("NATIVE_FOOTER_COVERED=revoked");

    cover
        .set_size(120, 120)
        .map_err(|error| error.to_string())?;
    if !cover.set_position(Positioned(adjacent), Positioned(y)) {
        return Err("Could not return the cover beside the editor".into());
    }
    qa_wait_ready(app, window, events, Duration::from_secs(5))?;
    assert_eq!(app.agent.label(), Some("Resume Agent"));
    assert!(qa_denied(&first));
    println!("NATIVE_FOOTER_UNCOVERED=still_revoked");

    let fresh = qa_grant(app, window)?;
    assert!(fresh != first, "the footer must publish a fresh session");
    assert!(qa_denied(&first));
    qa_reply_while_visible(app, window, events, &fresh)?;
    println!("NATIVE_FOOTER_EXPLICIT_RESUME=only_fresh_ready");

    qa_press_footer(app, window)?;
    assert_eq!(app.agent.label(), Some("Resume Agent"));
    assert!(
        app.agent_clipboard_capture
            .as_deref()
            .is_some_and(str::is_empty)
    );
    assert!(qa_denied(&fresh));
    println!("NATIVE_FOOTER_TAKE_OVER=revoked");
    Ok(())
}
