//! Direct tiny tools exercise normal output, hard cancellation and bounded noisy
//! stdout/stderr. One subsecond cancellation replaces the audit's eight-second
//! stall; no real map/compiler/bake and no child is allowed to remain alive.
#[cfg(unix)]
#[test]
fn validity_children_are_bounded_cancelled_and_reaped() -> Result<(), String> {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    let cancel = AtomicBool::new(false);
    let mut normal = Command::new("/bin/sh");
    let _normal_args = normal.args(["-c", "printf '{\"current\":true}'"]);
    assert_eq!(
        super::verify_output(&mut normal, &cancel)?,
        b"{\"current\":true}"
    );
    let pid_path = std::env::temp_dir().join(format!("places-verifier-pid-{}", std::process::id()));
    let mut stalled = Command::new("/bin/sh");
    let _stalled_args = stalled
        .args(["-c", "echo $$ > \"$1\"; exec sleep 30", "verifier"])
        .arg(&pid_path);
    let started = Instant::now();
    std::thread::scope(|scope| -> Result<(), String> {
        let _cancel = scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(200));
            cancel.store(true, Ordering::Relaxed);
        });
        assert!(super::verify_output(&mut stalled, &cancel).is_err());
        Ok(())
    })?;
    assert!(started.elapsed() < Duration::from_secs(3));
    let pid = std::fs::read_to_string(&pid_path).map_err(|e| e.to_string())?;
    std::fs::remove_file(pid_path).map_err(|e| e.to_string())?;
    let status = Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!status.status.success(), "cancelled tool must be reaped");
    cancel.store(false, Ordering::Relaxed);
    let mut noisy = Command::new("/bin/sh");
    let _noisy_args = noisy.args(["-c", "exec yes excessive-json-output"]);
    let noise_started = Instant::now();
    assert!(super::verify_output(&mut noisy, &cancel).is_err_and(|e| e.contains("1 MiB")));
    assert!(noise_started.elapsed() < Duration::from_secs(3));
    let mut deadline = Command::new("/bin/sleep");
    let _deadline_args = deadline.arg("30");
    assert!(
        super::verify_output_with_timeout(&mut deadline, &cancel, Duration::from_millis(100))
            .is_err_and(|e| e.contains("deadline"))
    );
    let mut errors = Command::new("/bin/sh");
    let _error_args = errors.args([
        "-c",
        "i=0; while [ $i -lt 2000 ]; do echo excessive-error-output >&2; i=$((i+1)); done; exit 1",
    ]);
    assert!(super::verify_output(&mut errors, &cancel).is_err_and(|e| e.len() <= 300));
    Ok(())
}
