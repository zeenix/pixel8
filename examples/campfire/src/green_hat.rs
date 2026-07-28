use pixel8::*;

pub struct GreenHat {
    last_updated: f32,
    lean_in: bool,
}

impl GreenHat {
    pub fn new() -> Self {
        Self {
            last_updated: 0.0,
            lean_in: false,
        }
    }

    pub fn update(&mut self, ctx: &mut Context) {
        if ctx.time() - self.last_updated > UPDATE_INTERVAL {
            self.last_updated = ctx.time();
            self.lean_in = !self.lean_in;
        }
    }

    pub fn draw(&self, gfx: &mut Graphics) {
        let sprite_id = if self.lean_in {
            LEAN_IN_SPRITE
        } else {
            LEAN_BACK_SPRITE
        };

        gfx.sprite_ext(sprite_id, X, Y, WIDTH, HEIGHT, false, false)
            .unwrap();
    }
}

// Just in front of the fire & wood, sat back a step from the flames.
const X: i16 = 8 * 8 - 4;
const Y: i16 = 8 * 12;
const WIDTH: i16 = 16;
const HEIGHT: i16 = 16;
const LEAN_IN_SPRITE: SpriteId = SpriteId(34);
const LEAN_BACK_SPRITE: SpriteId = SpriteId(36);
const UPDATE_INTERVAL: f32 = 3.0;
