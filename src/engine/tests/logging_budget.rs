//! Protects process-wide diagnostic growth under hostile unique warning keys.
//! Two isolated local stores cost 4096 short strings and one 1MiB string;
//! no process-global state, files, assets or timing assertions.
use super::{MAX_ONCE_KEY_BYTES, MAX_ONCE_KEYS, OnceDecision, OnceWarnings};

#[test]
fn once_warning_identity_count_and_bytes_stop_without_evicting_old_keys() {
    let mut count = OnceWarnings::default();
    for index in 0..MAX_ONCE_KEYS {
        assert!(
            matches!(count.admit(&format!("key-{index}")), OnceDecision::First),
            "each in-budget identity is admitted once"
        );
    }
    let bytes = count.bytes;
    assert!(
        matches!(count.admit("key-0"), OnceDecision::Duplicate),
        "known keys require no allocation even at capacity"
    );
    assert_eq!(count.bytes, bytes, "duplicates cannot grow retained bytes");
    assert!(
        matches!(count.admit("overflow"), OnceDecision::Saturated),
        "pressure produces one explicit saturation notice"
    );
    assert!(
        matches!(count.admit("another"), OnceDecision::Duplicate),
        "pressure never evicts old keys or repeats the notice"
    );
    assert_eq!(count.keys.len(), MAX_ONCE_KEYS, "count remains bounded");
    let mut storage = OnceWarnings::default();
    let boundary = "x".repeat(MAX_ONCE_KEY_BYTES);
    assert!(
        matches!(storage.admit(&boundary), OnceDecision::First),
        "the exact byte budget is valid"
    );
    assert!(
        matches!(storage.admit("extra"), OnceDecision::Saturated),
        "one further byte is refused without retaining its key"
    );
    assert!(
        matches!(storage.admit(""), OnceDecision::Duplicate),
        "even a smaller fitting key remains suppressed after terminal saturation"
    );
    assert_eq!(
        storage.bytes, MAX_ONCE_KEY_BYTES,
        "byte pressure preserves its ceiling"
    );
}
