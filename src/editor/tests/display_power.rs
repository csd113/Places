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
