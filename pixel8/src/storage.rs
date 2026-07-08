//! Persistent key-value storage: the cart's save file.
//!
//! [`Context::storage_set`] and [`Context::storage_get`] keep small amounts
//! of data across runs — high scores, unlocked levels, settings. Keys are
//! `&str`; values are primitives, anything convertible into a
//! [`StorageValue`] (integers, floats, bools):
//!
//! ```ignore
//! fn update(&mut self, ctx: &mut Context) {
//!     if self.best.is_none() {
//!         // First frame: read the previous best score (if any).
//!         self.best = Some(ctx.storage_get("best").and_then(|v| v.as_i64()).unwrap_or(0));
//!     }
//!     if self.score > self.best.unwrap_or(0) {
//!         self.best = Some(self.score);
//!         let _ = ctx.storage_set("best", self.score);
//!     }
//! }
//! ```
//!
//! Everything is allocation-free — values are `Copy` and formatting uses a
//! stack buffer — so the whole API works in `#![no_std]` carts.
//!
//! Values are stored as JSON. Where the store lives is the console's
//! business: on the desktop console and player it is a JSON file in the
//! user's cache directory, keyed by the cart's name; the browser player
//! keeps it for the session. The whole store is capped at 128 KiB
//! serialized — a `storage_set` that would exceed it fails with
//! [`StorageFull`] and stores nothing.

use crate::{ffi, fmt::FmtBuf, Context};

impl Context {
    /// Persistently store `value` under `key`. Data survives restarts — the
    /// console keeps a per-cart save file — and is read back with
    /// [`Context::storage_get`]. Non-finite floats (NaN, infinities) have
    /// no JSON spelling and are stored as [`StorageValue::Null`].
    ///
    /// Errors with [`StorageFull`] — storing nothing — if the whole store
    /// would exceed its 128 KiB serialized cap.
    pub fn storage_set(
        &mut self,
        key: &str,
        value: impl Into<StorageValue>,
    ) -> Result<(), StorageFull> {
        let json = value.into().to_json_buf();
        let json = json.as_str();
        let ok = unsafe {
            ffi::storage_set(
                key.as_ptr(),
                key.len() as u32,
                json.as_ptr(),
                json.len() as u32,
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err(StorageFull)
        }
    }

    /// The value stored under `key`, or `None` — also for a value this
    /// primitives-only API cannot represent (e.g. a string hand-edited into
    /// the save file).
    pub fn storage_get(&self, key: &str) -> Option<StorageValue> {
        let mut buf = [0u8; JSON_CAP];
        let len = unsafe {
            ffi::storage_get(
                key.as_ptr(),
                key.len() as u32,
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        let len = usize::try_from(len).ok()?; // -1: no such key.
        let json = buf.get(..len)?; // Longer than the buffer: not a primitive.
        StorageValue::from_json(core::str::from_utf8(json).ok()?)
    }

    /// Remove `key` from the store; `true` if it existed.
    pub fn storage_remove(&mut self, key: &str) -> bool {
        unsafe { ffi::storage_remove(key.as_ptr(), key.len() as u32) != 0 }
    }

    /// Remove everything from the store.
    pub fn storage_clear(&mut self) {
        unsafe { ffi::storage_clear() }
    }

    /// Alias for [`Context::storage_set`], for PICO-8 fingers.
    pub fn dset(&mut self, key: &str, value: impl Into<StorageValue>) -> Result<(), StorageFull> {
        self.storage_set(key, value)
    }

    /// Alias for [`Context::storage_get`].
    pub fn dget(&self, key: &str) -> Option<StorageValue> {
        self.storage_get(key)
    }
}

/// A `storage_set` was rejected: the store would exceed its 128 KiB
/// serialized cap (see docs/LIMITS.md). Nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageFull;

/// A storage value: a JSON primitive.
///
/// Anything a cart wants to save converts into one of these via [`From`] —
/// `ctx.storage_set("lives", 3)` just works. Integers stay exact (`i64`);
/// floats are [`StorageValue::Float`]. Non-finite floats have no JSON
/// spelling and are stored as [`StorageValue::Null`]. Deliberately
/// primitives-only, so saving and loading never allocates and works in
/// `#![no_std]` carts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
}

impl StorageValue {
    pub fn is_null(&self) -> bool {
        *self == StorageValue::Null
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            StorageValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The exact integer, if this is [`StorageValue::Int`].
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            StorageValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The number as a float; integers convert.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            StorageValue::Int(i) => Some(*i as f64),
            StorageValue::Float(f) => Some(*f),
            _ => None,
        }
    }
}

impl From<()> for StorageValue {
    fn from((): ()) -> StorageValue {
        StorageValue::Null
    }
}

impl From<bool> for StorageValue {
    fn from(b: bool) -> StorageValue {
        StorageValue::Bool(b)
    }
}

macro_rules! value_from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for StorageValue {
            fn from(i: $t) -> StorageValue {
                StorageValue::Int(i as i64)
            }
        }
    )*};
}
value_from_int!(i8, i16, i32, i64, u8, u16, u32);

impl From<f32> for StorageValue {
    fn from(f: f32) -> StorageValue {
        StorageValue::Float(f as f64)
    }
}

impl From<f64> for StorageValue {
    fn from(f: f64) -> StorageValue {
        StorageValue::Float(f)
    }
}

impl<T: Into<StorageValue>> From<Option<T>> for StorageValue {
    fn from(o: Option<T>) -> StorageValue {
        o.map(Into::into).unwrap_or(StorageValue::Null)
    }
}

// ---------------------------------------------------------------------------
// The JSON codec: how a value crosses the ABI
// ---------------------------------------------------------------------------

impl StorageValue {
    /// The value as JSON text in a stack buffer (the form that crosses the
    /// ABI). Non-finite floats have no JSON spelling and serialize as `null`.
    fn to_json_buf(self) -> FmtBuf<JSON_CAP> {
        use crate::fmt::format_args_to_buf as buf;
        match self {
            StorageValue::Null => buf(format_args!("null")),
            StorageValue::Bool(b) => buf(format_args!("{b}")),
            StorageValue::Int(i) => buf(format_args!("{i}")),
            // Debug, not Display: both are the shortest round-trip decimal,
            // but Debug keeps whole values recognizably float ("2.0") and
            // uses exponent form for extremes ("5e-324") — so the host's
            // JSON layer keeps the number an f64 and the Float variant
            // survives a round trip. Display's "2" would come back as an
            // integer, and its digit-by-digit expansion of huge values is
            // rejected outright.
            StorageValue::Float(f) if f.is_finite() => buf(format_args!("{f:?}")),
            StorageValue::Float(_) => buf(format_args!("null")),
        }
    }

    /// Parse JSON text into a primitive value; `None` on malformed input or
    /// on a non-primitive (string, array, object).
    fn from_json(text: &str) -> Option<StorageValue> {
        let text = text.trim();
        match text {
            "null" => Some(StorageValue::Null),
            "true" => Some(StorageValue::Bool(true)),
            "false" => Some(StorageValue::Bool(false)),
            _ => {
                if text.is_empty()
                    || !text
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                {
                    return None;
                }
                if !text.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) {
                    if let Ok(i) = text.parse::<i64>() {
                        return Some(StorageValue::Int(i));
                    }
                }
                // Fractional, exponent-form, or an integer too big for i64.
                text.parse::<f64>().ok().map(StorageValue::Float)
            }
        }
    }
}

/// JSON text buffer, both directions: `null`, a bool, an `i64`/`u64`, or a
/// shortest-round-trip `f64` (exponent form included) all fit well within
/// 32 bytes — ours going out (`Debug` float formatting) and the host's
/// canonical form coming back (itoa/ryu). A longer stored value cannot be a
/// primitive.
const JSON_CAP: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: StorageValue) -> StorageValue {
        StorageValue::from_json(v.to_json_buf().as_str()).expect("own output must parse")
    }

    #[test]
    fn conversions_cover_the_primitives() {
        assert_eq!(StorageValue::from(()), StorageValue::Null);
        assert_eq!(StorageValue::from(true), StorageValue::Bool(true));
        assert_eq!(StorageValue::from(42u8), StorageValue::Int(42));
        assert_eq!(StorageValue::from(-7i64), StorageValue::Int(-7));
        assert_eq!(StorageValue::from(2.5f32), StorageValue::Float(2.5));
        assert_eq!(StorageValue::from(2.5f64), StorageValue::Float(2.5));
        assert_eq!(StorageValue::from(None::<i32>), StorageValue::Null);
        assert_eq!(StorageValue::from(Some(3)), StorageValue::Int(3));
    }

    #[test]
    fn accessors_pick_the_right_variant() {
        assert!(StorageValue::Null.is_null());
        assert_eq!(StorageValue::Bool(true).as_bool(), Some(true));
        assert_eq!(StorageValue::Int(5).as_i64(), Some(5));
        assert_eq!(StorageValue::Int(5).as_f64(), Some(5.0));
        assert_eq!(StorageValue::Float(2.5).as_f64(), Some(2.5));
        assert_eq!(StorageValue::Float(2.5).as_i64(), None);
        assert_eq!(StorageValue::Bool(true).as_i64(), None);
    }

    #[test]
    fn primitives_roundtrip_through_json() {
        for v in [
            StorageValue::Null,
            StorageValue::Bool(false),
            StorageValue::Bool(true),
            StorageValue::Int(0),
            StorageValue::Int(i64::MAX),
            StorageValue::Int(i64::MIN),
            StorageValue::Float(2.5),
            StorageValue::Float(-0.125),
            // Whole-number floats must stay floats (the Display-vs-Debug trap).
            StorageValue::Float(2.0),
            StorageValue::Float(-0.0),
            StorageValue::Float(1e16),
            StorageValue::Float(f64::MAX),
            StorageValue::Float(5e-324), // Smallest subnormal.
        ] {
            assert_eq!(roundtrip(v), v);
        }
    }

    #[test]
    fn serialized_form_is_plain_json() {
        assert_eq!(StorageValue::Null.to_json_buf().as_str(), "null");
        assert_eq!(StorageValue::Bool(true).to_json_buf().as_str(), "true");
        assert_eq!(StorageValue::Int(-42).to_json_buf().as_str(), "-42");
        assert_eq!(StorageValue::Float(2.5).to_json_buf().as_str(), "2.5");
        // A whole-number float keeps its decimal point, so it stays a float
        // across the host's JSON layer.
        assert_eq!(StorageValue::Float(2.0).to_json_buf().as_str(), "2.0");
    }

    #[test]
    fn non_finite_floats_serialize_as_null() {
        assert_eq!(StorageValue::Float(f64::NAN).to_json_buf().as_str(), "null");
        assert_eq!(
            StorageValue::Float(f64::INFINITY).to_json_buf().as_str(),
            "null"
        );
    }

    #[test]
    fn numbers_parse_as_int_or_float() {
        assert_eq!(StorageValue::from_json("42"), Some(StorageValue::Int(42)));
        assert_eq!(StorageValue::from_json(" -3 "), Some(StorageValue::Int(-3)));
        assert_eq!(
            StorageValue::from_json("2.5"),
            Some(StorageValue::Float(2.5))
        );
        // The host's canonical floats may use exponent form (ryu), with an
        // explicit '+' on positive exponents.
        assert_eq!(
            StorageValue::from_json("1e3"),
            Some(StorageValue::Float(1000.0))
        );
        assert_eq!(
            StorageValue::from_json("1e+16"),
            Some(StorageValue::Float(1e16))
        );
        assert_eq!(
            StorageValue::from_json("5e-324"),
            Some(StorageValue::Float(5e-324))
        );
        // An integer beyond i64 falls back to float rather than failing.
        assert_eq!(
            StorageValue::from_json("18446744073709551615"),
            Some(StorageValue::Float(1.8446744073709552e19))
        );
    }

    #[test]
    fn non_primitives_and_malformed_json_are_rejected() {
        for bad in [
            "",
            "nul",
            "truth",
            "1..2",
            "--1",
            "1e",
            "42 tail", // Malformed.
            "\"hi\"",
            "[1,2]",
            "{\"a\":1}", // Valid JSON, but not a primitive.
        ] {
            assert_eq!(StorageValue::from_json(bad), None, "{bad:?} must not parse");
        }
    }

    /// The SDK codec and the host codec are two independent JSON
    /// implementations that only ever meet at runtime; this pins them to
    /// each other. serde_json is a dev-dependency only — carts still build
    /// against a zero-dependency SDK.
    #[test]
    fn codec_agrees_with_the_host_codec() {
        for v in [
            StorageValue::Null,
            StorageValue::Bool(true),
            StorageValue::Bool(false),
            StorageValue::Int(0),
            StorageValue::Int(i64::MAX),
            StorageValue::Int(i64::MIN),
            StorageValue::Float(2.5),
            StorageValue::Float(2.0),
            StorageValue::Float(-0.0),
            StorageValue::Float(0.1),
            StorageValue::Float(1e16),
            StorageValue::Float(f64::MAX),
            StorageValue::Float(f64::MIN),
            StorageValue::Float(5e-324),
        ] {
            // Out: our JSON must parse on the host side...
            let ours = v.to_json_buf();
            let host: serde_json::Value = serde_json::from_str(ours.as_str())
                .unwrap_or_else(|e| panic!("host rejected {v:?} as {:?}: {e}", ours.as_str()));
            // ...without changing the number's type (Float stays f64)...
            if matches!(v, StorageValue::Float(_)) {
                assert!(host.is_f64(), "{v:?} became non-float {host}");
            }
            // ...and back: the host's canonical re-serialization (what
            // storage_get returns) must parse to the same value, within the
            // read buffer.
            let canon = serde_json::to_string(&host).unwrap();
            assert!(canon.len() <= JSON_CAP, "{v:?} canon {canon:?} overflows");
            assert_eq!(StorageValue::from_json(&canon), Some(v), "via {canon:?}");
        }
    }

    #[test]
    fn context_methods_forward_to_the_stubs() {
        // Native stubs: an empty store that accepts writes — the methods
        // compile, run, and report "nothing stored".
        let mut ctx = Context { _private: () };
        assert_eq!(ctx.storage_set("best", 42), Ok(()));
        assert_eq!(ctx.storage_set("ratio", 0.5), Ok(()));
        assert_eq!(ctx.storage_get("best"), None);
        assert_eq!(ctx.dset("best", 42), Ok(()));
        assert_eq!(ctx.dget("best"), None);
        assert!(!ctx.storage_remove("best"));
        ctx.storage_clear();
    }
}
