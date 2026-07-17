# Saving data

Carts get a persistent key-value store — the cartridge's save file. High
scores, unlocked levels, settings: anything that should survive closing the
console.

```rust
fn update(&mut self, ctx: &mut Context) {
    if self.best.is_none() {
        // First frame: read the previous best score (if any).
        self.best = Some(
            ctx.storage_get("best").and_then(|v| v.as_i64()).unwrap_or(0),
        );
    }
    if self.score > self.best.unwrap_or(0) {
        self.best = Some(self.score);
        let _ = ctx.storage_set("best", self.score);
    }
}
```

Keys are `&str`; values are primitives — integers, floats, bools — anything
that converts into a [`StorageValue`]. Reads come back as a `StorageValue`
with `as_i64()`, `as_f64()`, `as_bool()` and `is_null()` accessors.
`storage_remove(key)` deletes one entry, `storage_clear()` wipes the save.
(`dset`/`dget` exist as aliases for PICO-8 fingers.)

The whole API is allocation-free, so it works unchanged in the default
`#![no_std]` carts.

## Where saves live

That's the console's business, not the cart's: the desktop console and player
keep a JSON file in the user's cache directory, keyed by the cart's name.
The browser player keeps saves for the session only, and headless `verify`
keeps them in memory. Your code is the same everywhere.

## The cap

The whole store, serialized, is capped at **128 KiB** — a `storage_set` that
would exceed it returns `Err(StorageFull)` and stores nothing (the old value
under that key is kept). For perspective, PICO-8 gives carts 256 *bytes* of
save data; if you hit this limit, what you're storing probably isn't save
data.

You can see the full pattern in action in the
[platformer](examples.md#platformer): the best score is loaded once in the
first `update`, and written back only when a run beats it.

[`StorageValue`]: https://docs.rs/pixel8/latest/pixel8/enum.StorageValue.html
