//! Essential regression: cached history charges must follow the allocations
//! actually retained through trim, clone, undo/redo and redo invalidation.
//! Eighty tiny JSON revisions cost under 64 KiB and no assets, GPU or bake.
use super::{History, Revision, value_heap_charge};
use serde_json::json;

fn verify(history: &History) {
    let oracle = history.undo.iter().chain(&history.redo).fold(
        history
            .undo
            .capacity()
            .saturating_add(history.redo.capacity())
            .saturating_mul(std::mem::size_of::<Revision>()),
        |bytes, revision| bytes.saturating_add(value_heap_charge(&revision.value)),
    );
    assert_eq!(
        history.heap_charge(),
        oracle,
        "cached charges match retained trees and deque storage"
    );
}

#[test]
fn retained_history_charges_survive_trim_restore_and_redo_invalidation() {
    let mut history = History::default();
    for index in 0_u16..80 {
        let mut text = String::with_capacity(256);
        text.push_str("spare capacity survives edit but can shrink on restore clone");
        history.edit(json!({"index":index,"text":text}), 512);
        verify(&history);
    }
    assert_eq!(
        history.usage().0,
        64,
        "the existing revision limit is preserved"
    );
    let mut current = json!({"index":80_u16});
    for _ in 0_u8..16 {
        let Some((previous, _)) = history.restore(false, &current, 512) else {
            break;
        };
        current = previous;
        verify(&history);
    }
    for _ in 0_u8..8 {
        let Some((next, _)) = history.restore(true, &current, 512) else {
            break;
        };
        current = next;
        verify(&history);
    }
    history.edit(current, 512);
    assert!(!history.can_redo(), "a new edit invalidates redo");
    verify(&history);
}
