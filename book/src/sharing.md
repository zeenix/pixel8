# Sharing your cart

A finished Pixel8 game ships as one of two artifacts, both produced by
`export`: a **PNG cartridge** or a **single-file web page**.

## PNG cartridges

```text
> export mygame.png
```

or headless: `pixel8 export mygame/ mygame.png`. The output is a real PNG —
cartridge art, label, title — that any image viewer shows and any Pixel8
console plays:

```text
pixel8 mygame.png            # boot with the cart loaded, then `run`
pixel8 run mygame.png        # boot and run immediately
```

Before exporting, give the cartridge its face:

- **Label**: press `F6` while the game runs to capture the current screen as
  the cartridge label.
- **Metadata**: at the prompt, `title Space Miner 8` and `author you` (shown
  by `info` and on the cartridge).

By default the compressed Rust **source is embedded** in the cart, so anyone
can turn your cartridge back into an editable project — the console's
`import mygame.png mydir`, or `pixel8 extract mygame.png mydir` headless.
This is how fantasy-console culture spreads: play a cart, crack it open, see
how it's made. Export with `--no-source` if you'd rather not.

It's good hygiene to check a cart before sharing:

```text
pixel8 verify mygame.png     # load it and run 60 frames headless
```

## Web export

```text
> export mygame.html
```

or `pixel8 export-web mygame/ mygame.html` (it also accepts a `.png` cart as
input). This produces **one self-contained HTML file**: the cart plus the
whole console runtime compiled to WebAssembly, embedded. No server, no asset
folder — send the file, double-click it, play. Opening it shows the
cartridge art; clicking boots the cart (the click also satisfies the
browser's autoplay rule, so audio just works). While the cart runs,
`pause` and `stop` controls sit under the canvas — `Esc` pauses too,
a hidden tab pauses automatically, and `stop` returns to the
click-to-play screen. On touch screens the page grows an on-screen
d-pad and `O`/`X` buttons.

The playable carts embedded throughout this book are exactly these files —
each frame is one `export-web` output. Anything that hosts static files can
host one: itch.io, GitHub Pages, your blog.

Two things to know: every export weighs ~1.7 MB regardless of cart size (the
embedded runtime dominates), and a web export is a *player*, not a console —
no editors, no prompt, and no source embedded (ship the `.png` alongside if
you want people to crack it open). Details in
[docs/WEB_EXPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/WEB_EXPORT.md).

## The standalone player

`pixel8-player` plays carts with no editors attached — a console-style cart
picker over a folder of `.png` files:

```sh
cargo install pixel8-player
pixel8-player ~/carts
```

Its second life is on **retro handhelds** (PowKiddy RGB10S, Anbernic
RG351/353 and friends on ArkOS/ROCKNIX): a static-musl build runs on the bare
display with evdev input and ALSA sound — copy it into the ports folder, drop
carts next to it, play. The recipe is in
[docs/HANDHELD.md](https://github.com/zeenix/pixel8/blob/main/docs/HANDHELD.md).

## Which one, when

| you want                              | ship                        |
| ------------------------------------- | --------------------------- |
| players who have (or will get) Pixel8 | the `.png` cart             |
| anyone with a browser, zero friction  | the `.html` export          |
| a handheld in someone's pocket        | the `.png` cart + the player |
| people to learn from your code        | the `.png` cart with source (the default) |
