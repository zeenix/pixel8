//! The air a scene is played in.

use super::{force::weighed, Force, Kinetic};

/// The air itself: it drags on everything that moves through it, on both axes, forever.
///
/// [`new`](Self::new) is air at sea level and [`vacuum`](Self::vacuum) is none at all. What sits
/// between them is [`density`](Self::density) — the share of an entity's speed the air takes back
/// each update — so thin air on a mountain, or the soup at the bottom of a lake, is a number.
///
/// **Mass is the whole point of it.** The drag is the density over what the entity weighs, so the
/// same air takes most of a feather's speed and almost none of an anvil's.
/// [`Gravity`](super::Gravity) pulls the two alike; it is the air that tells them apart, and a
/// scene wanting that difference wants an atmosphere rather than a heavier pull. See the [module
/// docs](super#atmosphere).
///
/// ```no_run
/// # use pixel8::physics::{Atmosphere, Force, Gravity, Velocity};
/// // Thin air: things fall four times as fast up here before they settle.
/// const MOUNTAIN: Atmosphere = Atmosphere::new().with_density(Atmosphere::DEFAULT_DENSITY / 4.0);
/// // Air alone deciding how fast things fall, with gravity's own cap kept out of the way.
/// const PULL: Gravity = Gravity::new().with_terminal_velocity(f32::MAX);
/// const WEATHER: &[&dyn Force] = &[&PULL, &MOUNTAIN];
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Atmosphere {
    density: f32,
}

impl Atmosphere {
    /// The density of [`new`](Self::new): the air a cart is played in unless it says otherwise.
    ///
    /// A sixteenth, which is where the platformer feel comes from rather than from any real
    /// figure. Drag takes `density` of the speed each update and the default
    /// [`Gravity`](super::Gravity) adds [`DEFAULT_STRENGTH`](super::Gravity::DEFAULT_STRENGTH)
    /// to it, so a `1.0`-mass fall stops gaining where the two cancel — at `strength /
    /// density`, which is `0.25 / 0.0625` — four pixels an update, exactly the
    /// [`DEFAULT_TERMINAL_VELOCITY`](super::Gravity::DEFAULT_TERMINAL_VELOCITY) the
    /// cheap cap gives. Sea level, in other words, is defined here as the air that makes the two
    /// answers agree.
    pub const DEFAULT_DENSITY: f32 = 0.0625;

    /// Air at sea level: [`DEFAULT_DENSITY`](Self::DEFAULT_DENSITY), tuned to settle an ordinary
    /// fall exactly where the default [`Gravity`](super::Gravity) caps one.
    ///
    /// `const`, so a scene's air can be a constant of the cart's own:
    /// `const AIR: Atmosphere = Atmosphere::new();`.
    pub const fn new() -> Self {
        Self {
            density: Self::DEFAULT_DENSITY,
        }
    }

    /// No air at all: [`apply`](Force::apply) does nothing, and everything falls forever.
    ///
    /// The honest thing to put in a scene set in space, and a way to switch the air off — a
    /// [`density`](Self::density) of zero is a vacuum however it was arrived at.
    pub const fn vacuum() -> Self {
        Self { density: 0.0 }
    }

    /// Sets how thick the air is, to chain onto [`new`](Self::new).
    ///
    /// `density` is the share of an ordinary entity's speed the air takes back each update,
    /// clamped to `0.0..=1.0`: `0.0` is a vacuum and `1.0` is air that stops anything of mass
    /// `1.0` dead the moment it lets go. Denser air both slows things sooner and settles a fall
    /// lower, the two being the same arithmetic.
    pub const fn with_density(mut self, density: f32) -> Self {
        self.density = clamped(density);
        self
    }

    /// Changes how thick the air is while the game runs — a dive into water, a ship leaving the
    /// atmosphere. `density` is clamped to `0.0..=1.0`.
    pub fn set_density(&mut self, density: f32) {
        self.density = clamped(density);
    }

    /// How thick the air is: the share of an ordinary entity's speed it takes back each update.
    pub fn density(&self) -> f32 {
        self.density
    }
}

impl Force for Atmosphere {
    /// The mass divides the drag: the same air barely holds an anvil and takes a feather at once.
    fn apply(&self, entity: &mut dyn Kinetic) {
        // A share of the speed rather than a constant subtracted from it, which is what makes the
        // drag grow with the speed and a fall settle where it matches the pull. The clamp holds
        // that for something light enough for the air to take whole: a share past 1.0 would stop
        // it and start dragging it backwards.
        let take = (self.density / weighed(entity.mass())).clamp(0.0, 1.0);
        let velocity = entity.velocity_mut();
        velocity.dx -= velocity.dx * take;
        velocity.dy -= velocity.dy * take;
    }
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self::new()
    }
}

/// A density fit to take a share by: outside `0.0..=1.0` there is no share to take, and `NaN` is
/// not a density at all — it reads as a vacuum, the quiet answer everywhere else here.
const fn clamped(density: f32) -> f32 {
    // Written out rather than `f32::clamp`, which is not `const`; the `NaN` falls through both
    // comparisons to the vacuum.
    if density > 1.0 {
        1.0
    } else if density > 0.0 {
        density
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{force::Mob, Gravity, Velocity},
        *,
    };

    #[test]
    fn the_default_density_settles_a_fall_where_the_cheap_cap_would_have() {
        // The derivation the constant is picked from, and the whole reason it is a sixteenth:
        // gravity adds `strength` an update and the air takes `density` of the speed, so a fall
        // stops gaining at `strength / density`.
        assert_eq!(
            Gravity::DEFAULT_STRENGTH / Atmosphere::DEFAULT_DENSITY,
            Gravity::DEFAULT_TERMINAL_VELOCITY
        );

        // And it does: with gravity's own cap put out of the way, the air alone settles an
        // ordinary fall at four pixels an update.
        const AIR: Atmosphere = Atmosphere::new();
        const PULL: Gravity = Gravity::new().with_terminal_velocity(f32::MAX);
        let mut mob = Mob::new();
        for _ in 0..1_000 {
            AIR.apply(&mut mob);
            PULL.apply(&mut mob);
        }
        assert!(
            (mob.velocity.dy - Gravity::DEFAULT_TERMINAL_VELOCITY).abs() < 1e-3,
            "settled at {} rather than the terminal velocity",
            mob.velocity.dy
        );

        // Running the pull first settles it one update's pull lower — `strength * (1 - density) /
        // density` — because that update's drag has not felt the pull yet. Which order the forces
        // run in is the slice's, and this is the whole of what it costs.
        let mut mob = Mob::new();
        for _ in 0..1_000 {
            PULL.apply(&mut mob);
            AIR.apply(&mut mob);
        }
        let settled = Gravity::DEFAULT_TERMINAL_VELOCITY - Gravity::DEFAULT_STRENGTH;
        assert!(
            (mob.velocity.dy - settled).abs() < 1e-3,
            "settled at {} rather than {settled}",
            mob.velocity.dy
        );
    }

    #[test]
    fn a_vacuum_does_nothing_at_all() {
        const SPACE: Atmosphere = Atmosphere::vacuum();
        assert_eq!(SPACE.density(), 0.0);
        assert_eq!(
            Mob::moving(3.0, -7.5).under(&SPACE, 100),
            Velocity::new(3.0, -7.5)
        );

        // However arrived at: a density of nothing is a vacuum.
        let mut air = Atmosphere::new();
        air.set_density(0.0);
        assert_eq!(air, SPACE);
    }

    #[test]
    fn the_air_drags_both_axes_towards_a_standstill() {
        let air = Atmosphere::new();
        let mut mob = Mob::moving(2.0, -2.0);
        let (mut previous_x, mut previous_y) = (mob.velocity.dx, mob.velocity.dy);
        for _ in 0..600 {
            air.apply(&mut mob);
            // Always towards rest, never past it: a share of what is left is never more than what
            // is left.
            assert!(mob.velocity.dx >= 0.0 && mob.velocity.dx <= previous_x);
            assert!(mob.velocity.dy <= 0.0 && mob.velocity.dy >= previous_y);
            (previous_x, previous_y) = (mob.velocity.dx, mob.velocity.dy);
        }
        assert!(mob.velocity.dx < 0.001, "still moving: {}", mob.velocity.dx);
    }

    #[test]
    fn mass_divides_the_drag() {
        // The same air on three things that differ in nothing but what they weigh. This is the
        // difference gravity refuses to make.
        let air = Atmosphere::new();
        let drifting = |mass| {
            let mut mob = Mob::with_mass(mass);
            mob.velocity = Velocity::new(4.0, 0.0);
            mob.under(&air, 30)
        };
        let (feather, ordinary, anvil) = (drifting(0.25), drifting(1.0), drifting(4.0));
        assert!(
            feather.dx < ordinary.dx && ordinary.dx < anvil.dx,
            "the air did not tell them apart: {} {} {}",
            feather.dx,
            ordinary.dx,
            anvil.dx
        );

        // And a mass that means nothing is an ordinary one, rather than a division by zero.
        for mass in [0.0, -3.0, f32::NAN] {
            assert_eq!(
                drifting(mass),
                ordinary,
                "a mass of {mass} was not read as 1.0"
            );
        }
    }

    #[test]
    fn something_light_enough_is_stopped_rather_than_blown_backwards() {
        // A share past the whole would take more speed than there is and start pushing the other
        // way, which is not what air does.
        let mut mote = Mob::with_mass(0.001);
        mote.velocity = Velocity::new(4.0, 4.0);
        assert_eq!(mote.under(&Atmosphere::new(), 1), Velocity::default());
    }

    #[test]
    fn a_density_that_means_nothing_is_taken_quietly() {
        assert_eq!(Atmosphere::new().with_density(9.0).density(), 1.0);
        assert_eq!(Atmosphere::new().with_density(-9.0).density(), 0.0);
        assert_eq!(Atmosphere::new().with_density(f32::NAN).density(), 0.0);

        // The setter reads them exactly as the builder it backs does.
        let mut air = Atmosphere::default();
        assert_eq!(air.density(), Atmosphere::DEFAULT_DENSITY);
        air.set_density(9.0);
        assert_eq!(air.density(), 1.0);
        air.set_density(f32::NAN);
        assert_eq!(air.density(), 0.0);
    }
}
