//! Suspend before assigning a private kill-on-close job, then resume the initial
//! thread. No descendant can start before the operation owns its job membership.
use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct BasicLimits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    min_working_set: usize,
    max_working_set: usize,
    active_processes: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
#[derive(Default)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[repr(C)]
#[derive(Default)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    id: u32,
    owner: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, size: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, code: u32) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn OpenThread(access: u32, inherit: i32, id: u32) -> Handle;
    fn ResumeThread(thread: Handle) -> u32;
    fn PeekNamedPipe(
        pipe: Handle,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        remaining: *mut u32,
    ) -> i32;
}

pub(super) trait Pipe: std::io::Read + AsRawHandle {}
impl<R: std::io::Read + AsRawHandle> Pipe for R {}

pub(super) struct Containment(OwnedHandle);

pub(super) fn configure(command: &mut Command) {
    let _suspended = command.creation_flags(0x0000_0004); // CREATE_SUSPENDED
}

fn owned(handle: Handle) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == std::ptr::without_provenance_mut::<c_void>(usize::MAX) {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful Win32 creation/open returned a unique closeable handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

impl Containment {
    pub(super) fn new(child: &Child) -> io::Result<Self> {
        // SAFETY: null security/name create a private non-inheritable job.
        let job = owned(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
        let limits = ExtendedLimits {
            basic: BasicLimits {
                flags: 0x0000_2000,
                ..BasicLimits::default()
            },
            ..ExtendedLimits::default()
        };
        let size =
            u32::try_from(std::mem::size_of::<ExtendedLimits>()).map_err(io::Error::other)?;
        // SAFETY: job is live; limits is the exact repr(C) class-9 structure.
        if unsafe {
            SetInformationJobObject(job.as_raw_handle(), 9, (&raw const limits).cast(), size)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the owned child is still suspended and cannot spawn descendants.
        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = Self(job);
        resume(child.id())?;
        Ok(result)
    }

    pub(super) fn exited(&self, child: &mut Child) -> io::Result<bool> {
        child.try_wait().map(|status| status.is_some())
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        // SAFETY: this unique job contains only this operation and its descendants.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn resume(process: u32) -> io::Result<()> {
    // SAFETY: TH32CS_SNAPTHREAD requests an owned read-only thread snapshot.
    let snapshot = owned(unsafe { CreateToolhelp32Snapshot(0x0000_0004, 0) })?;
    let mut entry = ThreadEntry {
        size: u32::try_from(std::mem::size_of::<ThreadEntry>()).map_err(io::Error::other)?,
        ..ThreadEntry::default()
    };
    // SAFETY: entry has the required size and exclusive storage; snapshot is live.
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &raw mut entry) };
    while found != 0 {
        if entry.owner == process {
            // SAFETY: the still-owned suspended child anchors this thread id.
            let thread = owned(unsafe { OpenThread(0x0002, 0, entry.id) })?;
            // SAFETY: the initial thread belongs to our child and has suspend count 1.
            if unsafe { ResumeThread(thread.as_raw_handle()) } != 1 {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
        // SAFETY: snapshot and correctly sized entry remain valid and exclusive.
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &raw mut entry) };
    }
    Err(io::Error::other(
        "suspended tool initial thread was not found",
    ))
}

pub(super) const fn configure_pipe(_stream: &impl AsRawHandle) -> io::Result<()> {
    Ok(())
}

pub(super) fn read(
    stream: &mut (impl std::io::Read + AsRawHandle),
    buffer: &mut [u8],
    stop: &AtomicBool,
) -> io::Result<usize> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(0);
        }
        let mut available = 0;
        // SAFETY: the pipe is live and has exactly one reader. Peek copies no
        // data; a following read of at most available bytes cannot wait on a writer.
        let result = unsafe {
            PeekNamedPipe(
                stream.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &raw mut available,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(109) {
                Ok(0)
            } else {
                Err(error)
            };
        }
        let count = buffer
            .len()
            .min(usize::try_from(available).map_err(io::Error::other)?);
        if count > 0 {
            return stream.read(buffer.get_mut(..count).unwrap_or_default());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
