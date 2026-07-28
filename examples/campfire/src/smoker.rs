use pixel8::{physics::Wind, plume::Smoke, *};

pub struct Smoker {
    smoke: Smoke<1>,
    last_updated: f32,
    inhaling: bool,
}

impl Smoker {
    pub fn new() -> Self {
        Self {
            smoke: Smoke::new(EXTENDED_SMOKE_X, EXTENDED_SMOKE_Y)
                .with_direction(Direction::UpLeft)
                .with_puffs(SMOKE_PUFFS),
            last_updated: 0.0,
            inhaling: false,
        }
    }

    pub fn update(&mut self, ctx: &mut Context, wind: &Wind) {
        let duration = if self.inhaling {
            INHALING_DURATION
        } else {
            EXTENDING_DURATION
        };
        if ctx.time() - self.last_updated > duration {
            self.last_updated = ctx.time();
            self.inhaling = !self.inhaling;
            // Nothing new comes off the cigarette while it is at their lips, but the last of it
            // is still in the air and drifts away on its own.
            self.smoke.set_puffing(!self.inhaling);
        }

        // The same air the fire sways in, on a wisp small enough to show every bit of it. It
        // takes the place of the drifting the smoke does for itself, rather than adding to it.
        self.smoke.blown_by(wind);
        self.smoke.update(ctx);
    }

    pub fn draw(&self, gfx: &mut Graphics) {
        let sprite_id = if self.inhaling {
            INHALING_SPRITE
        } else {
            EXTENDING_HAND_SPRITE
        };

        gfx.sprite_ext(sprite_id, X, Y, WIDTH, HEIGHT, false, false)
            .unwrap();
        self.smoke.draw(gfx);
    }
}

// Just in front of the fire & wood (on the right side).
const X: i16 = 8 * 14;
const Y: i16 = 8 * 12;
const WIDTH: i16 = 16;
const HEIGHT: i16 = 16;
const EXTENDED_SMOKE_X: i16 = X + 1;
const EXTENDED_SMOKE_Y: i16 = Y + 6;
const EXTENDING_HAND_SPRITE: SpriteId = SpriteId(12);
const INHALING_SPRITE: SpriteId = SpriteId(14);
// A cigarette's worth of smoke: the smallest plume there is, and thinned right down, or it sits
// on the smoker's face as a lump instead of drifting off it.
const SMOKE_PUFFS: usize = 8;
const EXTENDING_DURATION: f32 = 5.0;
const INHALING_DURATION: f32 = 3.0;
