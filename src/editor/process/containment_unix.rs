//! Keep the group leader unreaped until its operation's group is terminated.
//! This anchors the process-group id even after a wrapper exits, so delayed
//! cleanup cannot target a later operation which reused the numeric pid.
use std::io;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(super) trait Pipe: std::io::Read + std::os::fd::AsRawFd {}
impl<R: std::io::Read + std::os::fd::AsRawFd> Pipe for R {}

pub(super) struct Containment {
    group: libc::pid_t,
}

pub(super) fn configure(command: &mut Command) {
    let _group = command.process_group(0);
}

impl Containment {
    pub(super) fn new(child: &Child) -> io::Result<Self> {
        Ok(Self {
            group: libc::pid_t::try_from(child.id()).map_err(io::Error::other)?,
        })
    }

    pub(super) fn exited(tree: &Self, _child: &mut Child) -> io::Result<bool> {
        tree.leader_exited()
    }

    fn leader_exited(&self) -> io::Result<bool> {
        // SAFETY: zero is a valid initialized siginfo_t; waitid writes only to
        // this exclusive value and WNOWAIT deliberately preserves our child.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let id = libc::id_t::try_from(self.group).map_err(io::Error::other)?;
        // SAFETY: id names the still-owned direct child, never a reaped pid.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                id,
                &raw mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1_i32 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: waitid initialized the child-event fields; zero means no event.
        Ok(unsafe { info.si_pid() } != 0)
    }

    pub(super) fn terminate(&self) -> io::Result<()> {
        // SAFETY: this private group was created for our unreaped child. Its
        // leader anchors its id until this final group signal, before wait().
        let result = unsafe { libc::kill(self.group.saturating_neg(), libc::SIGKILL) };
        if result == -1_i32 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                #[cfg(target_os = "macos")]
                if error.raw_os_error() == Some(libc::EPERM)
                    && self.leader_exited()?
                    && self.only_zombie_leader()
                {
                    // Darwin killpg filters zombies then returns EPERM if no
                    // signalable member remains. Never hide a live denied group.
                    return Ok(());
                }
                return Err(error);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn only_zombie_leader(&self) -> bool {
        #[link(name = "proc")]
        unsafe extern "C" {
            fn proc_listpgrppids(
                group: libc::pid_t,
                buffer: *mut libc::c_void,
                size: libc::c_int,
            ) -> libc::c_int;
        }
        let mut pids = [0_i32; 2];
        let Ok(size) = libc::c_int::try_from(std::mem::size_of_val(&pids)) else {
            return false;
        };
        // SAFETY: this bounded writable array holds two pid_t values. The
        // unreaped leader anchors the queried group. Exactly one returned pid,
        // equal to that known zombie, proves no live member can create children.
        let count = unsafe { proc_listpgrppids(self.group, pids.as_mut_ptr().cast(), size) };
        count == 1_i32 && pids.first() == Some(&self.group)
    }
}

pub(super) fn configure_pipe(stream: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let fd = stream.as_raw_fd();
    // SAFETY: fd is borrowed from a live exclusively owned child pipe; these
    // fcntl operations change only its read mode and preserve all existing flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1_i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is still exclusively owned; preserve its existing flags.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1_i32 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn read(
    stream: &mut (impl std::io::Read + std::os::fd::AsRawFd),
    buffer: &mut [u8],
    stop: &AtomicBool,
) -> io::Result<usize> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(0);
        }
        match stream.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}
