//! What every force here has in common, and the arithmetic they share.

use core::f32::consts::FRAC_1_SQRT_2;

use super::Kinetic;
use crate::Direction;

/// A force field: anything that bends what an entity is travelling at, one update at a time.
///
/// [`Gravity`](super::Gravity), [`Atmosphere`](super::Atmosphere) and [`Wind`](super::Wind) are
/// the three this module ships, and a cart's own is an `impl Force` that works everywhere they do
/// — see the [module docs](super#forces-of-your-own).
///
/// A force reaches an entity through what [`Kinetic`] grants and nothing else: it reads whatever
/// it cares about — the [`mass`](Kinetic::mass), usually — and bends the
/// [`velocity_mut`](Kinetic::velocity_mut). Position is never touched, which is what leaves the
/// cart in charge of movement: [`Kinetic::step`] applies the forces, stops whatever ran into the
/// map, and moves the body with what survives, in that order.
///
/// `Force` is dyn-compatible, so forces of different types can be gathered up and handed over
/// together — which is exactly what [`step`](Kinetic::step) takes:
///
/// ```no_run
/// # use pixel8::{physics::{Force, Gravity, Kinetic, Wind}, Context};
/// # fn f(entity: &mut impl Kinetic, ctx: &Context) {
/// let gravity = Gravity::new();
/// let wind = Wind::new(0.3);
/// let weather: [&dyn Force; 2] = [&gravity, &wind];
///
/// entity.step(ctx, &weather);
/// # }
/// ```
pub trait Force {
    /// Bends `entity`'s velocity by one update's worth of this force.
    ///
    /// What the entity's [`mass`](Kinetic::mass) is worth is this force's own business:
    /// [`Wind`](super::Wind) and [`Atmosphere`](super::Atmosphere) divide their grip by it, while
    /// [`Gravity`](super::Gravity) never reads it at all and pulls everything alike. See the
    /// [module docs](super#mass).
    ///
    /// Read whatever is wanted off the entity *before* taking its velocity, or the two borrows
    /// overlap:
    ///
    /// ```no_run
    /// # use pixel8::physics::{Force, Kinetic};
    /// /// A current: it pushes, and it pushes something heavy less.
    /// struct Current {
    ///     push: f32,
    /// }
    ///
    /// impl Force for Current {
    ///     fn apply(&self, entity: &mut dyn Kinetic) {
    ///         let mass = entity.mass();
    ///         entity.velocity_mut().dx += self.push / mass;
    ///     }
    /// }
    /// ```
    ///
    /// Most carts never call this: they hand their forces to [`Kinetic::step`], which applies them
    /// in the order the slice puts them.
    fn apply(&self, entity: &mut dyn Kinetic);
}

/// A [`mass`](Kinetic::mass) fit to divide by: one that means nothing — zero, negative, or `NaN`
/// — reads as the default `1.0`.
///
/// Quietly, like everything else here: a cart that works out a mass from something that turned
/// out to be empty gets an ordinary entity for it, not a division by zero that puts the entity a
/// thousand screens away.
pub(super) fn weighed(mass: f32) -> f32 {
    // `NaN` fails this comparison along with the zero and the negatives, which is the point.
    if mass > 0.0 {
        mass
    } else {
        1.0
    }
}

impl Direction {
    /// The unit vector pointing this way in screen space, where `y` grows downwards.
    ///
    /// The diagonals are as long as the straight ones rather than the √2 that adding two whole
    /// axes together would give, so a force pulling into a corner pulls exactly as hard as one
    /// pulling along an axis.
    pub(super) fn unit(self) -> (f32, f32) {
        match self {
            Self::Up => (0.0, -1.0),
            Self::UpRight => (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
            Self::Right => (1.0, 0.0),
            Self::DownRight => (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Self::Down => (0.0, 1.0),
            Self::DownLeft => (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
            Self::Left => (-1.0, 0.0),
            Self::UpLeft => (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
        }
    }
}

/// Every direction, in the order they turn — for the tests that have to try all eight.
#[cfg(test)]
pub(super) const DIRECTIONS: [Direction; 8] = [
    Direction::Up,
    Direction::UpRight,
    Direction::Right,
    Direction::DownRight,
    Direction::Down,
    Direction::DownLeft,
    Direction::Left,
    Direction::UpLeft,
];

/// A minimal [`Kinetic`] for the tests throughout this module to push around: a body, a velocity
/// and a mass, and nothing else.
#[cfg(test)]
pub(super) struct Mob {
    pub(super) body: crate::Body,
    pub(super) velocity: super::Velocity,
    mass: f32,
}

#[cfg(test)]
impl Mob {
    /// One at a standstill, of the weight nobody has to think about.
    pub(super) fn new() -> Self {
        Self {
            body: crate::Body::new(0.0, 0.0),
            velocity: super::Velocity::default(),
            mass: 1.0,
        }
    }

    /// One that takes some shifting, or hardly any.
    pub(super) fn with_mass(mass: f32) -> Self {
        Self {
            mass,
            ..Self::new()
        }
    }

    /// One already travelling.
    pub(super) fn moving(dx: f32, dy: f32) -> Self {
        Self {
            velocity: super::Velocity::new(dx, dy),
            ..Self::new()
        }
    }

    /// The velocity it is left with after `updates` of `force` and nothing else.
    pub(super) fn under(mut self, force: &dyn Force, updates: usize) -> super::Velocity {
        for _ in 0..updates {
            force.apply(&mut self);
        }
        self.velocity
    }
}

#[cfg(test)]
impl Kinetic for Mob {
    fn body(&self) -> &crate::Body {
        &self.body
    }

    fn body_mut(&mut self) -> &mut crate::Body {
        &mut self.body
    }

    fn velocity_mut(&mut self) -> &mut super::Velocity {
        &mut self.velocity
    }

    fn bounds(&self) -> super::Bounds {
        super::Bounds::of(&self.body, 8, 8)
    }

    fn mass(&self) -> f32 {
        self.mass
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Velocity, *};

    #[test]
    fn unit_vectors_point_where_they_say_and_are_all_one_long() {
        // Screen space: `y` grows downwards, so up is negative.
        assert_eq!(Direction::Up.unit(), (0.0, -1.0));
        assert_eq!(Direction::Down.unit(), (0.0, 1.0));
        assert_eq!(Direction::Right.unit(), (1.0, 0.0));
        assert_eq!(Direction::Left.unit(), (-1.0, 0.0));
        // A diagonal splits itself between the two axes rather than taking a whole one of each.
        let (x, y) = Direction::UpLeft.unit();
        assert!(x < 0.0 && y < 0.0);

        for direction in DIRECTIONS {
            let (x, y) = direction.unit();
            assert!(
                (x * x + y * y - 1.0).abs() < 1e-6,
                "{direction:?} is not a unit vector: ({x}, {y})"
            );
        }
    }

    #[test]
    fn opposite_directions_have_opposite_units() {
        for (there, back) in [
            (Direction::Up, Direction::Down),
            (Direction::Left, Direction::Right),
            (Direction::UpLeft, Direction::DownRight),
            (Direction::UpRight, Direction::DownLeft),
        ] {
            let ((x, y), (bx, by)) = (there.unit(), back.unit());
            assert_eq!((x, y), (-bx, -by), "{there:?} does not face {back:?}");
        }
    }

    #[test]
    fn a_mass_that_means_nothing_weighs_one() {
        assert_eq!(weighed(4.0), 4.0);
        for mass in [0.0, -3.0, f32::NAN, f32::NEG_INFINITY] {
            assert_eq!(weighed(mass), 1.0, "a mass of {mass} was not read as 1.0");
        }
    }

    #[test]
    fn a_force_reaches_an_entity_through_the_traits_alone() {
        // A cart's own force applied to a cart's own entity, both as trait objects: the whole of
        // what the two traits promise each other.
        struct Updraft;

        impl Force for Updraft {
            fn apply(&self, entity: &mut dyn Kinetic) {
                let mass = entity.mass();
                entity.velocity_mut().dy -= 1.0 / mass;
            }
        }

        let force: &dyn Force = &Updraft;
        let mut mob = Mob::with_mass(2.0);
        let entity: &mut dyn Kinetic = &mut mob;
        force.apply(entity);
        assert_eq!(mob.velocity, Velocity::new(0.0, -0.5));
        // And nothing moved: a force bends a velocity, and it is the entity's own step that
        // turns that into movement.
        assert_eq!(mob.body.pos(), (0.0, 0.0));
    }
}
