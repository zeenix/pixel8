# Input

A Pixel8 console has six buttons: the four directions plus two action
buttons, **O** and **X**. On a keyboard:

| button | keys |
| ------ | ---- |
| `Button::Left` / `Right` / `Up` / `Down` | arrow keys |
| `Button::O` | <kbd>Z</kbd> (also <kbd>C</kbd>, <kbd>N</kbd>) |
| `Button::X` | <kbd>X</kbd> (also <kbd>V</kbd>, <kbd>M</kbd>) |

On retro handhelds and in web exports on touch screens, the d-pad and two
face buttons map to the same six. Design for six buttons and your game runs
everywhere Pixel8 does.

## Held vs. pressed

Input is read in `update`, from the [`Context`]:

```rust
fn update(&mut self, ctx: &mut Context) {
    // Held: true every frame while the key is down. Movement.
    if ctx.is_button_down(Button::Right) {
        self.x += 1;
    }
    // Pressed: true on the frame it goes down (then key-repeat after a
    // short delay: 15 frames, then every 4). Jumping, menus, toggles.
    if ctx.is_button_pressed(Button::O) {
        self.jump();
    }
}
```

The aliases `btn` and `btnp` are the same two functions with PICO-8's names.

## Whole-state reads

`buttons_down()` and `buttons_pressed()` return all six as a `BitFlags<Button>`
set — often tidier than six ifs, and the natural shape for diagonals:

```rust
let held = ctx.buttons_down();
if held.contains(Button::UP_RIGHT) {   // Up and Right together
    // ...
}
let dx = i16::from(held.contains(Button::Right)) - i16::from(held.contains(Button::Left));
let dy = i16::from(held.contains(Button::Down)) - i16::from(held.contains(Button::Up));
```

`Button::UP_LEFT`, `UP_RIGHT`, `DOWN_LEFT` and `DOWN_RIGHT` are provided as
ready-made two-button sets.

## Smooth sub-pixel movement: `Body`

Speeds don't have to be whole pixels — keep positions as `f32` and cast when
drawing. But there's a classic gotcha: a sprite moving *diagonally* at less
than a pixel per frame zigzags, because `x` and `y` cross their pixel
boundaries on different frames. (PICO-8 has this too; it's integer-grid
geometry, not a bug.)

The SDK's opt-in [`Body`] fixes it. It owns the exact position, and emits a
*phase-coherent* pixel position for drawing — both axes step together, so the
diagonal is a clean staircase:

```rust
struct Mob { body: Body }   // Body::new(x, y) to create

fn update(&mut self, ctx: &mut Context) {
    let speed = 0.6;
    let held = ctx.buttons_down();
    let mut dx = 0.0;
    let mut dy = 0.0;
    if held.contains(Button::Left)  { dx -= speed; }
    if held.contains(Button::Right) { dx += speed; }
    if held.contains(Button::Up)    { dy -= speed; }
    if held.contains(Button::Down)  { dy += speed; }
    self.body.move_by(dx, dy);
}

fn draw(&self, gfx: &mut Graphics) {
    gfx.clear(Color::BLACK);
    // Draw at the coherent pixel; collide against the exact body.x()/y().
    gfx.sprite(SpriteId(1), self.body.draw_x(), self.body.draw_y());
}
```

The drawn pixel never strays more than one pixel from the true position, so
collision against `x()`/`y()` stays honest. The platformer's hero rides a
`Body`, which is why a running jump doesn't shimmer.

## There is no mouse, and that's fine

No mouse, no text entry, no gamepad rumble — six buttons is the entire input
model. Like the 128×128 screen, it's a constraint that designs half your
control scheme for you.

[`Context`]: https://docs.rs/pixel8/latest/pixel8/struct.Context.html
[`Body`]: https://docs.rs/pixel8/latest/pixel8/struct.Body.html
