//! Weak immutable image identity avoids repeated content hashing at commit.
use super::RawImage;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[derive(Clone, Default)]
pub(super) struct ModelImageKeys(HashMap<usize, (Weak<RawImage>, String)>);
impl ModelImageKeys {
    pub fn get(&self, image: &Arc<RawImage>) -> Option<String> {
        self.0
            .get(&Arc::as_ptr(image).addr())
            .filter(|(source, _)| {
                source
                    .upgrade()
                    .is_some_and(|live| Arc::ptr_eq(&live, image))
            })
            .map(|(_, key)| key.clone())
    }
    pub fn key(&mut self, image: &Arc<RawImage>) -> String {
        if let Some(key) = self.get(image) {
            return key;
        }
        let key = super::model_image_key(image);
        self.remember(Arc::downgrade(image), key.clone());
        key
    }
    pub fn remember(&mut self, source: Weak<RawImage>, key: String) {
        let _previous = self.0.insert(source.as_ptr().addr(), (source, key));
    }
    pub fn retain_live(&mut self) {
        self.0.retain(|_, (image, _)| image.strong_count() != 0);
    }
}

#[cfg(test)]
#[path = "../../../tests/model_image_keys.rs"]
mod tests;
