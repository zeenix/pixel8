//! The stream of particles behind [`Fire`] and [`Smoke`].
//!
//! [`Fire`]: super::Fire
//! [`Smoke`]: super::Smoke

use core::{array, ops::Range};

use heapless::Deque;

use super::{
    fire::FIRE_SPEED,
    scale::{capped, scaled, SUBPIXELS},
    MAX_SCALE,
};
#[cfg(feature = "physics")]
use crate::physics::Wind;
use crate::{Color, Context, Direction, Graphics};

/// The number of updates a plume's particles live for by default: [`Fire`] burns for this many
/// and [`Smoke`] drifts for twice as long.
///
/// [`Fire`]: super::Fire
/// [`Smoke`]: super::Smoke
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

/// The fastest a plume's particles may travel, in sub-pixel units per update at `FULL_SCALE`.
/// [`MAX_LIFETIME`] is derived from it, so a plume asking for more is checked against it.
const MAX_PARTICLE_SPEED: i32 = FIRE_SPEED.end;

/// The engine behind [`Fire`] and [`Smoke`], which differ only in how they color and pace it.
///
/// The simulation runs in the plume's own frame, where particles always travel towards negative
/// `y` and sway along `x`; [`Self::direction`] is applied when drawing. Rotating 260 particles
/// every update would cost far more than rotating them as they are drawn, and the plume's own
/// frame keeps the sway perpendicular to the travel for free.
///
/// [`Fire`]: super::Fire
/// [`Smoke`]: super::Smoke
#[derive(Debug)]
pub(super) struct Plume<const SCALE: usize, const LIFETIME: usize> {
    /// Generations of particles in spawn order: the front holds the oldest. Every update spawns
    /// one generation and every particle lives exactly `LIFETIME` updates, so a full deque means
    /// the front generation has expired, and the deque works as a ring buffer — no per-particle
    /// liveness checks and no compaction.
    generations: Deque<Generation<SCALE>, LIFETIME>,
    /// The sideways velocity the sway is currently at, in pixels per update at `FULL_SCALE`.
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
    /// The range particle speeds are drawn from, in sub-pixel units per update at `FULL_SCALE`.
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
    pub(super) fn new(x: i16, y: i16, speed: Range<i32>, slow_after: Option<usize>) -> Self {
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
    pub(super) fn set_puffs(&mut self, puffs: usize) {
        let puffs = puffs.clamp(1, LIFETIME);
        self.interval = (LIFETIME / puffs) as u8;
        self.life = (puffs * (LIFETIME / puffs)) as u8;
    }

    /// Stops or restarts the puffing, leaving everything already in the air to carry on.
    pub(super) fn set_puffing(&mut self, puffing: bool) {
        self.puffing = puffing;
    }

    /// Moves where the plume spawns from. Particles already let go of keep the base they were
    /// spawned against, so what is already in the air stays where it is.
    pub(super) fn move_to(&mut self, x: i16, y: i16) {
        self.x = x;
        self.y = y;
    }

    /// Points the plume a new way. Particles already let go of keep the direction they were
    /// spawned with, so the plume bends rather than swinging around.
    pub(super) fn set_direction(&mut self, direction: Direction) {
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
    pub(super) fn blown_by(&mut self, wind: &Wind) {
        let (x, y) = wind.blow();
        let x = x.clamp(-MAX_WIND_SPEED, MAX_WIND_SPEED);
        let y = y.clamp(-MAX_WIND_SPEED, MAX_WIND_SPEED);
        self.drift = Some(((x * SUBPIXELS as f32) as i32, (y * SUBPIXELS as f32) as i32));
    }

    pub(super) fn update(&mut self, ctx: &mut Context) {
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
    pub(super) fn draw(&self, gfx: &mut Graphics, birth: Color, stops: &[(usize, Color)]) {
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
///
/// [`Fire::blown_by`]: super::Fire::blown_by
/// [`Smoke::blown_by`]: super::Smoke::blown_by
/// [`SmokingFire`]: super::SmokingFire
/// [`SMOKING_FIRE_LIFETIME`]: super::SMOKING_FIRE_LIFETIME
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
    use super::{super::FULL_SCALE, *};

    /// The pace these run their plumes at, which is a smoke's: fast enough to move
    /// every update, slow enough that nothing leaves the screen mid-test.
    const TEST_SPEED: Range<i32> = SUBPIXELS / 4..SUBPIXELS * 3 / 4;

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
        let mut plume: Plume<4, 26> = Plume::new(0, 0, TEST_SPEED, None);

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
        let mut still: Plume<4, 26> = Plume::new(0, 0, TEST_SPEED, None);
        let mut blown: Plume<4, 26> = Plume::new(0, 0, TEST_SPEED, None);
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
        let mut plume: Plume<FULL_SCALE, 26> = Plume::new(0, 0, TEST_SPEED, None);
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
