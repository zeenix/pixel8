//! What every force here has in common, and the arithmetic they share.

use core::f32::consts::FRAC_1_SQRT_2;

use super::Velocity;
use crate::Direction;

/// A force field: anything that bends what a cast member is travelling at, one update at a time.
///
/// [`Gravity`](super::Gravity), [`Atmosphere`](super::Atmosphere) and [`Wind`](super::Wind) are
/// the three this module ships, and a cart's own is an `impl Force` that works everywhere they do
/// — see the [module docs](super#forces-of-your-own).
///
/// A force is shown one [`Subject`] at a time and touches one thing on it: the velocity. Position
/// is never written, which is what leaves the movement in one place —
/// [`World::step`](super::World::step) runs the forces over the whole cast, stops whatever ran
/// into the world, and moves what survives, in that order.
///
/// Forces compose: a tuple of forces is one force, applied left to right, and `()` is the still
/// air. That is the shape a scene's whole weather takes as the one value its
/// [`World`](super::World) [owns](super::World::with_forces):
///
/// ```no_run
/// # use pixel8::{physics::{Gravity, Wind, World}, Context};
/// # fn f(ctx: &Context) {
/// let mut world: World<8, _> = World::new().with_forces((Gravity::new(), Wind::new(0.3)));
///
/// world.step(ctx);
/// # }
/// ```
pub trait Force {
    /// Bends `subject`'s velocity by one update's worth of this force.
    ///
    /// What the subject's [`mass`](Subject::mass) is worth is this force's own business:
    /// [`Wind`](super::Wind) and [`Atmosphere`](super::Atmosphere) divide their grip by it, while
    /// [`Gravity`](super::Gravity) never reads it at all and pulls everything alike. See the
    /// [module docs](super#mass).
    ///
    /// ```no_run
    /// # use pixel8::physics::{Force, Subject};
    /// /// A current: it pushes, and it pushes something heavy less.
    /// struct Current {
    ///     push: f32,
    /// }
    ///
    /// impl Force for Current {
    ///     fn apply(&self, subject: &mut Subject) {
    ///         let mass = subject.mass();
    ///         subject.velocity_mut().dx += self.push / mass;
    ///     }
    /// }
    /// ```
    ///
    /// Most carts never call this: they hand their forces to the
    /// [`World`](super::World::with_forces), whose step runs them over every member of the cast it
    /// moves, in the order they were composed.
    fn apply(&self, subject: &mut Subject);
}

/// What a force is handed to act on: a member's velocity to bend, and its mass and position to
/// read.
///
/// The world builds one of these per member as the weather runs, out of what it holds for that
/// seat, and takes the bent velocity back the moment the force returns. So a force never sees the
/// cast, never sees a handle, and cannot move anything: it is handed a speed and a couple of
/// facts, and the whole of its say is what it leaves in the speed.
///
/// Reading and bending are separate borrows of nothing at all — [`mass`](Self::mass) and
/// [`pos`](Self::pos) are plain copies — so the order they are asked in no longer matters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subject {
    velocity: Velocity,
    mass: f32,
    pos: (f32, f32),
}

impl Subject {
    /// A subject of a given velocity, mass and position.
    ///
    /// The world makes these; a cart wanting to push its own [`Force`] around in a test of its own
    /// makes one here.
    pub const fn new(velocity: Velocity, mass: f32, pos: (f32, f32)) -> Self {
        Self {
            velocity,
            mass,
            pos,
        }
    }

    /// What the member is travelling at, as the force found it.
    pub const fn velocity(&self) -> Velocity {
        self.velocity
    }

    /// The same velocity, to bend: the one thing a force may change, and the whole of what it
    /// leaves behind.
    pub const fn velocity_mut(&mut self) -> &mut Velocity {
        &mut self.velocity
    }

    /// How hard the member is to push, relative to everything else in the scene.
    ///
    /// `1.0` is the weight nobody has to think about; a member says otherwise once, with
    /// [`Enlisting::weighing`](super::Enlisting::weighing). A force that divides by it does well
    /// to clamp what it works out, the way the ones here do, so that a mass a cart arrived at from
    /// something empty — a zero, a `NaN` — gives an ordinary member rather than one flung off the
    /// screen.
    pub const fn mass(&self) -> f32 {
        self.mass
    }

    /// Where the member is: its exact sub-pixel position.
    ///
    /// For a force that varies over the scene rather than blowing everywhere alike — a magnet, a
    /// current down one side of a level, a fan at the end of a corridor.
    pub const fn pos(&self) -> (f32, f32) {
        self.pos
    }
}

/// The still air: no force at all, and the weather a [`World`](super::World) that never took any
/// owns.
impl Force for () {
    fn apply(&self, _: &mut Subject) {}
}

/// Forces compose: a tuple of them is one force, applied left to right — the whole of a scene's
/// weather as the one value its [`World`](super::World) owns.
macro_rules! forces_compose {
    ($( ( $($force:ident),+ ) )+) => {$(
        #[allow(non_snake_case)]
        impl<$($force: Force),+> Force for ($($force,)+) {
            fn apply(&self, subject: &mut Subject) {
                let ($($force,)+) = self;
                $($force.apply(subject);)+
            }
        }
    )+};
}
forces_compose!((A)(A, B)(A, B, C)(A, B, C, D)(A, B, C, D, E)(
    A, B, C, D, E, G
));

/// A [`mass`](Subject::mass) fit to divide by: one that means nothing — zero, negative, or `NaN`
/// — reads as the default `1.0`.
///
/// Quietly, like everything else here: a cart that works out a mass from something that turned
/// out to be empty gets an ordinary member for it, not a division by zero that puts it a thousand
/// screens away.
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

/// A minimal cast member for the tests throughout this module to push around: a body, a velocity,
/// a slot to be told what it met, and a mass. Nothing else.
///
/// The engine's own tests hand these to [`step_cast`](super::World::step_hosted) as the
/// [`Kinetic`](super::Kinetic)s they are; the forces' tests [shove](Mob::shove) them one update at
/// a time, exactly as the world's weather does.
#[cfg(test)]
pub(super) struct Mob {
    pub(super) body: crate::Body,
    pub(super) velocity: super::Velocity,
    pub(super) contacts: super::Contacts,
    mass: f32,
}

#[cfg(test)]
impl Mob {
    /// One at a standstill, of the weight nobody has to think about.
    pub(super) fn new() -> Self {
        Self {
            body: crate::Body::new(0.0, 0.0),
            velocity: super::Velocity::default(),
            contacts: super::Contacts::default(),
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

    /// One update of `force`, over the subject the world would have made of this one.
    pub(super) fn shove(&mut self, force: &dyn Force) {
        let mut subject = Subject::new(self.velocity, self.mass, self.body.pos());
        force.apply(&mut subject);
        self.velocity = subject.velocity();
    }

    /// The velocity it is left with after `updates` of `force` and nothing else.
    pub(super) fn under(mut self, force: &dyn Force, updates: usize) -> super::Velocity {
        for _ in 0..updates {
            self.shove(force);
        }
        self.velocity
    }

    /// This mob as the trait object the engine's cast is made of.
    pub(super) fn as_kinetic(&mut self) -> &mut dyn super::Kinetic {
        self
    }
}

#[cfg(test)]
impl super::Kinetic for Mob {
    fn body(&self) -> &crate::Body {
        &self.body
    }

    fn body_mut(&mut self) -> &mut crate::Body {
        &mut self.body
    }

    fn velocity_mut(&mut self) -> &mut super::Velocity {
        &mut self.velocity
    }

    fn contacts(&self) -> &super::Contacts {
        &self.contacts
    }

    fn contacts_mut(&mut self) -> &mut super::Contacts {
        &mut self.contacts
    }

    fn bounds(&self) -> super::Bounds {
        super::Bounds::of(&self.body, 8, 8)
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
    fn a_force_reaches_a_member_through_the_subject_alone() {
        // A cart's own force against the subject the world hands it: a velocity to bend, and what
        // the world knows about the member behind it.
        struct Updraft;

        impl Force for Updraft {
            fn apply(&self, subject: &mut Subject) {
                let mass = subject.mass();
                subject.velocity_mut().dy -= 1.0 / mass;
            }
        }

        let force: &dyn Force = &Updraft;
        let mut subject = Subject::new(Velocity::default(), 2.0, (8.0, 16.0));
        force.apply(&mut subject);
        assert_eq!(subject.velocity(), Velocity::new(0.0, -0.5));
        // And nothing moved: a force bends a velocity, and it is the world's step that turns that
        // into movement.
        assert_eq!(subject.pos(), (8.0, 16.0));
    }

    #[test]
    fn a_force_that_reads_where_it_is_pushing_is_shown_the_exact_position() {
        // A force of a cart's own that varies over the scene: everything left of the middle is
        // pushed one way and everything right of it the other.
        struct Draught;

        impl Force for Draught {
            fn apply(&self, subject: &mut Subject) {
                let (x, _) = subject.pos();
                subject.velocity_mut().dx += if x < 64.0 { 1.0 } else { -1.0 };
            }
        }

        let mut near = Subject::new(Velocity::default(), 1.0, (10.5, 0.0));
        let mut far = Subject::new(Velocity::default(), 1.0, (100.0, 0.0));
        Draught.apply(&mut near);
        Draught.apply(&mut far);
        assert_eq!(near.velocity().dx, 1.0);
        assert_eq!(far.velocity().dx, -1.0);
    }
}
