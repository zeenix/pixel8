# Pixel8 launch announcement

Post for LinkedIn & Mastodon.

---

🕹️ Pixel8 is out! A tiny fantasy console where the games are written in **Rust**.

Think PICO-8, but with a Rust compiler where the Lua interpreter would be: you write a
little Rust, it compiles to WebAssembly, and it runs sandboxed inside the console at a
steady 60 fps.

The constraints are the point:
▪️ 128x128 screen, 16 fixed colors
▪️ 256 8x8 sprites, a 128x64 tile map
▪️ 64 sound effects, 64 music patterns, 4 audio channels
▪️ 128 KiB carts, 128 KiB RAM, a per-frame CPU budget

What you get:
🖥️ A boot console + five built-in editors (code, sprite, map, sfx, music), all drawn on
the same 128x128 screen as the games — no native widgets anywhere.
📦 Projects are real Cargo crates. Prefer your own editor? `$EDITOR` + `cargo build`
works identically — the console hot-reloads the wasm on save.
🖼️ Carts are real PNG images — cartridge art with a screenshot of your game as the
label — and the *full game* is embedded inside: compiled wasm, all assets, and
(optionally) the Rust source. The picture you share IS the game; anyone can view it,
Pixel8 can play it, and with source included it re-imports back into an editable
project.
🌐 One-command web export: a single self-contained HTML file anyone can play in a
browser.
⌨️ A terminal frontend (`pixel8-tui`): the full console — editors, carts and all — over
sixel or unicode half-blocks.
🎮 A standalone player that runs on retro handhelds (PowKiddy, Anbernic & friends on
ArkOS/ROCKNIX) as a single static binary — drop it in the ports folder with your carts
and play.
🔁 A PICO-8 importer: sprites, flags, map, sfx and music from `.p8`/`.p8.png` carts
transfer almost one-to-one into a new Rust project.
🔒 A real sandbox: carts run in wasmi with no filesystem, no network, no WASI — just a
small C-like ABI — and fuel metering turns infinite loops into a friendly error screen
instead of a freeze.

Free and open source (GPL-3.0), on crates.io today:

    cargo install pixel8-console
    pixel8

Code, docs and example carts: https://github.com/zeenix/pixel8

#rust #rustlang #gamedev #wasm #webassembly #pico8 #fantasyconsole #opensource
