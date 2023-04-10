use crate::node;
use crate::node::path::Path;
use js_sys::JsString;
use std::convert::Into;
use wasm_bindgen::prelude::*;

const WORKSPACE_ENV_VAR: &str = "GITHUB_WORKSPACE";
const WORKSPACE_OVERRIDDEN_TAG: &str = "#WORKSPACE_OVERRIDEN";

// Actually getting caching to work cross platform is complicated. First of all,
// the action takes patterns not paths (which is unhelpful for apps that don't
// want to use globs), It also means that on Windows you're going to need to
// convert any paths to use forward slash as a separator.
//
// The cache action keys actions on patterns, but the key is simply the hash of
// the patterns passed in, without modification. This means that for cross
// platform caching, the patterns need to be for relative paths, not absolute
// ones. This in turn means that if your action needs to produce a consistent
// cache key for a path that's not at a consistent location relative to the CWD
// at the time of action invocation, you're going to need to change CWD.
//
// As a final complication, when files are archived, they are done so using
// paths which are specified relative to the GitHub workspace. So, even if you
// sit in a directory you want to restore things in, and generate the right
// relative paths to match a cache key, you might end up restoring to the wrong
// location (e.g. because $CARGO_HOME moved relative to the GitHub workspace).
//
// The issue with any sort of reusable file caching across OSes is that there
// needs to be some concept of a reference path or paths which are well defined
// on each platform and under which it is valid to cache and restore certain
// paths. GitHub actions chooses this to be $GITHUB_WORKSPACE. Unfortunately
// this is problematic for two reasons:
// - We have no guarantee that the path we want to cache (e.g. something in the
//   home directory) will remain at consistent path relative to
//   $GITHUB_WORKSPACE (or that is is on other OSes).
// - Patterns cannot contain `.` or `..`, meaning we cannot use the GitHub
//   workspace as our root location when we want to cache paths located in the
//   home directory.
//
// To work around this, we have the cache user specify a root path. `Entry` both
// changes CWD to that path and rewrites the supplied paths to be relative to
// the root path. In addition, it sets $GITHUB_WORKSPACE to this path too, which
// causes all files in the generated tarball to be specified relative to that
// location. This is a hack, but in general it means that we can reliably cache
// and restore paths to locations that may change across time.

/// Changes the current working directory and `$GITHUB_WORKSPACE` to a specified
/// path and changes it back when it is dropped. This enables us to:
/// - supply consistent relative paths (patterns rather) to the actions API
/// - avoid issues related to archive paths being encoded relative to
///   `$GITHUB_WORKSPACE`.
#[derive(Debug)]
struct ScopedWorkspace {
    original_cwd: Path,
    original_workspace: Option<String>,
}

impl ScopedWorkspace {
    /// Constructs a new `Entry` with the specified name which can be used to
    /// either save or restore files.
    pub fn new(new_cwd: &Path) -> Result<ScopedWorkspace, JsValue> {
        let original_cwd = node::process::cwd();
        let original_workspace = node::process::get_env().get(WORKSPACE_ENV_VAR).cloned();
        node::process::chdir(new_cwd)?;
        node::process::set_var(WORKSPACE_ENV_VAR, &new_cwd.to_string());
        Ok(ScopedWorkspace {
            original_cwd,
            original_workspace,
        })
    }
}

impl Drop for ScopedWorkspace {
    fn drop(&mut self) {
        if let Some(original_workspace) = self.original_workspace.as_deref() {
            node::process::set_var(WORKSPACE_ENV_VAR, original_workspace);
        } else {
            node::process::remove_var(WORKSPACE_ENV_VAR);
        }
        node::process::chdir(&self.original_cwd)
            .unwrap_or_else(|e| panic!("Unable to chdir back to original folder: {:?}", e));
    }
}

/// Saves and retrieves cache entries
pub struct Entry {
    key: JsString,
    paths: Vec<Path>,
    restore_keys: Vec<JsString>,
    cross_os_archive: bool,
    relative_to: Option<Path>,
}

impl Entry {
    pub fn new<K: Into<JsString>>(key: K) -> Entry {
        Entry {
            key: key.into(),
            paths: Vec::new(),
            restore_keys: Vec::new(),
            cross_os_archive: false,
            relative_to: None,
        }
    }

    /// Add the specified paths (not glob patterns) to be cached or restored
    pub fn paths<I: IntoIterator<Item = P>, P: Into<Path>>(&mut self, paths: I) -> &mut Entry {
        self.paths.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Add the specified path (not glob patterns) to be cached or restored
    pub fn path<P: Into<Path>>(&mut self, path: P) -> &mut Entry {
        self.paths(std::iter::once(path.into()))
    }

    /// Specifies a root path of the cache entry. This can be different on save
    /// and restore, but needs to be set to a path above all cache entry
    /// paths.
    ///
    /// This function is a Ferrous actions extension and not part of the GitHub
    /// Actions Toolkit API.
    pub fn root<P: Into<Path>>(&mut self, path: P) -> &mut Entry {
        self.relative_to = Some(path.into());
        self
    }

    /// Enables interaction between cache entries produced on Windows and other
    /// operating systems
    pub fn permit_sharing_with_windows(&mut self, allow: bool) -> &mut Entry {
        self.cross_os_archive = allow;
        self
    }

    /// Specify multiple restore keys
    pub fn restore_keys<I, K>(&mut self, restore_keys: I) -> &mut Entry
    where
        I: IntoIterator<Item = K>,
        K: Into<JsString>,
    {
        self.restore_keys.extend(restore_keys.into_iter().map(Into::into));
        self
    }

    /// Specifies a restore key.
    ///
    /// If the cache entry name fails to match, restore keys will be searched
    /// for and match so long as they form a prefix of the cache key.
    pub fn restore_key<K: Into<JsString>>(&mut self, restore_key: K) -> &mut Entry {
        self.restore_keys(std::iter::once(restore_key.into()))
    }

    /// Saves the cache entry and returns a numeric cache ID.
    pub async fn save(&self) -> Result<i64, JsValue> {
        let patterns = self.build_patterns();
        let result = {
            let _caching_scope = self.build_action_scope()?;
            ffi::save_cache(patterns, &self.key, None, self.cross_os_archive).await?
        };
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

    /// Saves the cache entry if either:
    /// - The name and restore keys do not match anything currently in the cache
    /// - A restore based on the restore keys match `old_restore_key`
    ///
    /// In other words, the cache entry is only saved if it is either completely
    /// new, or an update to the cache entry that was previously restored.
    ///
    /// This functionality is a Ferrous Actions extension and not part of the
    /// GitHub Actions Toolkit API.
    pub async fn save_if_update(&self, old_restore_key: Option<&str>) -> Result<Option<i64>, JsValue> {
        let new_restore_key = self.peek_restore().await?;
        if new_restore_key.is_none() || new_restore_key.as_deref() == old_restore_key {
            self.save().await.map(Some)
        } else {
            Ok(None)
        }
    }

    fn build_patterns(&self) -> Vec<JsString> {
        let cwd = node::process::cwd();
        let mut result = Vec::with_capacity(self.paths.len());
        for path in &self.paths {
            // Rewrite path to be relative if we have a root
            let path = if let Some(relative_to) = &self.relative_to {
                let absolute = cwd.join(path);
                absolute.relative_to(relative_to)
            } else {
                path.clone()
            };
            let pattern = Self::path_to_glob(&path);
            result.push(pattern.into());
        }
        if self.relative_to.is_some() {
            // If we are going to specify paths relative to some path that we also
            // override GITHUB_WORKSPACE to, we add a comment so it will get
            // incorporated into the path hash.
            result.push(WORKSPACE_OVERRIDDEN_TAG.into());
        }
        result
    }

    fn path_to_glob(path: &Path) -> String {
        let path = path.to_string();
        // This should be valid even for absolute paths on Windows
        let path = path.replace(node::path::separator().as_ref(), "/");
        // We do not escape ']' as it would close the character set
        let mut result = String::with_capacity(path.len());
        let is_windows = node::os::platform() == "windows";
        for (idx, c) in path.chars().enumerate() {
            match c {
                '*' | '?' | '#' | '~' | '[' => result.extend(['[', c, ']']),
                '!' if idx == 0 => {
                    // A leading ! inverts a pattern, but it cannot be escaped
                    // with a character class because it means a compliment there.
                    // We could backslash escape, but only on non-Windows platforms.
                    // For now we assume that files starting with ! are rare.
                    result.push('?');
                }
                '\\' if !is_windows => {
                    // The glob syntax is platform specific, because of course it is. Backslash is
                    // escape on Unix-like platforms, even in a character set. See
                    // `internal-pattern.ts`.
                    result.extend(['\\', c]);
                }
                _ => result.push(c),
            }
        }
        result
    }

    fn build_action_scope(&self) -> Result<Option<ScopedWorkspace>, JsValue> {
        self.relative_to.as_ref().map(ScopedWorkspace::new).transpose()
    }

    /// Attempts to restore the cache entry. If one was found, the key is
    /// returned.
    pub async fn restore(&self) -> Result<Option<String>, JsValue> {
        self.peek_or_restore(false).await
    }

    async fn peek_restore(&self) -> Result<Option<String>, JsValue> {
        self.peek_or_restore(true).await
    }

    pub async fn peek_or_restore(&self, peek: bool) -> Result<Option<String>, JsValue> {
        use js_sys::Object;

        let patterns = self.build_patterns();
        let options = {
            let options = js_sys::Map::new();
            options.set(&"lookupOnly".into(), &peek.into());
            Object::from_entries(&options).expect("Failed to convert options map to object")
        };
        let result = {
            let _caching_scope = self.build_action_scope()?;
            ffi::restore_cache(
                patterns,
                &self.key,
                self.restore_keys.clone(),
                Some(options),
                self.cross_os_archive,
            )
            .await?
        };
        Ok(result.dyn_ref::<JsString>().map(Into::into))
    }
}

/// Low-level bindings to the GitHub Actions Tookit "cache" API
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
}
