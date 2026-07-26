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
//! # use pixel8::plume::{Direction, Smoke};
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
//! # use pixel8::plume::{Direction, Smoke};
//! let exhaust: Smoke<4> = Smoke::new(64, 40).with_direction(Direction::Down);
//! ```
//!
//! A plume can be turned as it runs, with [`Fire::set_direction`] / [`Smoke::set_direction`].
//! Particles already in the air carry on the way they were going, so a plume that turns bends
//! rather than swinging around all at once.
//!
//! # Trails
//!
//! A plume is not pinned to where it started: [`Fire::move_to`] and [`Smoke::move_to`] move the
//! point it spawns from. Particles already in the air keep the base they came off, so moving a
//! plume trails it rather than dragging everything it has emitted along — which is what makes a
//! plume a trail. That damaged aircraft is a `Smoke` moved to the plane every frame:
//!
//! ```no_run
//! # use pixel8::{plume::{Direction, Smoke}, *};
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

use crate::{Color, Context, Graphics};

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

    /// Moves the fire to (`x`, `y`), for one carried or burning on something that moves.
    ///
    /// Flames already in the air stay where they were let go, so a fire on the move trails
    /// behind itself. Move it before [`update`](Self::update) and the next flames come off the
    /// new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
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

    /// Moves the source of the smoke to (`x`, `y`), for a trail off something that moves.
    ///
    /// Smoke already in the air stays where it was let go — which is what makes it a trail
    /// rather than a cloud dragged along. Move it before [`update`](Self::update) and the next
    /// puff comes off the new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
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

/// The direction a plume travels in, in steps of 45°. Plumes rise unless told otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

impl Direction {
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
    force: f32,
    /// How much [`Self::force`] changes per update; its sign flips at the ends of the sway.
    forced: f32,
    /// Where the next generation will spawn, in whole pixels. Particle positions are relative to
    /// the generation they belong to and in sub-pixel units; keeping bases in whole pixels is
    /// what lets a plume sit anywhere an `i16` reaches without the conversion overflowing.
    x: i16,
    y: i16,
    direction: Direction,
    /// The range particle speeds are drawn from, in sub-pixel units per update at
    /// [`FULL_SCALE`].
    speed: Range<i32>,
    /// The age at which particles drop to half their speed, if they ever do.
    slow_after: Option<usize>,
    /// How many generations the plume is made of: the deque's working capacity, which is its
    /// full `LIFETIME` unless the plume has been thinned out.
    puffs: u8,
    /// Updates between generations, `LIFETIME / puffs`. Particles move every update either way,
    /// so this thins a plume out without shortening it.
    interval: u8,
    /// Updates since the last generation spawned.
    waited: u8,
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
            speed,
            slow_after,
            puffs: LIFETIME as u8,
            interval: 1,
            waited: 0,
        }
    }

    /// Rebuilds the plume out of `puffs` generations instead of one per update.
    ///
    /// Only ever called before the plume runs, so there are no generations spawned under the old
    /// interval for the age arithmetic to get wrong. A particle then lives `puffs` whole
    /// intervals, which is `LIFETIME` rounded down — up to a puff's worth short of it.
    fn set_puffs(&mut self, puffs: usize) {
        let puffs = puffs.clamp(1, LIFETIME);
        self.puffs = puffs as u8;
        self.interval = (LIFETIME / puffs) as u8;
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
        let puffing = self.waited >= self.interval;
        if puffing {
            self.waited = 0;
            if self.generations.len() == self.puffs as usize {
                self.generations.pop_front();
            }

            let speed = self.speed.clone();
            let spawned = self.generations.push_back(Generation {
                particles: array::from_fn(|_| Particle::new(ctx, scale, speed.clone())),
                x: self.x,
                y: self.y,
                direction: self.direction,
            });
            debug_assert!(
                spawned.is_ok(),
                "the pop above leaves room for a generation"
            );
        }

        let force = scaled((self.force * SUBPIXELS as f32) as i32, scale) as i16;
        // At the smallest scales the scaled rate would round down to nothing, leaving particles
        // that never thin out at all; a sub-pixel an update is the least they may shrink by.
        let shrink = capped(scaled(RADIUS_SHRINK_SPEED, scale)).max(1);
        let slow_after = self.slow_after;
        let interval = self.interval as usize;
        let generations = self.generations.len();
        for (i, generation) in self.generations.iter_mut().enumerate() {
            // Ages only advance when a generation spawns, and then by a whole interval, so this
            // catches each generation on the one update its age crosses `slow_after`.
            let age = (generations - 1 - i) * interval;
            let slowing =
                puffing && slow_after.is_some_and(|after| age >= after && age < after + interval);
            for particle in &mut generation.particles {
                particle.x += force;
                particle.y -= particle.speed as i16;
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
        let interval = self.interval as usize;
        let generations = self.generations.len();
        for (i, generation) in self.generations.iter().enumerate() {
            // Generations are in spawn order, so how many updates one has lived follows from its
            // distance to the back. Drawing them oldest first layers the young over the old.
            let age = (generations - 1 - i) * interval;
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

/// Spatial values are fixed-point, in units of 1/64th of a pixel: multiply by this to go from
/// pixels to sub-pixel units, divide to go back.
const SUBPIXELS: i32 = 64;

// The sway, in pixels per update at `FULL_SCALE`: particles lean a quarter of a pixel per update
// at most, taking about a second and a half to swing from one side to the other.
const STARTING_FORCE: f32 = 0.0;
const STARTING_FORCED: f32 = 0.005;
const MAX_FORCE: f32 = 0.25;

// Particles spawn anywhere within this much of the base on either axis, a 10x10 pixel bed
// centered on it at `FULL_SCALE`.
const SPAWN_REACH: i32 = 5 * SUBPIXELS;

// Radii, in sub-pixel units at `FULL_SCALE`: up to 3 pixels, shrinking by a twentieth of a pixel
// per update. Shrinking is what thins a plume out towards its end, not what ends a particle —
// a fully shrunk one still draws as a single pixel until its generation is dropped.
const MIN_RADIUS: i32 = 0;
const MAX_RADIUS: i32 = SUBPIXELS * 3;
const RADIUS_SHRINK_SPEED: i32 = SUBPIXELS / 20;
