//! Preserve compilation on targets without native subprocess support. Tools
//! fail at their launch boundary instead of creating uncontained operations.
use std::io;
use std::process::{Child, Command};
use std::sync::atomic::AtomicBool;

pub(super) trait Pipe: io::Read {}
impl<R: io::Read> Pipe for R {}
pub(super) struct Containment;
pub(super) const fn configure(_command: &mut Command) {}
impl Containment {
    pub(super) fn new(_child: &Child) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native tool containment is unavailable on this target",
        ))
    }
    pub(super) fn exited(_tree: &Self, _child: &mut Child) -> io::Result<bool> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native tools are unavailable",
        ))
    }
    pub(super) fn terminate(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native tools are unavailable",
        ))
    }
}
pub(super) fn configure_pipe(_stream: &impl Pipe) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native pipes are unavailable",
    ))
}
pub(super) fn read(
    _stream: &mut impl Pipe,
    _buffer: &mut [u8],
    _stop: &AtomicBool,
) -> io::Result<usize> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native pipes are unavailable",
    ))
}
