# Pixel8 clipboard format

Pixel8's editors copy assets to the system clipboard as a plain-text blob:

    [pixel8]<json>[/pixel8]

`<json>` is a versioned, kind-tagged object:

    { "version": 1, "kind": "sprite" | "sfx" | "pattern" | "map", ... }

- Byte fields (sprite pixels, sprite flags, map tiles) are lowercase hex strings;
  SFX steps are `[pitch, wave, volume, effect]` quads.
- The `PIXEL8C` magic and hex wrapping are gone; the `[pixel8]` tag is the
  identifier, and the `"version"` field is the version contract.

The payload is a tagged union with four kinds:

| Kind    | Fields                                                                        |
|---------|-------------------------------------------------------------------------------|
| Sprite  | width, height, one palette index per pixel, one flag byte per 8×8 sprite     |
| SFX     | source slot + the full SFX (including any drawn custom waveform)              |
| Pattern | a music pattern + the SFX each of its channels references (as slot + SFX)    |
| Map     | width, height, one 8-bit tile index per cell                                  |

All dimensions are in their natural units: Sprite width/height are in pixels, Map
width/height are in tiles. All data is row-major.

Because the payload reuses the on-disk asset structs directly, a copy is lossless:
sprite flags, custom waveforms, and 8-bit map tiles all survive the round-trip.

## Pasting

Paste accepts two formats:

**Native `[pixel8]`** — decoded by checking the `[pixel8]` tag, parsing the JSON
body, and checking the exact `"version"`. The full payload is restored, including
sprite flags, custom waveforms, and map regions.

**PICO-8 editor formats** — for interoperability, Pixel8 also parses PICO-8's
clipboard blobs:
- `[gfx]` — sprite pixels only; no sprite flags are carried.
- `[sfx]` — SFX records and song patterns, without custom waveforms.

PICO-8 has no map clipboard format, so map regions can only be transferred via the
native format. Any unrecognised or malformed blob is ignored.

## Validation

On decode, Pixel8 checks the `[pixel8]` tag, that the body is valid JSON, and the
exact `"version"`. Any mismatch is an error; the paste is a no-op. Readers must
reject versions they do not know.

## Versioning policy

The version field covers the payload schema. Any change to a variant's fields
requires a version bump so old consoles can reject new blobs cleanly rather than
silently misinterpret them.
