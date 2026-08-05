# Living within the limits

Every runtime limit in Pixel8 is built around one easy number: **128 K**.
Keep it in mind and you will never be surprised.

| what           | limit   | when you exceed it                            |
| -------------- | ------- | --------------------------------------------- |
| cart size      | 128 KiB | build warns; `export` is rejected             |
| RAM            | 128 KiB | "ran out of memory" error screen              |
| per-frame work | 128 K   | "ran too long (infinite loop?)" error screen  |
| save data      | 128 KiB | `storage_set` returns `Err(StorageFull)`      |

None of these bite a normally-written game. A simple cart compiles to a few
KiB, real game logic uses a sliver of the frame budget, and static `no_std`
state barely dents the RAM. The limits exist to catch runaways — and to keep
a Pixel8 cart *a Pixel8 cart*.

## Watching the meters

Press `F1` while a game runs to overlay the resource stats. Its CPU and memory
rows refresh once a second, each holding that second's peak; the cart can read
the same numbers itself, live, per frame:

```rust
ctx.cpu_update()   // 0.0..1.0 of last frame's update budget used
ctx.cpu_draw()     // same, for draw
ctx.mem()          // 0.0..1.0 of the 128 KiB RAM cap (high-water)
ctx.fps()          // measured frames per second
```

The two CPU meters count the cart's own work, not the console's: a screenful
of `circle_fill` reads the same as a single pixel, because only the call is
charged and not what it paints. That is usually what you want — it measures
the thing you can actually optimize — and the console keeps its own side in
check by clipping each primitive to the screen before it draws, so a sprite
blown up to 4096x4096 costs only the part you can see. Sheer volume is what
the meters miss: a cart issuing tens of thousands of draw calls a frame can
still drop below 60 while they look relaxed, and `fps()` is the honest number
when that happens.

If a frame genuinely can't fit the budget at 60 fps, a cart can opt into 30:

```rust
impl Game for MyGame {
    const FRAME_RATE: FrameRate = FrameRate::Fps30;
    // update and draw now run 30 times per second, with double the
    // per-call work budget effectively available per second of gameplay.
}
```

## Staying small: `no_std` is the normal way

`pixel8 new` scaffolds a `#![no_std]` cart, and every game example ships this
way: no heap, no allocator, fully static memory. This is less exotic than it
sounds — most carts never notice, because the SDK itself is allocation-free
(even [`printf!`](https://docs.rs/pixel8/latest/pixel8/macro.printf.html) and
the storage API).

Two crates cover most of what `std` would have given you:

- **[`heapless`](https://docs.rs/heapless)** — fixed-capacity `Vec<T, N>`,
  `String<N>`, maps and more. The platformer keeps its collected coins in a
  `heapless::Vec<Taken, MAX_TAKEN>` and formats HUD text with
  `heapless::format!`.
- **[`libm`](https://docs.rs/libm)** — the float functions `core` lacks:
  `libm::sqrtf`, `sinf`, `floorf`... You often don't need it: converting a
  sub-pixel `f32` position for a draw call is just `x as i16`.

Dependency discipline is the real cart-size lever: every crate you add is
wasm you ship. The scaffolded release profile (`opt-level = "s"`, `lto`,
`panic = "abort"`) already squeezes hard, and an over-size build warns
locally before an `export` would refuse.

## When you really need a heap

Drop the `default-features = false` from the `pixel8` dependency and the
cart gets `std`, an allocator, and ordinary `Vec`/`String`. RAM is still
capped at 128 KiB total — with the default 32 KiB stack reserve, roughly
95 KiB of heap headroom remains. `examples/stress` is the one cart that takes
this path, deliberately allocating until it hits the cap (worth running once
just to see the error screen).

The stack reserve itself is tunable: the scaffolded `.cargo/config.toml`
carries a `stack-size=32768` rustflag you can raise or lower. What usually
decides how big it has to be is start-up: a `game!` initializer that is a
constant is *placed* — the state ships as part of the cart and nothing builds
it — while a `defer`red one is assembled on the stack and then moved into the
static that holds it, so for a moment a big game exists twice.

The full story — including exact accounting of the memory and fuel budgets —
is in [docs/LIMITS.md](https://github.com/zeenix/pixel8/blob/main/docs/LIMITS.md).
