use std::sync::Arc;

/// An in-process document identity and raw-scene revision, never persisted.
/// The retained identity prevents an old handle from aliasing a reopened scene.
#[derive(Clone, Debug)]
pub(in crate::editor) struct SceneStamp {
    epoch: Arc<()>,
    revision: u64,
}

impl SceneStamp {
    pub(in crate::editor) fn new() -> Self {
        Self {
            epoch: Arc::new(()),
            revision: 0,
        }
    }

    pub(in crate::editor) fn advance(&mut self) {
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
        } else {
            *self = Self::new();
        }
    }

    pub(super) fn same_epoch(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.epoch, &other.epoch)
    }

    #[cfg(windows)]
    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(windows)]
    pub(in crate::editor) fn matches(&self, other: &Self) -> bool {
        self.same_epoch(other) && self.same_revision(other)
    }

    pub(super) const fn same_revision(&self, other: &Self) -> bool {
        self.revision == other.revision
    }
}
