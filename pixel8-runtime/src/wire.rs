//! Serde codecs that keep Pixel8's JSON human-readable: hex-nibble rows for the
//! 16-colour sprite/label grids, hex-byte rows for the 8-bit map, a flat hex
//! string for sprite flags, and base64 for the one purely-binary field (the
//! compiled wasm). Also `Versioned<T>`, the format-version envelope stamped onto
//! the readable text formats, and a width-based pretty-printer that keeps small
//! arrays (note quads, channels) inline while breaking big grids to one row per
//! line.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    assets::{MAP_W, SHEET_W},
    pico8::{bytes_to_hex, hex_bytes},
};

/// A format-versioned wrapper: serialises as `{ "version": N, <flattened T> }`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Versioned<T> {
    pub version: u32,
    #[serde(flatten)]
    pub inner: T,
}

/// Serialise `value` to pretty JSON that stays readable: scalars and
/// size-appropriate arrays stay inline; objects always break, one field per line.
pub(crate) fn to_readable_json<T>(value: &T) -> serde_json::Result<String>
where
    T: Serialize,
{
    let v = serde_json::to_value(value)?;
    let mut out = String::new();
    write_value(&v, 0, &mut out);
    out.push('\n');
    Ok(out)
}

/// Hex-nibble rows (one char per pixel) for a `SHEET_W`-wide 16-colour grid.
pub(crate) mod pixel_rows {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(px: &[u8], s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let rows: Vec<String> = px.chunks(SHEET_W).map(nibbles_to_hex).collect();
        rows.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let rows = Vec::<String>::deserialize(d)?;
        Ok(rows.iter().flat_map(|r| hex_to_nibbles(r)).collect())
    }
}

/// Optional hex-nibble rows, for the cart label.
pub(crate) mod pixel_rows_opt {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(px: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match px {
            Some(px) => {
                let rows: Vec<String> = px.chunks(SHEET_W).map(nibbles_to_hex).collect();
                s.serialize_some(&rows)
            }
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<Vec<String>>::deserialize(d)?;
        Ok(opt.map(|rows| rows.iter().flat_map(|r| hex_to_nibbles(r)).collect()))
    }
}

/// Hex-byte rows (two chars per tile) for a `MAP_W`-wide 8-bit grid.
pub(crate) mod tile_rows {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(tiles: &[u8], s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let rows: Vec<String> = tiles.chunks(MAP_W).map(bytes_to_hex).collect();
        rows.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let rows = Vec::<String>::deserialize(d)?;
        Ok(rows.iter().flat_map(|r| hex_bytes(r)).collect())
    }
}

/// A flat lowercase hex string (two chars per byte), for sprite flags and
/// clipboard rects.
pub(crate) mod hex_string {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bytes_to_hex(bytes).serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(hex_bytes(&String::deserialize(d)?))
    }
}

/// Standard base64, for the compiled wasm module.
pub(crate) mod base64_bytes {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{de::Error as _, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        STANDARD.encode(bytes).serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(D::Error::custom)
    }
}

/// `cells` (each a palette index masked to a nibble) as a hex string, one char
/// per cell.
fn nibbles_to_hex(cells: &[u8]) -> String {
    cells
        .iter()
        .map(|&c| char::from_digit((c & 0x0f) as u32, 16).unwrap())
        .collect()
}

/// Each hex digit of `s` as a nibble byte `0..16`, skipping non-hex chars.
fn hex_to_nibbles(s: &str) -> Vec<u8> {
    s.chars()
        .filter_map(|c| c.to_digit(16))
        .map(|d| d as u8)
        .collect()
}

/// The maximum compact length (in bytes) for which an array (note quads, channel
/// lists, sample lists, ...) is written inline on one line; longer arrays break,
/// one element per line. Objects are never inlined regardless of this threshold.
const MAX_INLINE: usize = 512;

/// Recursively write `v`: scalars and arrays whose compact form fits `MAX_INLINE`
/// are written on one line; objects always break, one field per line, so struct
/// fields stay vertically scannable regardless of the struct's overall size.
fn write_value(v: &Value, indent: usize, out: &mut String) {
    let inline = match v {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
        Value::Array(_) => v.to_string().len() <= MAX_INLINE,
        Value::Object(_) => false,
    };
    if inline {
        out.push_str(&v.to_string());
        return;
    }
    match v {
        Value::Array(a) => {
            out.push_str("[\n");
            for (i, e) in a.iter().enumerate() {
                push_indent(out, indent + 1);
                write_value(e, indent + 1, out);
                if i + 1 < a.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push(']');
        }
        Value::Object(o) => {
            out.push_str("{\n");
            for (i, (k, e)) in o.iter().enumerate() {
                push_indent(out, indent + 1);
                out.push_str(&Value::String(k.clone()).to_string());
                out.push_str(": ");
                write_value(e, indent + 1, out);
                if i + 1 < o.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(out, indent);
            out.push('}');
        }
        _ => unreachable!("scalars handled above"),
    }
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Grid {
        #[serde(with = "pixel_rows")]
        px: Vec<u8>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct OptionalGrid {
        #[serde(with = "pixel_rows_opt", default)]
        px: Option<Vec<u8>>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Tiles {
        #[serde(with = "tile_rows")]
        t: Vec<u8>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Flags {
        #[serde(with = "hex_string")]
        f: Vec<u8>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Wasm {
        #[serde(with = "base64_bytes")]
        w: Vec<u8>,
    }

    #[test]
    fn pixel_rows_encode_as_nibble_strings_and_round_trip() {
        let mut px = vec![0u8; SHEET_W * 2];
        px[0] = 0x0f;
        px[SHEET_W] = 0x0a;
        let json = serde_json::to_string(&Grid { px: px.clone() }).unwrap();
        assert!(json.contains("\"f0"), "row 0 starts with f: {json}");
        assert!(json.contains("\"a0"), "row 1 starts with a: {json}");
        assert_eq!(serde_json::from_str::<Grid>(&json).unwrap().px, px);
    }

    #[test]
    fn pixel_rows_opt_round_trips_some_and_none() {
        let mut px = vec![0u8; SHEET_W + 1];
        px[0] = 0x0f;
        px[SHEET_W] = 0x0a;
        let json = serde_json::to_string(&OptionalGrid {
            px: Some(px.clone()),
        })
        .unwrap();
        assert!(json.contains("\"f0"), "row 0 starts with f: {json}");
        assert!(json.contains("\"a\""), "row 1 is a lone nibble: {json}");
        assert_eq!(
            serde_json::from_str::<OptionalGrid>(&json).unwrap().px,
            Some(px)
        );

        let json = serde_json::to_string(&OptionalGrid { px: None }).unwrap();
        assert!(json.contains("null"), "{json}");
        assert_eq!(
            serde_json::from_str::<OptionalGrid>(&json).unwrap().px,
            None
        );
    }

    #[test]
    fn tile_rows_encode_as_byte_strings_and_round_trip() {
        let mut t = vec![0u8; MAP_W * 2];
        t[0] = 0x2a;
        let json = serde_json::to_string(&Tiles { t: t.clone() }).unwrap();
        assert!(json.contains("\"2a00"), "tile row: {json}");
        assert_eq!(serde_json::from_str::<Tiles>(&json).unwrap().t, t);
    }

    #[test]
    fn flags_round_trip_as_flat_hex() {
        let f = vec![0x03, 0x00, 0xff];
        let json = serde_json::to_string(&Flags { f: f.clone() }).unwrap();
        assert!(json.contains("\"0300ff\""), "{json}");
        assert_eq!(serde_json::from_str::<Flags>(&json).unwrap().f, f);
    }

    #[test]
    fn wasm_round_trips_as_base64() {
        let w = b"\0asm\x01\0\0\0".to_vec();
        let json = serde_json::to_string(&Wasm { w: w.clone() }).unwrap();
        assert!(!json.contains("\\u"), "base64 is plain ascii: {json}");
        assert_eq!(serde_json::from_str::<Wasm>(&json).unwrap().w, w);
    }

    #[test]
    fn versioned_flattens_the_version_field() {
        let v = Versioned {
            version: 1,
            inner: Flags { f: vec![0x01] },
        };
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"version":1,"f":"01"}"#);
    }

    #[test]
    fn readable_json_inlines_small_arrays_and_breaks_grids() {
        let value = serde_json::json!({
            "quad": [33, 3, 5, 0],
            "rows": ["aaaa", "bbbb"],
        });
        let s = to_readable_json(&value).unwrap();
        // The small quad stays on one line.
        assert!(s.contains("\"quad\": [33,3,5,0]"), "{s}");
        // A long object breaks onto multiple lines.
        assert!(s.contains("{\n"), "{s}");
    }
}
