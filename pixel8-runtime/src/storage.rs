//! Persistent key-value cart storage: the save file.
//!
//! Carts reach this through four ABI imports (`storage_set` / `storage_get` /
//! `storage_remove` / `storage_clear`, see docs/ABI.md); values cross the
//! boundary as JSON text and live here as parsed [`serde_json::Value`]s.
//! Native frontends back the store with a JSON file under the user's cache
//! directory, keyed by cart name; the web player and headless `verify` keep
//! it in memory. The whole store serializes to at most 128 K — the same one
//! number as every other cart limit (docs/LIMITS.md).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Serialized-size cap for the whole store: 128 K, shared with the fuel,
/// memory and cart-size limits.
pub const MAX_BYTES: usize = crate::cart::MEMORY_CAP;

/// A cart's key-value store, plus its optional disk backing.
///
/// `Default` is a purely in-memory store (tests, the web player, headless
/// `verify`). Dropping a disk-backed store saves it, so every "cart stops"
/// path — stop, reload, reboot, console exit — persists without ceremony.
///
/// Two carts whose names sanitize to the same file stem share a save file,
/// and two live instances running the same cart each hold a full in-memory
/// copy that saves whole-store, last-complete-writer-wins — the name *is*
/// the save identity, by design.
#[derive(Default)]
pub struct Storage {
    map: serde_json::Map<String, Value>,
    /// Backing file; `None` keeps the store in memory only.
    path: Option<PathBuf>,
    dirty: bool,
}

impl Storage {
    /// The persistent store for a cart, loaded from (and saved to) a JSON
    /// file under the user's cache directory, keyed by the cart's name.
    /// Falls back to an in-memory store when no cache directory is
    /// discoverable. A missing, malformed, or wrong-version file starts
    /// empty rather than failing the cart.
    pub fn for_cart(name: &str) -> Storage {
        match storage_file(name) {
            Some(path) => Self::at_path(path),
            None => Storage::default(),
        }
    }

    /// Like [`Storage::for_cart`], but rooted at an explicit directory
    /// instead of the user's cache directory. Frontends and tests use this
    /// to keep saves out of (or hermetically inside) a chosen location.
    pub fn for_cart_in(root: &std::path::Path, name: &str) -> Storage {
        Self::at_path(root.join(format!("{}.json", sanitize_name(name))))
    }

    /// A store backed by an explicit file path (used by `for_cart` and by
    /// tests). The file need not exist yet. A missing, malformed,
    /// wrong-version, or over-cap file starts empty.
    pub fn at_path(path: PathBuf) -> Storage {
        let map = std::fs::read(&path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<StorageFile<serde_json::Map<String, Value>>>(&bytes).ok()
            })
            .filter(|f| f.version == FORMAT_VERSION)
            .map(|f| f.data)
            // Enforce the cap on load too (a hand-grown file), with the
            // same measure `set_json` uses — the compact serialization, not
            // the file's pretty-printed size.
            .filter(|map| serde_json::to_string(map).is_ok_and(|s| s.len() <= MAX_BYTES))
            .unwrap_or_default();
        Storage {
            map,
            path: Some(path),
            dirty: false,
        }
    }

    /// Store `json` (JSON text) under `key`. Returns `false` — storing
    /// nothing — when the text is not valid JSON or the store would exceed
    /// [`MAX_BYTES`] serialized; the cap rejects the write, it never
    /// clobbers old data.
    pub fn set_json(&mut self, key: &str, json: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return false;
        };
        let prev = self.map.insert(key.to_owned(), value);
        let size = serde_json::to_string(&self.map)
            .map(|s| s.len())
            .unwrap_or(usize::MAX);
        if size > MAX_BYTES {
            match prev {
                Some(v) => self.map.insert(key.to_owned(), v),
                None => self.map.remove(key),
            };
            return false;
        }
        self.dirty = true;
        true
    }

    /// The value under `key` as canonical JSON text, or `None`.
    pub fn get_json(&self, key: &str) -> Option<String> {
        let value = self.map.get(key)?;
        Some(serde_json::to_string(value).unwrap_or_default())
    }

    /// Remove `key`; `true` if it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        let existed = self.map.remove(key).is_some();
        self.dirty |= existed;
        existed
    }

    /// Remove every key.
    pub fn clear(&mut self) {
        if !self.map.is_empty() {
            self.map.clear();
            self.dirty = true;
        }
    }

    /// Write the store to its backing file if anything changed since the
    /// last save. In-memory stores are a no-op. The write goes through a
    /// per-process temp file + rename, so neither a crash mid-save nor
    /// another instance saving the same cart at the same moment can tear
    /// the file — concurrent saves land whole, last writer wins.
    pub fn save_if_dirty(&mut self) -> anyhow::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if !self.dirty {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = crate::wire::to_readable_json(&StorageFile {
            version: FORMAT_VERSION,
            data: &self.map,
        })?;
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        let _ = self.save_if_dirty();
    }
}

const FORMAT_VERSION: u32 = 1;

/// The on-disk shape: `{ "version": 1, "data": { ... } }`. The cart's keys
/// are nested under `data` (not flattened) so they can never collide with
/// the version header. Generic over the data field so saving can borrow
/// the live map instead of moving it out and back.
#[derive(Serialize, Deserialize)]
struct StorageFile<D> {
    version: u32,
    data: D,
}

/// `<cache dir>/pixel8/storage/<sanitized cart name>.json`, or `None` when
/// no cache directory is discoverable (the browser, bare environments).
fn storage_file(name: &str) -> Option<PathBuf> {
    Some(
        cache_dir()?
            .join("pixel8")
            .join("storage")
            .join(format!("{}.json", sanitize_name(name))),
    )
}

/// The platform cache directory, from environment variables alone so the
/// runtime stays dependency-free: `~/Library/Caches` on macOS,
/// `%LOCALAPPDATA%` on Windows, `$XDG_CACHE_HOME` or `~/.cache` elsewhere.
fn cache_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches"))
    } else if cfg!(windows) {
        Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".cache")))
    }
}

/// A cart name as a safe file stem: lowercased, runs of anything outside
/// `[a-z0-9_-]` collapsed to one `-`, empty falling back to `untitled`.
fn sanitize_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "untitled".into()
    } else {
        out.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pixel8_storage_{tag}_{}.json", std::process::id()))
    }

    #[test]
    fn set_get_remove_clear_roundtrip() {
        let mut s = Storage::default();
        assert!(s.set_json("score", "42"));
        assert!(s.set_json("name", "\"ada\""));
        assert!(s.set_json("pos", "[1,2]"));
        assert_eq!(s.get_json("score").as_deref(), Some("42"));
        assert_eq!(s.get_json("name").as_deref(), Some("\"ada\""));
        assert_eq!(s.get_json("pos").as_deref(), Some("[1,2]"));
        assert_eq!(s.get_json("missing"), None);
        assert!(s.remove("score"));
        assert!(!s.remove("score"));
        assert_eq!(s.get_json("score"), None);
        s.clear();
        assert_eq!(s.get_json("name"), None);
    }

    #[test]
    fn set_overwrites_and_canonicalizes() {
        let mut s = Storage::default();
        assert!(s.set_json("k", "1"));
        // Whitespace-laden input comes back as canonical JSON.
        assert!(s.set_json("k", "  {\"a\" : 1,  \"b\": [1, 2]}  "));
        assert_eq!(s.get_json("k").as_deref(), Some("{\"a\":1,\"b\":[1,2]}"));
    }

    #[test]
    fn invalid_json_is_rejected() {
        let mut s = Storage::default();
        assert!(!s.set_json("k", "not json"));
        assert!(!s.set_json("k", ""));
        assert_eq!(s.get_json("k"), None);
    }

    #[test]
    fn cap_rejects_without_clobbering() {
        let mut s = Storage::default();
        assert!(s.set_json("k", "\"small\""));
        // A single value bigger than the whole 128 K budget.
        let huge = format!("\"{}\"", "x".repeat(MAX_BYTES));
        assert!(!s.set_json("k", &huge));
        assert_eq!(s.get_json("k").as_deref(), Some("\"small\""));
        assert!(!s.set_json("fresh", &huge));
        assert_eq!(s.get_json("fresh"), None);
    }

    #[test]
    fn cap_boundary_is_exact_and_inclusive() {
        let mut s = Storage::default();
        // {"k":"xxx...x"} serializes to len(x-run) + 8 bytes of scaffolding,
        // so this lands on exactly MAX_BYTES: allowed.
        let exact = format!("\"{}\"", "x".repeat(MAX_BYTES - 8));
        assert!(s.set_json("k", &exact));
        // One byte more is rejected, and the exact-cap value survives.
        let over = format!("\"{}\"", "x".repeat(MAX_BYTES - 7));
        assert!(!s.set_json("k", &over));
        assert_eq!(s.get_json("k").as_deref(), Some(exact.as_str()));
    }

    #[test]
    fn saves_and_reloads_from_disk() {
        let path = temp_file("roundtrip");
        let _ = std::fs::remove_file(&path);
        {
            let mut s = Storage::at_path(path.clone());
            assert!(s.set_json("score", "42"));
            // Dropped here: saved.
        }
        let s = Storage::at_path(path.clone());
        assert_eq!(s.get_json("score").as_deref(), Some("42"));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn clean_store_does_not_touch_disk() {
        let path = temp_file("clean");
        let _ = std::fs::remove_file(&path);
        {
            let _s = Storage::at_path(path.clone());
        }
        assert!(!path.exists(), "nothing was set, nothing is written");
    }

    #[test]
    fn corrupt_or_wrong_version_file_starts_empty() {
        let path = temp_file("corrupt");
        std::fs::write(&path, "not json at all").unwrap();
        let s = Storage::at_path(path.clone());
        assert_eq!(s.get_json("anything"), None);
        std::fs::write(&path, r#"{"version": 999, "data": {"k": 1}}"#).unwrap();
        let s = Storage::at_path(path.clone());
        assert_eq!(s.get_json("k"), None);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn over_cap_file_starts_empty_but_the_cap_measures_compact_form() {
        let path = temp_file("overcap");
        // A hand-grown file whose *data* exceeds the cap starts empty, like
        // a corrupt one.
        let huge = format!(
            r#"{{"version": 1, "data": {{"k": "{}"}}}}"#,
            "x".repeat(MAX_BYTES)
        );
        std::fs::write(&path, huge).unwrap();
        let s = Storage::at_path(path.clone());
        assert_eq!(s.get_json("k"), None);
        drop(s); // Nothing was set: the oversized file is left alone.

        // But the measure is the compact serialization, not the on-disk
        // size: a store at exactly the cap saves as pretty JSON *larger*
        // than the cap on disk, and must still load.
        let exact = format!("\"{}\"", "x".repeat(MAX_BYTES - 8));
        {
            let mut s = Storage::at_path(path.clone());
            assert!(s.set_json("k", &exact));
        }
        assert!(std::fs::metadata(&path).unwrap().len() > MAX_BYTES as u64);
        let s = Storage::at_path(path.clone());
        assert_eq!(s.get_json("k").as_deref(), Some(exact.as_str()));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn for_cart_in_roots_the_save_under_the_given_dir() {
        let root = std::env::temp_dir().join(format!("pixel8_storage_root_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        {
            let mut s = Storage::for_cart_in(&root, "My Cool Game!");
            assert!(s.set_json("k", "1"));
        }
        let expected = root.join("my-cool-game.json");
        assert!(expected.exists(), "save lands under the injected root");
        let s = Storage::for_cart_in(&root, "My Cool Game!");
        assert_eq!(s.get_json("k").as_deref(), Some("1"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sanitize_names_for_files() {
        assert_eq!(sanitize_name("My Cool Game!"), "my-cool-game");
        assert_eq!(sanitize_name("platformer"), "platformer");
        assert_eq!(sanitize_name("snake_2"), "snake_2");
        assert_eq!(sanitize_name("  ...  "), "untitled");
        assert_eq!(sanitize_name(""), "untitled");
    }
}
