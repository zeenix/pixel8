//! The handle a cart keeps: which seat in the cast, and which occupancy of it.

/// One member of a [`World`](super::World)'s cast: a seat, and the right to ask about whoever is
/// in it.
///
/// What [`enlist`](super::World::enlist) hands back and the cart keeps beside its own game data.
/// Two bytes, [`Copy`], and nothing else: the position, the velocity, the rectangle and the
/// contacts are all the world's, and every one of them is asked for with this — `world.pos(hero)`,
/// `world.contacts(hero)`, `world.set_velocity(hero, v)`.
///
/// ```no_run
/// # use pixel8::physics::{Member, World};
/// struct Hero {
///     /// Where the hero is, how fast, and what it last ran into: all of it the world's.
///     member: Member,
///     /// And what is the cart's own, which the world has never heard of.
///     coins: u16,
/// }
///
/// # fn f(world: &mut World<8>) -> Option<Hero> {
/// let hero = Hero {
///     member: world.enlist(16.0, 80.0, 8, 8)?.member(),
///     coins: 0,
/// };
/// # Some(hero) }
/// ```
///
/// A member is only ever as good as its seat. [`retire`](super::World::retire) empties the seat and
/// the handle to it goes stale on the spot: asking the world anything with a stale one is a bug in
/// the cart, and it is answered with a panic naming the seat rather than with somebody else's
/// position. A cart that would rather ask than know asks [`seated`](super::World::seated).
///
/// Handles are the world's own: one from a different [`World`] means the seat of that number in
/// *this* one, which is a member the cart never meant. Carts with two scenes going at once keep
/// their handles with the world they came from.
///
/// The occupancy is a byte, so a seat let for the two hundred and fifty-seventh time comes round to
/// a number it has used before, and a handle kept unasked-about across all of them would answer for
/// whoever holds the seat now. Which is a way of saying: retire a member and forget it, the way a
/// cart does anyway.
///
/// [`World`]: super::World
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[must_use = "a seat with no handle kept to it can never be retired"]
pub struct Member {
    /// Which of the world's `N` seats.
    pub(super) slot: u8,
    /// Which occupancy of it: bumped every time the seat is emptied, so a handle to a member that
    /// has left can be told from a handle to whoever was seated there next.
    pub(super) generation: u8,
}

impl Member {
    /// A handle to nobody: what a cart's state holds for somebody not yet enlisted.
    ///
    /// The seat it names is past the sixty-four the wire carries, so no world has it and no
    /// [`enlist`](super::World::enlist) can ever hand it out. It is a `const`, which is the whole
    /// point of it: a cart whose state is [placed rather than built](crate::game) writes its actors
    /// down as constants and gives them their seats in [`Game::boot`](crate::Game::boot), and this
    /// is what they hold until it does.
    ///
    /// Asking the world anything with it is the same bug as asking with a retired handle, and
    /// panics the same way — an actor that was never seated is not standing anywhere.
    ///
    /// ```no_run
    /// # use pixel8::physics::Member;
    /// struct Hero {
    ///     member: Member,
    ///     coins: u16,
    /// }
    ///
    /// impl Hero {
    ///     /// The hero as the cart ships: everything about it but a seat.
    ///     const fn waiting() -> Self {
    ///         Self { member: Member::NOBODY, coins: 0 }
    ///     }
    /// }
    /// ```
    pub const NOBODY: Self = Self {
        slot: u8::MAX,
        generation: 0,
    };

    /// Which seat of the cast this is, counting from zero.
    ///
    /// The stepping order is seat order — see [`World::enlist`](super::World::enlist) — so this is
    /// the one thing a cart can read off a handle: who moves before whom.
    pub const fn seat(&self) -> usize {
        self.slot as usize
    }
}
