# Cart limits

Pixel8 is a fantasy console with deliberate constraints — that's the point.
Like PICO-8 before it, every limit is part of what makes a Pixel8 cart a
*Pixel8 cart*. Every runtime limit is built around one easy number:
**128 K**. If you keep that number in mind, you will never be surprised.

| what            | limit   | what happens when you exceed it              |
| --------------- | ------- | -------------------------------------------- |
| cart size       | 128 KiB | build warns; `export` is rejected            |
| RAM             | 128 KiB | "ran out of memory" error screen             |
| per-frame work  | 128 K   | "ran too long (infinite loop?)" error screen |
| save data       | 128 KiB | `storage_set` fails; nothing is stored       |

---

## Cart size — 128 KiB

The compiled game wasm must be no larger than 128 KiB. If you try to export
an over-size cart, the export is rejected. Building an over-size cart locally
also prints a warning, so you know before you share it.

In practice a simple game is tiny — a few kilobytes. The limit is mostly about
dependency discipline: keep your Rust dependencies lean and your cart stays
small. The built-in shrink profile (`opt-level = "s"`, `lto = true`,
`panic = "abort"`) already applies; fighting for the last byte is rarely
needed.

## RAM — 128 KiB

A running cart may use at most 128 KiB of total RAM. Overrunning that budget
stops the cart immediately and shows a "ran out of memory" error screen. The
player can then return to the console and you can inspect the output.

The shadow-stack reserve is owned by the cart itself: each cart's
`.cargo/config.toml` carries a `stack-size=32768` rustflag that sets it to
32 KiB by default. You can tune it up or down by editing that line. The
console builds straight and honors whatever value is set; a value large enough
to push the cart's initial memory to 2 pages (128 KiB, no headroom) is
reported as a warning, and anything over the cap is an error. With the
default 32 KiB reserve, roughly 95 KiB of heap headroom remains for carts
that opt into `std` and a heap allocator. Simple `no_std` carts use a small
fraction of this; even a heap-using cart typically stays well inside the budget.

What most often decides how big that reserve has to be is start-up. A cart's
game state lives in a `static`, which reserves its size whether or not anything
has been written into it — so the state costs the same RAM however it is made.
The stack is the part that varies: a state *built* at start-up is assembled on
the stack and then moved into that `static`, so for a moment the whole game
exists twice and the reserve must cover a copy of it. A `game!` initializer that
is a constant is placed by the loader instead — nothing is built and nothing is
copied — and a cart with a large state can then run on a far smaller stack than
its own size. See `Game::boot` for where the setup a constant cannot express
goes.

## Per-frame work — 128 K

Each call to `update` and each call to `draw` has a fixed work budget.
A cart that runs an infinite loop, or simply does too much in a single frame,
will be stopped and shown a "ran too long (infinite loop?)" error screen
instead of freezing the console.

Real game logic — move sprites, read input, draw a tilemap — uses a tiny
fraction of the budget. The limit catches runaway loops during development,
not carefully written games.

What the budget counts is the cart's own work. The console's side of a draw
call is not charged: `circle_fill` costs the same whether it paints one pixel
or twelve thousand, and drawing the whole tilemap costs the same as drawing
one tile. Only the call itself is billed, and at a flat one unit of the
128 K — the same price whether it takes ten arguments or none. What actually
sets the ceiling is the cart-side code around each call, so a `draw` written
in normal Rust fits something like ten thousand of them, and a tight loop
that does nothing else roughly thirty thousand.

The meter is therefore not a measure of how hard the console is working. It
measures the thing you can do something about — your own code — while the
console keeps its own side in check by clipping each primitive's sweep to the
screen before it starts. Asking `sprite_stretch` to blow an 8x8 sprite up to
4096x4096, or `map` for a hundred thousand cels, costs no more than the
128x128 that can actually be seen, and a line or a circle is walked across the
part of it the screen can show rather than end to end.

There are two things the meter still cannot see. One is sheer call volume: a
cart issuing draw calls by the tens of thousands can miss frames while
`cpu_draw` reads relaxed. The other is a circle whose radius runs to the
billions — the arc is walked near the screen rather than all the way round,
but the walk is still long enough that a handful of such calls costs a frame.
Watch `fps` rather than `cpu_draw` when a cart does either.

## Save data — 128 KiB

The persistent key-value store (`Context::storage_set` / `storage_get`, see
docs/ABI.md) holds the cart's save data as JSON. The whole store — every key
and value, serialized — is capped at 128 KiB. A `storage_set` that would push
past the cap fails (the SDK returns `Err(StorageFull)`) and stores nothing;
the previous value under that key is kept.

That is a *huge* amount of save data for a 128x128 game — PICO-8 gives carts
256 bytes — so hitting this limit usually means storing something that isn't
save data.

---

## Staying small — `#![no_std]` + `heapless`

This is the normal way Pixel8 carts are written. `pixel8 new` scaffolds a
`#![no_std]` cart, and every game example ships this way. There is no heap and no
allocator overhead — memory is fully static — which keeps carts tiny: the
examples weigh in at roughly 1–5 KiB.

A `no_std` cart looks like this:

```toml
# Cargo.toml
[dependencies]
pixel8 = { path = "../../pixel8", default-features = false }

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
```

```rust
// src/lib.rs
#![no_std]
use pixel8::*;

struct MyGame {
    x: i16,
    y: i16,
}

impl Game for MyGame {
    fn update(&mut self, ctx: &mut Context) {
        // ...
    }
    fn draw(&self, gfx: &mut Graphics) {
        // ...
    }
}

pixel8::game!(MyGame { x: 64, y: 64 });
```

### Fixed-size collections with `heapless`

When you need a vector, string, or map, reach for [`heapless`]. Its
collections are bounded at compile time (`Vec<T, N>`, `String<N>`,
`FnvIndexMap<K, V, N>`, …), so there is nothing to allocate and nothing to
fail at runtime due to memory pressure. Add it alongside `pixel8`:

```toml
heapless = "0.9"
```

See [`examples/platformer`](../examples/platformer) for a worked example: it
builds its HUD text with `heapless::format!`.

[`heapless`]: https://docs.rs/heapless

### Float math with `libm`

`core` only carries the float operations that don't need a math library:
arithmetic, comparisons, and `min`/`max`/`clamp`. Everything that maps to a
libm symbol — `floor`, `ceil`, `round`, `sqrt`, `sin`/`cos`, `abs`, `pow`,
… — lives in `std`, so it isn't available to a `no_std` cart. When you need
one, add [`libm`] and call its free functions:

```toml
libm = "0.2"
```

```rust
let d = libm::fabsf(dx);
let r = libm::sqrtf(dx * dx + dy * dy);
let snapped = libm::floorf(x / 8.0) * 8.0;
```

You often don't need it for drawing: the SDK's draw calls take `i16` positions and `u16` sizes,
and converting a sub-pixel `f32` position to the integer the API wants is just `x as i16`
(truncation toward zero — it equals `floor` only for non-negative values, so floor negative
positions yourself if they must step evenly). Reach for `libm` when the *cart's own* math needs
it.

[`libm`]: https://docs.rs/libm

### When you really need a heap

A cart that genuinely needs a growable heap can opt into `std` by depending on
`pixel8` with its default features — just drop `default-features = false`:

```toml
[dependencies]
pixel8 = { path = "../../pixel8" }
```

This brings in a heap allocator and lets you use ordinary `Vec`, `String`, and
the rest of `std`. [`examples/stress`](../examples/stress) is the one cart that
takes this path, deliberately allocating to probe the RAM cap.
