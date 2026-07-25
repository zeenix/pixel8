#![no_std]

use pixel8::{
    plume::{Direction, SmokingFire},
    *,
};

game!(Campfire = Campfire::new());

/// A campfire and nothing else, out of one of the SDK's plume effects: a fire whose spent flames
/// carry on as smoke, so the column reads as a single effect rather than a fire with smoke on top.
struct Campfire {
    fire: SmokingFire,
}

impl Campfire {
    fn new() -> Self {
        Self {
            fire: SmokingFire::new(FIRE_X, FIRE_Y, Direction::Up),
        }
    }
}

impl Game for Campfire {
    fn update(&mut self, ctx: &mut Context) {
        self.fire.update(ctx);
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);

        self.fire.draw(gfx);
    }
}

// The middle of the bed the flames rise from, near the bottom of the screen.
const FIRE_X: i16 = 64;
const FIRE_Y: i16 = 97;
