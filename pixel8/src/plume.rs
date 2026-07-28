//! Fire and smoke, as particle plumes.
//!
//! A *plume* is a stream of particles that spawn around a base point, travel away from it and
//! fade out on the way: the shape shared by a campfire, the smoke above it, a rocket's exhaust
//! and the trail from a damaged engine. [`Fire`] and [`Smoke`] are the two this module ships,
//! and they cost nothing but code — no sprites, no map, no assets of any kind.
//!
//! Both are sized at compile time and never allocate: a plume owns a fixed-capacity buffer of
//! particles, six bytes each. A full-size [`Fire`] holds 260 of them (about 1.5 KiB) and a
//! [`SmokingFire`] twice that.
//!
//! They are not free, though, and the bill lands in `draw`: every particle is one filled circle,
//! costing about 0.05% of the draw budget. A full-size [`Fire`] runs at roughly 6% of `update`
//! and 12% of `draw`, and a [`SmokingFire`] or a [`Smoke`] — twice the particles — about 10% and
//! 24%. Scale a plume down and both fall away with the particle count. Budget for the ones on
//! screen at once, and reach for a smaller `SCALE` before giving up on the effect.
//!
//! Those are budget figures, which count the cart's side of each circle and not the console's
//! rasterizing of it — see [`Context::cpu_draw`]. Particle radii are small, so the two track
//! each other closely here, but on a slow device trust [`Context::fps`] over the percentages.
//!
//! Everything here stays in this module — a cart's `use pixel8::*;` does not reach it, so name
//! what the effect needs:
//!
//! ```no_run
//! use pixel8::{plume::SmokingFire, *};
//!
//! struct Camp {
//!     fire: SmokingFire,
//! }
//!
//! impl Game for Camp {
//!     fn update(&mut self, ctx: &mut Context) {
//!         self.fire.update(ctx);
//!     }
//!
//!     fn draw(&self, gfx: &mut Graphics) {
//!         gfx.clear(Color::BLACK);
//!         self.fire.draw(gfx);
//!     }
//! }
//! ```
//!
//! # Size
//!
//! The `SCALE` parameter sizes a plume. [`FULL_SCALE`] is a fire about 30 pixels tall, a `2` or
//! a `3` the flame of a candle or a torch, and anything up to [`MAX_SCALE`] grows it further:
//!
//! ```no_run
//! # use pixel8::plume::Fire;
//! let candle: Fire<2> = Fire::new(64, 100);
//! let bonfire: Fire<20> = Fire::new(64, 100);
//! ```
//!
//! `SCALE` is also how many particles go in each puff, and that is the one part of a plume that
//! cannot keep shrinking: a puff holds at least one particle however small the plume is, while
//! the ground it covers shrinks with the square. So the smaller a plume gets the more crowded it
//! is — a `Fire<1>` is around twelve times as dense as a `Fire<10>` — which is what
//! [`with_puffs`](Fire::with_puffs) is for.
//!
//! # Thinning a small plume
//!
//! A plume puffs once an update by default, which at [`FULL_SCALE`] is what makes it look like
//! something billowing. Far below that the puffs land on top of each other and it reads as a
//! solid lump instead. Building the plume out of fewer, further-apart puffs fixes it:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, Direction};
//! // A cigarette: the smallest plume there is, and thinned, or it is a blob on someone's face.
//! let wisp: Smoke<1> = Smoke::new(64, 100)
//!     .with_direction(Direction::UpLeft)
//!     .with_puffs(8);
//! ```
//!
//! Particles still move every update, so the plume keeps the reach, pace and direction it had —
//! there is simply less in it, spaced further apart. The colors spread out with the puffs too,
//! so a thinned plume greys along its length instead of in its first pixel, and it costs
//! proportionally less to update and draw. What it does not give back is memory: the buffer is
//! sized for a puff an update whether or not the plume uses them.
//!
//! # Direction
//!
//! Plumes travel in one of eight [`Direction`]s. They rise unless told otherwise, which is what
//! a fire wants; smoke pouring from a damaged aircraft flying up-screen wants
//! [`Down`](Direction::Down), and a plume in a crosswind one of the diagonals. Particles sway
//! from side to side as they travel, so the sway follows the direction too.
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, Direction};
//! let exhaust: Smoke<4> = Smoke::new(64, 40).with_direction(Direction::Down);
//! ```
//!
//! A plume can be turned as it runs, with [`Fire::set_direction`] / [`Smoke::set_direction`].
//! Particles already in the air carry on the way they were going, so a plume that turns bends
//! rather than swinging around all at once.
// Everything this section documents — `blown_by`, the `Wind` it takes — exists only with the
// `physics` feature, and it links to all of it. So the section is gated too, or a cart that
// linked the plumes alone would document itself with links that resolve to nothing. Doc
// attributes render in source order, so it stays where it reads.
#![cfg_attr(
    feature = "physics",
    doc = r#"
# Wind

A plume leans from side to side as it travels, a slow wander that never repeats itself. That sway
is the weather a plume invents for itself, having none — and it is exactly what a [`Wind`] takes
the place of. With the `physics` feature on as well, [`Fire::blown_by`] and [`Smoke::blown_by`]
hand a plume the real thing, and from then on the wind is the only thing pushing it sideways. The
two never add up, so a scene that gives its plumes a gentle wind gets the sway it always had,
blowing the way the rest of the scene blows.

That leaves the shape of the wind the shape of the plume:

- A gusty wind wandering a range that straddles nothing sways a plume much as it swayed on its
  own — [`with_gusts`](Wind::with_gusts) wanders by the same trick the sway does, reversing at a
  random point so that neither settles into a rhythm.
- A steady wind holds the plume at a lean, with no wander left in it at all.
- A wind blowing at nothing stills it, and it travels dead straight.

A gust reaches the whole plume at once, but not evenly: a particle leaves the source already in a
share of the wind and is in all of it by the time it is old. So a column bends away along its
length instead of sliding sideways in one piece, and a gust shakes a fire where it stands while
carrying the smoke above it clean off.

```no_run
# use pixel8::{physics::Wind, plume::SmokingFire, *};
struct Camp {
    fire: SmokingFire,
    wind: Wind,
}

impl Game for Camp {
    fn update(&mut self, ctx: &mut Context) {
        // The gust first, so everything the wind touches this frame is bent by the same one.
        self.wind.update(ctx);
        self.fire.blown_by(&self.wind);
        self.fire.update(ctx);
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);
        self.fire.draw(gfx);
    }
}
```

The one place the swap shows is which way sideways is. A plume's own sway lies across its travel,
wherever it is pointed, while a wind blows its own way across the screen — so a plume that is not
pointed square across the wind takes part of it along its own length as well, as the headwind or
the tailwind it ought to be.

A [`Wind`] has a [`Direction`] of its own, and it is not this one. A plume's says where it is
pointed; a wind's says the side it comes *from*, weather being named that way round. The two are
about different things besides: the direction belongs to the source, while the wind bends what has
already left it. A fire in a gale still burns upwards; it is its smoke that ends up sideways.
"#
)]
//! # Starting and stopping
//!
//! [`Fire::set_puffing`] / [`Smoke::set_puffing`] turn the source off and on. A plume that has
//! stopped is not a plume that has vanished: it keeps everything it has already let go of, and
//! that carries on rising, greying and thinning out until it ages away. So the plume empties over
//! a `LIFETIME` from the base up, which is how a real one goes out.
//!
//! Simply not drawing it would blink the whole thing away instead — the difference between a
//! cigarette between draws and one that stops existing:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, *};
//! # struct Smoker { smoke: Smoke<1>, inhaling: bool }
//! impl Smoker {
//!     fn update(&mut self, ctx: &mut Context) {
//!         // Nothing new off the cigarette while it is at their lips; the last of it drifts off.
//!         self.smoke.set_puffing(!self.inhaling);
//!         self.smoke.update(ctx);
//!     }
//! }
//! ```
//!
//! # Trails
//!
//! A plume is not pinned to where it started: [`Fire::move_to`] and [`Smoke::move_to`] move the
//! point it spawns from. Particles already in the air keep the base they came off, so moving a
//! plume trails it rather than dragging everything it has emitted along — which is what makes a
//! plume a trail. That damaged aircraft is a `Smoke` moved to the plane every frame:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, *};
//! struct Plane {
//!     x: i16,
//!     y: i16,
//!     smoke: Smoke<4>,
//! }
//!
//! impl Plane {
//!     fn update(&mut self, ctx: &mut Context) {
//!         self.y -= 1;
//!         // Move first, and this frame's puff comes off where the plane is now.
//!         self.smoke.move_to(self.x, self.y);
//!         self.smoke.update(ctx);
//!     }
//! }
//! ```
//!
//! # Smoke from fire
//!
//! [`Smoke`] on its own is a plume that starts wherever it is placed. A fire that *turns into*
//! smoke is a different thing: its particles have to keep the position and the sway they had as
//! flames, or the two effects read as unrelated. That is what [`SmokingFire`] is — one plume
//! whose particles live twice as long, spending the second half of their life grey and drifting
//! at half the pace.

use core::{array, ops::Range};

use heapless::Deque;

#[cfg(feature = "physics")]
use crate::physics::Wind;
use crate::{Color, Context, Direction, Graphics};

/// A fire: a plume of flame particles, white-hot at the base and fading to orange at the tips.
///
/// `SCALE` sizes the fire (see the [module docs](self#size)) and `LIFETIME` is how many updates
/// each particle lives for, which is what makes the flames as long as they are. Particles that
/// outlive [`DEFAULT_LIFETIME`] turn to smoke, so a `LIFETIME` past it — [`SmokingFire`] — is a
/// fire that smokes.
#[derive(Debug)]
pub struct Fire<const SCALE: usize = FULL_SCALE, const LIFETIME: usize = DEFAULT_LIFETIME> {
    plume: Plume<SCALE, LIFETIME>,
}

impl<const SCALE: usize, const LIFETIME: usize> Fire<SCALE, LIFETIME> {
    /// A new fire based at the pixel position (`x`, `y`), burning upwards. Point it another way
    /// with [`with_direction`](Self::with_direction).
    ///
    /// The base is the middle of the bed the flames rise from — for a campfire, the logs — and
    /// is where they are widest; they narrow as they travel.
    pub fn new(x: i16, y: i16) -> Self {
        Self {
            plume: Plume::new(x, y, FIRE_SPEED, Some(DEFAULT_LIFETIME)),
        }
    }

    /// Sets which way the flames burn, to chain onto [`new`](Self::new).
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.set_direction(direction);
        self
    }

    /// Builds the fire out of `puffs` puffs of flame rather than one an update, to chain onto
    /// [`new`](Self::new).
    ///
    /// This is what keeps a small fire from burning as a solid lump — see
    /// [Thinning a small plume](self#thinning-a-small-plume). `puffs` is clamped to `1..=LIFETIME`,
    /// and `LIFETIME` (the default) is a puff an update.
    pub fn with_puffs(mut self, puffs: usize) -> Self {
        self.plume.set_puffs(puffs);
        self
    }

    /// Turns the fire to burn towards `direction`.
    ///
    /// Flames already in the air keep burning the way they were, so a fire that turns bends
    /// instead of swinging around all at once.
    pub fn set_direction(&mut self, direction: Direction) {
        self.plume.set_direction(direction);
    }

    /// Lets the fire go on throwing off flames, or stops it without clearing the screen of it.
    ///
    /// A fire that has stopped keeps what is already alight: the last flames rise, fade and go
    /// out on their own, so it dies down over a `LIFETIME` rather than blinking away. Start it
    /// again and it picks straight back up. See the [module docs](self#starting-and-stopping).
    pub fn set_puffing(&mut self, puffing: bool) {
        self.plume.set_puffing(puffing);
    }

    /// Moves the fire to (`x`, `y`), for one carried or burning on something that moves.
    ///
    /// Flames already in the air stay where they were let go, so a fire on the move trails
    /// behind itself. Move it before [`update`](Self::update) and the next flames come off the
    /// new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
    }

    /// Hands the flames over to `wind`, which from here on is the only thing that leans them.
    ///
    /// A fire sways on its own, having no weather of its own to answer to; this gives it some, and
    /// the sway stands down rather than adding to it. So a wind gusting either side of nothing
    /// sways the fire about as much as it swayed alone, a steady one holds it at a lean, and a
    /// wind blowing at nothing stands it up straight. See the [module docs](self#wind).
    ///
    /// The wind's speed is read as it stands, so this belongs in
    /// [`Game::update`](crate::Game::update) just after [`Wind::update`]; what it reads holds
    /// until the next call. A gust takes hold of the whole fire, though not evenly: the youngest
    /// flames are in a share of it and the spent ones above go with it entirely, so the fire
    /// flickers where it stands while a [`SmokingFire`]'s smoke is carried off.
    ///
    /// Only what the wind [`blow`](Wind::blow)s is read — its direction and its speed, and not
    /// its exposure: a plume sets its own pace against a wind through that age ramp. Either axis
    /// of it past [`MAX_WIND_SPEED`] is clamped to it.
    #[cfg(feature = "physics")]
    pub fn blown_by(&mut self, wind: &Wind) {
        self.plume.blown_by(wind);
    }

    /// Advances the flames by one frame. Call this from [`Game::update`](crate::Game::update).
    pub fn update(&mut self, ctx: &mut Context) {
        self.plume.update(ctx);
    }

    /// Draws the flames. Call this from [`Game::draw`](crate::Game::draw).
    pub fn draw(&self, gfx: &mut Graphics) {
        self.plume.draw(gfx, FIRE_BIRTH_COLOR, &FIRE_COLOR_STOPS);
    }
}

/// A fire whose burnt-out particles turn into smoke instead of vanishing.
///
/// The smoke picks up exactly where the flames end — same positions, same sway — and drifts on
/// at half their pace, so the two read as one column rather than two effects sharing a screen.
pub type SmokingFire<const SCALE: usize = FULL_SCALE> = Fire<SCALE, SMOKING_FIRE_LIFETIME>;

/// The `LIFETIME` of a [`SmokingFire`]: a full flame life followed by an equally long smoky one.
pub const SMOKING_FIRE_LIFETIME: usize = 2 * DEFAULT_LIFETIME;

/// The color a flame particle is born with.
const FIRE_BIRTH_COLOR: Color = Color::WHITE;

/// What a flame particle turns into, and at what age: yellow, then orange, then — for a
/// [`SmokingFire`], whose particles live long enough to reach them — the two greys of smoke.
const FIRE_COLOR_STOPS: [(usize, Color); 4] = [
    (4, Color::YELLOW),
    (10, Color::ORANGE),
    (DEFAULT_LIFETIME, Color::LIGHT_GREY),
    (DEFAULT_LIFETIME + 10, Color::DARK_GREY),
];

/// How fast flame particles rise, in sub-pixel units per update at [`FULL_SCALE`].
const FIRE_SPEED: Range<i32> = SUBPIXELS / 2..SUBPIXELS * 3 / 2;

/// Smoke: a plume of grey particles that darken as they disperse.
///
/// This is smoke with no fire under it — an exhaust or a smouldering wreck. For smoke that
/// continues a fire, use [`SmokingFire`].
///
/// `SCALE` sizes the plume (see the [module docs](self#size)) and `LIFETIME` is how many updates
/// each particle lives for. Smoke drifts at half a fire's pace, so its default lifetime is twice
/// as long: a plume the length of a fire's flames, but thinner and slower.
#[derive(Debug)]
pub struct Smoke<const SCALE: usize = FULL_SCALE, const LIFETIME: usize = SMOKE_LIFETIME> {
    plume: Plume<SCALE, LIFETIME>,
}

impl<const SCALE: usize, const LIFETIME: usize> Smoke<SCALE, LIFETIME> {
    /// A new smoke plume based at the pixel position (`x`, `y`) — the middle of the bed it rises
    /// from — billowing upwards. Point it another way with
    /// [`with_direction`](Self::with_direction).
    pub fn new(x: i16, y: i16) -> Self {
        Self {
            plume: Plume::new(x, y, SMOKE_SPEED, None),
        }
    }

    /// Sets which way the smoke billows, to chain onto [`new`](Self::new).
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.set_direction(direction);
        self
    }

    /// Builds the plume out of `puffs` puffs rather than one an update, to chain onto
    /// [`new`](Self::new).
    ///
    /// This is what turns the smallest scales from a solid lump into a wisp — see
    /// [Thinning a small plume](self#thinning-a-small-plume). `puffs` is clamped to `1..=LIFETIME`,
    /// and `LIFETIME` (the default) is a puff an update.
    pub fn with_puffs(mut self, puffs: usize) -> Self {
        self.plume.set_puffs(puffs);
        self
    }

    /// Turns the smoke to billow towards `direction`.
    ///
    /// Smoke already in the air keeps drifting the way it was, so the trail bends behind a
    /// source that turns rather than swinging around all at once.
    pub fn set_direction(&mut self, direction: Direction) {
        self.plume.set_direction(direction);
    }

    /// Lets the source go on puffing, or shuts it off without clearing the screen of it.
    ///
    /// Smoke already in the air is left to drift, fade and thin out on its own, so a source that
    /// shuts off empties over a `LIFETIME` rather than blinking away — which is what a cigarette
    /// between draws, or an engine that cuts out, actually looks like. Open it again and the next
    /// puff comes off the base as usual. See the [module docs](self#starting-and-stopping).
    pub fn set_puffing(&mut self, puffing: bool) {
        self.plume.set_puffing(puffing);
    }

    /// Moves the source of the smoke to (`x`, `y`), for a trail off something that moves.
    ///
    /// Smoke already in the air stays where it was let go — which is what makes it a trail
    /// rather than a cloud dragged along. Move it before [`update`](Self::update) and the next
    /// puff comes off the new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
    }

    /// Hands the smoke over to `wind`, which from here on is the only thing that leans it.
    ///
    /// Smoke drifts from side to side on its own, having no weather of its own to answer to; this
    /// gives it some, and that drifting stands down rather than adding to it. So a wind gusting
    /// either side of nothing wanders the trail about as much as it wandered alone, a steady one
    /// holds it at a lean, and a wind blowing at nothing sends it straight up. See the
    /// [module docs](self#wind).
    ///
    /// The wind's speed is read as it stands, so this belongs in
    /// [`Game::update`](crate::Game::update) just after [`Wind::update`]; what it reads holds
    /// until the next call. A puff leaves the source already in a share of the wind and is taken
    /// by all of it a little under half a second later, so the trail bends away along its length
    /// rather than shearing off as a block.
    ///
    /// Only what the wind [`blow`](Wind::blow)s is read — its direction and its speed, and not
    /// its exposure: a plume sets its own pace against a wind through that ramp. Either axis of
    /// it past [`MAX_WIND_SPEED`] is clamped to it.
    #[cfg(feature = "physics")]
    pub fn blown_by(&mut self, wind: &Wind) {
        self.plume.blown_by(wind);
    }

    /// Advances the smoke by one frame. Call this from [`Game::update`](crate::Game::update).
    pub fn update(&mut self, ctx: &mut Context) {
        self.plume.update(ctx);
    }

    /// Draws the smoke. Call this from [`Game::draw`](crate::Game::draw).
    pub fn draw(&self, gfx: &mut Graphics) {
        self.plume.draw(gfx, SMOKE_BIRTH_COLOR, &SMOKE_COLOR_STOPS);
    }
}

/// The default `LIFETIME` of [`Smoke`]: twice [`DEFAULT_LIFETIME`], which at half a fire's pace
/// spans the same length.
pub const SMOKE_LIFETIME: usize = 2 * DEFAULT_LIFETIME;

/// The color a smoke particle is born with.
const SMOKE_BIRTH_COLOR: Color = Color::WHITE;

/// What a smoke particle turns into, and at what age. The ages are twice a fire's, matching the
/// halved pace, so each band of color covers the same distance.
const SMOKE_COLOR_STOPS: [(usize, Color); 2] = [(8, Color::LIGHT_GREY), (20, Color::DARK_GREY)];

/// How fast smoke particles drift, in sub-pixel units per update at [`FULL_SCALE`] — half a
/// fire's pace.
const SMOKE_SPEED: Range<i32> = SUBPIXELS / 4..SUBPIXELS * 3 / 4;

// The plume's own reading of the crate's `Direction`: which way its particles travel, and the two
// rotations that take offsets between the screen and the frame a plume simulates itself in.
// Inherent methods rather than free functions, because a plume asks a direction to turn something
// dozens of times an update and the two belong to nothing else; kept here rather than beside the
// enum because plume space is a plume's business.
impl Direction {
    /// Screen-space offsets rotated into plume space — the inverse of [`Self::rotate`], for the
    /// things that reach a plume from outside it and have to be turned to face its own frame.
    ///
    /// Turning by the same angle the other way round is what undoes a rotation, so the four
    /// directions that come in pairs borrow each other's arithmetic. The diagonals lose a
    /// fraction of a sub-pixel to the two roundings of [`diagonal`] on the way there and back.
    #[cfg(any(feature = "physics", test))]
    fn unrotate(self, x: i32, y: i32) -> (i32, i32) {
        let back = match self {
            Self::Up => Self::Up,
            Self::UpRight => Self::UpLeft,
            Self::Right => Self::Left,
            Self::DownRight => Self::DownLeft,
            Self::Down => Self::Down,
            Self::DownLeft => Self::DownRight,
            Self::Left => Self::Right,
            Self::UpLeft => Self::UpRight,
        };

        back.rotate(x, y)
    }

    /// Plume-space offsets — where a plume travels towards negative `y` — rotated into screen
    /// space.
    fn rotate(self, x: i32, y: i32) -> (i32, i32) {
        match self {
            Self::Up => (x, y),
            Self::UpRight => diagonal(x - y, x + y),
            Self::Right => (-y, x),
            Self::DownRight => diagonal(-x - y, x - y),
            Self::Down => (-x, -y),
            Self::DownLeft => diagonal(y - x, -x - y),
            Self::Left => (y, -x),
            Self::UpLeft => diagonal(x + y, y - x),
        }
    }
}

/// A 45° rotation mixes the two axes into sums that are a factor √2 too long; this scales them
/// back, so a diagonal plume is as long as a straight one.
fn diagonal(x: i32, y: i32) -> (i32, i32) {
    // 181 / 256 ≈ 1 / √2.
    (x * 181 / 256, y * 181 / 256)
}

/// The `SCALE` of a full-size plume: a fire about 30 pixels tall, spawning 10 particles an
/// update.
pub const FULL_SCALE: usize = 10;

/// The largest `SCALE` a plume supports, twice [`FULL_SCALE`]. Exceeding it fails the build.
///
/// A plume this big already spans the screen, and it is as far as a particle's six bytes stretch:
/// speeds stop fitting the byte they are stored in a little past it. Radii run out sooner — from
/// around `14` the fattest particles saturate at four pixels instead of growing with the rest.
pub const MAX_SCALE: usize = 2 * FULL_SCALE;

/// The number of updates a plume's particles live for by default: [`Fire`] burns for this many
/// and [`Smoke`] drifts for twice as long.
pub const DEFAULT_LIFETIME: usize = 26;

/// The longest `LIFETIME` a plume supports — nearly three seconds, and more than three times what
/// the effects here use. Exceeding it fails the build.
///
/// Past this a particle could outrun the sub-pixel position it is stored in, and wrap around to
/// the far side of the screen instead of drifting off it. So it is how far an `i16` reaches from
/// the edge of the widest spawn bed, over the most ground the fastest particle of the biggest
/// plume covers in one update.
pub const MAX_LIFETIME: usize = {
    let reach = i16::MAX as i32 - scaled(SPAWN_REACH, MAX_SCALE as i32);

    (reach / scaled(MAX_PARTICLE_SPEED, MAX_SCALE as i32)) as usize
};

/// The fastest a plume's particles may travel, in sub-pixel units per update at [`FULL_SCALE`].
/// [`MAX_LIFETIME`] is derived from it, so a plume asking for more is checked against it.
const MAX_PARTICLE_SPEED: i32 = FIRE_SPEED.end;

/// The engine behind [`Fire`] and [`Smoke`], which differ only in how they color and pace it.
///
/// The simulation runs in the plume's own frame, where particles always travel towards negative
/// `y` and sway along `x`; [`Self::direction`] is applied when drawing. Rotating 260 particles
/// every update would cost far more than rotating them as they are drawn, and the plume's own
/// frame keeps the sway perpendicular to the travel for free.
#[derive(Debug)]
struct Plume<const SCALE: usize, const LIFETIME: usize> {
    /// Generations of particles in spawn order: the front holds the oldest. Every update spawns
    /// one generation and every particle lives exactly `LIFETIME` updates, so a full deque means
    /// the front generation has expired, and the deque works as a ring buffer — no per-particle
    /// liveness checks and no compaction.
    generations: Deque<Generation<SCALE>, LIFETIME>,
    /// The sideways velocity the sway is currently at, in pixels per update at [`FULL_SCALE`].
    /// Only the plume's own weather: a plume handed a wind stops reading it.
    force: f32,
    /// How much [`Self::force`] changes per update; its sign flips at the ends of the sway.
    forced: f32,
    /// Where the next generation will spawn, in whole pixels. Particle positions are relative to
    /// the generation they belong to and in sub-pixel units; keeping bases in whole pixels is
    /// what lets a plume sit anywhere an `i16` reaches without the conversion overflowing.
    x: i16,
    y: i16,
    direction: Direction,
    /// The wind's push as a screen-space `(x, y)`, in sub-pixel units per update, once the plume
    /// has been handed a wind at all — which is also what puts [`Self::force`] out of a job. Kept
    /// in screen space rather than the plume's own frame because a wind blows the same way
    /// whichever way the plume is pointed, and generations spawned before it turned still have to
    /// feel it.
    #[cfg(feature = "physics")]
    drift: Option<(i32, i32)>,
    /// The range particle speeds are drawn from, in sub-pixel units per update at
    /// [`FULL_SCALE`].
    speed: Range<i32>,
    /// The age at which particles drop to half their speed, if they ever do.
    slow_after: Option<usize>,
    /// Updates between puffs, `LIFETIME / puffs`. Particles move every update either way, so this
    /// thins a plume out without shortening it.
    interval: u8,
    /// How many updates a particle lives: a whole number of [`Self::interval`]s, and `LIFETIME`
    /// rounded down to one. Never more than [`MAX_LIFETIME`], which is what keeps ages in a byte.
    life: u8,
    /// Updates since the last puff.
    waited: u8,
    /// Whether the plume is still puffing. A plume that has stopped keeps drifting and ageing
    /// what it has already let go of; it just stops adding to it.
    puffing: bool,
}

impl<const SCALE: usize, const LIFETIME: usize> Plume<SCALE, LIFETIME> {
    fn new(x: i16, y: i16, speed: Range<i32>, slow_after: Option<usize>) -> Self {
        // In `new` rather than in `update`, so that a plume too big or too long-lived fails the
        // build of the cart that asks for one, not of the cart that gets around to running it.
        const {
            assert!(SCALE > 0, "a plume needs a non-zero SCALE");
            assert!(
                SCALE <= MAX_SCALE,
                "a plume's SCALE must not exceed MAX_SCALE"
            );
            assert!(LIFETIME > 0, "a plume needs a non-zero LIFETIME");
            assert!(
                LIFETIME <= MAX_LIFETIME,
                "a plume's LIFETIME must not exceed MAX_LIFETIME"
            );
        }
        // What `MAX_LIFETIME` is calculated against.
        debug_assert!(
            speed.end <= MAX_PARTICLE_SPEED,
            "a plume may not outrun MAX_PARTICLE_SPEED"
        );

        Self {
            generations: Deque::new(),
            force: STARTING_FORCE,
            forced: STARTING_FORCED,
            x,
            y,
            direction: Direction::default(),
            #[cfg(feature = "physics")]
            drift: None,
            speed,
            slow_after,
            interval: 1,
            life: LIFETIME as u8,
            waited: 0,
            puffing: true,
        }
    }

    /// Rebuilds the plume out of `puffs` generations instead of one per update.
    ///
    /// Only ever called before the plume runs, so there are no generations spawned under the old
    /// interval for the age arithmetic to get wrong. A particle then lives `puffs` whole
    /// intervals, which is `LIFETIME` rounded down — up to a puff's worth short of it.
    fn set_puffs(&mut self, puffs: usize) {
        let puffs = puffs.clamp(1, LIFETIME);
        self.interval = (LIFETIME / puffs) as u8;
        self.life = (puffs * (LIFETIME / puffs)) as u8;
    }

    /// Stops or restarts the puffing, leaving everything already in the air to carry on.
    fn set_puffing(&mut self, puffing: bool) {
        self.puffing = puffing;
    }

    /// Moves where the plume spawns from. Particles already let go of keep the base they were
    /// spawned against, so what is already in the air stays where it is.
    fn move_to(&mut self, x: i16, y: i16) {
        self.x = x;
        self.y = y;
    }

    /// Points the plume a new way. Particles already let go of keep the direction they were
    /// spawned with, so the plume bends rather than swinging around.
    fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// Takes the wind as it blows right now and keeps it as a screen-space drift, in sub-pixel
    /// units per update, until the next time it is asked.
    ///
    /// Each axis is clamped on its own rather than the vector as a whole: [`MAX_WIND_SPEED`] is a
    /// bound on how far a particle may be carried along one axis before its `i16` runs out, so
    /// holding each of them under it is exactly what that bound asks for — and it takes no square
    /// root to do.
    ///
    /// Holding it in an `Option` is what tells [`Self::update`] the plume has a wind of its own
    /// now, and that [`Self::force`] — the sway it was making do with — is no longer its weather.
    #[cfg(feature = "physics")]
    fn blown_by(&mut self, wind: &Wind) {
        let (x, y) = wind.blow();
        let x = x.clamp(-MAX_WIND_SPEED, MAX_WIND_SPEED);
        let y = y.clamp(-MAX_WIND_SPEED, MAX_WIND_SPEED);
        self.drift = Some(((x * SUBPIXELS as f32) as i32, (y * SUBPIXELS as f32) as i32));
    }

    fn update(&mut self, ctx: &mut Context) {
        // The sway reverses at a random point, so the plume never settles into a rhythm.
        if self.force < -MAX_FORCE || self.force > ctx.random(0.0..MAX_FORCE) {
            self.forced = -self.forced;
        }
        self.force += self.forced;

        let scale = SCALE as i32;
        // A thinned plume puffs less often than it updates. Particles still move every update,
        // so the plume keeps its length and loses only the generations in between.
        self.waited += 1;
        let interval = self.interval;
        let ticking = self.waited >= interval;
        if ticking {
            self.waited = 0;
            for generation in self.generations.iter_mut() {
                generation.age += interval;
            }
            // Generations are in spawn order, so the oldest is always the front and expiring one
            // is a pop rather than a search. Ageing them here rather than reading their place in
            // the deque is what lets a plume stop puffing: the gap it leaves would otherwise make
            // every particle behind it read as younger than it is.
            if self.generations.front().is_some_and(|g| g.age >= self.life) {
                self.generations.pop_front();
            }

            if self.puffing {
                let speed = self.speed.clone();
                let spawned = self.generations.push_back(Generation {
                    particles: array::from_fn(|_| Particle::new(ctx, scale, speed.clone())),
                    x: self.x,
                    y: self.y,
                    direction: self.direction,
                    age: 0,
                });
                debug_assert!(
                    spawned.is_ok(),
                    "expiring the oldest leaves room for a generation"
                );
            }
        }

        // The sway is the weather a plume invents for itself when it has none. A plume that has
        // been handed a real one lets it go rather than adding the two together, so that a wind
        // gusting around nothing sways a plume about as much as it swayed on its own.
        #[cfg(feature = "physics")]
        let swaying = self.drift.is_none();
        #[cfg(not(feature = "physics"))]
        let swaying = true;
        let force = if swaying {
            scaled((self.force * SUBPIXELS as f32) as i32, scale) as i16
        } else {
            0
        };
        // At the smallest scales the scaled rate would round down to nothing, leaving particles
        // that never thin out at all; a sub-pixel an update is the least they may shrink by.
        let shrink = capped(scaled(RADIUS_SHRINK_SPEED, scale)).max(1);
        let slow_after = self.slow_after;
        #[cfg(feature = "physics")]
        let (drift, waited) = (self.drift, self.waited as usize);
        for generation in self.generations.iter_mut() {
            // Ages advance a whole interval at a time, so this catches each generation on the one
            // update its age crosses `slow_after`.
            let age = generation.age as usize;
            let slowing = ticking
                && slow_after.is_some_and(|after| age >= after && age < after + interval as usize);
            // The wind blows across the screen and the particles do not live there, so it is
            // turned to face the plume once a generation rather than once a particle. Every
            // generation ages at the tick, so `age` alone stands still between puffs — the ramp
            // wants the updates actually lived, which is the wait since the tick on top of it.
            #[cfg(feature = "physics")]
            let (drift_x, drift_y) = drift.map_or((0, 0), |drift| {
                drifted(drift, generation.direction, age + waited)
            });
            for particle in &mut generation.particles {
                // Saturating throughout, because a particle's position is only an `i16` of
                // sub-pixels and the drift below piles into it for as long as the particle lives:
                // a plume at MAX_LIFETIME in a full-strength wind runs out of that, and pinning it
                // far off screen is what that should look like rather than wrapping round to the
                // other side — after which the sway and the travel must not overflow it either.
                particle.x = particle.x.saturating_add(force);
                particle.y = particle.y.saturating_sub(particle.speed as i16);
                #[cfg(feature = "physics")]
                {
                    particle.x = particle.x.saturating_add(drift_x);
                    particle.y = particle.y.saturating_add(drift_y);
                }
                particle.radius = particle.radius.saturating_sub(shrink);
                if slowing {
                    particle.speed /= 2;
                }
            }
        }
    }

    /// Draws every particle, in `birth` until it reaches the first of the `stops` — which are
    /// `(age, color)` pairs in ascending order — and in the color of the last stop it reaches
    /// after that.
    fn draw(&self, gfx: &mut Graphics, birth: Color, stops: &[(usize, Color)]) {
        for generation in &self.generations {
            // Generations are in spawn order, so drawing them front to back layers the young over
            // the old.
            let age = generation.age as usize;
            let mut color = birth;
            for &(stop, stop_color) in stops {
                if age >= stop {
                    color = stop_color;
                }
            }

            // Particle positions are sub-pixel and relative to their generation's base, so the
            // base joins them there — once per generation, rather than per particle.
            let base_x = generation.x as i32 * SUBPIXELS;
            let base_y = generation.y as i32 * SUBPIXELS;
            for particle in &generation.particles {
                let (x, y) = generation
                    .direction
                    .rotate(particle.x as i32, particle.y as i32);
                let x = ((base_x + x) / SUBPIXELS) as i16;
                let y = ((base_y + y) / SUBPIXELS) as i16;
                let radius = (particle.radius / SUBPIXELS as u8) as u16;
                gfx.circle_fill(x, y, radius, color);
            }
        }
    }
}

/// One update's worth of particles, and the base they spawned against.
///
/// Anchoring each generation where it was emitted is what lets a plume move: particles the plume
/// has already let go of keep their own base, so a moving fire leaves a trail behind it instead
/// of dragging everything it has ever emitted along. It costs four bytes per generation rather
/// than per particle, and it keeps particle positions small enough to stay in two bytes each —
/// storing them against the world instead would put a plume's reach at 512 pixels.
#[derive(Debug)]
struct Generation<const SCALE: usize> {
    particles: [Particle; SCALE],
    /// The base these spawned against, in whole pixels.
    x: i16,
    y: i16,
    /// The direction the plume was pointing when these spawned. Keeping it per generation is
    /// what lets a plume turn: what is already in the air carries on the way it was going, so a
    /// turn bends the plume instead of swinging all of it around at once.
    direction: Direction,
    /// How many updates these have lived, which decides their color and when they expire. It
    /// rides along in the padding the fields above leave behind, so it is free.
    age: u8,
}

/// One particle, in plume space: it spawns in a square around its generation's base and travels
/// towards negative `y`, shrinking as it goes.
///
/// Six bytes, because a plume holds hundreds of them. Positions are sub-pixel: most particles
/// move less than a pixel per update, so whole-pixel positions would round their motion away.
#[derive(Debug)]
struct Particle {
    x: i16,
    y: i16,
    radius: u8,
    speed: u8,
}

impl Particle {
    fn new(ctx: &mut Context, scale: i32, speed: Range<i32>) -> Self {
        Self {
            x: scaled(ctx.random_integer(-SPAWN_REACH..SPAWN_REACH), scale) as i16,
            y: scaled(ctx.random_integer(-SPAWN_REACH..SPAWN_REACH), scale) as i16,
            radius: capped(scaled(ctx.random_integer(MIN_RADIUS..MAX_RADIUS), scale)),
            speed: capped(scaled(ctx.random_integer(speed), scale)),
        }
    }
}

/// A `value` of the [`FULL_SCALE`] plume, adjusted for the given scale.
const fn scaled(value: i32, scale: i32) -> i32 {
    value * scale / FULL_SCALE as i32
}

/// A scaled value capped to what a [`Particle`]'s bytes hold, so that the biggest plumes
/// saturate their fattest particles instead of overflowing them.
fn capped(value: i32) -> u8 {
    value.clamp(0, u8::MAX as i32) as u8
}

/// One update's worth of a screen-space `drift` — an `(x, y)` in sub-pixel units — in the plume
/// space of a generation that has lived `age` updates, all of them and not just the ones it was
/// aged on.
///
/// A particle leaves the source already in part of the wind — [`WIND_AT_BIRTH`] says how much —
/// and climbs into all of it over its first [`WIND_RAMP_UPDATES`]. The share it starts with is
/// what puts a gust into the whole
/// plume at once, the base included — the sway a wind stands in for moved every particle alike,
/// and a fire that only answered a gust once its flames were old would read as painted on. The
/// climb from there is what bends the plume rather than shearing it: the further along it a
/// particle is, the further downwind it has been carried.
#[cfg(feature = "physics")]
fn drifted(drift: (i32, i32), direction: Direction, age: usize) -> (i16, i16) {
    // `(1 + (WIND_AT_BIRTH - 1) * lived / WIND_RAMP_UPDATES) / WIND_AT_BIRTH` of the drift, in
    // whole numbers: the share at birth, climbing to all of it at the top of the ramp. Both axes
    // ramp alike, so a plume in a diagonal wind bends the way it leans.
    let lived = (age as i32).min(WIND_RAMP_UPDATES);
    let ramped = |drift: i32| {
        drift * (WIND_RAMP_UPDATES + (WIND_AT_BIRTH - 1) * lived)
            / (WIND_AT_BIRTH * WIND_RAMP_UPDATES)
    };
    let (x, y) = direction.unrotate(ramped(drift.0), ramped(drift.1));

    (x as i16, y as i16)
}

/// Spatial values are fixed-point, in units of 1/64th of a pixel: multiply by this to go from
/// pixels to sub-pixel units, divide to go back.
const SUBPIXELS: i32 = 64;

// The sway, in pixels per update at `FULL_SCALE`: particles lean a quarter of a pixel per update
// at most, taking about a second and a half to swing from one side to the other.
const STARTING_FORCE: f32 = 0.0;
const STARTING_FORCED: f32 = 0.005;
const MAX_FORCE: f32 = 0.25;

/// The strongest wind a plume leans into on either axis, in pixels per update. A faster one is
/// clamped to it, so [`Fire::blown_by`] and [`Smoke::blown_by`] take any wind at all rather than
/// failing on a gale.
///
/// A particle's position is `i16` sub-pixels from the base its generation spawned at, and the
/// drift accumulates into it for every update the particle lives. This is what keeps that inside
/// the `i16` for the lifetimes the effects here use — a [`SmokingFire`]'s particles live the
/// longest of them, [`SMOKING_FIRE_LIFETIME`] updates — and a wind past it is not weather anyway:
/// it takes the plume clean off the screen in under a second.
#[cfg(feature = "physics")]
pub const MAX_WIND_SPEED: f32 = 4.0;

/// How long a particle takes to be going the wind's way entirely rather than the plume's: a
/// flame's whole life, which is a little under half a second at 60 fps.
#[cfg(feature = "physics")]
const WIND_RAMP_UPDATES: i32 = DEFAULT_LIFETIME as i32;

/// The share of the wind a particle is already in the moment it leaves the source, as one part in
/// this many: half of it, climbing to all of it over [`WIND_RAMP_UPDATES`].
///
/// A ramp that started at nothing bent a column beautifully and flickered not at all: every
/// particle young enough to still be flame sat too near the bottom of it to be moved by a gust, so
/// a fire stood there like something painted while its smoke streamed off. That is not what the
/// sway a wind stands in for did — it moved every particle alike. Half is what puts a gust back
/// into the flames, and it still leaves the far end of a column travelling twice as fast downwind
/// as its base, which is all the bend needs.
#[cfg(feature = "physics")]
const WIND_AT_BIRTH: i32 = 2;

// Particles spawn anywhere within this much of the base on either axis, a 10x10 pixel bed
// centered on it at `FULL_SCALE`.
const SPAWN_REACH: i32 = 5 * SUBPIXELS;

// Radii, in sub-pixel units at `FULL_SCALE`: up to 3 pixels, shrinking by a twentieth of a pixel
// per update. Shrinking is what thins a plume out towards its end, not what ends a particle —
// a fully shrunk one still draws as a single pixel until its generation is dropped.
const MIN_RADIUS: i32 = 0;
const MAX_RADIUS: i32 = SUBPIXELS * 3;
const RADIUS_SHRINK_SPEED: i32 = SUBPIXELS / 20;

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECTIONS: [Direction; 8] = [
        Direction::Up,
        Direction::UpRight,
        Direction::Right,
        Direction::DownRight,
        Direction::Down,
        Direction::DownLeft,
        Direction::Left,
        Direction::UpLeft,
    ];

    /// The most a round trip through a diagonal may lose: `diagonal` scales by 181/256 rather
    /// than by 1/√2 and truncates towards zero, once on the way out and once on the way back.
    const ROUNDING: i32 = 2;

    #[test]
    fn unrotating_a_rotation_gives_the_offset_back() {
        for direction in DIRECTIONS {
            for x in (-4 * SUBPIXELS..=4 * SUBPIXELS).step_by(37) {
                for y in (-4 * SUBPIXELS..=4 * SUBPIXELS).step_by(53) {
                    let (screen_x, screen_y) = direction.rotate(x, y);
                    let (back_x, back_y) = direction.unrotate(screen_x, screen_y);
                    assert!(
                        (back_x - x).abs() <= ROUNDING && (back_y - y).abs() <= ROUNDING,
                        "{direction:?}: ({x}, {y}) came back as ({back_x}, {back_y})"
                    );

                    // And the other way round, which is the direction the wind travels in: from
                    // the screen into the plume's own frame and back out.
                    let (plume_x, plume_y) = direction.unrotate(x, y);
                    let (back_x, back_y) = direction.rotate(plume_x, plume_y);
                    assert!(
                        (back_x - x).abs() <= ROUNDING && (back_y - y).abs() <= ROUNDING,
                        "{direction:?}: ({x}, {y}) came back as ({back_x}, {back_y})"
                    );
                }
            }
        }
    }

    #[test]
    fn unrotating_turns_a_screen_wind_across_the_plume() {
        // A plume pointing up is already in screen space; one pointing down hangs upside down in
        // it, so a wind to the right of the screen leans it to its own left.
        assert_eq!(Direction::Up.unrotate(SUBPIXELS, 0), (SUBPIXELS, 0));
        assert_eq!(Direction::Down.unrotate(SUBPIXELS, 0), (-SUBPIXELS, 0));

        // Sideways plumes travel along the screen's `x`, so a wind along it is a headwind or a
        // tailwind: it lands on the particles' own `y`, which is the way they travel.
        assert_eq!(Direction::Right.unrotate(SUBPIXELS, 0), (0, -SUBPIXELS));
        assert_eq!(Direction::Left.unrotate(SUBPIXELS, 0), (0, SUBPIXELS));

        // A diagonal splits it between the two, shorter on each axis by the √2 of the turn.
        let (x, y) = Direction::UpLeft.unrotate(SUBPIXELS, 0);
        assert_eq!((x, y), (SUBPIXELS * 181 / 256, SUBPIXELS * 181 / 256));
    }

    /// The ramp has to climb with every update a generation has lived, not with the ticks it was
    /// aged on: a plume thinned to one puff is aged once, at the end, and would otherwise sit at
    /// the bottom of the ramp for its whole life.
    #[cfg(feature = "physics")]
    #[test]
    fn the_wind_ramp_climbs_with_every_update_lived() {
        // A pixel an update of screen-space drift, into a plume that is already facing that way.
        let drift = (SUBPIXELS, 0);

        // A newborn particle is already in its share of the wind, or a gust would never reach the
        // base of a plume at all.
        let born = drifted(drift, Direction::Up, 0);
        assert_eq!(born, ((drift.0 / WIND_AT_BIRTH) as i16, 0));

        let mut previous = born.0;
        for age in 1..WIND_RAMP_UPDATES as usize {
            let (x, y) = drifted(drift, Direction::Up, age);
            assert!(x >= previous, "the ramp went backwards at {age}");
            assert_eq!(y, 0);
            previous = x;
        }

        // And it tops out at the whole drift rather than climbing past it.
        let full = drifted(drift, Direction::Up, WIND_RAMP_UPDATES as usize);
        assert_eq!(full, (drift.0 as i16, 0));
        assert_eq!(drifted(drift, Direction::Up, 10_000), full);
        assert!(full.0 > born.0, "{full:?} is no further than {born:?}");

        // What bends a column rather than shearing it sideways in one piece: a particle is
        // carried further per update as it ages, so how far it has been carried altogether grows
        // faster than its age does — the far end of a plume outruns the near end.
        let carried = |updates: i32| -> i32 {
            (0..updates)
                .map(|age| drifted(drift, Direction::Up, age as usize).0 as i32)
                .sum()
        };
        let flame = carried(WIND_RAMP_UPDATES);
        assert!(
            carried(2 * WIND_RAMP_UPDATES) > 2 * flame,
            "the wind carries the whole plume alike"
        );
    }

    /// A wind is a vector across the screen, not a speed along `x`: one blowing down the screen
    /// leans a plume down it, and a diagonal one leans it both ways at once.
    #[cfg(feature = "physics")]
    #[test]
    fn a_plume_takes_the_whole_of_a_wind_and_not_just_its_x() {
        let mut plume: Plume<4, 26> = Plume::new(0, 0, SMOKE_SPEED, None);

        // A wind off the top of the screen blows *down* it, so what it leaves is a drift on `y`.
        plume.blown_by(&Wind::new(1.0).with_direction(Direction::Up));
        assert_eq!(plume.drift, Some((0, SUBPIXELS)));

        // The default wind comes in over the left edge and blows to the right, which is where a
        // cart tuned before any of this leaves its numbers exactly as they were.
        plume.blown_by(&Wind::new(-0.5));
        assert_eq!(plume.drift, Some((-SUBPIXELS / 2, 0)));

        // Either axis of a gale is held to what a particle's `i16` will carry.
        plume.blown_by(&Wind::new(10.0 * MAX_WIND_SPEED).with_direction(Direction::Down));
        assert_eq!(plume.drift, Some((0, -(MAX_WIND_SPEED as i32) * SUBPIXELS)));

        // And a wind off a corner lands on both axes at once.
        plume.blown_by(&Wind::new(1.0).with_direction(Direction::UpLeft));
        let (x, y) = plume.drift.unwrap();
        assert!(x > 0 && y > 0, "a wind off the top left drifted ({x}, {y})");

        // It reaches the particles as one, too: a plume pointing up is already in screen space,
        // so a drift down the screen is a drift down its own frame, through the same ramp.
        let carried = drifted((0, SUBPIXELS), Direction::Up, WIND_RAMP_UPDATES as usize);
        assert_eq!(carried, (0, SUBPIXELS as i16));
    }

    #[cfg(feature = "physics")]
    #[test]
    fn a_thinned_plume_leans_as_far_as_a_full_one() {
        // `ffi::rnd` returns 0.0 natively, so two plumes built the same way run the same way:
        // the only thing between these is the wind.
        let mut ctx = Context { _private: () };
        let mut still: Plume<4, 26> = Plume::new(0, 0, SMOKE_SPEED, None);
        let mut blown: Plume<4, 26> = Plume::new(0, 0, SMOKE_SPEED, None);
        // One puff for the whole `LIFETIME`, so its generation is never aged until it expires.
        still.set_puffs(1);
        blown.set_puffs(1);
        blown.blown_by(&Wind::new(2.0));

        // The puff comes at the first tick, a whole `LIFETIME` in, and the rest is it living.
        for _ in 0..26 + 14 {
            still.update(&mut ctx);
            blown.update(&mut ctx);
        }

        let calm = still.generations.front().unwrap();
        let leaning = blown.generations.front().unwrap();
        assert_eq!(calm.age, 0, "the puff was aged after all");
        for (calm, leaning) in calm.particles.iter().zip(&leaning.particles) {
            assert!(
                leaning.x > calm.x,
                "left standing in the wind: {} against {}",
                leaning.x,
                calm.x
            );
        }
    }

    /// The sway and a wind are the same job, so a plume handed one lets the other go: they never
    /// stack, and a wind blowing at nothing leaves a plume travelling dead straight.
    #[cfg(feature = "physics")]
    #[test]
    fn a_wind_stands_in_for_the_sway() {
        fn lateral(plume: &Plume<FULL_SCALE, 26>) -> [i16; FULL_SCALE] {
            let mut x = [0; FULL_SCALE];
            let particles = &plume.generations.front().unwrap().particles;
            for (x, particle) in x.iter_mut().zip(particles) {
                *x = particle.x;
            }
            x
        }

        let mut ctx = Context { _private: () };
        let mut plume: Plume<FULL_SCALE, 26> = Plume::new(0, 0, SMOKE_SPEED, None);
        // The sway held at its widest: `ffi::rnd` returns 0.0 natively, so the reversal at the top
        // of `update` only ever flips a rate of nothing and the force stays where it is put.
        plume.force = MAX_FORCE;
        plume.forced = 0.0;
        plume.update(&mut ctx);
        let spawned = lateral(&plume);

        // Swaying, a plume at `FULL_SCALE` leans by the whole of the force every update.
        plume.update(&mut ctx);
        let swayed = lateral(&plume);
        let lean = (MAX_FORCE * SUBPIXELS as f32) as i16;
        for (before, after) in spawned.iter().zip(&swayed) {
            assert_eq!(after - before, lean, "the sway is not what it was");
        }

        // Handed a wind that blows at nothing, it stands still sideways instead.
        plume.blown_by(&Wind::new(0.0));
        plume.update(&mut ctx);
        assert_eq!(lateral(&plume), swayed, "the sway carried on under a wind");

        // And one that does blow moves it by the wind alone — at the share of it particles three
        // updates old are in, with nothing of the sway added on top.
        plume.blown_by(&Wind::new(1.0));
        plume.update(&mut ctx);
        let ramped = drifted((SUBPIXELS, 0), Direction::Up, 3).0;
        assert!(ramped > 0, "the wind never reached them");
        for (before, after) in swayed.iter().zip(&lateral(&plume)) {
            assert_eq!(after - before, ramped, "the wind is not blowing alone");
        }
    }
}
