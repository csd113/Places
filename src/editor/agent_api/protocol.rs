//! The original status probe and the bounded pilot share one private channel.

pub(super) const REQUEST_LEN: usize = 22;
pub(super) const RESPONSE_LEN: usize = 6;
pub(super) type Nonce = [u8; 16];

const MAGIC: [u8; 4] = *b"PLEA";
const VERSION: u8 = 1;
const STATUS: u8 = 1;
const PILOT: u8 = 16;

pub(super) fn pilot_message(nonce: Nonce, body: &[u8]) -> Vec<u8> {
    let mut message = request(nonce).to_vec();
    if let Some(op) = message.get_mut(5) {
        *op = PILOT;
    }
    message.extend_from_slice(body);
    message
}

pub(super) fn pilot_call(message: &[u8], nonce: Nonce) -> Option<Result<super::wire::Call, Reply>> {
    let [a, b, c, d, version, PILOT, credential_and_body @ ..] = message else {
        return None;
    };
    let result = if [*a, *b, *c, *d] != MAGIC {
        Err(Reply::BadMessage)
    } else if *version != VERSION {
        Err(Reply::UnsupportedVersion)
    } else if let Some((credential, body)) = credential_and_body.split_at_checked(nonce.len()) {
        if credential == nonce {
            serde_json::from_slice(body).map_err(|_error| Reply::BadMessage)
        } else {
            Err(Reply::StaleSession)
        }
    } else {
        Err(Reply::BadMessage)
    };
    Some(result)
}

pub(super) fn pilot_reply(value: &serde_json::Value) -> Result<Vec<u8>, serde_json::Error> {
    let mut message = MAGIC.to_vec();
    message.extend([VERSION, PILOT]);
    serde_json::to_writer(&mut message, value)?;
    Ok(message)
}

pub(super) fn pilot_response(message: &[u8]) -> Result<String, String> {
    let [a, b, c, d, version, PILOT, body @ ..] = message else {
        return response(message)
            .map_err(str::to_owned)
            .and_then(|_| Err("Unexpected status reply".into()));
    };
    if [*a, *b, *c, *d] != MAGIC || *version != VERSION {
        return Err("Invalid editor reply".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|error| error.to_string())?;
    Ok(value.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Reply {
    Ready = 0,
    BadMessage = 1,
    UnsupportedVersion = 2,
    UnsupportedOperation = 3,
    StaleSession = 4,
}

impl Reply {
    pub(super) const fn bytes(self) -> [u8; RESPONSE_LEN] {
        let [a, b, c, d] = MAGIC;
        let code = match self {
            Self::Ready => 0,
            Self::BadMessage => 1,
            Self::UnsupportedVersion => 2,
            Self::UnsupportedOperation => 3,
            Self::StaleSession => 4,
        };
        [a, b, c, d, VERSION, code]
    }
}

pub(super) fn request(nonce: Nonce) -> [u8; REQUEST_LEN] {
    let mut message = [0; REQUEST_LEN];
    for (dst, src) in message
        .iter_mut()
        .zip(MAGIC.into_iter().chain([VERSION, STATUS]).chain(nonce))
    {
        *dst = src;
    }
    message
}

pub(super) fn dispatch(message: &[u8], nonce: Nonce) -> Reply {
    let [a, b, c, d, version, op, credential @ ..] = message else {
        return Reply::BadMessage;
    };
    if [*a, *b, *c, *d] != MAGIC || credential.len() != nonce.len() {
        return Reply::BadMessage;
    }
    if *version != VERSION {
        return Reply::UnsupportedVersion;
    }
    if credential != nonce {
        return Reply::StaleSession;
    }
    if *op != STATUS {
        return Reply::UnsupportedOperation;
    }
    Reply::Ready
}

pub(super) fn response(message: &[u8]) -> Result<&'static str, &'static str> {
    let [a, b, c, d, version, code] = message else {
        return Err("Invalid editor reply");
    };
    if [*a, *b, *c, *d] != MAGIC || *version != VERSION {
        return Err("Invalid editor reply");
    }
    match *code {
        0 => Ok("Places Editor agent session is active."),
        1 => Err("Editor rejected a malformed request"),
        2 => Err("Editor does not support this protocol version"),
        3 => Err("Editor does not support this operation"),
        4 => Err("Editor rejected a stale session"),
        _ => Err("Invalid editor reply"),
    }
}

pub(super) fn encode(nonce: Nonce) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(32);
    for byte in nonce {
        let _written = write!(result, "{byte:02x}");
    }
    result
}

pub(super) fn decode(value: &str) -> Option<Nonce> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut nonce = [0; 16];
    for (dst, chunk) in nonce.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
        *dst = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(nonce)
}
