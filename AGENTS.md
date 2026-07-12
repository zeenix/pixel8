# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

For contribution conventions — commit-message format, atomic commits, code layout, and
more — follow the guidelines in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## What Pixel8 is

A PICO-8-like fantasy console where games ("carts") are written in **Rust**, compiled
to `wasm32-unknown-unknown`, and run sandboxed at 60 fps (or 30) inside the console.
The constraints are the product: 128x128 screen, 16 fixed colors, 256 8x8 sprites,
128x64 tile map, 64 SFX / 64 music patterns, 4 audio channels. Carts are shareable
PNG images with the wasm + assets (and optionally source) embedded.

## Build, test, lint

A one-time setup is required because cart-build tests cross-compile to wasm and the
console links ALSA:

```sh
rustup target add wasm32-unknown-unknown
sudo apt install libasound2-dev   # debian/ubuntu (or alsa-lib-devel on fedora)
```

```sh
# Boot the console (alias in .cargo/config.toml for `run --release -p pixel8-console`)
cargo console
cargo console -- examples/platformer    # boot with a project loaded, then type `run`

# Boot it in the terminal (alias for `run --release -p pixel8-tui`). Keep the `-p`
# when invoking cargo directly: `cargo run --bin pixel8-tui` at the workspace root
# unifies features across members and builds the wgpu/winit stack for nothing.
cargo tui

# Tests
cargo test --workspace
cargo test -p pixel8-runtime audio::   # single module/test

# Format — .rustfmt.toml uses nightly-only options, so fmt MUST run on nightly
cargo +nightly fmt --all

# Lint — CI treats warnings as errors
cargo clippy --workspace --all-targets -- -D warnings
```

CI (`.github/workflows/ci.yml`) runs three jobs that must stay green: `fmt` (nightly),
`clippy` (`-D warnings`), and `test` (workspace). Match them locally before pushing.

## Workspace layout

The workspace excludes `examples/` (those are standalone wasm crates). Six members:

- **`pixel8/`** — the SDK carts depend on, *deliberately zero-dependency*. `ffi.rs`
  declares the raw ABI imports (stubbed on non-wasm so carts type-check natively);
  `lib.rs` wraps them in zero-sized `Context` (update-time) and `Graphics` (draw-time);
  the `game!` macro exports `pixel8_init/update/draw` and installs the panic-forwarding
  hook. Defaults to `std`; disabling the `std` feature makes it `#![no_std]` for
  allocation-free carts with `heapless`.
- **`pixel8-runtime/`** — the heart. Modules: `fb` (128x128 indexed framebuffer),
  `font`, `palette`, `vm` (wasmi + ABI linking + fuel metering), `input`, `audio`
  (4-ch synth + cpal layer behind the `audio` feature), `assets`, `project`, `cart`
  (PNG codec), `pico8` (importer), `ui`.
- **`pixel8-console/`** — the console: a library (shell + editors) plus the windowed
  desktop frontend. **The binary it builds is named `pixel8`, not `pixel8-console`.**
  `lib.rs` exports `shell.rs` (the mode machine), `builder.rs`, `webexport.rs`, `ui.rs`
  and `editor/` (the five editors: `code`, `sprite`, `map`, `sfx`, `music`) for reuse
  by other frontends; the winit event loop in `main.rs` and `gpu.rs` (wgpu present)
  sit behind the default-on `window` feature, so frontends depending on the library
  with `default-features = false` never build the GPU stack. The `pixel8` binary and
  its headless subcommands build with or without `window` — CI's example/player
  workflows rely on the featureless binary to orchestrate cart builds.
- **`pixel8-tui/`** — the terminal frontend, a separate `pixel8-tui` binary: `tui.rs`
  (viuer sixel/half-block presenter + crossterm input) and `raw_keys.rs` (evdev key
  state for chords on terminals without key-release reporting; on Linux one of the
  two is required at startup) over the `pixel8-console` library with the `window`
  feature off. No winit/wgpu in its dependency tree, and no viuer/crossterm in the
  windowed console's.
- **`pixel8-web/`** — the browser player: `pixel8-runtime` compiled to wasm and wrapped
  in a C-like export surface. `cdylib` + `rlib` (rlib so player logic is host-testable).
- **`pixel8-player/`** — pure-Rust cart player with two cargo-feature backends:
  **`window`** (winit + softbuffer, the desktop default — opens a window with keyboard
  input) and **`kms`** (static-musl KMS/evdev/ALSA, built with
  `--no-default-features --features kms`, for handhelds and bare TTYs).

## Architecture essentials

Read `docs/ARCHITECTURE.md` for the full picture. Key invariants worth knowing before
touching code:

- **One screen, one rasterizer.** Everything visible — running carts, the boot console,
  every editor — is software-rendered into a single `Framebuffer` of palette indices.
  The GPU only uploads that as a texture and integer-scales/letterboxes it. There are no
  native widgets anywhere. A consequence: the whole console is **testable headless** —
  tests and the `verify`/`snap` subcommands drive the same framebuffer with no window.
- **The sandbox is allowlist-only.** `GameVm::load` links exactly the `"pixel8"` import
  set (~50 C-like functions, documented in `docs/ABI.md`); unknown imports fail
  instantiation. No WASI, no filesystem, no network. Fuel metering caps each
  `update`/`draw` so infinite loops become an error screen, not a freeze. The VM holds a
  *copy* of sprite/map assets so runtime `mset` writes are RAM-only, like a real cart.
- **Assets are one shared data model.** `pixel8-runtime/src/assets.rs` defines the serde
  models used by editors (mutate), the VM (draw/play), and the cart codec (embed). Sizes
  are fixed by design. On disk inside a project: one JSON, version-headered
  `assets.pixel8.json`; inside a cart: the `pxRt` PNG chunk (`docs/CART_FORMAT.md`).
- **The shell is a mode machine** (`Console`, `Run`, five editors). Loaded state is
  either a *project* (a real Cargo crate: full build/run/export) or a *cart* (PNG run
  as-is). `run` spawns `cargo build --release --target wasm32-unknown-unknown` on a
  thread, streams trimmed errors to the console, and hot-reloads when the wasm mtime
  changes (polled once a second).
- **Projects are real crates.** `pixel8 new` scaffolds an ordinary cargo crate building a
  `cdylib` for wasm + an `assets.pixel8.json`. `$EDITOR` + `cargo build` works identically to
  the integrated editor.

## Headless / CLI surface

Every pipeline stage also exists as a subcommand of the `pixel8` binary (dispatched in
`pixel8-console/src/main.rs`), which is how CI keeps examples runnable:
`new`, `build`, `export`, `extract`, `import-pico8`, `export-web`, `verify`, `snap`.

## Conventions

Commit messages, atomic commits, and top-down module ordering are covered in
`CONTRIBUTING.md` (see the top of this file). Project-specific note:

- Audio is feature-gated (`audio`, on by default). Code must still build and run
  (silently) with `--no-default-features` on the console/runtime for machines without
  ALSA.
- The windowed frontend is feature-gated too (`window`, on by default; it gates the
  `pixel8` binary and winit/wgpu). The console library must keep building with
  `--no-default-features` and with each feature alone — that featureless build is
  exactly what `pixel8-tui` depends on.

## Docs index

`docs/ABI.md` (wasm import surface), `docs/ARCHITECTURE.md`, `docs/CART_FORMAT.md`,
`docs/CLIPBOARD_FORMAT.md` (native JSON clipboard wire format + PICO-8 interop),
`docs/LIMITS.md` + `docs/LIMITS_TESTING.md`, `docs/PICO8_IMPORT.md`,
`docs/WEB_EXPORT.md`, `docs/HANDHELD.md`, `docs/TUI.md` (the `pixel8-tui` terminal
frontend). Design plans/specs live under `docs/superpowers/`.
