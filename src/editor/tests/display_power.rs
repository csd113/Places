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

#[test]
#[ignore = "Opens two disposable windows on an idle, unlocked Windows console"]
#[expect(
    clippy::print_stdout,
    reason = "Explicitly invoked native QA prints sanitized receipts."
)]
fn native_session_requires_new_code_after_real_cover() -> Result<(), String> {
    use sdl3::video::WindowPos::Positioned;

    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let work = video
        .get_primary_display()
        .and_then(|display| display.get_usable_bounds())
        .map_err(|error| error.to_string())?;
    if work.width() < 1200 || work.height() < 550 {
        return Err("Not enough usable desktop area for two normal QA windows".into());
    }

    let x = work.x() + 60_i32;
    let y = work.y() + 90_i32;
    let adjacent = x + 550_i32;
    let window = video
        .window("Places QA viewport (test-only session)", 480, 300)
        .position(x, y)
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window.into_canvas();
    canvas.set_draw_color(sdl3::pixels::Color::RGB(20, 38, 48));
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    let mut session = Session::default();
    session.install(canvas.window());
    assert_eq!(session.label(), Some("Agent Off"));
    if !canvas.window_mut().raise() {
        return Err("Could not show the owned QA viewport above ordinary windows".into());
    }
    let lower_y =
        work.y() + i32::try_from(work.height()).map_err(|error| error.to_string())? - 385_i32;
    let mut picked = None;
    for (test_x, test_y, cover_x) in [
        (x, y, adjacent),
        (adjacent, y, x),
        (x, lower_y, adjacent),
        (adjacent, lower_y, x),
    ] {
        if !canvas
            .window_mut()
            .set_position(Positioned(test_x), Positioned(test_y))
        {
            continue;
        }
        let monitor = session
            .monitor_for_qa()
            .ok_or("Cannot install the production monitor")?;
        match qa_wait_for_visibility(&session, &mut events, Ok(())) {
            Ok(()) => {
                picked = Some((test_x, test_y, cover_x));
                break;
            }
            Err(reason) if monitor.verified() == Err(VisibilityDenial::Covered) => {
                println!("NATIVE_MONITOR_SLOT_BLOCKED {reason}");
                println!("{}", describe_qa_cover(monitor));
            }
            Err(reason) => return Err(reason),
        }
    }
    let (viewport_x, viewport_y, cover_x) =
        picked.ok_or("No uncovered desktop slot for the disposable viewport")?;
    println!("NATIVE_MONITOR_UNCOVERED=verified");
    let mut cover = video
        .window("Places QA ordinary cover", 510, 330)
        .position(cover_x, viewport_y - 12)
        .build()
        .map_err(|error| error.to_string())?;
    qa_wait_for_visibility(&session, &mut events, Ok(()))?;
    qa_exercise_session(
        &mut session,
        &mut canvas,
        &mut events,
        &mut cover,
        (viewport_x, viewport_y, cover_x),
    )
}

use crate::editor::agent_api::{
    SceneStamp,
    session_windows::{Session, Tick, client},
};
use sdl3::{EventPump, render::WindowCanvas, video::Window};
use std::time::{Duration, Instant};

fn qa_wait_for_visibility(
    session: &Session,
    events: &mut EventPump,
    expected: Result<(), VisibilityDenial>,
) -> Result<(), String> {
    let monitor = session
        .monitor_for_qa()
        .ok_or("Missing native viewport monitor")?;
    let started = Instant::now();
    loop {
        events.pump_events();
        let observed = monitor.verified();
        if observed == expected {
            return Ok(());
        }
        if started.elapsed() >= Duration::from_secs(2) {
            return Err(format!("Expected {expected:?}, observed {observed:?}"));
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn qa_frame(canvas: &mut WindowCanvas) -> Result<bool, String> {
    canvas.clear();
    if canvas.present() {
        Ok(true)
    } else {
        Err("Could not present the disposable viewport".into())
    }
}

fn qa_code(session: &mut Session, scope: &SceneStamp) -> Result<String, String> {
    // A deliberate QA call exercises the same native gate as the editor, with
    // no App, real project, clipboard, saved credentials or automatic UI input.
    let command = session.enable(scope)?;
    command
        .strip_prefix("places-editor --agent status ")
        .map(str::to_owned)
        .ok_or_else(|| "Missing test session command".into())
}

fn qa_reply_while_visible(
    session: &mut Session,
    scope: &SceneStamp,
    canvas: &mut WindowCanvas,
    events: &mut EventPump,
    code: &str,
) -> Result<(), String> {
    let _poll = session.tick(Some(scope), qa_frame(canvas)?);
    let query = ["status".into(), code.into()];
    let request = std::thread::spawn(move || {
        client(&query)
            .is_ok_and(|reply| reply == "Places Editor agent session is active (status only).")
    });
    let started = Instant::now();
    let mut replied = false;
    while started.elapsed() < Duration::from_secs(2) && !request.is_finished() {
        events.pump_events();
        replied |= matches!(session.tick(Some(scope), qa_frame(canvas)?), Tick::Read);
        std::thread::sleep(Duration::from_millis(5));
    }
    let ready = request.join().unwrap_or(false);
    if !replied || !ready {
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
fn qa_exercise_session(
    session: &mut Session,
    canvas: &mut WindowCanvas,
    events: &mut EventPump,
    cover: &mut Window,
    (x, y, adjacent): (i32, i32, i32),
) -> Result<(), String> {
    use sdl3::video::WindowPos::Positioned;

    let scope = SceneStamp::new();
    assert!(matches!(
        session.tick(Some(&scope), qa_frame(canvas)?),
        Tick::Idle
    ));
    let first = qa_code(session, &scope)?;
    assert_eq!(session.label(), Some("Take Over"));
    qa_reply_while_visible(session, &scope, canvas, events, &first)?;
    println!("NATIVE_QA_VISIBLE=ready");

    if !cover.set_position(
        Positioned(x.saturating_sub(12)),
        Positioned(y.saturating_sub(12)),
    ) {
        return Err("Could not place the disposable cover".into());
    }
    qa_wait_for_visibility(session, events, Err(VisibilityDenial::Covered))?;
    assert!(matches!(
        session.tick(Some(&scope), qa_frame(canvas)?),
        Tick::Paused
    ));
    assert_eq!(session.label(), Some("Resume Agent"));
    assert!(qa_denied(&first));
    assert!(
        qa_code(session, &scope).is_err(),
        "a covered viewport cannot grant"
    );
    println!("NATIVE_QA_COVERED=revoked");

    if !cover.set_position(Positioned(adjacent), Positioned(y.saturating_sub(12))) {
        return Err("Could not return the cover beside the viewport".into());
    }
    qa_wait_for_visibility(session, events, Ok(()))?;
    assert!(matches!(
        session.tick(Some(&scope), qa_frame(canvas)?),
        Tick::Idle
    ));
    assert_eq!(session.label(), Some("Resume Agent"));
    assert!(qa_denied(&first));
    println!("NATIVE_QA_UNCOVERED=still_revoked");

    let fresh = qa_code(session, &scope)?;
    assert!(fresh != first, "the test must receive a fresh session");
    assert_eq!(session.label(), Some("Take Over"));
    assert!(qa_denied(&first));
    qa_reply_while_visible(session, &scope, canvas, events, &fresh)?;
    println!("NATIVE_QA_EXPLICIT_RESUME=only_fresh_ready");

    assert!(session.pause(), "Take Over must retire the grant");
    assert_eq!(session.label(), Some("Resume Agent"));
    assert!(qa_denied(&fresh));
    println!("NATIVE_QA_TAKE_OVER=revoked");
    Ok(())
}
fn describe_qa_cover(monitor: &super::Monitor) -> &'static str {
    use super::{
        GW_HWNDNEXT, GetTopWindow, GetWindow, GetWindowRect, IsIconic, IsWindowVisible, RECT,
        client_rect, cloaked, null_mut,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let Some(client) = client_rect(monitor.window) else {
        return "NATIVE_MONITOR_BLOCKER=unknown";
    };
    // SAFETY: read-only desktop z-order traversal.
    let mut above = unsafe { GetTopWindow(null_mut()) };
    for _ in 0_u8..64 {
        if above.is_null() || above == monitor.window {
            break;
        }
        // SAFETY: read attributes of the current z-order entry, no references retained.
        let visible = unsafe { IsWindowVisible(above) } != 0_i32;
        // SAFETY: the same read-only z-order entry is still in scope.
        let minimized = unsafe { IsIconic(above) } != 0_i32;
        if visible && !minimized && cloaked(above) != Some(true) {
            let mut bounds = RECT::default();
            // SAFETY: bounds is valid output storage for this call.
            if unsafe { GetWindowRect(above, &raw mut bounds) } != 0_i32
                && bounds.left < client.right
                && client.left < bounds.right
                && bounds.top < client.bottom
                && client.top < bounds.bottom
            {
                let mut owner = 0;
                // SAFETY: only query the owner of the specific covering window.
                let _thread = unsafe { GetWindowThreadProcessId(above, &raw mut owner) };
                if owner != 0
                    && std::env::var("PLACES_QA_CONTROL_PROCESSES").is_ok_and(|pids| {
                        pids.split(',').any(|pid| pid.parse::<u32>() == Ok(owner))
                    })
                {
                    return "NATIVE_MONITOR_BLOCKER=desktop-control-helper";
                }
                return "NATIVE_MONITOR_BLOCKER=another-window";
            }
        }
        // SAFETY: read the next z-order entry without changing any window.
        above = unsafe { GetWindow(above, GW_HWNDNEXT) };
    }
    "NATIVE_MONITOR_BLOCKER=unknown"
}
