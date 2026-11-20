//! Native identity for one open package on Windows.

use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle as _, RawHandle};

/// Win32 `FILE_ID_INFO`, including the full file identifier used on `ReFS`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub(super) struct FileId {
    volume_serial: u64,
    identifier: [u8; 16],
}

const FILE_ID_INFO_CLASS: i32 = 18;
const FILE_ID_INFO_SIZE: u32 = 24;
const _: () = assert!(
    std::mem::size_of::<FileId>() == 24,
    "FileId must match the Win32 FILE_ID_INFO layout",
);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        handle: RawHandle,
        class: i32,
        information: *mut std::ffi::c_void,
        size: u32,
    ) -> i32;
}

impl FileId {
    pub(super) fn from_file(file: &File) -> io::Result<Self> {
        let mut identity = Self {
            volume_serial: 0,
            identifier: [0; 16],
        };
        // SAFETY: The File keeps its handle open, and the initialized output
        // buffer has the layout and size required by the FileIdInfo class.
        let success = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FILE_ID_INFO_CLASS,
                (&raw mut identity).cast(),
                FILE_ID_INFO_SIZE,
            )
        };
        if success == 0_i32 {
            return Err(io::Error::last_os_error());
        }
        Ok(identity)
    }
}
