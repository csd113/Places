use std::collections::VecDeque;

use serde_json::Value;

const MAX_REVISIONS: usize = 64;
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;

/// A retained revision is immutable. Charge its actual allocation once, after
/// any clone, rather than traversing every historical JSON tree on each edit.
struct Revision {
    value: Value,
    serialized_bytes: usize,
    heap_bytes: usize,
}

impl Revision {
    fn new(value: Value, serialized_bytes: usize) -> Self {
        let heap_bytes = value_heap_charge(&value);
        Self {
            value,
            serialized_bytes,
            heap_bytes,
        }
    }
}

#[derive(Default)]
pub(super) struct History {
    undo: VecDeque<Revision>,
    redo: VecDeque<Revision>,
}

impl History {
    pub(super) fn heap_charge(&self) -> usize {
        self.undo.iter().chain(&self.redo).fold(
            self.undo
                .capacity()
                .saturating_add(self.redo.capacity())
                .saturating_mul(std::mem::size_of::<Revision>()),
            |bytes, revision| bytes.saturating_add(revision.heap_bytes),
        )
    }

    pub(super) fn usage(&self) -> (usize, usize) {
        self.undo
            .iter()
            .chain(&self.redo)
            .fold((0_usize, 0_usize), |(count, bytes), revision| {
                (
                    count.saturating_add(1),
                    bytes.saturating_add(revision.serialized_bytes),
                )
            })
    }

    pub(super) fn candidate(&self, redo: bool) -> Option<&Value> {
        let source = if redo { &self.redo } else { &self.undo };
        source.back().map(|revision| &revision.value)
    }

    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(super) fn edit(&mut self, previous: Value, byte_count: usize) {
        self.redo.clear();
        self.undo.push_back(Revision::new(previous, byte_count));
        trim(&mut self.undo);
    }

    pub(super) fn restore(
        &mut self,
        redo: bool,
        current: &Value,
        byte_count: usize,
    ) -> Option<(Value, usize)> {
        let (source, destination) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let revision = source.pop_back()?;
        destination.push_back(Revision::new(current.clone(), byte_count));
        trim(destination);
        Some((revision.value, revision.serialized_bytes))
    }
}

fn trim(revisions: &mut VecDeque<Revision>) {
    let mut total = revisions.iter().fold(0_usize, |sum, revision| {
        sum.saturating_add(revision.serialized_bytes)
    });
    while revisions.len() > MAX_REVISIONS || (total > MAX_HISTORY_BYTES && revisions.len() > 1) {
        if let Some(revision) = revisions.pop_front() {
            total = total.saturating_sub(revision.serialized_bytes);
        }
    }
}

/// Conservative owned JSON heap estimate, including strings, array capacity and
/// map-node overhead. Serialized text length alone misses heavily fragmented JSON.
pub(super) fn value_heap_charge(value: &Value) -> usize {
    match value {
        Value::String(text) => text.capacity(),
        Value::Array(values) => values
            .capacity()
            .saturating_mul(std::mem::size_of::<Value>())
            .saturating_add(
                values
                    .iter()
                    .map(value_heap_charge)
                    .fold(0_usize, usize::saturating_add),
            ),
        Value::Object(values) => values.iter().fold(0_usize, |bytes, (key, member)| {
            bytes
                .saturating_add(key.capacity())
                .saturating_add(value_heap_charge(member))
                .saturating_add(
                    std::mem::size_of::<(String, Value)>()
                        .saturating_add(4_usize.saturating_mul(std::mem::size_of::<usize>())),
                )
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

#[cfg(test)]
#[path = "../tests/history_charge.rs"]
mod tests;
