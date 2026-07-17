# Installation

Pixel8 is two things: the **console** (a desktop app with the `>` prompt and
the editors) and the **SDK** (the `pixel8` crate your game code uses). You
install the console; it takes care of the SDK when it scaffolds a project.

## Prerequisites

You need Rust, installed via [rustup](https://rustup.rs/), plus the
WebAssembly target that carts compile to:

```sh
rustup target add wasm32-unknown-unknown
```

On Linux, the console's audio backend links ALSA, so you need its headers
once:

```sh
sudo apt install libasound2-dev        # debian/ubuntu
sudo dnf install alsa-lib-devel        # fedora
```

(macOS and Windows need nothing extra.)

## Installing the console

```sh
cargo install pixel8-console
```

The crate is called `pixel8-console`, but the command it installs is
`pixel8`. Run it:

```sh
pixel8
```

You should land at the boot console — a black 128×128 screen with a `>`
prompt. Type `help` and press Enter. If you see the command list, you're
done; skip ahead to [Your first cart](first-cart.md).

On a machine without a sound card (or without ALSA headers), install a silent
console instead:

```sh
cargo install pixel8-console --no-default-features
```

Everything works identically, minus audio output.

## The console in a terminal

The same console — editors, carts and all — also runs *inside a terminal*, as
a separate binary:

```sh
cargo install pixel8-tui
pixel8-tui                    # boot the console in the terminal
pixel8-tui run mygame.png     # boot, load, and run immediately
```

Terminals with [sixel](https://en.wikipedia.org/wiki/Sixel) support (foot,
WezTerm, Konsole, iTerm2, xterm...) get real pixels; everywhere else the
screen is drawn with unicode half-blocks. `Ctrl+Q` quits. On Linux, game
input needs either a terminal with the kitty keyboard protocol or read access
to `/dev/input` (one-time: `sudo usermod -aG input $USER`). See
[docs/TUI.md](https://github.com/zeenix/pixel8/blob/main/docs/TUI.md) for the
details and tuning knobs.

The rest of this book assumes the desktop console, but everything applies to
the terminal one too.

## Building from source

If you'd rather run from a checkout (or want to hack on the console itself):

```sh
git clone https://github.com/zeenix/pixel8
cd pixel8
cargo console                  # alias for: cargo run --release -p pixel8-console
cargo tui                      # alias for: cargo run --release -p pixel8-tui
```

A source checkout also gets you the bundled examples:

```sh
cargo console -- examples/platformer
```

then type `run` at the prompt.
