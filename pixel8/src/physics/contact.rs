//! The sides an entity was stopped on, whatever stopped it.

use crate::{flags::bitflag_enum, BitFlags};

bitflag_enum! {
    /// One side of an entity that ran into something solid.
    ///
    /// Screen space, like everything else: [`Below`](Self::Below) is the floor an entity landed
    /// on and [`Above`](Self::Above) the ceiling it bumped its head on.
    pub enum Contact {
        /// Something solid under the entity — it landed. A platformer's *grounded*.
        Below = 1 << 0,
        /// Something solid over the entity — it bumped its head.
        Above = 1 << 1,
        /// A wall to the entity's left.
        Left = 1 << 2,
        /// A wall to the entity's right.
        Right = 1 << 3,
    }
}

/// The sides an entity was stopped on — none of them, one, or two at once for something wedged
/// into a corner.
///
/// What [`Kinetic::step`](super::Kinetic::step) reports of the map and
/// [`Kinetic::keep_within`](super::Kinetic::keep_within) of the edge of the world, so a cart
/// standing an entity on the bottom of the screen and one standing it on a floor tile read the
/// same answer the same way.
///
/// [`step`](super::Kinetic::step) reports a side only when the entity was *moving* that way and
/// something stopped it, so one resting against a wall it is not pushing into reports nothing.
/// [`keep_within`](super::Kinetic::keep_within) reports where it *held* the entity, which is the
/// same thing for anything that walked there under its own steam.
pub type Contacts = BitFlags<Contact>;

impl Contacts {
    /// Something solid stopped the entity falling: it is standing on something.
    ///
    /// The one a platformer reads every update, to know whether a jump is allowed.
    pub fn below(self) -> bool {
        self.contains(Contact::Below)
    }

    /// Something solid stopped the entity rising: it bumped its head.
    pub fn above(self) -> bool {
        self.contains(Contact::Above)
    }

    /// A wall stopped the entity moving left.
    pub fn left(self) -> bool {
        self.contains(Contact::Left)
    }

    /// A wall stopped the entity moving right.
    pub fn right(self) -> bool {
        self.contains(Contact::Right)
    }
}
