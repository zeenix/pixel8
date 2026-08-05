//! What a step ran into: the sides that were stopped, and the flags of everything met.

use core::ops::{BitOr, BitOrAssign};

use crate::{flags::bitflag_enum, BitFlags, SpriteFlag};

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

/// Everything one step of an entity ran into: the sides that were stopped, and the sprite flags
/// of all it met on the way.
///
/// What [`World::step`](super::World::step) writes into every entity's
/// [`contacts`](super::Kinetic::contacts) slot — the whole of it, walls and the edge of the world
/// together — so a cart standing an entity on the bottom of the level, on a floor tile and on a
/// moving platform reads the same answer the same way.
///
/// The two halves answer two kinds of question. The *sides* are the solid story: a side is
/// reported when the entity was moving that way and something stopped it — or held it, at the
/// edge of its [`confines`](super::Kinetic::confines), or pushed it back out, for a cast member
/// that had come to stand on it — so one resting against a wall it is not pushing into reports
/// nothing, and [`below`](Self::below) is what a platformer calls *grounded*. The *flags* —
/// [`touched`](Self::touched) — are everything else: every sprite flag carried by a tile or by
/// another cast member anywhere on the ground the step covered, solid to this entity or not.
/// Water, a hazard, a pickup, a switch: one step, and the slot says which of them the entity met.
///
/// The two are answered over different ground, and on purpose. A side is the endpoint's answer —
/// the entity is stopped where it was trying to go, which is where a wall has to be to be one — and
/// the flags are the whole step's, taken over where it began, everything it crossed and where it
/// ended up. So something thin enough to be stepped clean over in one update does not stop the
/// entity and is still named.
///
/// ```no_run
/// # use pixel8::{physics::Kinetic, SpriteFlag};
/// # const SPIKES: SpriteFlag = SpriteFlag::Flag2;
/// # fn f(hero: &impl Kinetic) -> (bool, bool) {
/// // The walls, the spikes and the edge of the world, in one answer the world left behind.
/// let contacts = hero.contacts();
///
/// (contacts.below(), contacts.touches(SPIKES))
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contacts {
    /// The sides that were stopped, held, or pushed.
    pub(super) sides: BitFlags<Contact>,
    /// The flags of everything met, whatever it meant to this entity.
    pub(super) touched: BitFlags<SpriteFlag>,
}

impl Contacts {
    /// The two halves as raw bits, for the physics step's crossing of the ABI. Not a cart's
    /// business.
    #[doc(hidden)]
    pub fn wire(self) -> (u8, u8) {
        (self.sides.bits(), self.touched.bits())
    }

    /// A `Contacts` back off the wire. Not a cart's business.
    #[doc(hidden)]
    pub fn from_wire(sides: u8, touched: u8) -> Self {
        let mismatch = "step_cast answered an unknown flag bit (pixel8 host/SDK mismatch)";

        Self {
            sides: BitFlags::from_bits(sides).expect(mismatch),
            touched: BitFlags::from_bits(touched).expect(mismatch),
        }
    }

    /// A step that met nothing at all: no side stopped, no flag touched.
    pub const fn empty() -> Self {
        Self {
            sides: BitFlags::empty(),
            touched: BitFlags::empty(),
        }
    }

    /// Something solid stopped the entity falling: it is standing on something.
    ///
    /// The one a platformer reads every update, to know whether a jump is allowed.
    pub fn below(self) -> bool {
        self.sides().contains(Contact::Below)
    }

    /// Something solid stopped the entity rising: it bumped its head.
    pub fn above(self) -> bool {
        self.sides().contains(Contact::Above)
    }

    /// A wall stopped the entity moving left.
    pub fn left(self) -> bool {
        self.sides().contains(Contact::Left)
    }

    /// A wall stopped the entity moving right.
    pub fn right(self) -> bool {
        self.sides().contains(Contact::Right)
    }

    /// Every side that was stopped, held, or pushed, as the set it is.
    ///
    /// The four above are the spellings a cart writes down; this is the same answer for the
    /// question that only knows its side at run time — the way a patrol happens to be walking, the
    /// side a blow came from — and for the update that would rather look at the whole set at once
    /// than ask it four questions.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Contact, Kinetic};
    /// # fn f(patrol: &impl Kinetic, walking_left: bool) -> bool {
    /// let ahead = if walking_left { Contact::Left } else { Contact::Right };
    ///
    /// // A wall in front of the patrol, whichever way "in front" is this update.
    /// patrol.contacts().sides().contains(ahead)
    /// # }
    /// ```
    pub fn sides(self) -> BitFlags<Contact> {
        self.sides
    }

    /// Every sprite flag carried by anything on the ground the step covered — where it began, what
    /// it crossed, what stopped it, and what it ended up inside — and by anything whose own step
    /// arrived on this entity.
    ///
    /// The whole of the step and not its two ends: an entity that walked out of the pond this
    /// update is told it was in the pond, and one that crossed a two-pixel trickle in the middle of
    /// a twelve-pixel stride is told about the trickle. That is more than the sides can promise —
    /// stopping is resolved where the entity was trying to go, so a thing thin enough to be stepped
    /// clean over is reported and not stopped at, and what keeps a fall from doing it to a floor is
    /// [`Gravity`](super::Gravity)'s terminal velocity.
    ///
    /// A meeting between cast members reaches both of them, whichever one's movement made it. An
    /// entity standing still as something flies into it is told in the very update the arrival
    /// happens — not a frame later, when its own step would find the overlap, and not never, where
    /// the arriver dies of the meeting and is dropped from the cast before its wreck could be
    /// walked into. Only the flags carry the news: an arrival never writes a side, since the one
    /// arrived on was stopped by nothing.
    ///
    /// The flags say what *kind* of thing was met, never which one: a step through two patches
    /// of water reads exactly like a step through one. A cart that must know which — the coin to
    /// take off the map, the enemy to kill — looks at the state it already holds: the map, under
    /// [`bounds`](super::Kinetic::bounds), or the one badie it keeps, asked
    /// [`overlaps`](super::Kinetic::overlaps) if the rectangles have to be compared at all.
    ///
    /// ```no_run
    /// # use pixel8::{physics::Kinetic, SpriteFlag};
    /// # const WATER: SpriteFlag = SpriteFlag::Flag3;
    /// # fn f(hero: &impl Kinetic) {
    /// let swimming = hero.contacts().touched().contains(WATER);
    /// # }
    /// ```
    pub fn touched(self) -> BitFlags<SpriteFlag> {
        self.touched
    }

    /// Whether the step met anything carrying any of `flags`.
    ///
    /// [`touched`](Self::touched) with the question a cart actually asks put to it: one flag or
    /// several, and any of them in common is a yes. So the water, the lava and the spikes can be
    /// asked about in one call, and an entity that met none of them is told so once.
    ///
    /// ```no_run
    /// # use pixel8::{physics::Kinetic, SpriteFlag};
    /// # const WATER: SpriteFlag = SpriteFlag::Flag3;
    /// # const LAVA: SpriteFlag = SpriteFlag::Flag4;
    /// # fn f(hero: &impl Kinetic) {
    /// let contacts = hero.contacts();
    /// let swimming = contacts.touches(WATER);
    /// let burning = contacts.touches(LAVA | WATER);
    /// # }
    /// ```
    pub fn touches(self, flags: impl Into<BitFlags<SpriteFlag>>) -> bool {
        self.touched.intersects(flags)
    }
}

/// Two answers about the same update, folded into one: the sides of either, the flags of either.
///
/// One step is one answer, so a cart rarely needs this for a single entity. What it is for is a
/// whole cast read as one — what the swarm ran into this update, whether *anything* reached the
/// water — where the fold is what turns a slice's worth of answers back into one:
///
/// ```no_run
/// # use pixel8::physics::{Contacts, Kinetic};
/// # fn f(swarm: &[impl Kinetic]) -> Contacts {
/// let mut met = Contacts::empty();
/// for wasp in swarm {
///     met |= *wasp.contacts();
/// }
///
/// met
/// # }
/// ```
impl BitOr for Contacts {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self {
            sides: self.sides | other.sides,
            touched: self.touched | other.touched,
        }
    }
}

/// The same fold, for a cart gathering an update's answers as they arrive. See [`BitOr`].
impl BitOrAssign for Contacts {
    fn bitor_assign(&mut self, other: Self) {
        *self = *self | other;
    }
}

/// A step that met nothing, which is what an entity that has not been stepped yet has met.
///
/// The slot every [`Kinetic`](super::Kinetic) holds starts here, so a cart writing an entity down
/// spells it `Contacts::default()` — or derives `Default` for the whole entity and never mentions
/// it at all.
impl Default for Contacts {
    fn default() -> Self {
        Self::empty()
    }
}

/// A single stopped side and nothing else — what most of the tests reach for.
impl From<Contact> for Contacts {
    fn from(side: Contact) -> Self {
        Self {
            sides: side.into(),
            touched: BitFlags::empty(),
        }
    }
}

/// Stopped sides and nothing touched — the shape of a hold at the edge of the world, which is a
/// place rather than a thing with flags on it.
impl From<BitFlags<Contact>> for Contacts {
    fn from(sides: BitFlags<Contact>) -> Self {
        Self {
            sides,
            touched: BitFlags::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a cart flags its walls with, its water, and the spikes in between.
    const WALL: SpriteFlag = SpriteFlag::Flag0;
    const WATER: SpriteFlag = SpriteFlag::Flag1;
    const SPIKES: SpriteFlag = SpriteFlag::Flag2;

    /// A step that was stopped `sides` and met `touched` — what a resolution hands back.
    fn met(sides: BitFlags<Contact>, touched: BitFlags<SpriteFlag>) -> Contacts {
        Contacts { sides, touched }
    }

    #[test]
    fn touches_answers_for_any_flag_in_common() {
        let swum = met(BitFlags::empty(), WATER.into());
        assert!(swum.touches(WATER));
        assert!(!swum.touches(WALL));

        // Several at once: any of them in common is a yes, which is how one call asks about a
        // whole family of hazards.
        assert!(swum.touches(WATER | SPIKES));
        assert!(!swum.touches(WALL | SPIKES));

        // And the empty question, which nothing answers — not even a step that met everything.
        let everything = met(BitFlags::empty(), WALL | WATER | SPIKES);
        assert!(everything.touches(WALL) && everything.touches(SPIKES));
        assert!(!everything.touches(BitFlags::empty()));
        assert!(!Contacts::empty().touches(WALL));
    }

    #[test]
    fn the_stopped_sides_come_back_as_the_set_they_are() {
        // Cornered: stopped on the way down and on the way left, in one update.
        let cornered = met(Contact::Below | Contact::Left, WALL.into());
        assert_eq!(cornered.sides(), Contact::Below | Contact::Left);

        // The side a patrol works out rather than writes down, asked of the whole set — which is
        // the question the four named spellings cannot be handed.
        let walking_left = true;
        let ahead = if walking_left {
            Contact::Left
        } else {
            Contact::Right
        };
        assert!(cornered.sides().contains(ahead));
        assert!(!cornered.sides().contains(Contact::Above));

        // And the named spellings are that same set, read one side at a time.
        assert!(cornered.below() && cornered.left());
        assert!(!cornered.above() && !cornered.right());
        assert_eq!(Contacts::empty().sides(), BitFlags::empty());
    }

    #[test]
    fn two_answers_about_one_update_fold_into_one() {
        // Two entities of a cast, or two updates of one: both halves of each survive the fold.
        let stepped = met(Contact::Left.into(), WATER.into());
        let held = met(Contact::Below.into(), WALL.into());
        let both = stepped | held;
        assert!(both.left() && both.below());
        assert_eq!(both.touched(), WALL | WATER);

        // And gathered as they arrive, which is the same thing said in place.
        let mut gathered = Contacts::empty();
        gathered |= stepped;
        gathered |= held;
        assert_eq!(gathered, both);

        // Nothing folded in changes nothing.
        assert_eq!(both | Contacts::empty(), both);
    }
}
