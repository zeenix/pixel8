# Drawing

All drawing happens in `draw`, through the [`Graphics`] handle. The screen is
128×128 pixels; `(0, 0)` is the top-left corner, `x` grows right, `y` grows
down. Positions are `i16` and sizes are `u16` — comfortably wider than the
screen, so things can sit off-screen and slide in.

You never manage a framebuffer, textures or "surfaces": you call draw
functions, the console rasterizes them, and everything drawn off-screen is
safely clipped.

## The palette

There are exactly 16 colors, and they never change. The `Color` type has a
named constant for each:

| | index | constant | | index | constant |
|---|---|---|---|---|---|
| <span class="swatch" style="background:#000000"></span> | 0 | `Color::BLACK` | <span class="swatch" style="background:#ff004d"></span> | 8 | `Color::RED` |
| <span class="swatch" style="background:#1d2b53"></span> | 1 | `Color::DARK_BLUE` | <span class="swatch" style="background:#ffa300"></span> | 9 | `Color::ORANGE` |
| <span class="swatch" style="background:#7e2553"></span> | 2 | `Color::DARK_PURPLE` | <span class="swatch" style="background:#ffec27"></span> | 10 | `Color::YELLOW` |
| <span class="swatch" style="background:#008751"></span> | 3 | `Color::DARK_GREEN` | <span class="swatch" style="background:#00e436"></span> | 11 | `Color::GREEN` |
| <span class="swatch" style="background:#ab5236"></span> | 4 | `Color::BROWN` | <span class="swatch" style="background:#29adff"></span> | 12 | `Color::BLUE` |
| <span class="swatch" style="background:#5f574f"></span> | 5 | `Color::DARK_GREY` | <span class="swatch" style="background:#83769c"></span> | 13 | `Color::LAVENDER` |
| <span class="swatch" style="background:#c2c3c7"></span> | 6 | `Color::LIGHT_GREY` | <span class="swatch" style="background:#ff77a8"></span> | 14 | `Color::PINK` |
| <span class="swatch" style="background:#fff1e8"></span> | 7 | `Color::WHITE` | <span class="swatch" style="background:#ffccaa"></span> | 15 | `Color::PEACH` |

A color from a runtime index is `Color::new(i)` (returns `None` past 15). It
is `const`, so `const ACCENT: Color = Color::new(8).unwrap();` fails at
*compile* time if the index is out of range.

## Shapes and text

```rust
fn draw(&self, gfx: &mut Graphics) {
    gfx.clear(Color::BLACK);                              // fill the screen
    gfx.set_pixel(10, 10, Color::WHITE);                  // one pixel
    gfx.line(0, 0, 127, 127, Color::DARK_GREY);           // inclusive endpoints
    gfx.rect(4, 4, 40, 20, Color::BLUE).unwrap();         // outline, w x h
    gfx.rect_fill(50, 4, 40, 20, Color::DARK_BLUE).unwrap();
    gfx.circle(64, 80, 10, Color::YELLOW);                // outline, radius
    gfx.circle_fill(64, 80, 6, Color::ORANGE);
    gfx.ellipse_fill(20, 70, 30, 16, Color::GREEN).unwrap(); // inside a w x h box
    gfx.print("score", 2, 2, Color::WHITE);               // 4x6 pixel font
}
```

Size-taking calls return `Result<(), ZeroSize>`: a zero (or negative,
computed) width or height draws nothing and tells you so. With literal sizes,
`.unwrap()` is the idiom; with computed sizes, handle or ignore with `let _ =`
as your game prefers.

`print` returns the x position after the last glyph, so you can continue a
line. For formatted text there's [`printf!`] — `format!` arguments with no
allocator, into a fixed stack buffer:

```rust
printf!(gfx, 2, 2, Color::YELLOW, "coins {}", self.coins);
```

There is also a persistent pen: `set_pen_color` / `set_cursor` /
`print_pen("...")` prints at the cursor in the pen color and advances one
line — handy for debug readouts.

## Sprites

The sprite sheet is a 128×128 pixel canvas divided into 256 cells of 8×8 —
sprite 0 is the top-left cell, numbering runs left-to-right, top-to-bottom.
Draw yours in the [sprite editor](console.md#the-five-editors), then:

```rust
gfx.sprite(SpriteId(1), self.x, self.y);   // one 8x8 cell
```

`sprite_ext` adds flipping and multi-cell sizes — `w`/`h` are in *pixels*, so
`16, 16` draws a 2×2-cell block and `8, 4` the top half of one cell:

```rust
gfx.sprite_ext(SpriteId(1), self.x, self.y, 8, 8, self.facing_left, false)
    .unwrap();
```

`sprite_stretch` (alias `sspr`) draws any sheet rectangle scaled to any
screen rectangle, nearest-neighbor — chunky zooms and simple scaling effects.

Animation is just choosing a different sprite each frame. The
[sprite_move](examples.md) example does the classic two-frame walk:

```rust
let frame = if self.walking && (self.frame / 4).is_multiple_of(2) { 2 } else { 1 };
```

<div class="cart-embed">
<iframe src="play/sprite_move.html" title="sprite_move — playable cart" loading="lazy"></iframe>
<span class="cart-caption">sprite_move: sprites, flipping and a two-frame walk animation
(<a href="https://github.com/zeenix/pixel8/blob/main/examples/sprite_move/src/lib.rs">source</a>).</span>
</div>

### Transparency and palette tricks

By default color 0 (black) is transparent in sprite draws. Change that per
color with `set_transparent_color` (alias `palt`), reset with
`reset_transparency`.

Two remapping tables unlock the classic tricks:

- `remap_color(from, to)` (alias `pal`) changes what later *draws* write —
  recolor one sprite into four enemy variants.
- `remap_display_color(from, to)` (alias `pal_display`) changes how the whole
  screen *shows* a color — flash the screen, fade to black.
- `reset_palette()` undoes both.

Filled shapes can also paint with a two-color 4×4 stipple via
`set_fill_pattern` / `fillp` — dithered skies, checkerboards, hatched
shadows. `clear_fill_pattern()` returns to solid fills.

## The map

The map is 128×64 tiles; each tile holds a sprite number (0 = empty). Paint
it in the map editor, then draw regions of it:

```rust
// Draw a 16x16-tile region, starting at map tile (0, 0), at screen (0, 0).
gfx.map(0, 0, 0, 0, 16, 16, BitFlags::empty()).unwrap();
```

The last argument filters by sprite flag: pass `SpriteFlag::Flag0` (or a
`|`-combination) to draw only tiles whose sprite has one of those flags set —
that's how you split one map into background and foreground layers.

The game can also *read* the map — `ctx.map_tile(x, y)` (alias `mget`) — and
that plus sprite flags is the whole collision story in tile-based games:

```rust
fn is_solid(ctx: &Context, px: i16, py: i16) -> bool {
    ctx.map_tile(px / 8, py / 8)
        .map(|tile| ctx.has_sprite_flag(tile, SpriteFlag::Flag0))
        .unwrap_or(false)
}
```

Writes (`ctx.set_map_tile` / `mset`) go to console RAM only and are discarded
on reload, like any self-respecting cartridge — the platformer uses this to
remove collected coins and put them back on restart. The same read/write pair
exists for sprite-sheet pixels (`sprite_pixel`/`set_sprite_pixel`).

## Camera and clipping

`gfx.camera(x, y)` offsets every subsequent draw by `(-x, -y)`. Scrolling a
level is: point the camera at the player, draw the map and actors in world
coordinates, then reset the camera to draw the HUD in screen coordinates:

```rust
fn draw(&self, gfx: &mut Graphics) {
    gfx.clear(Color::DARK_BLUE);
    gfx.camera(self.player_x - 64, 0);              // follow the player
    gfx.map(0, 0, 0, 0, 32, 16, BitFlags::empty()).unwrap();
    gfx.sprite(SpriteId(1), self.player_x, self.player_y);
    gfx.camera(0, 0);                               // back to screen space
    printf!(gfx, 2, 2, Color::YELLOW, "Score {}", self.score);
}
```

`gfx.clip(x, y, w, h)` restricts drawing to a rectangle (`clip_reset()`
lifts it) — split screens, minimaps, transition wipes.

## PICO-8 fingers welcome

Every drawing call also has its PICO-8-style short alias: `cls`, `pset`,
`pget`, `circ`, `circfill`, `rectfill`, `oval`, `spr`, `sspr`, `pal`, `palt`,
`fillp`... They are the same functions; use whichever names your fingers
know.

[`Graphics`]: https://docs.rs/pixel8/latest/pixel8/struct.Graphics.html
[`printf!`]: https://docs.rs/pixel8/latest/pixel8/macro.printf.html
