# A tour of the examples

The repository ships example carts under
[`examples/`](https://github.com/zeenix/pixel8/tree/main/examples) — each a
standalone project you can open, run and take apart. They are all playable
right here (and on the [cart shelf](play/index.html)), and each one is the
worked answer to one of this book's chapters.

From a source checkout, any of them opens in the console:

```sh
cargo console -- examples/platformer     # then type `run`
```

Or crack open the published cartridges: save a `.png` from the
[shelf](play/index.html) and `load` it — the examples embed their source, so
`import` turns any of them back into an editable project.

## sprite_move

Sprites, flipping, and a two-frame walk cycle driven by a frame counter —
the [Drawing](drawing.md) chapter, in 60 lines.
[Source](https://github.com/zeenix/pixel8/blob/main/examples/sprite_move/src/lib.rs).

<div class="cart-embed">
<iframe src="play/sprite_move.html" title="sprite_move — playable cart" loading="lazy"></iframe>
</div>

## platformer

The capstone: run, jump, collect coins, stomp (or dodge) the badie, grab the
trophy before the clock runs out. Almost every chapter of this book is in
here, in a few hundred lines split into small modules:

- **map + sprite flags** as the collision system (solid tiles carry flag 0);
- a **camera** that follows the hero, with the HUD drawn in screen space;
- a **[`Body`](input.md#smooth-sub-pixel-movement-body)** for the hero, so
  running jumps climb clean staircases;
- coins collected by **rewriting the map** in RAM and put back on restart;
- the best score kept in **[storage](storage.md)** across runs;
- win/lose **music held inside the game-state enum** — leaving the state
  drops the [`PlayingMusic`](audio.md#playing-music) handle, stopping the song;
- `heapless::Vec` and `heapless::format!` in a `#![no_std]` cart.

[Source](https://github.com/zeenix/pixel8/tree/main/examples/platformer/src).

<div class="cart-embed">
<iframe src="play/platformer.html" title="platformer — playable cart" loading="lazy"></iframe>
</div>

## sfx_demo

A four-pad soundboard: each arrow key fires a slot via `ctx.sfx`, with the
pads lighting up on the screen — the sound half of
[Sound and music](audio.md).
[Source](https://github.com/zeenix/pixel8/blob/main/examples/sfx_demo/src/lib.rs).

<div class="cart-embed">
<iframe src="play/sfx_demo.html" title="sfx_demo — playable cart" loading="lazy"></iframe>
</div>

## music_demo

Starting and stopping a song with fades, the `PlayingMusic` handle held in
an `Option`, and dancing bars while it plays — the music half of the same
chapter.
[Source](https://github.com/zeenix/pixel8/blob/main/examples/music_demo/src/lib.rs).

<div class="cart-embed">
<iframe src="play/music_demo.html" title="music_demo — playable cart" loading="lazy"></iframe>
</div>

## stress

Not on the shelf, and not a game: a cart that deliberately allocates and
burns CPU to probe the [limits](limits.md). It's the one example that opts
into `std`, and the fastest way to see the "ran out of memory" and "ran too
long" error screens on purpose.
[Source](https://github.com/zeenix/pixel8/blob/main/examples/stress/src/lib.rs).
