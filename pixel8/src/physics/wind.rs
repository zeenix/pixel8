//! The weather that blows across a scene.

use core::ops::RangeInclusive;

use super::{force::weighed, Force, Kinetic};
use crate::{Context, Direction};

/// A wind, named for the side it comes *from*.
///
/// Meteorological convention, and the one thing to get straight about this type: a north wind
/// blows south. [`Direction::Left`] — the default — is a wind arriving over the left edge of the
/// screen and pushing everything to the right, and [`Direction::Up`] one that comes over the top
/// and blows down.
///
/// The speed is signed *along that line*: positive blows away from where the wind comes from, and
/// negative is the air momentarily backing the other way — which is what lets a gust range
/// straddle zero and a breeze reverse without changing which wind it is.
///
/// The wind is steady until [`with_gusts`](Self::with_gusts) sets it wandering. It does not
/// accelerate what it pushes without limit — velocity eases towards the wind's speed and settles
/// there — so it is safe to apply to the same entity forever. See the [module docs](super#wind).
///
/// ```no_run
/// # use pixel8::{physics::Wind, Direction};
/// // A breeze off the right of the screen, never quite the same two updates running: it pushes
/// // things to the left, and the odd gust in the range pushes them back.
/// let wind = Wind::new(0.3).with_direction(Direction::Right).with_gusts(-0.05..=0.7);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wind {
    /// The side the wind comes from. It blows the other way.
    direction: Direction,
    /// The steady speed: what the wind blows at when nothing is gusting, and the anchor a gust
    /// range hangs off.
    base: f32,
    /// The speed right now, gusts included. Equal to [`Self::base`] for a steady wind.
    speed: f32,
    gusts: Option<Gusts>,
    /// The fraction of the gap between a velocity and [`Self::speed`] closed per update, in
    /// `0.0..=1.0`.
    exposure: f32,
}

impl Wind {
    /// How exposed to the wind something is unless told otherwise: it closes a twentieth of the
    /// gap to the wind's speed per update, which is about 95% of it after a second at 60 fps.
    pub const DEFAULT_EXPOSURE: f32 = 0.05;

    /// Where the wind comes from unless told otherwise: over the left edge of the screen, blowing
    /// to the right, so that a positive speed moves things towards larger `x`.
    pub const DEFAULT_DIRECTION: Direction = Direction::Left;

    /// A steady wind blowing at `speed` pixels per update, in off the left of the screen.
    ///
    /// This is the speed the wind pushes things *towards*, not the speed it adds to them, and it
    /// is signed along the line it blows: negative is the air backing the other way.
    ///
    /// `const`, so a scene's weather can be a constant of the cart's own:
    /// `const BREEZE: Wind = Wind::new(0.3);`.
    pub const fn new(speed: f32) -> Self {
        Self {
            direction: Self::DEFAULT_DIRECTION,
            base: speed,
            speed,
            gusts: None,
            exposure: Self::DEFAULT_EXPOSURE,
        }
    }

    /// Sets which side the wind comes from — not the way it blows — to chain onto
    /// [`new`](Self::new).
    ///
    /// [`Direction::Up`] is a wind off the top of the screen, and it blows *down*. The default is
    /// [`Direction::Left`], which blows to the right.
    pub const fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Makes the wind gust: its speed wanders inside `range` instead of holding still. To chain
    /// onto [`new`](Self::new).
    ///
    /// It turns unpredictably rather than at the ends, so it spends most of its time around the
    /// middle of the range and only rarely comes near the extremes — give it the full spread you
    /// want felt, not the one you want reached.
    ///
    /// The ends may both be negative — a wind that mostly backs against its own direction — and a
    /// range straddling zero is a wind that drops and picks up again. A reversed range is read the
    /// way round it was meant. The wind starts at [`new`](Self::new)'s speed, clamped into the
    /// range, and a range with no width in it is simply a steady wind at that speed. A range with
    /// a `NaN` end is no range at all, and leaves the wind steady.
    ///
    /// A gusty wind needs [`update`](Self::update) once an update or it never moves off where it
    /// started.
    pub fn with_gusts(mut self, range: RangeInclusive<f32>) -> Self {
        let (start, end) = (*range.start(), *range.end());
        // A `NaN` end puts the two in no order at all, and every clamp below would panic on it.
        // A wind that simply stays steady is the quiet way to read a range that means nothing.
        if start.is_nan() || end.is_nan() {
            return self;
        }
        let (low, high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        self.speed = self.speed.clamp(low, high);
        // The base follows the speed into the range. A later re-anchoring shifts the range by how
        // far the base moved, so it has to sit where the wind actually is rather than at the speed
        // `new` was handed and the range then clamped away.
        self.base = self.speed;
        self.gusts = Some(Gusts {
            low,
            high,
            rate: (high - low) / GUST_SWING_UPDATES,
        });
        self
    }

    /// Sets how hard the wind grips what it pushes, to chain onto [`new`](Self::new).
    ///
    /// `exposure` is the fraction of the gap between an entity's speed along the wind and the
    /// wind's own closed each update, clamped to `0.0..=1.0`: a leaf is near `1.0` and takes the
    /// wind at once, a boulder near `0.0` and hardly feels it. The default is
    /// [`DEFAULT_EXPOSURE`](Self::DEFAULT_EXPOSURE), which reaches about 95% of the wind's speed
    /// in a second at 60 fps.
    ///
    /// This is also the drag that keeps a wind from accelerating things forever, so `0.0` is a
    /// wind that does nothing rather than a gentle one.
    ///
    /// An entity's [`mass`](super::Kinetic::mass) divides it: exposure is how much of the wind a
    /// thing catches, mass is how much there is of it to shift, and the wind's grip is the one
    /// over the other.
    pub fn with_exposure(mut self, exposure: f32) -> Self {
        self.set_exposure(exposure);
        self
    }

    /// Advances the gusts by one update. Call this from [`Game::update`](crate::Game::update),
    /// before the wind is applied to anything, so that everything in the scene is pushed by the
    /// same gust.
    ///
    /// A steady wind has nothing to advance and does not mind being left out.
    pub fn update(&mut self, ctx: &mut Context) {
        let Some(gusts) = &mut self.gusts else {
            return;
        };

        // Turning at a random point rather than at the end of the range is what keeps gusts from
        // settling into a rhythm; the clamp is what keeps them inside it regardless.
        let turn = if gusts.rate > 0.0 {
            ctx.random(gusts.middle()..=gusts.high)
        } else {
            ctx.random(gusts.low..=gusts.middle())
        };
        let turning = if gusts.rate > 0.0 {
            self.speed >= turn
        } else {
            self.speed <= turn
        };
        if turning {
            gusts.rate = -gusts.rate;
        }
        self.speed = (self.speed + gusts.rate).clamp(gusts.low, gusts.high);
    }

    /// Turns the wind around while the game runs — the weather coming from somewhere else.
    ///
    /// The speed is unchanged; it is the line it blows along that moves, so a wind that was
    /// pushing things right at `0.3` now pushes them down at `0.3`.
    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// Re-anchors the wind: the weather picking up, or a fan being turned down.
    ///
    /// A gust range moves with it, keeping its width, so a wind gusting in `-0.7..=-0.1` re-based
    /// from `-0.3` to `0.3` gusts in `-0.1..=0.5`.
    ///
    /// A `NaN` speed would take a gusty wind's range off with it and leave nothing orderable
    /// behind, so it is ignored.
    pub fn set_base_speed(&mut self, base: f32) {
        match &mut self.gusts {
            Some(gusts) => {
                if base.is_nan() {
                    return;
                }
                let shift = base - self.base;
                gusts.low += shift;
                gusts.high += shift;
                self.speed = (self.speed + shift).clamp(gusts.low, gusts.high);
                self.base = base;
            }
            None => {
                self.base = base;
                self.speed = base;
            }
        }
    }

    /// Changes how hard the wind grips what it pushes while the game runs — a diver leaving the
    /// water, a cart that swaps in a heavier entity. `exposure` is clamped to `0.0..=1.0`.
    pub fn set_exposure(&mut self, exposure: f32) {
        self.exposure = exposure.clamp(0.0, 1.0);
    }

    /// Which side the wind comes from. It blows the other way.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// The wind's steady speed: what it blows at with no gust on it, and the anchor a gust range
    /// hangs off.
    ///
    /// [`speed`](Self::speed) is where the gusts have taken it right now; the two are equal for a
    /// steady wind.
    pub fn base_speed(&self) -> f32 {
        self.base
    }

    /// The speed the wind is blowing at right now, gusts included, in pixels per update.
    ///
    /// Signed along the line it blows: a negative speed is the air backing towards the side it
    /// came from.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// The fraction of the gap to the wind's speed that whatever it pushes closes each update.
    pub fn exposure(&self) -> f32 {
        self.exposure
    }

    /// Which way the wind is actually pushing, and how hard: its current speed as a screen-space
    /// `(dx, dy)` in pixels per update.
    ///
    /// The opposite of [`direction`](Self::direction), at [`speed`](Self::speed) — which is worth
    /// having spelled out, given that the direction names the side the wind comes from. This is
    /// what to reach for to lean something the wind's way that is not a [`Kinetic`](super::Kinetic)
    /// at all.
    pub fn blow(&self) -> (f32, f32) {
        let (ux, uy) = self.direction.unit();
        (-ux * self.speed, -uy * self.speed)
    }
}

impl Force for Wind {
    /// The mass divides the wind's grip: twice as much of an entity to shift is half the shove.
    fn apply(&self, entity: &mut dyn Kinetic) {
        // Easing towards the wind's speed instead of adding to the velocity is what makes the
        // wind self-limiting: the gap it closes shrinks as the two converge, so a velocity
        // approaches the wind's speed and never overshoots it.
        //
        // The clamp is what holds that for something light enough to be taken whole: a share of
        // the gap past 1.0 would carry a velocity past the wind and set it swinging about it.
        let take = (self.exposure / weighed(entity.mass())).clamp(0.0, 1.0);
        let velocity = entity.velocity_mut();
        // Only the component along the line the wind blows is eased; movement across it is
        // nothing to do with the wind. The four straight winds say so on the one axis they touch,
        // which is the vector arithmetic below with the zeroes taken out — and the same float,
        // to the bit.
        match self.direction {
            Direction::Left => velocity.dx += (self.speed - velocity.dx) * take,
            Direction::Right => velocity.dx += (-self.speed - velocity.dx) * take,
            Direction::Up => velocity.dy += (self.speed - velocity.dy) * take,
            Direction::Down => velocity.dy += (-self.speed - velocity.dy) * take,
            diagonal => {
                let (ux, uy) = diagonal.unit();
                let (bx, by) = (-ux, -uy);
                let along = velocity.dx * bx + velocity.dy * by;
                let eased = (self.speed - along) * take;
                velocity.dx += eased * bx;
                velocity.dy += eased * by;
            }
        }
    }
}

/// The wander a gusty [`Wind`]'s speed does, between the ends of the range it was given.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Gusts {
    low: f32,
    high: f32,
    /// How much the speed changes per update; its sign flips when the wind turns. Zero for a
    /// range with no width, which is a steady wind.
    rate: f32,
}

impl Gusts {
    /// The middle of the range — the earliest the wind may turn back, so that every gust crosses
    /// at least half the range and none of them are imperceptible.
    fn middle(&self) -> f32 {
        (self.low + self.high) / 2.0
    }
}

/// How many updates a gust takes to cross its whole range — about a second and a half at 60 fps.
const GUST_SWING_UPDATES: f32 = 90.0;

#[cfg(test)]
mod tests {
    use super::{
        super::{
            force::{Mob, DIRECTIONS},
            Velocity,
        },
        *,
    };

    #[test]
    fn wind_eases_towards_its_speed_without_overshooting() {
        let wind = Wind::new(2.0);
        let mut mob = Mob::new();
        let mut previous = 0.0;
        for _ in 0..600 {
            wind.apply(&mut mob);
            // Never backwards, never past the wind's own speed — the last few updates close a
            // gap too small for an `f32` to hold, so they stand still rather than climbing —
            // and `dy` is none of a left-hand wind's business.
            assert!(mob.velocity.dx >= previous);
            assert!(mob.velocity.dx <= wind.speed());
            assert_eq!(mob.velocity.dy, 0.0);
            previous = mob.velocity.dx;
        }
        assert!(previous > 1.99, "should be nearly there: {previous}");
    }

    #[test]
    fn wind_slows_something_moving_faster_than_it() {
        let wind = Wind::new(-0.5);
        let mut mob = Mob::moving(-3.0, 0.0);
        for _ in 0..600 {
            wind.apply(&mut mob);
            assert!(mob.velocity.dx <= -0.5);
        }
        assert!(
            mob.velocity.dx < -0.49,
            "eased up to the wind: {}",
            mob.velocity.dx
        );
    }

    #[test]
    fn a_wind_blows_away_from_where_it_comes_from() {
        // The whole convention in one test: the same positive speed, four sides, four ways.
        for (from, expected) in [
            (Direction::Left, Velocity::new(1.0, 0.0)),
            (Direction::Right, Velocity::new(-1.0, 0.0)),
            (Direction::Up, Velocity::new(0.0, 1.0)),
            (Direction::Down, Velocity::new(0.0, -1.0)),
        ] {
            let wind = Wind::new(1.0).with_direction(from).with_exposure(1.0);
            assert_eq!(wind.direction(), from);
            // Exactly there in one update at full exposure, and exactly on the one axis: the
            // straight winds are scalar arithmetic, with no float detour to round them off.
            assert_eq!(
                Mob::new().under(&wind, 1),
                expected,
                "a wind from {from:?} blew the wrong way"
            );
            // And the blow vector says the same thing.
            assert_eq!(wind.blow(), (expected.dx, expected.dy));
        }

        // The default is the left-hand wind, which is what keeps a positive speed meaning "to
        // the right" for a cart that never mentions a direction.
        assert_eq!(Wind::new(1.0).direction(), Wind::DEFAULT_DIRECTION);
        assert_eq!(Wind::DEFAULT_DIRECTION, Direction::Left);
    }

    #[test]
    fn a_negative_speed_backs_the_other_way() {
        // Signed along the line it blows, which is what lets a gust range straddle zero: the wind
        // is still a wind from the left, momentarily going the other way.
        let wind = Wind::new(-1.0).with_exposure(1.0);
        assert_eq!(Mob::new().under(&wind, 1), Velocity::new(-1.0, 0.0));
        assert_eq!(wind.blow(), (-1.0, 0.0));

        let wind = Wind::new(-1.0)
            .with_direction(Direction::Up)
            .with_exposure(1.0);
        assert_eq!(Mob::new().under(&wind, 1), Velocity::new(0.0, -1.0));
    }

    #[test]
    fn a_diagonal_wind_eases_along_itself_and_leaves_the_rest() {
        let from = Direction::UpLeft;
        let wind = Wind::new(2.0).with_direction(from).with_exposure(1.0);
        let (bx, by) = (-from.unit().0, -from.unit().1);

        // Moving across the wind: that part must survive, whatever the wind does along itself.
        let (acrossx, acrossy) = (by, -bx);
        let velocity = Mob::moving(1.5 * acrossx, 1.5 * acrossy).under(&wind, 1);

        let along = velocity.dx * bx + velocity.dy * by;
        let across = velocity.dx * acrossx + velocity.dy * acrossy;
        assert!((along - 2.0).abs() < 1e-5, "not taken by the wind: {along}");
        assert!((across - 1.5).abs() < 1e-5, "blown across it: {across}");

        // And it is blowing down and to the right, being a wind off the top-left corner.
        let (dx, dy) = wind.blow();
        assert!(dx > 0.0 && dy > 0.0, "an up-left wind blew ({dx}, {dy})");
    }

    #[test]
    fn every_direction_blows_at_the_speed_it_says() {
        for from in DIRECTIONS {
            let wind = Wind::new(1.5).with_direction(from);
            let (dx, dy) = wind.blow();
            let (ux, uy) = from.unit();
            // Away from where it comes from, at its speed, whichever of the eight it is.
            assert!((dx * dx + dy * dy - 1.5 * 1.5).abs() < 1e-4, "{from:?}");
            assert!(dx * ux + dy * uy < 0.0, "{from:?} blew back at itself");
        }
    }

    #[test]
    fn exposure_is_clamped_to_a_fraction_of_the_gap() {
        // Above 1.0 the velocity would overshoot the wind and oscillate; below 0.0 it would be
        // blown backwards.
        let over = Mob::new().under(&Wind::new(1.0).with_exposure(9.0), 1);
        assert_eq!(over.dx, 1.0);
        let under = Mob::moving(1.0, 0.0).under(&Wind::new(-1.0).with_exposure(-9.0), 1);
        assert_eq!(under.dx, 1.0);

        // And the setter clamps exactly as the builder it backs does.
        let mut wind = Wind::new(1.0);
        assert_eq!(wind.exposure(), Wind::DEFAULT_EXPOSURE);
        wind.set_exposure(9.0);
        assert_eq!(wind.exposure(), 1.0);
        wind.set_exposure(-9.0);
        assert_eq!(wind.exposure(), 0.0);
    }

    #[test]
    fn mass_divides_the_winds_grip() {
        // The same wind at the same exposure: all that separates these three is what they weigh.
        let wind = Wind::new(2.0);
        let mut heavy = Mob::with_mass(4.0);
        let mut ordinary = Mob::new();
        let mut light = Mob::with_mass(0.25);

        for update in 0..600 {
            for mob in [&mut heavy, &mut ordinary, &mut light] {
                wind.apply(&mut *mob);
                // Whatever it weighs, the shove is a share of the gap, so the gap only ever
                // closes: nothing is blown past the wind and left oscillating about it.
                assert!(mob.velocity.dx <= wind.speed());
                assert_eq!(mob.velocity.dy, 0.0);
            }
            // Four times the mass is a quarter of the shove. Checked while there is still a gap
            // to close — blow long enough and all three end up at the wind's speed.
            if update < 60 {
                assert!(
                    light.velocity.dx > ordinary.velocity.dx,
                    "the light one was not taken first: {} against {}",
                    light.velocity.dx,
                    ordinary.velocity.dx
                );
                assert!(
                    ordinary.velocity.dx > heavy.velocity.dx,
                    "the heavy one was not left behind: {} against {}",
                    heavy.velocity.dx,
                    ordinary.velocity.dx
                );
            }
        }

        // And the heavy one does get there in the end; it just takes the whole of it.
        assert!(
            heavy.velocity.dx > 1.99,
            "still lagging after 600 updates: {}",
            heavy.velocity.dx
        );
    }

    #[test]
    fn something_light_enough_takes_the_whole_wind_at_once() {
        // A share of the gap past 1.0 would carry the velocity beyond the wind and set it
        // swinging, so the grip is capped at the whole gap however light the entity is.
        let wind = Wind::new(1.0).with_exposure(0.5);
        assert_eq!(Mob::with_mass(0.01).under(&wind, 1).dx, 1.0);
        // And it sits at the wind's speed rather than overshooting on the next update.
        assert_eq!(Mob::with_mass(0.01).under(&wind, 2).dx, 1.0);
    }

    #[test]
    fn a_mass_that_means_nothing_weighs_one() {
        // Nothing divides by zero or carries a `NaN` off: a mass a cart worked out from something
        // that turned out to be empty gives an ordinary entity, not one flung off the screen.
        let wind = Wind::new(2.0);
        let ordinary = Mob::new().under(&wind, 1);

        for mass in [0.0, -3.0, f32::NAN, f32::NEG_INFINITY] {
            assert_eq!(
                Mob::with_mass(mass).under(&wind, 1),
                ordinary,
                "a mass of {mass} was not read as 1.0"
            );
        }
    }

    #[test]
    fn gusts_stay_inside_their_range() {
        // `ffi::rnd` returns 0.0 natively, so every turning point drawn here is the low end of
        // the half the wind is heading into: the wander is at its least varied, and still has to
        // stay in the range and actually get somewhere.
        let mut ctx = Context { _private: () };
        let mut wind = Wind::new(-0.3).with_gusts(-0.7..=-0.05);
        let (mut lowest, mut highest) = (wind.speed(), wind.speed());
        for _ in 0..1_000 {
            wind.update(&mut ctx);
            assert!(
                (-0.7..=-0.05).contains(&wind.speed()),
                "gust escaped its range: {}",
                wind.speed()
            );
            lowest = lowest.min(wind.speed());
            highest = highest.max(wind.speed());
        }
        // It turned at both ends of its wander rather than sticking at one.
        assert!(lowest <= -0.69, "never reached the low end: {lowest}");
        assert!(highest >= -0.4, "never came back up: {highest}");
    }

    #[test]
    fn a_reversed_or_widthless_gust_range_is_still_a_wind() {
        let mut ctx = Context { _private: () };

        let mut backwards = Wind::new(0.4).with_gusts(0.6..=0.2);
        let mut pinned = Wind::new(0.4).with_gusts(0.3..=0.3);
        for _ in 0..200 {
            backwards.update(&mut ctx);
            pinned.update(&mut ctx);
            assert!((0.2..=0.6).contains(&backwards.speed()));
            // A range with no width in it has nowhere to wander to.
            assert_eq!(pinned.speed(), 0.3);
        }
    }

    #[test]
    fn a_steady_wind_ignores_updates() {
        let mut ctx = Context { _private: () };
        let mut wind = Wind::new(0.75);
        for _ in 0..10 {
            wind.update(&mut ctx);
        }
        assert_eq!(wind.speed(), 0.75);
    }

    #[test]
    fn re_anchoring_moves_a_gust_range_without_resizing_it() {
        let mut wind = Wind::new(-0.3).with_gusts(-0.7..=-0.1);
        wind.set_base_speed(0.3);
        assert_eq!(wind.base_speed(), 0.3);
        assert_eq!(wind.speed(), 0.3);

        let mut ctx = Context { _private: () };
        for _ in 0..500 {
            wind.update(&mut ctx);
            assert!(
                (-0.1..=0.5).contains(&wind.speed()),
                "gust outside the re-anchored range: {}",
                wind.speed()
            );
        }

        let mut steady = Wind::new(1.0);
        steady.set_base_speed(-2.0);
        assert_eq!(steady.base_speed(), -2.0);
        assert_eq!(steady.speed(), -2.0);
    }

    #[test]
    fn a_gust_range_re_anchors_the_base_it_clamps_the_speed_into() {
        // `new`'s speed is outside the range, so the wind starts at the near edge of it — and it
        // is that edge, not the speed that was asked for, a re-anchoring then shifts the range
        // from.
        let mut wind = Wind::new(0.0).with_gusts(-0.8..=-0.2);
        assert_eq!(wind.speed(), -0.2);
        assert_eq!(wind.base_speed(), -0.2);

        // Anchored 0.3 further back, so the range is `-1.1..=-0.5` and the wind sits at its top.
        // A stale anchor would have shifted by 0.8 instead, landing at -0.7 in `-1.4..=-0.8`.
        wind.set_base_speed(-0.5);
        assert_eq!(wind.base_speed(), -0.5);
        assert!(
            (wind.speed() + 0.5).abs() < 1e-6,
            "re-anchored off a stale base: {}",
            wind.speed()
        );

        let mut ctx = Context { _private: () };
        for _ in 0..500 {
            wind.update(&mut ctx);
            assert!(
                (-1.100_001..=-0.499_999).contains(&wind.speed()),
                "gust outside the re-anchored range: {}",
                wind.speed()
            );
        }
    }

    #[test]
    fn a_gust_range_with_a_nan_end_leaves_the_wind_steady() {
        // Neither end orders against the other, so there is no range to wander in — and every
        // clamp the gusts would reach panics on one. A steady wind is the quiet reading.
        let mut ctx = Context { _private: () };
        for range in [f32::NAN..=1.0, 0.0..=f32::NAN, f32::NAN..=f32::NAN] {
            let mut wind = Wind::new(0.4).with_gusts(range);
            assert_eq!(wind.base_speed(), 0.4);
            for _ in 0..100 {
                wind.update(&mut ctx);
                assert_eq!(wind.speed(), 0.4);
            }
        }
    }

    #[test]
    fn re_anchoring_a_gusty_wind_at_nan_leaves_it_alone() {
        let mut wind = Wind::new(-0.3).with_gusts(-0.7..=-0.1);
        wind.set_base_speed(f32::NAN);
        assert_eq!(wind.base_speed(), -0.3);
        assert_eq!(wind.speed(), -0.3);

        // Still gusting inside the range it was given, rather than carrying a `NaN` around.
        let mut ctx = Context { _private: () };
        for _ in 0..200 {
            wind.update(&mut ctx);
            assert!(
                (-0.7..=-0.1).contains(&wind.speed()),
                "gust escaped its range: {}",
                wind.speed()
            );
        }
    }

    #[test]
    fn turning_a_wind_keeps_its_speed_and_moves_its_line() {
        // The gusts are the wind's own and have nothing to do with which way it points, so a
        // turn mid-scene leaves them exactly where they were.
        let mut wind = Wind::new(0.4).with_gusts(0.1..=0.6);
        let speed = wind.speed();
        wind.set_direction(Direction::Down);
        assert_eq!(wind.direction(), Direction::Down);
        assert_eq!(wind.speed(), speed);
        assert_eq!(wind.blow(), (0.0, -speed));
    }
}
