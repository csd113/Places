//! One channel message, one worker and no assets protect menu cache retirement
//! when replacement loads are canceled before the worker starts their callback.
use super::{Control, Job, Message, Worker};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[test]
fn canceled_before_start_still_processes_retirement() -> Result<(), String> {
    let retired = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&retired);
    let mut worker = Worker::spawn(move |(), control| {
        assert!(control.cancelled.load(Ordering::Relaxed));
        observed.store(true, Ordering::Release);
        Ok(None::<()>)
    })?;
    let control = Control::new();
    control.cancel();
    worker
        .sender
        .as_ref()
        .ok_or("missing worker sender")?
        .send(Message::Prepare(Job {
            id: 1,
            input: (),
            control,
        }))
        .map_err(|error| error.to_string())?;
    let done = worker
        .receiver
        .as_ref()
        .ok_or("missing worker receiver")?
        .recv_timeout(Duration::from_secs(1))
        .map_err(|error| error.to_string())?;
    assert!(done.outcome?.is_none());
    assert!(retired.load(Ordering::Acquire));
    worker.shutdown();
    worker
        .handle
        .take()
        .ok_or("missing worker handle")?
        .join()
        .map_err(|payload| crate::engine::logging::panic_message(payload.as_ref()).to_string())?;
    Ok(())
}

#[test]
fn canceling_replacement_preserves_pending_retirement() -> Result<(), String> {
    let release_requested = Arc::new(AtomicBool::new(false));
    let release_worker = Arc::clone(&release_requested);
    let retired = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&retired);
    let (started, start) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let mut first = true;
    let worker = Worker::spawn(move |_, _| {
        if first {
            first = false;
            // Hold a callback after its last retirement check. Menu cleanup
            // then queues behind it, followed by a canceled replacement load.
            started.send(()).map_err(|error| error.to_string())?;
            resume.recv().map_err(|error| error.to_string())?;
        } else if release_worker.swap(false, Ordering::AcqRel) {
            observed.store(true, Ordering::Release);
        }
        Ok(None)
    })?;
    let mut loader = super::Loader {
        worker,
        release_requested,
    };
    let _running = loader.worker.request(None)?;
    start
        .recv_timeout(Duration::from_secs(1))
        .map_err(|error| error.to_string())?;
    loader.release_retained()?;
    let _replacement = loader.worker.request(None)?;
    loader.cancel();
    release.send(()).map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !retired.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        drop(loader.poll());
        std::thread::yield_now();
    }
    assert!(retired.load(Ordering::Acquire));
    loader.shutdown();
    loader
        .worker
        .handle
        .take()
        .ok_or("missing worker handle")?
        .join()
        .map_err(|payload| crate::engine::logging::panic_message(payload.as_ref()).to_string())?;
    Ok(())
}
