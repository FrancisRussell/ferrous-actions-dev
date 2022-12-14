use crate::node::path::Path;
use js_sys::JsString;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::Into;
use wasm_bindgen::prelude::*;

pub struct Entry {
    key: JsString,
    paths: Vec<JsString>,
    restore_keys: Vec<JsString>,
    cross_os_archive: bool,
}

impl Entry {
    pub fn new<K: Into<JsString>>(key: K) -> Entry {
        Entry {
            key: key.into(),
            paths: Vec::new(),
            restore_keys: Vec::new(),
            cross_os_archive: false,
        }
    }

    pub fn paths<I: IntoIterator<Item = P>, P: Borrow<Path>>(&mut self, paths: I) -> &mut Entry {
        self.paths.extend(paths.into_iter().map(|p| p.borrow().into()));
        self
    }

    pub fn path<P: Borrow<Path>>(&mut self, path: P) -> &mut Entry {
        self.paths(std::iter::once(path.borrow()))
    }

    pub fn permit_sharing_with_windows(&mut self, allow: bool) -> &mut Entry {
        self.cross_os_archive = allow;
        self
    }

    pub fn restore_keys<I, K>(&mut self, restore_keys: I) -> &mut Entry
    where
        I: IntoIterator<Item = K>,
        K: Into<JsString>,
    {
        self.restore_keys.extend(restore_keys.into_iter().map(Into::into));
        self
    }

    pub fn restore_key<K: Into<JsString>>(&mut self, restore_key: K) -> &mut Entry {
        self.restore_keys(std::iter::once(restore_key.into()))
    }

    pub async fn save(&self) -> Result<i64, JsValue> {
        use wasm_bindgen::JsCast;
        let result = ffi::save_cache(self.paths.clone(), &self.key, None, self.cross_os_archive).await?;
        let result = result
            .dyn_ref::<js_sys::Number>()
            .ok_or_else(|| JsError::new("saveCache didn't return a number"))
            .map(|n| {
                #[allow(clippy::cast_possible_truncation)]
                let id = n.value_of() as i64;
                id
            })?;
        Ok(result)
    }

    pub async fn save_if_update(&self, old_restore_key: Option<&str>) -> Result<Option<i64>, JsValue> {
        let new_restore_key = self.peek_restore().await?;
        if new_restore_key.is_none() || new_restore_key.as_deref() == old_restore_key {
            self.save().await.map(Some)
        } else {
            Ok(None)
        }
    }

    pub async fn restore(&self) -> Result<Option<String>, JsValue> {
        let result = ffi::restore_cache(
            self.paths.clone(),
            &self.key,
            self.restore_keys.clone(),
            None,
            self.cross_os_archive,
        )
        .await?;
        if result == JsValue::NULL || result == JsValue::UNDEFINED {
            Ok(None)
        } else {
            let result: JsString = result.into();
            Ok(Some(result.into()))
        }
    }

    async fn peek_restore(&self) -> Result<Option<String>, JsValue> {
        use js_sys::Object;

        let compression_method: JsString = ffi::get_compression_method().await?.into();
        let keys: Vec<JsString> = std::iter::once(&self.key)
            .chain(self.restore_keys.iter())
            .cloned()
            .collect();
        let paths = self.paths.clone();
        let options = {
            let options = js_sys::Map::new();
            options.set(&"compressionMethod".into(), &compression_method.into());
            options.set(&"enableCrossOsArchive".into(), &self.cross_os_archive.into());
            Object::from_entries(&options).expect("Failed to convert options map to object")
        };
        let result = ffi::get_cache_entry(keys, paths, Some(options)).await?;
        if result == JsValue::NULL || result == JsValue::UNDEFINED {
            Ok(None)
        } else {
            let result: Object = result.into();
            let entries = Object::entries(&result);
            let mut entries: HashMap<String, JsValue> = entries
                .iter()
                .map(Into::<js_sys::Array>::into)
                .map(|e| (e.get(0), e.get(1)))
                .map(|(k, v)| (Into::<JsString>::into(k), v))
                .map(|(k, v)| (Into::<String>::into(k), v))
                .collect();
            Ok(entries
                .remove("cacheKey")
                .map(Into::<JsString>::into)
                .map(Into::<String>::into))
        }
    }
}

pub mod ffi {
    use js_sys::{JsString, Object};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "@actions/cache")]
    extern "C" {
        #[wasm_bindgen(js_name = "saveCache", catch)]
        pub async fn save_cache(
            paths: Vec<JsString>,
            key: &JsString,
            upload_options: Option<Object>,
            cross_os_archive: bool,
        ) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(js_name = "restoreCache", catch)]
        pub async fn restore_cache(
            paths: Vec<JsString>,
            primary_key: &JsString,
            restore_keys: Vec<JsString>,
            download_options: Option<Object>,
            cross_os_archive: bool,
        ) -> Result<JsValue, JsValue>;
    }

    #[wasm_bindgen(module = "@actions/cache/lib/internal/cacheUtils")]
    extern "C" {
        #[wasm_bindgen(js_name = "getCompressionMethod", catch)]
        pub(super) async fn get_compression_method() -> Result<JsValue, JsValue>;
    }

    #[wasm_bindgen(module = "@actions/cache/lib/internal/cacheHttpClient")]
    extern "C" {
        #[wasm_bindgen(js_name = "getCacheEntry", catch)]
        pub(super) async fn get_cache_entry(
            keys: Vec<JsString>,
            paths: Vec<JsString>,
            options: Option<Object>,
        ) -> Result<JsValue, JsValue>;
    }
}
