//! [`Explosion`]: particles gone the other way about, all at once.

use super::{
    scale::{capped, scaled, SUBPIXELS},
    FULL_SCALE, MAX_SCALE,
};
use crate::{Color, Context, Graphics};

/// An explosion: a burst of sparks thrown out from a point, thinning away as they fly.
///
/// Where a plume is a source that goes on giving, an explosion is a single moment — every spark
/// it has leaves at once, and the whole thing is over in about a quarter of a second. See the
/// [module docs](super#bursts).
///
/// `SCALE` sizes the blast the way it sizes a plume (see the [module docs](super#size)): how far
/// the sparks are thrown, and how fat they start. `SPARKS` is how many there are, and a blast
/// scaled right down wants fewer of them or the little ground it covers is solid with sparks —
/// `Explosion<3, 15>` is a spark or two off something small breaking.
#[derive(Debug)]
pub struct Explosion<const SCALE: usize = FULL_SCALE, const SPARKS: usize = DEFAULT_SPARKS> {
    sparks: [Spark; SPARKS],
    /// The middle of the blast, in whole pixels. Sparks are placed against it in sub-pixel units,
    /// so an explosion may go off anywhere an `i16` reaches without the conversion overflowing.
    x: i16,
    y: i16,
    color: Color,
    /// Updates since the sparks were thrown.
    age: u8,
    /// The age at which the last spark will have shrunk away — and zero until they are thrown,
    /// which is what tells the first update from the rest.
    life: u8,
}

impl<const SCALE: usize, const SPARKS: usize> Explosion<SCALE, SPARKS> {
    /// A new explosion centered on the pixel position (`x`, `y`).
    ///
    /// Nothing has gone off yet: the sparks are thrown by the first [`update`](Self::update), so
    /// an explosion can be built wherever it is convenient and blow when it is next run.
    pub fn new(x: i16, y: i16) -> Self {
        // In `new` rather than in `update`, so that an explosion too big fails the build of the
        // cart that asks for one, not of the cart that gets around to setting it off.
        const {
            assert!(SCALE > 0, "an explosion needs a non-zero SCALE");
            assert!(
                SCALE <= MAX_SCALE,
                "an explosion's SCALE must not exceed MAX_SCALE"
            );
            assert!(SPARKS > 0, "an explosion needs at least one spark");
        }

        Self {
            sparks: [Spark::UNTHROWN; SPARKS],
            x,
            y,
            color: SPARK_COLOR,
            age: 0,
            life: 0,
        }
    }

    /// Sets what the sparks are made of, to chain onto [`new`](Self::new).
    ///
    /// The default is the ash grey of debris, which is what most things throw off. A fireball is
    /// [`Color::YELLOW`] or [`Color::ORANGE`]; something that shatters is the color it was.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Advances the burst by one frame. Call this from [`Game::update`](crate::Game::update).
    ///
    /// The first call is the bang: it throws the sparks out from where the explosion was placed.
    pub fn update(&mut self, ctx: &mut Context) {
        if self.life == 0 {
            self.throw(ctx);
            return;
        }
        // Nothing left to age once the last spark has gone, and stopping there is what keeps the
        // count inside the byte it is kept in.
        if self.age < self.life {
            self.age += 1;
        }
    }

    /// Draws the sparks. Call this from [`Game::draw`](crate::Game::draw).
    pub fn draw(&self, gfx: &mut Graphics) {
        let age = self.age as i32;
        let shrunk = Self::shrunk(age);
        // Sparks are placed against the middle of the blast in sub-pixel units, so the middle
        // joins them there — once for the burst, rather than once a spark.
        let base_x = self.x as i32 * SUBPIXELS;
        let base_y = self.y as i32 * SUBPIXELS;
        for spark in &self.sparks {
            // A spark is gone once it has shrunk away entirely. One merely under a pixel across
            // is not: it draws as the single pixel a shrinking spark ends its life on.
            let radius = spark.radius as i32 - shrunk;
            if radius <= 0 {
                continue;
            }

            // Where its heading has carried it since it was thrown, which is the whole of the
            // simulation: nothing about a spark changes over its life but how old it is.
            let x = whole_pixels(base_x + spark.x_step as i32 * age);
            let y = whole_pixels(base_y + spark.y_step as i32 * age);
            gfx.circle_fill(x, y, (radius / SUBPIXELS) as u16, self.color);
        }
    }

    /// Whether the last spark has shrunk away, leaving nothing to draw.
    ///
    /// An explosion never goes off twice, so this is a cart's cue to drop it — see the
    /// [module docs](super#bursts).
    pub fn finished(&self) -> bool {
        self.life > 0 && self.age >= self.life
    }

    /// Throws the sparks: a heading and a size for each, and the life of the longest-lived.
    fn throw(&mut self, ctx: &mut Context) {
        let scale = SCALE as i32;
        let mut fattest = 0;
        for spark in &mut self.sparks {
            *spark = Spark::new(ctx, scale);
            fattest = fattest.max(spark.radius);
        }

        // Every spark thins at the same rate, so the last one left is the one thrown fattest —
        // rounded up, a spark with anything at all left of it still being on the screen. Never
        // zero, which is what marks a burst that has yet to go off.
        let shrink = SPARK_SHRINK_SPEED * SCALE as i32;
        let life = (fattest as i32 * FULL_SCALE as i32 + shrink - 1) / shrink;
        self.life = capped(life).max(1);
    }

    /// How much a spark has thinned by, in sub-pixel units, `age` updates after it was thrown.
    ///
    /// It is the whole of the shrinking that is scaled and not the rate it happens at: a rate
    /// scaled down would round to nothing below about half size, leaving sparks that never
    /// shrink away at all. This way a burst lasts as long whatever size it is, which is what a
    /// smaller explosion should be — the same bang over less ground, not a slower one.
    fn shrunk(age: i32) -> i32 {
        scaled(SPARK_SHRINK_SPEED * age, SCALE as i32)
    }
}

/// The number of sparks in an [`Explosion`] by default: enough for a blast at [`FULL_SCALE`] to
/// read as a cloud coming apart rather than a handful of dots.
pub const DEFAULT_SPARKS: usize = 50;

/// One spark of an [`Explosion`].
///
/// Six bytes, like a plume's particle, and rather less to it: a spark keeps no position, because
/// it is always its heading times its age away from the middle of the blast.
#[derive(Debug, Clone, Copy)]
struct Spark {
    /// Per-update movement, i.e. `velocity / mass`. Neither of the two changes over a spark's
    /// life, so only their ratio is kept — in sub-pixel units, as most sparks move less than a
    /// pixel an update and whole ones would round their flight away.
    x_step: i16,
    y_step: i16,
    /// The radius it was thrown at, in sub-pixel units. What is left of it is this less what the
    /// updates since have taken off.
    radius: u8,
}

impl Spark {
    /// A spark that has yet to be thrown: no heading and no size, so it draws as nothing.
    const UNTHROWN: Self = Self {
        x_step: 0,
        y_step: 0,
        radius: 0,
    };

    fn new(ctx: &mut Context, scale: i32) -> Self {
        Self {
            x_step: scaled(spark_step(ctx), scale) as i16,
            y_step: scaled(spark_step(ctx), scale) as i16,
            radius: capped(scaled(
                ctx.random_integer(MIN_SPARK_RADIUS..MAX_SPARK_RADIUS),
                scale,
            )),
        }
    }
}

/// One axis of a spark's heading, in sub-pixel units per update at [`FULL_SCALE`]: a random
/// velocity over a random mass, which is what makes a burst ragged — the lightest sparks are
/// thrown right out of it while the heaviest barely leave the middle.
fn spark_step(ctx: &mut Context) -> i32 {
    let velocity = ctx.random_integer(-SPARK_SPEED..SPARK_SPEED);
    let mass = ctx.random_integer(MIN_SPARK_MASS..MAX_SPARK_MASS);

    velocity * SUBPIXELS / mass
}

/// A sub-pixel position as the whole pixel it lands in, pinned to the ends of an `i16` rather
/// than wrapping round to the other side of the world — a blast set off at the far edge of one
/// throws sparks past what an `i16` holds.
fn whole_pixels(position: i32) -> i16 {
    (position / SUBPIXELS).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// What sparks are made of unless the cart says otherwise: the ash grey of debris.
const SPARK_COLOR: Color = Color::LIGHT_GREY;

/// The fastest a spark is thrown before the mass it is divided by, in sub-pixel units per update
/// at [`FULL_SCALE`] — a pixel an update, either way along either axis.
const SPARK_SPEED: i32 = SUBPIXELS;

/// What a spark's speed is divided by: half a nominal mass to two and a half times it, so the
/// lightest sparks of a burst are thrown five times as far as the heaviest.
const MIN_SPARK_MASS: i32 = SUBPIXELS / 2;
const MAX_SPARK_MASS: i32 = SUBPIXELS * 5 / 2;

// Spark radii, in sub-pixel units at `FULL_SCALE`: half a pixel to a pixel and a half, thinning
// by a tenth of a pixel an update. So a spark lives somewhere between five and fifteen updates,
// and the burst is over in about a quarter of a second.
const MIN_SPARK_RADIUS: i32 = SUBPIXELS / 2;
const MAX_SPARK_RADIUS: i32 = SUBPIXELS * 3 / 2;
const SPARK_SHRINK_SPEED: i32 = SUBPIXELS / 10;

#[cfg(test)]
mod tests {
    use super::*;

    /// An explosion placed but never run has not gone off: nothing of it is in the air, and it is
    /// not finished either — a cart that drops finished explosions would drop it before the bang.
    #[test]
    fn an_explosion_is_nothing_until_it_is_run() {
        let mut ctx = Context { _private: () };
        let mut blast: Explosion = Explosion::new(64, 64);
        assert!(!blast.finished());
        assert!(blast.sparks.iter().all(|spark| spark.radius == 0));

        blast.update(&mut ctx);
        assert!(!blast.finished());
        assert!(blast.sparks.iter().all(|spark| spark.radius > 0));
    }

    /// The burst ends when the last spark has thinned away, and not before: `finished` has to
    /// outlast the fattest spark it threw, whatever that turned out to be.
    #[test]
    fn an_explosion_is_finished_once_its_sparks_are_spent() {
        let mut ctx = Context { _private: () };
        let mut blast: Explosion = Explosion::new(64, 64);
        blast.update(&mut ctx);

        let fattest = blast.sparks.iter().map(|spark| spark.radius).max().unwrap() as i32;
        let shrunk = Explosion::<FULL_SCALE, DEFAULT_SPARKS>::shrunk;
        for age in 1..blast.life {
            blast.update(&mut ctx);
            assert!(!blast.finished(), "over at {age} of {}", blast.life);
            assert!(
                fattest - shrunk(age as i32) > 0,
                "the fattest spark was gone at {age}"
            );
        }

        blast.update(&mut ctx);
        assert!(blast.finished(), "still going at {}", blast.life);
        assert!(fattest - shrunk(blast.age as i32) <= 0);

        // And it stays finished however long the cart holds on to it, rather than the age
        // wrapping round the byte it lives in and starting the burst over.
        for _ in 0..10 * u8::MAX as usize {
            blast.update(&mut ctx);
            assert!(blast.finished());
        }
    }

    /// `SCALE` sizes a burst as it sizes a plume: the same sparks, thrown less far.
    #[test]
    fn a_smaller_explosion_throws_its_sparks_less_far() {
        let mut ctx = Context { _private: () };
        // `ffi::rnd` returns 0.0 natively, so both bursts roll the same spark: the two differ by
        // nothing but their scale.
        let mut full: Explosion = Explosion::new(64, 64);
        let mut small: Explosion<2> = Explosion::new(64, 64);
        full.update(&mut ctx);
        small.update(&mut ctx);

        let (full, small) = (&full.sparks[0], &small.sparks[0]);
        assert!(full.x_step.abs() > small.x_step.abs());
        assert!(full.y_step.abs() > small.y_step.abs());
        assert!(full.radius > small.radius);
    }

    /// A burst takes the same time to go off whatever size it is — the same bang over less
    /// ground, rather than a slower one. Which is what scaling the whole of the shrinking rather
    /// than the rate it happens at is for: a rate scaled down rounds to nothing.
    #[test]
    fn every_size_of_explosion_is_over_in_the_same_time() {
        fn life<const SCALE: usize>() -> i32 {
            // The fattest spark there is at this scale, which is the last one to go.
            let fattest = capped(scaled(MAX_SPARK_RADIUS - 1, SCALE as i32)) as i32;

            (1..)
                .find(|&age| Explosion::<SCALE>::shrunk(age) >= fattest)
                .unwrap()
        }

        let full = life::<FULL_SCALE>();
        assert_eq!(full, 16, "a full-size burst is no longer a quarter-second");
        for (scale, life) in [
            (1, life::<1>()),
            (3, life::<3>()),
            (20, life::<MAX_SCALE>()),
        ] {
            assert!(
                (life - full).abs() <= 1,
                "a burst at scale {scale} lasts {life} updates against {full}"
            );
        }
    }
}
