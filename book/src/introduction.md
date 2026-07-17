# Introduction

**Pixel8** (pronounced "pixelate") is a *fantasy console*: a complete little
game machine — screen, controls, sound chip, editors and all — that never
existed as hardware and lives entirely in software, dreamed up with the charm
and the limits of a 1980s handheld. Pixel8's particulars: a 128×128 screen,
16 fixed colors, a 4×6 pixel font, 256 sprites, a 128×64 tile map, four audio
channels — and where other fantasy consoles build in an interpreter for a
scripting language, Pixel8 has a **Rust** compiler. You write a little Rust,
it compiles to WebAssembly, and it runs sandboxed inside the console at a
steady 60 frames per second.

Here is one, running right on this page. Click the cartridge to boot it —
arrow keys run, `Z` jumps, collect the coins and grab the trophy:

<div class="cart-embed">
<iframe src="play/platformer.html" title="platformer — a playable Pixel8 cart" loading="lazy"></iframe>
<span class="cart-caption">A Pixel8 cart. The whole console fits in this box; the game inside it is <a href="https://github.com/zeenix/pixel8/tree/main/examples/platformer">a few hundred lines of Rust</a>.</span>
</div>

A finished game is a **cart**: a real PNG image with the compiled WebAssembly,
all the art and sound, and (by default) the compressed Rust source embedded
inside. Anyone can *look* at the cartridge; Pixel8 can *play* it; and if the
source is included, anyone can turn it back into an editable project.

## The constraints are the point

| thing     | size                              |
| --------- | --------------------------------- |
| screen    | 128 × 128 pixels, 16 fixed colors |
| sprites   | 256 of 8×8 pixels, 8 flags each   |
| map       | 128 × 64 tiles                    |
| sfx       | 64 slots, 32 steps, 8 waveforms   |
| music     | 64 patterns, 4 channels           |
| framerate | 60 fps (or 30, the cart's choice) |
| cart      | one PNG file, at most 128 KiB     |

Like the consoles it dreams of, Pixel8 is small on purpose. A blank canvas the
size of the ocean is paralyzing; 128×128 pixels and 16 colors you can fill by
Tuesday. The limits keep projects finishable, carts shareable, and the whole
system knowable — you can hold all of Pixel8 in your head.

## Who this book is for

You should know a little Rust — enough to read a `struct` and an `impl` block.
You do *not* need to know anything about game development, graphics or audio
programming; the whole point of a fantasy console is that those are simple
here.

Fantasy consoles were popularized by
[PICO-8](https://www.lexaloffle.com/pico-8.php), a much-loved commercial
console whose games are written in Lua, and Pixel8 is an open-source homage
to it: the palette, the editors, the `>` prompt and the workflow are all
lovingly borrowed. If you know PICO-8 you'll feel at home immediately — the
difference is the language, and everything that comes with it: a real type
system, real modules, `cargo`, and your usual editor and tooling if you want
them. If you've never touched PICO-8, no matter; nothing in this book
assumes it.

## How the book is organized

- **Getting started** installs the console, walks through creating and running
  your first cart, and tours the built-in editors.
- **Making a game** covers the SDK a chapter at a time: drawing, input, sound,
  saving data, and how to live comfortably inside the console's limits.
- **Shipping** shows how to turn a project into a shareable PNG cartridge or a
  single-file web page, and how to import assets from PICO-8 carts.
- **Reference** tours the example carts (all playable in this book) and points
  at the deeper documentation.

Everything in this book — every screen, every editor, every playable cart —
is produced by the same open-source console, which lives at
[github.com/zeenix/pixel8](https://github.com/zeenix/pixel8).
