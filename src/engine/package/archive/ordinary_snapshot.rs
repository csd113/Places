//! Bounded anonymous snapshots of ordinary archives for compiler cache reads.
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Seek as _};
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::os::unix::{ffi::OsStrExt as _, fs::MetadataExt as _};
use std::path::{Component, Path};

use super::{MAX_TRANSPORT_BYTES, copy_bounded};

pub fn snapshot_ordinary_file(
    source: &Path,
    directory: &File,
    check: &impl Fn() -> Result<(), String>,
) -> Result<File, String> {
    check()?;
    let input = open_anchored(source, libc::O_RDONLY | libc::O_NONBLOCK)?;
    let metadata = input.metadata().map_err(|error| error.to_string())?;
    let length = metadata.len();
    if !metadata.is_file() || length > MAX_TRANSPORT_BYTES {
        return Err(
            "Sibling must be a regular file within the ordinary map transport limit".into(),
        );
    }

    let directory_metadata = directory.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid takes no arguments and returns the calling process identity.
    let owner = unsafe { libc::geteuid() };
    if directory_metadata.uid() != owner || directory_metadata.mode() & 0o077 != 0 {
        return Err("Sibling staging directory must be private and owned by the compiler".into());
    }
    let name = c"ordinary-snapshot";
    let mut snapshot = open_at(directory, name, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
    // SAFETY: the directory descriptor and C string are live. Unlink only
    // after exclusive creation in private staging, and keep the opened file.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0_i32) } != 0_i32 {
        return Err(io::Error::last_os_error().to_string());
    }
    let _copied = copy_bounded(input, &mut snapshot, length, MAX_TRANSPORT_BYTES, check)?;
    snapshot.rewind().map_err(|error| error.to_string())?;
    Ok(snapshot)
}

fn open_anchored(path: &Path, final_flags: libc::c_int) -> Result<File, String> {
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut parts = absolute.components().peekable();
    if parts.next() != Some(Component::RootDir) {
        return Err("Sibling path must start at the filesystem root".into());
    }
    let mut directory = File::open("/").map_err(|error| error.to_string())?;
    while let Some(Component::Normal(part)) = parts.next() {
        let name = CString::new(part.as_bytes()).map_err(|error| error.to_string())?;
        if parts.peek().is_none() {
            return open_at(&directory, &name, final_flags);
        }
        directory = open_at(&directory, &name, libc::O_RDONLY | libc::O_DIRECTORY)?;
    }
    Err("Sibling path must contain only ordinary path components".into())
}

fn open_at(directory: &File, name: &CStr, flags: libc::c_int) -> Result<File, String> {
    // SAFETY: the descriptor and C string are live, and the mode is passed
    // for exclusive file creation. No component can resolve through a link.
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            libc::c_int::from(0o600_u16),
        )
    };
    if descriptor < 0_i32 {
        return Err(io::Error::last_os_error().to_string());
    }
    // SAFETY: successful openat returned a newly owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}
