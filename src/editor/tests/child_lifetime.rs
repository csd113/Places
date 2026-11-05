//! Essential RLA-02 regression: a self-terminating six-second owned descendant
//! holds each pipe after its wrapper exits. Completion/cancel/drop must retire
//! its group, both readers and their capture in about one second, without map
//! baking or native UI. The redirected-output control protects tree ownership
//! independently of EOF. Numeric grace is advanced only in the unit fixture;
//! the actual eight-second policy is exercised by explicit native tooling.
#![cfg(unix)]
use super::{CANCEL_GRACE, CapturedChild, StopPolicy};
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait(process: &mut CapturedChild) -> Result<ExitStatus, String> {
    let start = Instant::now();
    loop {
        if let Some(status) = process.poll()? {
            return Ok(status);
        }
        if start.elapsed() > Duration::from_secs(4) {
            return Err("tool retained inherited output beyond bounded drain".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn no_process(pid: libc::pid_t) -> bool {
    let start = Instant::now();
    loop {
        // SAFETY: signal 0 only queries this fixture's recorded child pid.
        if unsafe { libc::kill(pid, 0) } == -1 {
            return true;
        }
        if start.elapsed() > Duration::from_secs(1) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn completed_wrappers_retire_stdout_stderr_and_redirected_descendants() -> Result<(), String> {
    for redirects in ["2>/dev/null", ">/dev/null", ">/dev/null 2>&1"] {
        let mut command = Command::new("/bin/sh");
        let _args = command.arg("-c").arg(format!(
            "sleep 6 {redirects} & printf '%s\\n' $!; printf 'kept diagnostic\\n' >&2; exit 0"
        ));
        let mut process = CapturedChild::spawn(&mut command, StopPolicy::Cooperative, false)?;
        let capture = Arc::downgrade(&process.capture);
        assert!(wait(&mut process)?.success());
        let state = process.capture();
        let pid = state
            .diagnostics
            .lines()
            .find_map(|line| line.parse::<libc::pid_t>().ok())
            .ok_or("descendant pid missing")?;
        assert!(state.diagnostics.contains("kept diagnostic"));
        assert!(state.protocol_error.is_none(), "{:?}", state.protocol_error);
        assert!(process.readers.is_empty(), "all readers must be joined");
        assert_eq!(
            Arc::strong_count(&process.capture),
            1,
            "reader captures must retire"
        );
        assert!(
            no_process(pid),
            "owned descendant {pid} survived completion"
        );
        drop(process);
        assert!(capture.upgrade().is_none());
    }
    Ok(())
}

#[test]
fn cancellation_and_drop_retire_owned_readers_without_waiting_on_ui() -> Result<(), String> {
    for policy in [StopPolicy::Cooperative, StopPolicy::Terminate] {
        let mut command = Command::new("/bin/sh");
        let _args = command
            .arg("-c")
            .arg("sleep 6 & printf '%s\\n' $!; read ignored; exit 3");
        let mut process = CapturedChild::spawn(&mut command, policy, false)?;
        process.stop();
        process.stopped = Instant::now().checked_sub(CANCEL_GRACE);
        assert!(!wait(&mut process)?.success());
        assert!(process.readers.is_empty());
        assert_eq!(Arc::strong_count(&process.capture), 1);
    }
    let mut command = Command::new("/bin/sh");
    let _args = command.arg("-c").arg("sleep 6 & wait");
    let process = CapturedChild::spawn(&mut command, StopPolicy::Terminate, false)?;
    let capture = Arc::downgrade(&process.capture);
    let start = Instant::now();
    drop(process);
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "Drop must not block rendered frames"
    );
    while capture.upgrade().is_some() && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        capture.upgrade().is_none(),
        "cleanup must join both readers and release captures"
    );
    Ok(())
}

#[test]
fn json_success_failure_and_verifier_deadline_use_the_same_lifetime_boundary() -> Result<(), String>
{
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut success = Command::new("/bin/sh");
    let _success_args = success
        .arg("-c")
        .arg("sleep 6 2>/dev/null & printf '{\"ok\":true}'");
    assert_eq!(
        super::super::verify_output_with_timeout(&mut success, &cancel, Duration::from_secs(3))?,
        b"{\"ok\":true}"
    );
    let mut failure = Command::new("/bin/sh");
    let _failure_args = failure
        .arg("-c")
        .arg("printf 'required failure diagnostic' >&2; exit 7");
    assert!(
        super::super::verify_output_with_timeout(&mut failure, &cancel, Duration::from_secs(3))
            .is_err_and(|error| error.contains("required failure diagnostic"))
    );
    let mut timeout = Command::new("/bin/sh");
    let _timeout_args = timeout.arg("-c").arg("sleep 6 & wait");
    let started = Instant::now();
    assert!(
        super::super::verify_output_with_timeout(&mut timeout, &cancel, Duration::from_millis(30))
            .is_err_and(|error| error.contains("deadline"))
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    Ok(())
}
