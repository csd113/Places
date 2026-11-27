//! One nonblocking, local-only message pipe per grant. No worker or backlog.

use std::{
    io,
    os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_MORE_DATA, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
        ERROR_PIPE_LISTENING, HANDLE, INVALID_HANDLE_VALUE,
    },
    Security::{
        ACL, ACL_REVISION, AddAccessAllowedAce,
        Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        GetTokenInformation, InitializeAcl, InitializeSecurityDescriptor, SECURITY_ATTRIBUTES,
        SECURITY_DESCRIPTOR, SetSecurityDescriptorDacl, TOKEN_GROUPS, TOKEN_QUERY, TokenLogonSid,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
        FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
        SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, SYNCHRONIZE, WriteFile,
    },
    System::{
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientSessionId,
            PIPE_NOWAIT, PIPE_READMODE_MESSAGE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE,
            SetNamedPipeHandleState,
        },
        SystemServices::SECURITY_DESCRIPTOR_REVISION,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

use super::{
    protocol::{self, Nonce, Reply},
    wire,
};

const PEER_LIFETIME: Duration = Duration::from_secs(2);
const CONNECT_RETRY: Duration = Duration::from_millis(500);
const ACCESS: u32 =
    FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE;

fn bytes<T>() -> u32 {
    u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX)
}

const fn number(error: u32) -> i32 {
    i32::from_ne_bytes(error.to_ne_bytes())
}

fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: this is a successful creation of a unique closeable Win32 handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub(super) fn nonce() -> io::Result<Nonce> {
    let mut value = [0; 16];
    // SAFETY: value is writable for precisely the indicated length.
    if unsafe {
        BCryptGenRandom(
            null_mut(),
            value.as_mut_ptr(),
            bytes::<Nonce>(),
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    } < 0_i32
    {
        return Err(io::Error::other("Windows random generator unavailable"));
    }
    Ok(value)
}

fn name(key: Nonce) -> Vec<u16> {
    format!(r"\\.\pipe\places-editor-v1-{}", protocol::encode(key))
        .encode_utf16()
        .chain([0])
        .collect()
}

fn logon_pipe(path: &[u16]) -> io::Result<OwnedHandle> {
    let mut token_handle = null_mut();
    // SAFETY: GetCurrentProcess returns a borrowed pseudo-handle.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: the output pointer is writable and process is the current process.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token_handle) } == 0_i32 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(token_handle)?;
    let mut group_buffer = [0_usize; 64];
    let mut read = 0;
    // SAFETY: aligned buffer and length are exact. On failure it is never viewed as TOKEN_GROUPS.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenLogonSid,
            group_buffer.as_mut_ptr().cast(),
            bytes::<[usize; 64]>(),
            &raw mut read,
        )
    } == 0_i32
        || read < bytes::<TOKEN_GROUPS>()
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful TokenLogonSid produced a TOKEN_GROUPS in this aligned live buffer.
    let groups = unsafe { &*group_buffer.as_ptr().cast::<TOKEN_GROUPS>() };
    let [group] = groups.Groups;
    if groups.GroupCount != 1 || group.Sid.is_null() {
        return Err(io::Error::other("No unique Windows logon SID"));
    }
    let mut acl_buffer = [0_usize; 32];
    let acl = acl_buffer.as_mut_ptr().cast::<ACL>();
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let security = (&raw mut descriptor).cast();
    // SAFETY: Windows initializes an ACL inside the exact aligned buffer.
    if unsafe { InitializeAcl(acl, bytes::<[usize; 32]>(), ACL_REVISION) } == 0_i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: initialized ACL and the kernel-supplied logon SID remain live.
    if unsafe { AddAccessAllowedAce(acl, ACL_REVISION, ACCESS, group.Sid) } == 0_i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: descriptor has the documented size/alignment.
    if unsafe { InitializeSecurityDescriptor(security, SECURITY_DESCRIPTOR_REVISION) } == 0_i32 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: Windows consumes this initialized descriptor and ACL synchronously.
    if unsafe { SetSecurityDescriptorDacl(security, 1, acl, 0) } == 0_i32 {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: bytes::<SECURITY_ATTRIBUTES>(),
        lpSecurityDescriptor: security,
        bInheritHandle: 0,
    };
    // SAFETY: path is locally generated and NUL terminated, and the descriptor
    // and its DACL are live for this synchronous create. Fail on name collision.
    owned(unsafe {
        CreateNamedPipeW(
            path.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            wire::MAX_REPLY.try_into().unwrap_or(8192),
            wire::MAX_PACKET.try_into().unwrap_or(4096),
            0,
            &raw const attributes,
        )
    })
}

fn read(handle: HANDLE, out: &mut [u8]) -> io::Result<usize> {
    let capacity = u32::try_from(out.len()).map_err(io::Error::other)?;
    let mut count = 0;
    // SAFETY: callers pass live synchronous, nonblocking pipe handles and writable buffers.
    if unsafe {
        ReadFile(
            handle,
            out.as_mut_ptr(),
            capacity,
            &raw mut count,
            null_mut(),
        )
    } == 0_i32
    {
        return Err(io::Error::last_os_error());
    }
    usize::try_from(count).map_err(io::Error::other)
}

fn write(handle: HANDLE, message: &[u8]) -> io::Result<()> {
    let length = u32::try_from(message.len()).map_err(io::Error::other)?;
    let mut written = 0;
    // SAFETY: callers pass live synchronous, nonblocking pipe handles and readable buffers.
    if unsafe {
        WriteFile(
            handle,
            message.as_ptr(),
            length,
            &raw mut written,
            null_mut(),
        )
    } == 0_i32
    {
        return Err(io::Error::last_os_error());
    }
    if written != length {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Editor pipe busy",
        ));
    }
    Ok(())
}

pub(super) enum Input {
    Message(Vec<u8>),
    Malformed,
}

pub(super) struct Pipe {
    handle: OwnedHandle,
    session: u32,
    peer_since: Option<Instant>,
    answered: bool,
}

impl Pipe {
    pub(super) fn open(nonce: Nonce, session: u32) -> io::Result<Self> {
        Ok(Self {
            handle: logon_pipe(&name(nonce))?,
            session,
            peer_since: None,
            answered: false,
        })
    }

    fn reset(&mut self) {
        // SAFETY: the live handle is this instance's server end. The peer may have closed.
        let _disconnected = unsafe { DisconnectNamedPipe(self.handle.as_raw_handle()) };
        self.peer_since = None;
        self.answered = false;
    }

    pub(super) fn poll(&mut self) -> Option<Input> {
        let pipe = self.handle.as_raw_handle();
        if self.peer_since.is_none() {
            // SAFETY: the server end is live and configured PIPE_NOWAIT.
            if unsafe { ConnectNamedPipe(pipe, null_mut()) } == 0_i32 {
                let error = io::Error::last_os_error().raw_os_error();
                if error != Some(number(ERROR_PIPE_CONNECTED)) {
                    if error != Some(number(ERROR_PIPE_LISTENING)) {
                        self.reset();
                    }
                    return None;
                }
            }
            let mut session = 0;
            // SAFETY: live server end and writable output for the authenticated local peer.
            if unsafe { GetNamedPipeClientSessionId(pipe, &raw mut session) } == 0_i32
                || session != self.session
            {
                self.reset();
                return None;
            }
            self.peer_since = Some(Instant::now());
        }
        if self
            .peer_since
            .is_some_and(|start| start.elapsed() >= PEER_LIFETIME)
        {
            self.reset();
            return None;
        }
        let mut packet = [0; wire::MAX_PACKET];
        match read(pipe, &mut packet) {
            Ok(bytes) if !self.answered => Some(
                packet
                    .get(..bytes)
                    .map_or(Input::Malformed, |message| Input::Message(message.to_vec())),
            ),
            Err(error) if error.raw_os_error() == Some(number(ERROR_NO_DATA)) => None,
            Err(error)
                if error.raw_os_error() == Some(number(ERROR_MORE_DATA)) && !self.answered =>
            {
                Some(Input::Malformed)
            }
            Ok(_) | Err(_) => {
                self.reset();
                None
            }
        }
    }

    pub(super) fn reply(&mut self, reply: Reply) -> bool {
        self.reply_packet(&reply.bytes())
    }

    pub(super) fn reply_packet(&mut self, reply: &[u8]) -> bool {
        if reply.len() > wire::MAX_REPLY || write(self.handle.as_raw_handle(), reply).is_err() {
            self.reset();
            return false;
        }
        // Don't disconnect until the client reads and closes or the deadline expires.
        self.answered = true;
        true
    }
}

pub(super) fn probe(nonce: Nonce) -> Result<&'static str, String> {
    let reply = exchange(nonce, &protocol::request(nonce), false)?;
    protocol::response(&reply).map_err(str::to_owned)
}

pub(super) fn call(nonce: Nonce, body: &[u8], mutating: bool) -> Result<String, String> {
    let packet = protocol::pilot_message(nonce, body);
    let reply = exchange(nonce, &packet, mutating)?;
    protocol::pilot_response(&reply).map_err(|error| uncertain(error, mutating))
}

fn uncertain(error: String, mutating: bool) -> String {
    if mutating {
        format!(
            "Apply outcome uncertain: {error}. Reconcile with the same request ID and digest before retrying."
        )
    } else {
        error
    }
}

fn exchange(nonce: Nonce, packet: &[u8], mutating: bool) -> Result<Vec<u8>, String> {
    let path = name(nonce);
    let connecting = Instant::now();
    let pipe = loop {
        // SAFETY: only this fixed-format local pipe name is accepted; no general path
        // and no server impersonation beyond identification are granted.
        match owned(unsafe {
            CreateFileW(
                path.as_ptr(),
                ACCESS,
                0,
                null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        }) {
            Ok(handle) => break handle,
            Err(error)
                if error.raw_os_error() == Some(number(ERROR_PIPE_BUSY))
                    && connecting.elapsed() < CONNECT_RETRY =>
            {
                // The one native pipe instance may still be retiring the previous
                // reply. No request bytes were sent, so this is safe even for Apply.
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("Editor session unavailable: {error}")),
        }
    };
    let mode = PIPE_READMODE_MESSAGE | PIPE_NOWAIT;
    // SAFETY: this is a live local client handle and mode is passed by value through a pointer.
    if unsafe { SetNamedPipeHandleState(pipe.as_raw_handle(), &raw const mode, null(), null()) }
        == 0_i32
    {
        return Err(format!(
            "Cannot configure editor pipe: {}",
            io::Error::last_os_error()
        ));
    }
    write(pipe.as_raw_handle(), packet).map_err(|error| uncertain(error.to_string(), mutating))?;
    let started = Instant::now();
    let mut buffer = [0; wire::MAX_REPLY];
    while started.elapsed() < PEER_LIFETIME {
        match read(pipe.as_raw_handle(), &mut buffer) {
            Ok(count) => {
                return Ok(buffer.get(..count).unwrap_or_default().to_vec());
            }
            Err(error) if error.raw_os_error() == Some(number(ERROR_NO_DATA)) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(uncertain(
                    format!("Editor session disconnected: {error}"),
                    mutating,
                ));
            }
        }
    }
    Err(uncertain("Editor session timed out".into(), mutating))
}
