# Importing from PICO-8

Pixel8 shares its palette, waveforms and sprite layout with
[PICO-8](https://www.lexaloffle.com/pico-8.php) — a deliberate act of
heritage. The practical payoff: a PICO-8 cart's *assets* import almost
one-to-one.

```text
pixel8 import-pico8 mygame.p8 mygame      # or mygame.p8.png
```

(or `import-pico8 mygame.p8 mygame` at the console prompt.) This creates a
new project with the cart's graphics, sprite flags, map, sound effects and
music transferred in, plus a stub `src/lib.rs` to write the game in Rust.

**Only the assets come across — the Lua code is ignored.** Pixel8 doesn't
run or translate Lua; porting the game logic to Rust is your (fun) job. In
practice this is a lovely way to port a game: all the art and sound are
instantly in place, so you can concentrate on the code, comparing behavior
side by side.

## Appending into an existing project

`--into` merges selected assets into a project you already have, instead of
creating a new one:

```text
pixel8 import-pico8 mygame.p8 --into myproject [--sprites R] [--sfx R] [--music R]
```

with `R` selecting ranges of slots — cherry-pick a sprite sheet from one
cart and sound effects from another.

## What maps, and how faithfully

Graphics and maps transfer essentially exactly (same palette, same 8×8
cells, same 128×64 map). Audio maps waveform-for-waveform onto Pixel8's
synth, which is close but not sample-identical to PICO-8's. The fine print —
per-asset mapping tables and known differences — lives in
[docs/PICO8_IMPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/PICO8_IMPORT.md).

One direction only: Pixel8 imports from PICO-8; it doesn't export to it.
