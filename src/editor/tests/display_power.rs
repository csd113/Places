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
fn native_monitor_verifies_adjacent_and_rejects_a_real_cover() -> Result<(), String> {
    use super::Monitor;
    use sdl3::video::WindowPos::Positioned;
    use std::time::{Duration, Instant};

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
    let mut window = video
        .window("Places QA viewport (no grants)", 480, 300)
        .position(x, y)
        .build()
        .map_err(|error| error.to_string())?;
    let monitor = Monitor::install(&window).ok_or("Cannot install the production monitor")?;
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    let mut wait_for = |expected| {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            events.pump_events();
            let observed = monitor.verified();
            if observed == expected {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(format!("Expected {expected:?}, observed {observed:?}"));
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    };

    if !window.raise() {
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
        if !window.set_position(Positioned(test_x), Positioned(test_y)) {
            continue;
        }
        match wait_for(Ok(())) {
            Ok(()) => {
                picked = Some((test_x, test_y, cover_x));
                break;
            }
            Err(reason) if monitor.verified() == Err(VisibilityDenial::Covered) => {
                println!("NATIVE_MONITOR_SLOT_BLOCKED {reason}");
                println!("{}", describe_qa_cover(&monitor));
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
    wait_for(Ok(()))?;
    println!("NATIVE_MONITOR_ADJACENT=verified");
    if !cover.set_position(Positioned(viewport_x - 12), Positioned(viewport_y - 12)) {
        return Err("Could not place the disposable cover".into());
    }
    wait_for(Err(VisibilityDenial::Covered))?;
    println!("NATIVE_MONITOR_OVERLAP=covered");
    if !cover.set_position(Positioned(cover_x), Positioned(viewport_y - 12)) {
        return Err("Could not return the cover beside the viewport".into());
    }
    wait_for(Ok(()))?;
    println!("NATIVE_MONITOR_UNCOVERED_AGAIN=verified");
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
