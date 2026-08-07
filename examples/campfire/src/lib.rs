#![no_std]

mod green_hat;
mod owl;
mod smoker;

use core::ops::RangeInclusive;

use pixel8::{physics::Wind, plume::SmokingFire, *};

use green_hat::GreenHat;
use smoker::Smoker;

use crate::owl::Owl;

game!(Campfire = defer Campfire::new());

/// A campfire and nothing else, out of one of the SDK's plume effects: a fire whose spent flames
/// carry on as smoke, so the column reads as a single effect rather than a fire with smoke on top.
struct Campfire {
    /// Kept small: the night air leans the flames a few pixels off their bed, and the pile of
    /// wood they burn on is only two tiles wide.
    fire: SmokingFire<5>,
    /// The night air. A plume handed a wind takes all of its swaying from it, so this is not an
    /// extra on top of the fire — it is what the fire sways in, and one of them for the whole
    /// scene means the cigarette wanders on the same breath.
    wind: Wind,
    green_hat: GreenHat,
    smoker: Smoker,
    owl: Owl,
    /// The night's ambience. Held onto because dropping the handle stops the song.
    ambience: Option<PlayingMusic>,
}

impl Campfire {
    /// Built at start-up rather than shipped placed — the `defer` form of `game!` — because the
    /// plumes and the gusting wind are put together by ordinary constructors, and a constant may
    /// call none of them. The cost is the stack: for a moment the whole campfire exists twice,
    /// once here and once in the static it is moved into.
    fn new() -> Self {
        Self {
            fire: SmokingFire::new(FIRE_X, FIRE_Y),
            wind: Wind::new(WIND_SPEED).with_gusts(WIND_GUSTS),
            green_hat: GreenHat::new(),
            smoker: Smoker::new(),
            owl: Owl::new(),
            ambience: None,
        }
    }
}

impl Game for Campfire {
    fn boot(&mut self, ctx: &mut Context) {
        // Started here rather than in `new`, which has no `Context` to ask. The song loops on its
        // own, and boot runs once, so this is the whole of it.
        self.ambience = ctx
            .music(AMBIENCE)
            .fade_in(AMBIENCE_FADE_IN_MS)
            .reserve_channels(Channel::Channel0 | Channel::Channel1 | Channel::Channel2)
            .play()
            .ok();
    }

    fn update(&mut self, ctx: &mut Context) {
        // The gust first: everything that sways this frame sways on the same one.
        self.wind.update(ctx);
        self.fire.blown_by(&self.wind);

        self.fire.update(ctx);
        self.green_hat.update(ctx);
        self.smoker.update(ctx, &self.wind);
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

// On top of the wood, in the middle of the pile: the air only sways the column, so the flames
// stay over the bed they burn on and well short of the figure sat to the left of it.
const FIRE_X: i16 = 8 * 11;
const FIRE_Y: i16 = 8 * 13 + 5;

// Air that barely moves, wandering either side of still with a lean towards the tree — the way
// the cigarette smoke already drifts. A plume takes all of its swaying from the wind it is given,
// so this is tuned against how the fire swayed before it had one: the same wander, a touch wider,
// and a column that spends most of its time upright. The ends are further out than they look,
// because a gust turns back at a random point past the middle of its range and so spends little
// of its time near them.
const WIND_SPEED: f32 = -0.05;
const WIND_GUSTS: RangeInclusive<f32> = -0.55..=0.45;

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
