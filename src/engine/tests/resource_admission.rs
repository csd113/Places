//! Budget boundary/overflow and cancellation regressions. These small fixtures
//! protect concurrent admission and parser progress, beyond package-format tests;
//! no OS pressure, asset, GPU or map-bake dependency is required.
use super::{DecodeBudget, Reservation, enter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[test]
fn shared_reservations_refuse_overflow_and_release_on_drop() -> Result<(), String> {
    let budget = Arc::new(DecodeBudget {
        cancelled: Arc::new(AtomicBool::new(false)),
        decoded: AtomicU64::new(0),
        refused: AtomicBool::new(false),
        reserved: AtomicU64::new(0),
        limit: 1024,
    });
    budget.reserve(128, 896)?;
    let owner = Reservation {
        budget: Some(Arc::clone(&budget)),
        bytes: 896,
    };
    assert!(
        budget.reserve(128, 1).is_err(),
        "in-flight allocation counts against the same aggregate budget"
    );
    assert!(
        budget.reserve(u64::MAX, 1).is_err(),
        "overflow must refuse rather than wrap"
    );
    {
        let _scope = enter(Some(Arc::clone(&budget)));
        assert!(
            super::checkpoint().is_err(),
            "asset fallback cannot clear a budget refusal"
        );
    }
    drop(owner);
    assert_eq!(budget.reserved.load(Ordering::Acquire), 0);
    budget.reserve(1024, 0)?;
    Ok(())
}

#[test]
fn cancelled_binary_decode_does_not_consume_the_next_block() {
    let cancelled = Arc::new(AtomicBool::new(true));
    let _scope = enter(Some(DecodeBudget::new(cancelled)));
    let bytes = vec![0_u8; 65_536];
    let mut reader = crate::engine::package::binary::Reader::new(&bytes);
    assert!(reader.bytes(bytes.len()).is_err());
    assert_eq!(
        reader.remaining(),
        bytes.len(),
        "cancelled decoding preserves the cursor"
    );
}
