//! The pull a level falls under.

use super::{Force, Kinetic};
use crate::Direction;

/// A steady pull: the level's gravity, applied to everything that falls in it.
///
/// [`new`](Self::new) is a feel-tuned default — [`DEFAULT_STRENGTH`](Self::DEFAULT_STRENGTH)
/// px/update² downwards, capped at
/// [`DEFAULT_TERMINAL_VELOCITY`](Self::DEFAULT_TERMINAL_VELOCITY) px/update — and the builders
/// retune it, [`with_direction`](Self::with_direction) included, for the ceiling a station spins
/// against or the wall a puzzle turns the room onto. See the [module docs](super#gravity).
///
/// **Mass does not enter into it.** Everything falls alike, whatever it weighs, so an anvil and a
/// feather drop side by side and a cart that gives an entity a [`mass`](super::Kinetic::mass) does
/// not change how it falls by doing so. What tells the two apart is the air between them: see
/// [`Atmosphere`](super::Atmosphere), which reads mass exactly where gravity refuses to.
///
/// ```no_run
/// # use pixel8::{physics::{Gravity, Kinetic}, Context};
/// // The level's pull, as a constant of the cart's own, handed to whatever falls in it.
/// const GRAVITY: Gravity = Gravity::new();
///
/// # fn fall(entity: &mut impl Kinetic, ctx: &Context) {
/// entity.step(ctx, &[&GRAVITY]);
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gravity {
    strength: f32,
    terminal: f32,
    direction: Direction,
}

impl Gravity {
    /// The strength [`new`](Self::new) pulls with, in pixels per update squared.
    pub const DEFAULT_STRENGTH: f32 = 0.25;

    /// The fastest [`new`](Self::new) lets something fall, in pixels per update.
    pub const DEFAULT_TERMINAL_VELOCITY: f32 = 4.0;

    /// The way [`new`](Self::new) pulls: down the screen, as gravity mostly does.
    pub const DEFAULT_DIRECTION: Direction = Direction::Down;

    /// The default pull: a quarter of a pixel per update squared, straight down, falling no
    /// faster than four pixels an update.
    ///
    /// `const`, so a level's pull can be a constant of the cart's own:
    /// `const GRAVITY: Gravity = Gravity::new();`.
    pub const fn new() -> Self {
        Self {
            strength: Self::DEFAULT_STRENGTH,
            terminal: Self::DEFAULT_TERMINAL_VELOCITY,
            direction: Self::DEFAULT_DIRECTION,
        }
    }

    /// Sets how hard the pull is, in pixels per update squared, to chain onto
    /// [`new`](Self::new).
    ///
    /// A negative strength is buoyancy: it pushes back the way the pull points, and the terminal
    /// velocity does not hold it back — that caps falling only. Something rising through water
    /// wants its own drag, or a ceiling to stop at.
    pub const fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }

    /// Sets the fastest fall the pull produces, in pixels per update, to chain onto
    /// [`new`](Self::new).
    ///
    /// Worth keeping low: an entity that falls further in one update than a wall is thick passes
    /// through it, whatever the collision code does. A negative terminal is taken literally — the
    /// cap is below every falling speed, so applying it turns a fall into a rise in a single
    /// update — which is almost never what a cart wants.
    ///
    /// A cart leaving the settling to an [`Atmosphere`](super::Atmosphere) instead puts this out
    /// of the way with `f32::MAX`.
    pub const fn with_terminal_velocity(mut self, terminal: f32) -> Self {
        self.terminal = terminal;
        self
    }

    /// Sets which way the pull pulls, to chain onto [`new`](Self::new).
    ///
    /// Down is the default and what a platformer wants. Anything else is a cart with an opinion
    /// about which way is down: a ship under a station's spin, a wall the room turns onto, a
    /// diagonal that drags everything into one corner.
    pub const fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Changes how hard the pull is while the game runs — a level that floods, a switch that
    /// turns the gravity down.
    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength;
    }

    /// Changes the fastest fall the pull produces while the game runs.
    pub fn set_terminal_velocity(&mut self, terminal: f32) {
        self.terminal = terminal;
    }

    /// Turns the pull while the game runs — the moment the station's spin reverses.
    ///
    /// Whatever is already falling keeps the speed it had, so it curves round onto the new pull
    /// rather than snapping onto it.
    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// How hard the pull is, in pixels per update squared.
    pub fn strength(&self) -> f32 {
        self.strength
    }

    /// The fastest fall the pull produces, in pixels per update.
    pub fn terminal_velocity(&self) -> f32 {
        self.terminal
    }

    /// Which way the pull pulls.
    pub fn direction(&self) -> Direction {
        self.direction
    }
}

impl Force for Gravity {
    /// The entity's mass is never read, deliberately: everything falls alike. See [`Gravity`].
    fn apply(&self, entity: &mut dyn Kinetic) {
        let velocity = entity.velocity_mut();
        // The cap is one-sided on purpose: it is a *terminal* velocity, the speed a fall stops
        // gaining at, so a negative strength (buoyancy) rises past it unhindered — and it holds
        // the component along the pull alone, leaving movement across it to whatever else is
        // pushing.
        //
        // The four straight pulls are done on the one axis they touch rather than through the
        // vector below, which is the same arithmetic with two multiplications by one and a zero
        // in it. Exactly the same, in fact: a level tuned against a falling entity gets the float
        // it has always got, to the bit.
        match self.direction {
            Direction::Down => velocity.dy = (velocity.dy + self.strength).min(self.terminal),
            Direction::Up => velocity.dy = (velocity.dy - self.strength).max(-self.terminal),
            Direction::Right => velocity.dx = (velocity.dx + self.strength).min(self.terminal),
            Direction::Left => velocity.dx = (velocity.dx - self.strength).max(-self.terminal),
            diagonal => {
                let (ux, uy) = diagonal.unit();
                velocity.dx += self.strength * ux;
                velocity.dy += self.strength * uy;
                // How fast it is going *along* the pull; the rest of the velocity lies across it
                // and is none of gravity's business.
                let along = velocity.dx * ux + velocity.dy * uy;
                if along > self.terminal {
                    let excess = along - self.terminal;
                    velocity.dx -= excess * ux;
                    velocity.dy -= excess * uy;
                }
            }
        }
    }
}

impl Default for Gravity {
    fn default() -> Self {
        Self::new()
    }
}

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
    fn gravity_accumulates_then_holds_at_terminal_velocity() {
        // `new` is `const`, so a level's pull can be a constant of the cart's own.
        const GRAVITY: Gravity = Gravity::new();
        assert_eq!(GRAVITY.strength(), Gravity::DEFAULT_STRENGTH);
        assert_eq!(
            GRAVITY.terminal_velocity(),
            Gravity::DEFAULT_TERMINAL_VELOCITY
        );
        assert_eq!(GRAVITY.direction(), Direction::Down);

        let mut mob = Mob::new();
        GRAVITY.apply(&mut mob);
        assert_eq!(mob.velocity, Velocity::new(0.0, Gravity::DEFAULT_STRENGTH));
        GRAVITY.apply(&mut mob);
        assert_eq!(mob.velocity.dy, 2.0 * Gravity::DEFAULT_STRENGTH);
        // Only `dy`: gravity has no opinion about sideways movement.
        assert_eq!(mob.velocity.dx, 0.0);

        let settled = Mob::new().under(&GRAVITY, 1_000);
        assert_eq!(settled.dy, Gravity::DEFAULT_TERMINAL_VELOCITY);
    }

    #[test]
    fn buoyancy_rises_past_the_terminal_velocity() {
        // A negative strength pushes up, and the cap on falling must not hold it back.
        let buoyancy = Gravity::new()
            .with_strength(-0.5)
            .with_terminal_velocity(1.0);
        assert_eq!(Mob::new().under(&buoyancy, 100).dy, -50.0);
    }

    #[test]
    fn retuned_gravity_uses_the_new_constants() {
        let mut gravity = Gravity::default();
        gravity.set_strength(0.1);
        gravity.set_terminal_velocity(0.35);
        assert_eq!(Mob::new().under(&gravity, 10).dy, 0.35);
    }

    #[test]
    fn gravity_falls_the_same_whatever_the_mass() {
        // Mass is how hard a thing is to push, not how hard it falls — and gravity does not so
        // much as read it off the entity.
        let gravity = Gravity::new();
        let (mut feather, mut anvil) = (Mob::with_mass(0.01), Mob::with_mass(100.0));
        for _ in 0..100 {
            gravity.apply(&mut feather);
            gravity.apply(&mut anvil);
            assert_eq!(feather.velocity, anvil.velocity);
        }
        assert_eq!(feather.velocity.dy, Gravity::DEFAULT_TERMINAL_VELOCITY);
    }

    #[test]
    fn each_straight_direction_pulls_and_caps_along_its_own_axis() {
        // The same pull, turned four ways: it gains on one axis, tops out at the terminal
        // velocity, and never touches the other.
        for (direction, settled) in [
            (Direction::Down, (0.0, 4.0)),
            (Direction::Up, (0.0, -4.0)),
            (Direction::Right, (4.0, 0.0)),
            (Direction::Left, (-4.0, 0.0)),
        ] {
            let gravity = Gravity::new().with_direction(direction);
            let (ux, uy) = direction.unit();
            let first = Mob::new().under(&gravity, 1);
            assert_eq!(
                (first.dx, first.dy),
                (
                    Gravity::DEFAULT_STRENGTH * ux,
                    Gravity::DEFAULT_STRENGTH * uy
                ),
                "{direction:?} pulled somewhere else"
            );

            let velocity = Mob::new().under(&gravity, 1_000);
            assert_eq!(
                (velocity.dx, velocity.dy),
                settled,
                "{direction:?} did not settle at its terminal velocity"
            );
        }
    }

    #[test]
    fn a_sideways_pull_leaves_the_other_axis_alone() {
        // Whatever else is moving the entity across the pull carries on untouched, and uncapped.
        let gravity = Gravity::new().with_direction(Direction::Right);
        let velocity = Mob::moving(0.0, -9.0).under(&gravity, 1_000);
        assert_eq!(velocity.dy, -9.0);
        assert_eq!(velocity.dx, Gravity::DEFAULT_TERMINAL_VELOCITY);
    }

    #[test]
    fn a_diagonal_pull_caps_along_itself_and_not_across() {
        let direction = Direction::DownRight;
        let (ux, uy) = direction.unit();
        let gravity = Gravity::new().with_direction(direction);

        // Moving across the pull to begin with: that part of the velocity must survive the cap,
        // which holds the component along the pull alone.
        let (acrossx, acrossy) = (uy, -ux);
        let velocity = Mob::moving(3.0 * acrossx, 3.0 * acrossy).under(&gravity, 1_000);

        let along = velocity.dx * ux + velocity.dy * uy;
        let across = velocity.dx * acrossx + velocity.dy * acrossy;
        assert!(
            (along - Gravity::DEFAULT_TERMINAL_VELOCITY).abs() < 1e-4,
            "the pull did not settle at its terminal velocity: {along}"
        );
        assert!(
            (across - 3.0).abs() < 1e-4,
            "the sideways movement was capped too: {across}"
        );
    }

    #[test]
    fn every_direction_accelerates_by_the_whole_strength() {
        // However it is turned, one update's pull is one strength's worth of speed along it —
        // the diagonals are not √2 times as strong for being two axes at once.
        for direction in DIRECTIONS {
            let (ux, uy) = direction.unit();
            let velocity = Mob::new().under(&Gravity::new().with_direction(direction), 1);
            let along = velocity.dx * ux + velocity.dy * uy;
            assert!(
                (along - Gravity::DEFAULT_STRENGTH).abs() < 1e-6,
                "{direction:?} pulled by {along}"
            );
        }
    }
}
