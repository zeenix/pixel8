#![no_std]

mod green_hat;
mod owl;
mod smoker;

use pixel8::{plume::SmokingFire, *};

use green_hat::GreenHat;
use smoker::Smoker;

use crate::owl::Owl;

game!(Campfire = Campfire::new());

/// A campfire and nothing else, out of one of the SDK's plume effects: a fire whose spent flames
/// carry on as smoke, so the column reads as a single effect rather than a fire with smoke on top.
struct Campfire {
    fire: SmokingFire<6>,
    green_hat: GreenHat,
    smoker: Smoker,
    owl: Owl,
    /// The night's ambience. Held onto because dropping the handle stops the song.
    ambience: Option<PlayingMusic>,
}

impl Campfire {
    fn new() -> Self {
        Self {
            fire: SmokingFire::new(FIRE_X, FIRE_Y),
            green_hat: GreenHat::new(),
            smoker: Smoker::new(),
            owl: Owl::new(),
            ambience: None,
        }
    }
}

impl Game for Campfire {
    fn update(&mut self, ctx: &mut Context) {
        // Started here rather than in `new`, which has no `Context` to ask. The song loops on its
        // own, so this only ever runs on the first frame.
        if self.ambience.is_none() {
            self.ambience = ctx
                .music(AMBIENCE)
                .fade_in(AMBIENCE_FADE_IN_MS)
                .reserve_channels(Channel::Channel0 | Channel::Channel1 | Channel::Channel2)
                .play()
                .ok();
        }

        self.fire.update(ctx);
        self.green_hat.update(ctx);
        self.smoker.update(ctx);
        self.owl.update(ctx);
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);
        gfx.map(0, 0, 0, 0, 16, 16, BitFlags::empty()).unwrap();

        self.fire.draw(gfx);
        self.green_hat.draw(gfx);
        self.smoker.draw(gfx);
        self.owl.draw(gfx);
    }
}

// On top of the wood.
const FIRE_X: i16 = 8 * 11;
const FIRE_Y: i16 = 8 * 13 + 5;

// The night: the fire on one channel, crickets on another, and an owl on a third, over sixteen
// patterns that hand round to each other so nothing lands in the same place twice for about
// thirteen seconds. The owl calls once in that, and its call is split across two patterns because
// a channel starts afresh at each one. Channel three is left free for a cart to make its own noise
// over the top.
const AMBIENCE: MusicId = match MusicId::new(0) {
    Some(id) => id,
    None => panic!("0 is a music slot"),
};
const AMBIENCE_FADE_IN_MS: u32 = 1500;
