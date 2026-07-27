use pixel8::*;

pub struct Owl {
    last_updated: f32,
    eyes_closed: bool,
}

impl Owl {
    pub fn new() -> Self {
        Self {
            last_updated: 0.0,
            eyes_closed: false,
        }
    }

    pub fn update(&mut self, ctx: &mut Context) {
        let duration = if self.eyes_closed {
            EYES_CLOSED_DURATION
        } else {
            EYES_OPEN_DURATION
        };
        if ctx.time() - self.last_updated > duration {
            self.last_updated = ctx.time();
            self.eyes_closed = !self.eyes_closed;
        }
    }

    pub fn draw(&self, gfx: &mut Graphics) {
        let sprite_id = if self.eyes_closed {
            EYES_CLOSED_SPRITE
        } else {
            EYES_OPEN_SPRITE
        };

        gfx.sprite(sprite_id, X, Y);
    }
}

// Just in front of the fire & wood (on the right side).
const X: i16 = 8 * 6;
const Y: i16 = 8 * 10;
const EYES_OPEN_SPRITE: SpriteId = SpriteId(48);
const EYES_CLOSED_SPRITE: SpriteId = SpriteId(49);
const EYES_OPEN_DURATION: f32 = 5.0;
const EYES_CLOSED_DURATION: f32 = 0.5;
