# Your first cart

Boot the console and create a project at the prompt:

```text
> new mygame
```

This creates `./mygame`, loads it, and drops you into the code editor. Press
`Esc` to hop back to the prompt at any time, and type:

```text
> run
```

The console compiles your Rust to WebAssembly and boots it: a pink square on
a dark blue screen that you can move with the arrow keys. `Esc` returns to
the console. That's the whole loop — edit, `run`, play, `Esc`.

## What `new` made

A Pixel8 project is a **real Cargo crate**, not a proprietary bundle:

```text
mygame/
  Cargo.toml           # an ordinary manifest, builds a cdylib for wasm32
  src/lib.rs           # your game
  assets.pixel8.json   # sprites, map, sfx, music, metadata
  .cargo/config.toml   # defaults `cargo build` to the wasm target
```

`Cargo.toml` is small enough to read in full:

```toml
[package]
name = "mygame"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
pixel8 = { version = "0.1", default-features = false }

[workspace]

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
```

The one dependency is the SDK. `default-features = false` makes the cart
`#![no_std]` — the normal way Pixel8 carts are written, and what keeps them
tiny (the examples weigh 1–5 KiB). The release profile is pre-tuned to shrink
the WebAssembly.

## The game, line by line

`src/lib.rs` starts as:

```rust
#![no_std]

use pixel8::*;

game!(MyGame { x: 60, y: 70 });

struct MyGame {
    x: i16,
    y: i16,
}

impl Game for MyGame {
    fn update(&mut self, ctx: &mut Context) {
        if ctx.btn(Button::Left) { self.x -= 1; }
        if ctx.btn(Button::Right) { self.x += 1; }
        if ctx.btn(Button::Up) { self.y -= 1; }
        if ctx.btn(Button::Down) { self.y += 1; }
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::DARK_BLUE);
        gfx.print("Hello, Pixel8!", 36, 48, Color::WHITE);
        gfx.rect_fill(self.x, self.y, 8, 8, Color::PINK).unwrap();
    }
}
```

A game is a struct holding your state, plus the [`Game`] trait:

- **`update(&mut self, ctx: &mut Context)`** runs 60 times per second (or 30,
  if you set `const FRAME_RATE: FrameRate = FrameRate::Fps30;` in the impl).
  This is where you read input and move the world. [`Context`] is the handle
  to everything a game *does*: input, audio, the map, random numbers, saved
  data.
- **`draw(&self, gfx: &mut Graphics)`** runs after each `update` and paints
  the frame. [`Graphics`] is the handle to the screen. Note it takes `&self`:
  drawing observes the world, it doesn't change it. That split — mutate in
  `update`, render in `draw` — is enforced by the types, and it keeps game
  logic untangled from presentation.
- **`game!(MyGame { x: 60, y: 70 })`** declares the entry point and the
  initial state. Any constructor works: `game!(MyGame = MyGame::new())`, or
  just `game!(MyGame)` if your type implements `Default`.

There is no main loop to write, no window to open, no timing code: the
console calls you.

## The edit-run loop

Make a change — say, a splash of randomness in `update`:

```rust
if ctx.btnp(Button::X) {
    self.x = ctx.rndi(120) as i16;
    self.y = ctx.rndi(120) as i16;
}
```

(`btnp` is "button just pressed"; `btn` is "button held". `X` is the
<kbd>X</kbd> key.) Then press `Ctrl+R` — from the editor, from anywhere — to
rebuild and run. `Ctrl+S` saves and kicks off a background build check,
flashing `saved` / `building...` / `build ok` in the editor's bottom bar;
compile errors land in the console, trimmed to the useful part.

### Using your own editor

Because a project is a plain crate, the integrated editor is optional. Open
`mygame/` in your usual editor and build from a terminal:

```sh
cargo build --release
```

(The scaffolded `.cargo/config.toml` already targets
`wasm32-unknown-unknown`.) A console with the project loaded polls the
compiled wasm once a second and **hot-reloads it when it changes** — save in
your editor, `cargo build`, and the running game restarts with your change.
`rust-analyzer`, `clippy`, unit tests: everything works, it's just Rust.

## When things go wrong

Panics don't crash the console: the cart stops on a friendly error screen
showing the actual panic message. An accidental infinite loop in `update` is
caught by the console's per-frame work budget and reported as "ran too long"
instead of freezing anything.

For printf-debugging, log to the console (visible after `Esc`):

```rust
ctx.log("checkpoint");
logf!(ctx, "frame {} pos ({},{})", self.frame, self.x, self.y);
```

[`Game`]: https://docs.rs/pixel8/latest/pixel8/trait.Game.html
[`Context`]: https://docs.rs/pixel8/latest/pixel8/struct.Context.html
[`Graphics`]: https://docs.rs/pixel8/latest/pixel8/struct.Graphics.html
