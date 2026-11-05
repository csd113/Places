pub(super) use crate::engine::project::check_editable_source;

// Bound fragmented JSON before ProjectFolder materializes any raw/typed trees.
// Charge source/disk/typed storage and two JSON copies conservatively, including
// Vec growth and map nodes. Quoted punctuation cannot inflate structural counts.
// The shared parser still performs all syntax, precision and uniqueness checks.
pub(super) fn opening_charge(bytes: &[u8]) -> Result<usize, String> {
    let mut charge = bytes.len().saturating_mul(5);
    let mut containers = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match *byte {
            b'"' => quoted = true,
            b'[' | b'{' => {
                if containers.len() >= 128 {
                    return Err("Level JSON nesting exceeds editor admission limit".into());
                }
                if *byte == b'[' {
                    charge = charge.saturating_add(
                        8_usize.saturating_mul(std::mem::size_of::<serde_json::Value>()),
                    );
                }
                containers.push(*byte);
            }
            b']' | b'}' => {
                let _closed = containers.pop();
            }
            b',' if containers.last() == Some(&b'[') => {
                charge = charge.saturating_add(
                    4_usize.saturating_mul(std::mem::size_of::<serde_json::Value>()),
                );
            }
            b':' => {
                charge = charge.saturating_add(
                    2_usize.saturating_mul(
                        std::mem::size_of::<(String, serde_json::Value)>()
                            .saturating_add(4_usize.saturating_mul(std::mem::size_of::<usize>())),
                    ),
                );
            }
            _ => {}
        }
    }
    Ok(charge)
}
