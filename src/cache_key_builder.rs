use crate::actions::cache::Entry as CacheEntry;
use crate::hasher::Blake3 as Blake3Hasher;
use crate::safe_encoding;
use std::collections::BTreeMap;
use std::hash::Hasher as _;

const CACHE_ENTRY_VERSION: &str = "5";

pub struct CacheKeyBuilder {
    name: String,
    hasher: Blake3Hasher,
    attributes: BTreeMap<String, String>,
}

impl CacheKeyBuilder {
    pub fn new(name: &str) -> CacheKeyBuilder {
        use std::hash::Hash as _;

        let mut hasher = Blake3Hasher::default();
        CACHE_ENTRY_VERSION.hash(&mut hasher);
        CacheKeyBuilder {
            name: name.into(),
            hasher,
            attributes: BTreeMap::new(),
        }
    }

    pub fn add_key_data<T: std::hash::Hash>(&mut self, data: &T) {
        data.hash(&mut self.hasher);
    }

    pub fn set_attribute(&mut self, name: &str, value: &str) {
        self.attributes.insert(name.into(), value.into());
    }

    pub fn set_attribute_nonce(&mut self, name: &str) {
        use crate::nonce;
        let nonce = nonce::build(8);
        let nonce = safe_encoding::encode(nonce);
        self.set_attribute(name, &nonce);
    }

    pub fn into_entry(self) -> CacheEntry {
        let id: [u8; 32] = self.hasher.inner().finalize().into();
        let id = &id[..8];
        let id = safe_encoding::encode(id);
        let restore_key = format!("Ferrous Actions: {} - id={}", self.name, id);
        let restore_key = restore_key.replace(',', ";");
        let mut save_key = restore_key.clone();
        if !self.attributes.is_empty() {
            save_key += " (";
            let mut first = true;
            for (attribute, value) in self.attributes {
                if first {
                    first = false;
                } else {
                    save_key += "; ";
                }
                save_key += &format!("{}={}", attribute, value);
            }
            save_key += ")";
        }
        let save_key = save_key.replace(',', ";");
        let mut result = CacheEntry::new(save_key.as_str());
        result.restore_key(restore_key);
        result
    }
}
