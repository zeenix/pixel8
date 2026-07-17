# Pixel8

**A tiny fantasy console where the games are written in Rust.**

Pixel8 (pronounced "pixelate") is a tiny, self-contained game console that never existed: a 128x128
screen, 16 fixed colors, a 4x6 pixel font, four audio channels, 256 sprites, a 128x64 tile map — and
a Rust compiler where the Lua interpreter would be. You write a little Rust, it compiles to
WebAssembly, and it runs inside the console's sandbox at a steady 60 fps (or 30, the cart's choice).
Carts are shareable PNG images with the game embedded inside.

New to Pixel8? **[The Pixel8 Book](https://zeenix.github.io/pixel8/)** is a hands-on tutorial —
installation to shipped cartridge — with the [example carts](https://zeenix.github.io/pixel8/play/)
playable right inside it.

```rust
use pixel8::*;

struct MyGame {
    x: i16,
    y: i16,
}

impl Game for MyGame {
    fn update(&mut self, ctx: &mut Context) {
        if ctx.is_button_down(Button::Right) {
            self.x += 1;
        }
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);
        gfx.rect_fill(self.x, self.y, 8, 8, Color::WHITE).unwrap();
    }
}

pixel8::game!(MyGame { x: 64, y: 64 });
```

## The console

The console is the `pixel8-console` crate; installing it gives you a `pixel8` command:

```text
cargo install pixel8-console
pixel8
```

You land at the boot console. Type `help`. The workflow is PICO-8's:

```text
> new mygame          create ./mygame (a real cargo crate!)
> run                 compile to wasm + run     (esc returns)
> save                save code + assets
> export mygame.png   write a shareable png cartridge
> load mygame.png     load a cart back
```

`Esc` flips between the console and the editors; the tab icons (or `Alt+←/→`) switch between
**code**, **sprite**, **map**, **sfx** and **music** editors. All UI is drawn by the console itself
on the same 128x128 screen the games use — there are no native widgets anywhere.

Games are played with the arrow keys plus `Z`/`X` (also `C`/`V`, `N`/`M`). `Ctrl+R` rebuilds and
runs from anywhere; `Ctrl+S` saves and kicks off a background build, flashing `saved` /
`building...` / `build ok` in the editor's bottom bar (compile errors land in the console). `F6`
while a game runs captures the screen as the cartridge label. Type `keys` in the console for the
full list.

### In the terminal

The same console also runs inside a terminal — editors, carts and all — as the separate
`pixel8-tui` binary (so neither frontend drags in the other's dependencies):

```text
cargo install pixel8-tui
pixel8-tui                    boot the console in the terminal
pixel8-tui run mygame.png     boot, load, and run immediately
```

Terminals with [sixel](https://en.wikipedia.org/wiki/Sixel) support (foot, WezTerm, Konsole,
iTerm2, xterm...) get real pixels via a pure-Rust encoder; everywhere else the screen is drawn
with unicode half-blocks. `Ctrl+Q` quits. On Linux, game input needs either a terminal with the
kitty keyboard protocol or read access to `/dev/input` (one-time:
`sudo usermod -aG input $USER`). See
[docs/TUI.md](https://github.com/zeenix/pixel8/blob/main/docs/TUI.md) for input details and
tuning knobs.

### Constraints (they are the point)

| thing      | size                                  |
| ---------- | ------------------------------------- |
| screen     | 128 x 128, 16 fixed colors            |
| sprites    | 256 of 8x8 pixels, 8 flags each       |
| map        | 128 x 64 tiles                        |
| sfx        | 64 slots, 32 steps, 8 waveforms       |
| music      | 64 patterns, 4 channels               |
| framerate  | 60 fps (or 30, the cart's choice)     |
| cart       | one PNG file                          |

Carts also have runtime limits: 128 KiB cart size, 128 KiB RAM, and a 128 K per-frame work budget.
By default a cart is `#![no_std]` and depends only on the `pixel8` SDK — that is what `pixel8 new`
scaffolds, and it keeps carts tiny. When one needs a growable vector, string or map, it can pull in
[`heapless`](https://docs.rs/heapless) for fixed-size collections. Full details in
[docs/LIMITS.md](https://github.com/zeenix/pixel8/blob/main/docs/LIMITS.md).

## PNG cartridges

`export` produces a real PNG image — cartridge art, label, title — with the compiled wasm, all
assets and (by default) the compressed Rust source embedded in a private chunk. Anyone can *see* the
cart; Pixel8 can *play* it; and if the source is included, `import` turns it back into an editable
project. See [docs/CART_FORMAT.md](https://github.com/zeenix/pixel8/blob/main/docs/CART_FORMAT.md).

`export mygame.html` instead produces a single self-contained web page: the cart and the whole
console runtime (compiled to wasm) embedded in one file you can double-click or host anywhere,
PICO-8-web style. See
[docs/WEB_EXPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/WEB_EXPORT.md).

Carts also run via `pixel8-player`, a pure-Rust player with a console-style cart picker. On the
desktop it opens a window with keyboard input; on retro handhelds (PowKiddy RGB10S, Anbernic
RG351/353 and friends on ArkOS/ROCKNIX) it runs as a static-musl KMS/evdev/ALSA binary — copy it
into the ports folder, drop `.png` carts next to it, play. See
[docs/HANDHELD.md](https://github.com/zeenix/pixel8/blob/main/docs/HANDHELD.md).

## Inspired by PICO-8

Pixel8 is heavily inspired by [PICO-8](https://www.lexaloffle.com/pico-8.php). The palette, the
fixed constraints, the editor modes, the `>` prompt and the overall charm all come from it. What
differs is the whole point of the project: a cart is Rust compiled to WebAssembly rather than Lua,
the font, code and cartridge formats are entirely original, and Pixel8 is free and open source
(GPL-3.0) rather than a paid product.

That shared heritage — the same palette, waveforms and sprite layout — means a PICO-8 cart's assets
import almost one-to-one:

```text
pixel8 import-pico8 mygame.p8 mygame      # or mygame.p8.png
```

The graphics, sprite flags, map, sound effects and music transfer into a new project. Only the
assets come across — the cart's Lua code is ignored — and the project gets a stub `src/lib.rs` to
write your game in Rust. See
[docs/PICO8_IMPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/PICO8_IMPORT.md).

## Projects are real crates

A Pixel8 project is an ordinary Cargo crate that builds a `cdylib` for `wasm32-unknown-unknown`,
plus an `assets.pixel8.json` bundle. The integrated editor is the charming way to work, but
`$EDITOR` + `cargo build` works exactly the same — the console hot-reloads the wasm when it
changes on disk.
Headless commands support scripts and CI:

```text
pixel8 new <dir>                  create a project
pixel8 build <dir>                compile it to wasm
pixel8 export <dir> <out.png>     build + write a png cart
pixel8 extract <cart.png> <dir>   editable cart -> project
pixel8 import-pico8 <c> <dir>     pico-8 cart (.p8/.p8.png) -> project
pixel8 export-web <dir> <o.html>  one playable web page
pixel8 verify <cart.png>          run 60 frames headless
```

## The sandbox

Carts execute inside [wasmi](https://github.com/wasmi-labs/wasmi) with no WASI, no filesystem, no
network and no host memory access. The only imports a cart gets are the ~26 small, C-like functions
of the Pixel8 ABI ([docs/ABI.md](https://github.com/zeenix/pixel8/blob/main/docs/ABI.md)) — draw,
input, audio, map, log. Fuel metering turns infinite loops into a friendly error screen instead of a
hung console.

## Crates

Pixel8 is a handful of crates. Most people only ever touch the first two — the SDK a cart is written
against, and the console that builds and runs it.

- **[`pixel8`](https://crates.io/crates/pixel8)** — the SDK your cart depends on, and the only crate
  a game links against. Deliberately zero-dependency and `#![no_std]`-friendly: the `Game` trait,
  the `Context` (update-time) and `Graphics` (draw-time) handles, the 16-color palette and the
  `game!` macro that wires it all up. `cargo add pixel8` in a `cdylib` crate and you have a cart.
- **[`pixel8-console`](https://crates.io/crates/pixel8-console)** — the desktop console and
  toolchain. `cargo install pixel8-console` gives you the `pixel8` command: the boot prompt, the
  five editors (code, sprite, map, sfx, music), the build-and-hot-reload loop and every headless
  subcommand (`new`, `build`, `export`, `extract`, `import-pico8`, `export-web`, `verify`). The
  crate is `pixel8-console`; the binary it installs is `pixel8`. The shell and editors are also
  exposed as a library (the windowed frontend sits behind the default-on `window` feature), which
  is how `pixel8-tui` reuses them.
- **[`pixel8-tui`](https://crates.io/crates/pixel8-tui)** — the console in your terminal:
  the same shell and editors rendered over sixel (pure-Rust encoder) or unicode half-blocks,
  with crossterm input. A thin frontend over the `pixel8-console` library that never builds
  the winit/wgpu window stack.
- **[`pixel8-runtime`](https://crates.io/crates/pixel8-runtime)** — the console's engine, as a
  reusable library: the 128x128 indexed framebuffer and software rasterizer, font and palette, the
  wasmi VM with ABI linking and fuel metering, the input model, the 4-channel synth, the shared
  asset model and the PNG cart codec. Depend on it to embed Pixel8 in your own frontend or build
  tools around carts — the console and both players are thin shells over it.
- **[`pixel8-player`](https://crates.io/crates/pixel8-player)** — a standalone cart player, no
  editors. Its default `window` backend opens a desktop window with keyboard input; its `kms`
  backend is a static-musl KMS/evdev/ALSA binary for retro handhelds. Point it at a folder of `.png`
  carts and play.

The browser player — `pixel8-runtime` compiled to wasm and wrapped in a small C-like export surface
— lives in the repo as `pixel8-web` and powers `export mygame.html`. It ships inside exported web
pages rather than to crates.io.

## Building from source

Requires Rust (with the `wasm32-unknown-unknown` target for building carts) and, on Linux, ALSA
headers for audio:

```text
rustup target add wasm32-unknown-unknown
sudo apt install libasound2-dev        # debian/ubuntu
sudo dnf install alsa-lib-devel        # fedora
# (or build silent with `--no-default-features`)
cargo console                          # alias for: cargo run --release -p pixel8-console
cargo tui                              # alias for: cargo run --release -p pixel8-tui
```

Try a bundled cart:

```text
cargo console -- examples/platformer
```

then type `run`.

