//! The handle a cart keeps — which seat in the cast, and which occupancy of it — and the two
//! views the world lends out for it: one to read the seat, one to change it.

use core::fmt;

use super::{
    wire,
    world::{corner, mirrored, span_byte},
    Bounds, Contacts, Force, Velocity, World,
};
use crate::{motion::floor_i16, BitFlags, SpriteFlag, SpriteId};

/// One member of a [`World`](super::World)'s cast: a seat, and the right to ask the world to
/// borrow whoever is in it.
///
/// What [`MemberBuilder::enlist`] hands back and the cart keeps beside its own game data.
/// Two bytes, [`Copy`], and nothing else: the position, the velocity, the rectangle, the contacts
/// and the look are all the world's, and every one of them is asked of the seat this id names,
/// borrowed from the world it was enlisted into — [`world.member(hero)`](World::member) to read
/// it, [`world.member_mut(hero)`](World::member_mut) to change it.
///
/// ```no_run
/// # use pixel8::physics::{Member, MemberId, World};
/// struct Hero {
///     /// Where the hero is, how fast, and what it last ran into: all of it the world's.
///     member: MemberId,
///     /// And what is the cart's own, which the world has never heard of.
///     coins: u16,
/// }
///
/// # fn f(world: &mut World<8>) -> Option<Hero> {
/// let hero = Hero {
///     member: Member::builder(16.0, 80.0, 8, 8).enlist(world)?,
///     coins: 0,
/// };
/// # Some(hero) }
/// ```
///
/// A member is only ever as good as its seat. [`retire`](MemberMut::retire) empties the seat and
/// the id to it goes stale on the spot: asking the world to borrow anything with a stale one is a
/// bug in the cart, and it is answered with a panic naming the seat rather than with somebody
/// else's position. A cart that would rather ask than know asks
/// [`get_member`](World::get_member).
///
/// Ids are the world's own: one from a different [`World`] means the seat of that number in
/// *this* one, which is a member the cart never meant. Carts with two scenes going at once keep
/// their ids with the world they came from.
///
/// A seat's ids come round again: one let for the hundred and twenty-ninth time is handed the id
/// it was handed the first time, and an id kept unasked-about across all of them would answer for
/// whoever holds the seat now. Which is a way of saying: retire a member and forget it, the way a
/// cart does anyway.
///
/// [`World`]: super::World
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[must_use = "a seat with no id kept to it can never be retired"]
pub struct MemberId {
    /// Which of the world's `N` seats.
    pub(super) slot: u8,
    /// Which occupancy of it: the seat's count as it was taken. The count moves on as the seat
    /// is taken and again as it is emptied, so it is odd for exactly as long as somebody sits
    /// there: no id is ever an empty seat's count, and an id to a member that has left can be
    /// told from an id to whoever was seated there next.
    pub(super) generation: u8,
}

impl MemberId {
    /// An id for nobody: what a cart's state holds for somebody not yet enlisted.
    ///
    /// The seat it names is past the sixty-four the wire carries, so no world has it and no
    /// [`enlist`](MemberBuilder::enlist) can ever hand it out. It is a `const`, which is the whole
    /// point of it: a cart whose state is [placed rather than built](crate::game) writes its actors
    /// down as constants and gives them their seats in [`Game::boot`](crate::Game::boot), and this
    /// is what they hold until it does.
    ///
    /// Asking the world to borrow anything with it is the same bug as asking with a retired id,
    /// and panics the same way — an actor that was never seated is not standing anywhere.
    ///
    /// ```no_run
    /// # use pixel8::physics::MemberId;
    /// struct Hero {
    ///     member: MemberId,
    ///     coins: u16,
    /// }
    ///
    /// impl Hero {
    ///     /// The hero as the cart ships: everything about it but a seat.
    ///     const fn waiting() -> Self {
    ///         Self { member: MemberId::NOBODY, coins: 0 }
    ///     }
    /// }
    /// ```
    pub const NOBODY: Self = Self {
        slot: u8::MAX,
        generation: 0,
    };

    /// Which seat of the cast this is, counting from zero.
    ///
    /// The stepping order is seat order — see [`enlist`](MemberBuilder::enlist) — so this is
    /// the one thing a cart can read off an id: who moves before whom.
    pub const fn seat(self) -> usize {
        self.slot as usize
    }
}

/// A member borrowed from its world, to be read.
///
/// What [`World::member`] and [`World::get_member`] hand back: the seat's whole story, without a
/// call back to the world for each question asked of it. Borrowed shared, so a cart may hold as
/// many of these at once as it has questions — a stomp told from a ram by comparing two members'
/// [`bounds`](Self::bounds).
#[derive(Clone, Copy)]
pub struct Member<'a> {
    /// The id the member was borrowed with.
    pub(super) id: MemberId,
    /// The seat's record: everything the step and the draw read of the member.
    pub(super) record: &'a wire::Record,
    /// What the member weighs, which never crosses the wire and so is kept beside the record.
    pub(super) mass: f32,
    /// Whether what means *wall* to the member is a rule of its own rather than the scene's.
    pub(super) own_solid: bool,
}

// Every method a view has, bar `builder`, is `#[inline]`, like the borrows that hand the views out:
// carts are built for size, where a getter left out of line is a call, and one taking `&self`
// makes the cart write the whole view to memory first. Inlined, it is the loads it reads.
impl Member<'_> {
    /// Describes a member `width` x `height` pixels at (`x`, `y`), covering that rectangle from
    /// the pixel it will draw at, and hands back the [`MemberBuilder`] the rest of it is
    /// described through.
    ///
    /// The position is exact and sub-pixel, like everything else that moves here; the rectangle
    /// is whole pixels, and it is the one rectangle a member has — what the walls stop, what the
    /// rest of the cast meets, and what the edge of the world holds. A hurtbox narrower than the
    /// sprite says so with [`offset`](MemberBuilder::offset).
    ///
    /// That is a whole member already: standing still, wearing nothing, stopped by whatever the
    /// scene calls solid, told about everything it meets, free to walk off the map and of the
    /// weight nobody has to think about. [`MemberBuilder`]'s own builders say the rest, and
    /// [`enlist`](MemberBuilder::enlist) closes the description, seats it in a world and hands
    /// over the id the cart asks the world about the seat with.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Member, World};
    /// # fn f(world: &mut World<8>) {
    /// let Some(spark) = Member::builder(64.0, 64.0, 2, 2).moving(0.0, -1.5).enlist(world) else {
    ///     // Every seat is taken; this one waits for the next explosion.
    ///     return;
    /// };
    /// # }
    /// ```
    pub fn builder(x: f32, y: f32, width: u16, height: u16) -> MemberBuilder {
        let (rx, ry) = (floor_i16(x), floor_i16(y));

        MemberBuilder {
            record: wire::Record {
                x,
                y,
                rx,
                ry,
                bx: rx,
                by: ry,
                bw: width,
                bh: height,
                heeds: BitFlags::<SpriteFlag>::all().bits(),
                ..wire::EMPTY
            },
            solid: None,
            mass: 1.0,
            offset: (0, 0),
        }
    }

    /// The [`MemberId`] this member was borrowed with — what the cart keeps to ask the world for
    /// it again.
    #[inline]
    pub fn id(&self) -> MemberId {
        self.id
    }

    /// Where the member is: its exact sub-pixel position.
    ///
    /// The truth for a cart's own arithmetic — which tile it is over, how far it is from something.
    /// What to *draw* at is [`draw_pos`](Self::draw_pos).
    #[inline]
    pub fn pos(&self) -> (f32, f32) {
        (self.record.x, self.record.y)
    }

    /// The coherent pixel the member draws at.
    ///
    /// Where [`draw`](World::draw) puts the member — the top left of the block it is drawn from —
    /// and the pixel for anything a cart draws at it on its own: a rotor, a shadow, a name over its
    /// head. It is [`Body`](crate::Body)'s phase-coherent pixel — a sub-pixel diagonal climbs a
    /// clean staircase through it instead of shimmering — and the step keeps it coherent across the
    /// wire, so a running jump climbs that same staircase in the console.
    #[inline]
    pub fn draw_pos(&self) -> (i16, i16) {
        (self.record.rx, self.record.ry)
    }

    /// What the member is travelling at, in pixels per update.
    ///
    /// After a step, what survived it: an axis that ran into something has been spent, so a fall
    /// that landed reads zero and something that walked into a wall is not still walking.
    #[inline]
    pub fn velocity(&self) -> Velocity {
        Velocity::new(self.record.dx, self.record.dy)
    }

    /// What the member's last step ran into: the sides it was stopped at, and the flags of
    /// everything it met.
    ///
    /// The whole answer, walls and the edge of the world together, so a cart standing a member on
    /// the bottom of the level, on a floor tile and on a moving platform reads all three the same
    /// way. A [prop](MemberBuilder::prop) is never given contacts: the cart drives it, and there
    /// is nobody home to tell.
    #[inline]
    pub fn contacts(&self) -> Contacts {
        Contacts::from_wire(self.record.sides, self.record.touched)
    }

    /// The rectangle the member covers, where it now stands.
    ///
    /// The one rectangle a member has: what the walls stopped, what the rest of the cast met, and
    /// what the edge of the world held. It follows the body through every step, so this is always
    /// the rectangle that was collided with.
    ///
    /// The step says *the hero met a badie*; which badie, and what that costs, is the cart's, and
    /// this is what it settles it with — a stomp told from a ram by comparing two rectangles the
    /// world has just moved.
    ///
    /// ```no_run
    /// # use pixel8::physics::{MemberId, World};
    /// # fn f(world: &World<4>, hero: MemberId, badie: MemberId) -> bool {
    /// // Level with the badie is a ram; anything else is the hero coming down on it.
    /// world.member(hero).bounds().y() == world.member(badie).bounds().y()
    /// # }
    /// ```
    #[inline]
    pub fn bounds(&self) -> Bounds {
        Bounds::new(
            self.record.bx,
            self.record.by,
            self.record.bw,
            self.record.bh,
        )
    }

    /// The rectangle the member may not leave, if it named one — see
    /// [`MemberBuilder::confined_to`].
    #[inline]
    pub fn confines(&self) -> Option<Bounds> {
        (self.record.meta & wire::CONFINED != 0).then(|| {
            Bounds::new(
                self.record.cx,
                self.record.cy,
                self.record.cw,
                self.record.ch,
            )
        })
    }

    /// The cell the member wears, if any — see [`MemberBuilder::wearing`].
    ///
    /// One cell for both halves of a member's part in the scene: [`draw`](World::draw) draws the
    /// member from it and [`step`](World::step) steps it by the flags on it, so what is drawn and
    /// what is met can never be two different sprites.
    #[inline]
    pub fn sprite(&self) -> Option<SpriteId> {
        match self.record.sprite {
            wire::UNWORN => None,
            id => Some(SpriteId(id as u8)),
        }
    }

    /// The member's own answer to what means *wall* to it, where it gave one — see
    /// [`MemberBuilder::stopped_by`].
    ///
    /// `None` is a member that goes by the scene's word, whatever
    /// [`with_solid`](World::with_solid) declared it to be.
    #[inline]
    pub fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
        self.own_solid.then(|| {
            BitFlags::from_bits(self.record.solid)
                .expect("a seat's solid was written from real flags")
        })
    }

    /// Which flags the member cares to be told about — see [`MemberBuilder::heeding`].
    #[inline]
    pub fn heeds(&self) -> BitFlags<SpriteFlag> {
        BitFlags::from_bits(self.record.heeds).expect("a seat's heeds was written from real flags")
    }

    /// What the member weighs — see [`MemberBuilder::weighing`].
    #[inline]
    pub fn mass(&self) -> f32 {
        self.mass
    }

    /// Which way round the member is drawn: mirrored across, and mirrored up and down — see
    /// [`MemberBuilder::flipped`].
    #[inline]
    pub fn flip(&self) -> (bool, bool) {
        (
            self.record.meta & wire::FLIP_X != 0,
            self.record.meta & wire::FLIP_Y != 0,
        )
    }

    /// How many cells the member is drawn from, across and down — see [`MemberBuilder::spanning`].
    #[inline]
    pub fn span(&self) -> (u8, u8) {
        ((self.record.span & 0x0f) + 1, (self.record.span >> 4) + 1)
    }

    /// Whether the member is left off the screen — see [`MemberBuilder::hidden`].
    #[inline]
    pub fn hidden(&self) -> bool {
        self.record.meta & wire::HIDDEN != 0
    }
}

/// Everything the member's getters answer, in their order, and whether it is a
/// [prop](MemberBuilder::prop).
impl fmt::Debug for Member<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Member")
            .field("id", &self.id)
            .field("pos", &self.pos())
            .field("draw_pos", &self.draw_pos())
            .field("velocity", &self.velocity())
            .field("contacts", &self.contacts())
            .field("bounds", &self.bounds())
            .field("confines", &self.confines())
            .field("sprite", &self.sprite())
            .field("solid", &self.solid())
            .field("heeds", &self.heeds())
            .field("mass", &self.mass())
            .field("flip", &self.flip())
            .field("span", &self.span())
            .field("hidden", &self.hidden())
            .field("prop", &(self.record.meta & wire::PROP != 0))
            .finish()
    }
}

/// A member borrowed from its world, to be changed.
///
/// What [`World::member_mut`] and [`World::get_member_mut`] hand back: every setter a cart might
/// write into the seat, and [`retire`](Self::retire) to empty it. Borrowed exclusively, so it is
/// held for the few lines that change one member and let go before the world
/// [steps](World::step) or another member is borrowed.
pub struct MemberMut<'a> {
    /// The id the member was borrowed with, and so the seat whose bit is its own in the words.
    pub(super) id: MemberId,
    /// The seat's record: everything the step and the draw read of the member.
    pub(super) record: &'a mut wire::Record,
    /// What the member weighs.
    pub(super) mass: &'a mut f32,
    /// Where the member's rectangle sits relative to the pixel it draws at.
    pub(super) offset: &'a mut (i16, i16),
    /// The world's word of which seats are taken, a bit apiece.
    pub(super) seated: &'a mut u64,
    /// The world's word of which members answered what is solid with a rule of their own.
    pub(super) own_solid: &'a mut u64,
    /// The seat's count, which retiring moves on.
    pub(super) generation: &'a mut u8,
    /// The scene's word for *wall*, which a member handed back to it goes by.
    pub(super) scene_solid: BitFlags<SpriteFlag>,
}

// Inlined throughout, for the reason given at `impl Member`.
impl MemberMut<'_> {
    /// The [`MemberId`] this member was borrowed with — see [`Member::id`].
    #[inline]
    pub fn id(&self) -> MemberId {
        self.read().id()
    }

    /// Where the member is — see [`Member::pos`].
    #[inline]
    pub fn pos(&self) -> (f32, f32) {
        self.read().pos()
    }

    /// The coherent pixel the member draws at — see [`Member::draw_pos`].
    #[inline]
    pub fn draw_pos(&self) -> (i16, i16) {
        self.read().draw_pos()
    }

    /// What the member is travelling at — see [`Member::velocity`].
    #[inline]
    pub fn velocity(&self) -> Velocity {
        self.read().velocity()
    }

    /// What the member's last step ran into — see [`Member::contacts`].
    #[inline]
    pub fn contacts(&self) -> Contacts {
        self.read().contacts()
    }

    /// The rectangle the member covers — see [`Member::bounds`].
    #[inline]
    pub fn bounds(&self) -> Bounds {
        self.read().bounds()
    }

    /// The rectangle the member may not leave, if it named one — see [`Member::confines`].
    #[inline]
    pub fn confines(&self) -> Option<Bounds> {
        self.read().confines()
    }

    /// The cell the member wears, if any — see [`Member::sprite`].
    #[inline]
    pub fn sprite(&self) -> Option<SpriteId> {
        self.read().sprite()
    }

    /// The member's own answer to what means *wall* to it, where it gave one — see
    /// [`Member::solid`].
    #[inline]
    pub fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
        self.read().solid()
    }

    /// Which flags the member cares to be told about — see [`Member::heeds`].
    #[inline]
    pub fn heeds(&self) -> BitFlags<SpriteFlag> {
        self.read().heeds()
    }

    /// What the member weighs — see [`Member::mass`].
    #[inline]
    pub fn mass(&self) -> f32 {
        self.read().mass()
    }

    /// Which way round the member is drawn — see [`Member::flip`].
    #[inline]
    pub fn flip(&self) -> (bool, bool) {
        self.read().flip()
    }

    /// How many cells the member is drawn from, across and down — see [`Member::span`].
    #[inline]
    pub fn span(&self) -> (u8, u8) {
        self.read().span()
    }

    /// Whether the member is left off the screen — see [`Member::hidden`].
    #[inline]
    pub fn hidden(&self) -> bool {
        self.read().hidden()
    }

    /// Puts the member at (`x`, `y`) — a teleport, not a movement.
    ///
    /// The drawn pixel is re-snapped to the floor of the new position rather than eased towards it,
    /// because this is a jump: a respawn, a room the player has walked into, the rails a
    /// [prop](MemberBuilder::prop) is driven along. The rectangle goes with it, keeping whatever
    /// [offset](MemberBuilder::offset) it was given.
    ///
    /// Ordinary movement is not this. A member is moved by having a velocity
    /// ([`set_velocity`](Self::set_velocity)) and being stepped: that is what is stopped by walls,
    /// held inside limits and reported in contacts, and none of it happens here.
    #[inline]
    pub fn set_pos(&mut self, x: f32, y: f32) {
        (self.record.x, self.record.y) = (x, y);
        (self.record.rx, self.record.ry) = (floor_i16(x), floor_i16(y));
        (self.record.bx, self.record.by) = corner((self.record.rx, self.record.ry), *self.offset);
    }

    /// Sets what the member is travelling at: what the cart means it to do this update.
    ///
    /// Where the buttons, the patrol and the jump all end up. It is written before
    /// [`step`](World::step), which is what turns it into movement — and written afresh every
    /// update by anything that leans on a wall, since the step spends the speed that ran into one.
    #[inline]
    pub fn set_velocity(&mut self, velocity: Velocity) {
        (self.record.dx, self.record.dy) = (velocity.dx, velocity.dy);
    }

    /// Sets how big the member's rectangle is.
    ///
    /// For a hitbox that follows the animation — a crouch, a blast that grows, a hurtbox switched
    /// off by giving it no size at all, which is a member nothing resolves and everything lets
    /// through. Where the rectangle sits on the body is [`set_offset`](Self::set_offset).
    #[inline]
    pub fn resize(&mut self, width: u16, height: u16) {
        (self.record.bw, self.record.bh) = (width, height);
    }

    /// Sets where the member's rectangle sits relative to the pixel it draws at — see
    /// [`MemberBuilder::offset`].
    #[inline]
    pub fn set_offset(&mut self, dx: i16, dy: i16) {
        *self.offset = (dx, dy);
        (self.record.bx, self.record.by) = corner((self.record.rx, self.record.ry), (dx, dy));
    }

    /// Sets the rectangle the member may not leave, or takes its limits away — see
    /// [`MemberBuilder::confined_to`].
    ///
    /// The room the player has just walked into, an arena closing in, a level that grows. `None`
    /// is a member let go: free to walk off the map, which is what a bullet or a spent enemy wants.
    #[inline]
    pub fn set_confines(&mut self, confines: Option<Bounds>) {
        match confines {
            Some(limits) => {
                self.record.meta |= wire::CONFINED;
                (self.record.cx, self.record.cy) = (limits.x(), limits.y());
                (self.record.cw, self.record.ch) = (limits.width(), limits.height());
            }
            None => self.record.meta &= !wire::CONFINED,
        }
    }

    /// Sets the cell the member wears, or takes it off — see [`MemberBuilder::wearing`].
    ///
    /// How an animation is shown: the next frame of a walk cycle, written in the update that took
    /// the step. Cells carrying the same flags change how the member looks and nothing about what
    /// everybody meets, which is what a walk cycle wants; a badie that turns into a puff of smoke
    /// changes both.
    #[inline]
    pub fn set_sprite(&mut self, sprite: Option<SpriteId>) {
        self.record.sprite = match sprite {
            Some(sprite) => sprite.0 as u16,
            None => wire::UNWORN,
        };
    }

    /// Sets what means *wall* to the member, or hands it back to the scene's word — see
    /// [`MemberBuilder::stopped_by`].
    ///
    /// `None` is the scene's word as it stands now ([`with_solid`](World::with_solid)), and the
    /// member follows it from here on.
    #[inline]
    pub fn set_solid(&mut self, solid: Option<BitFlags<SpriteFlag>>) {
        let slot = self.id.seat();
        let word = match solid {
            Some(solid) => {
                *self.own_solid |= 1 << slot;
                solid
            }
            None => {
                *self.own_solid &= !(1 << slot);
                self.scene_solid
            }
        };
        self.record.solid = word.bits();
    }

    /// Sets which flags the member cares to be told about — see [`MemberBuilder::heeding`].
    #[inline]
    pub fn set_heeds(&mut self, heeds: impl Into<BitFlags<SpriteFlag>>) {
        self.record.heeds = heeds.into().bits();
    }

    /// Sets what the member weighs: a crate that fills with water, a ship that burns its fuel off.
    #[inline]
    pub fn set_mass(&mut self, mass: f32) {
        *self.mass = mass;
    }

    /// Sets which way round the member is drawn — see [`MemberBuilder::flipped`].
    ///
    /// The walker turning round: written in the update that turned it, beside the velocity that
    /// sends it back the way it came.
    #[inline]
    pub fn set_flip(&mut self, flip_x: bool, flip_y: bool) {
        self.record.meta = mirrored(self.record.meta, flip_x, flip_y);
    }

    /// Sets how many cells the member is drawn from, across and down — see
    /// [`MemberBuilder::spanning`].
    ///
    /// The pose that needs more room than the rest: a sword swung out a cell in front, a stretch a
    /// cell taller. It is the look alone — the rectangle the member is met by is
    /// [`resize`](Self::resize)'s, if it changes at all — and a block no sheet holds panics here as
    /// it does in [`spanning`](MemberBuilder::spanning).
    #[inline]
    pub fn set_span(&mut self, width: u8, height: u8) {
        self.record.span = span_byte(width, height);
    }

    /// Hides the member, or shows it again — see [`MemberBuilder::hidden`].
    ///
    /// The blink: a hero flickering through the frames after a hit is hidden on every other one
    /// of them, and stepped, met and told on all of them alike.
    #[inline]
    pub fn set_hidden(&mut self, hidden: bool) {
        if hidden {
            self.record.meta |= wire::HIDDEN;
        } else {
            self.record.meta &= !wire::HIDDEN;
        }
    }

    /// Empties the member's seat: it leaves the cast, and the id to it goes stale on the spot.
    ///
    /// What a cart does with a bullet that has left the screen, a badie that has been stomped, an
    /// explosion that has burned out. The seat is the next one [`enlist`](MemberBuilder::enlist)
    /// fills, and until it is filled it is nothing to anybody — the step carries it as a prop
    /// covering no pixels, which nothing meets and no force reaches.
    ///
    /// The member is gone the moment this returns, so the meeting it died of has already been
    /// reported to whoever it met: the whole cast is stepped where it stands, and nothing is
    /// waiting on a picture of it.
    ///
    /// Retiring a member twice, or asking the world to borrow it afterwards, is a bug in the cart
    /// and panics saying so.
    #[inline]
    pub fn retire(self) {
        let slot = self.id.seat();
        *self.seated &= !(1 << slot);
        *self.own_solid &= !(1 << slot);
        *self.record = wire::VACANT;
        *self.mass = 1.0;
        *self.offset = (0, 0);
        // Even again, like every empty seat's count, so no id handed out answers to it.
        *self.generation = self.generation.wrapping_add(1);
    }

    /// This member, read rather than changed — what every getter above answers through.
    #[inline]
    fn read(&self) -> Member<'_> {
        Member {
            id: self.id,
            record: self.record,
            mass: *self.mass,
            own_solid: *self.own_solid & (1 << self.id.seat()) != 0,
        }
    }
}

/// The member as [`Member`] prints it, wrapped in `MemberMut(..)`.
impl fmt::Debug for MemberMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MemberMut").field(&self.read()).finish()
    }
}

/// A member described and not yet seated: everything it will be, kept here until
/// [`enlist`](Self::enlist) takes a seat and writes the whole of it in at once.
///
/// [`Member::builder`] starts one at (`x`, `y`), covering `width` x `height` pixels from the
/// pixel it will draw at: standing still, wearing nothing, stopped by whatever the scene calls
/// solid, told about everything it meets, free to walk off the map and of the weight nobody has
/// to think about — a whole member already, in everything but a seat. Every builder here says one
/// more thing about it, and [`enlist`](Self::enlist) closes the description, claims the lowest
/// empty seat and hands back the [`MemberId`].
///
/// `Copy`, so one description seats as many members as a cart enlists from it: a gun that fires
/// the same shot over and over keeps one description and enlists a fresh bullet from it every
/// time.
///
/// ```no_run
/// # use pixel8::{physics::{Bounds, Member, World}, SpriteId};
/// # const BADIE_SPRITE: SpriteId = SpriteId(6);
/// # const LEVEL: Bounds = Bounds::new(0, 0, 256, 128);
/// # fn f(world: &mut World<4>) {
/// // The badie: one sprite's worth of it, wearing the cell its flag is written on, patrolling
/// // inside the level and never let out of it.
/// let badie = Member::builder(200.0, 104.0, 8, 8)
///     .wearing(BADIE_SPRITE)
///     .confined_to(LEVEL)
///     .enlist(world)
///     .expect("a seat for the badie");
/// # }
/// ```
#[derive(Clone, Copy)]
#[must_use = "a member described and never enlisted is nobody: `enlist` seats it"]
pub struct MemberBuilder {
    /// Everything a step or a draw reads out of a seat, built up field by field and written into
    /// the seat whole, in one move, when [`enlist`](Self::enlist) claims one.
    record: wire::Record,
    /// The member's own rule for what means *wall* to it, if [`stopped_by`](Self::stopped_by)
    /// gave one — `None` until then, which is what asks the scene's word at the moment it is
    /// enlisted.
    solid: Option<BitFlags<SpriteFlag>>,
    /// What the member will weigh — see [`weighing`](Self::weighing).
    mass: f32,
    /// Where the member's rectangle will sit relative to the pixel it draws at — see
    /// [`offset`](Self::offset).
    offset: (i16, i16),
}

impl MemberBuilder {
    /// The same member, already travelling.
    ///
    /// Standing still is the default, and what most members want: a velocity is written afresh
    /// every update with [`set_velocity`](MemberMut::set_velocity), out of the buttons or a patrol
    /// or whatever else the cart is thinking.
    pub fn moving(mut self, dx: f32, dy: f32) -> Self {
        (self.record.dx, self.record.dy) = (dx, dy);

        self
    }

    /// The same member, wearing `sprite`: the cell whose flags everybody else meets in it, and the
    /// cell the world draws it from.
    ///
    /// The other side of [`stopped_by`](Self::stopped_by). That says which flags stop *me*; this
    /// says which flags I carry, and they are the flags the cart wrote on that cell in the sprite
    /// editor — the same one vocabulary the map's tiles already speak. So a badie is a badie
    /// because its cell is flagged `BADIE`, and everything that meets it is told `BADIE` in
    /// [`Contacts::touched`](super::Contacts::touched).
    ///
    /// It is the member's look as well: [`World::draw`] draws it from this very cell, mirrored or
    /// spanning more cells as the builders below say, so what is drawn and what is met are one
    /// sprite.
    ///
    /// Wearing nothing — the default — is a member nobody is stopped by, nobody is told about, and
    /// nobody sees. It is still stopped by everything, and still told everything: a sensor needs no
    /// flag of its own. A member whose look changes with its state changes what it wears with
    /// [`set_sprite`](MemberMut::set_sprite); two walk-cycle cells carrying the same flag change
    /// nothing anybody meets, which is the usual case.
    pub fn wearing(mut self, sprite: SpriteId) -> Self {
        self.record.sprite = sprite.0 as u16;

        self
    }

    /// The same member, drawn mirrored: across for `flip_x`, and up and down for `flip_y`.
    ///
    /// Which way a member faces is part of how it looks, and the world [draws](World::draw) it
    /// that way. Nothing about the step reads it: the rectangle stays where it is, and the flags
    /// everybody else meets are the worn cell's either way round. A member that turns as it walks
    /// turns with [`set_flip`](MemberMut::set_flip), in the update that turned it.
    pub fn flipped(mut self, flip_x: bool, flip_y: bool) -> Self {
        self.record.meta = mirrored(self.record.meta, flip_x, flip_y);

        self
    }

    /// The same member, drawn from a block of `width` x `height` cells of the sheet, with the cell
    /// it wears at its top left.
    ///
    /// The way [`Graphics::sprite_ext`](crate::Graphics::sprite_ext) draws a block: a 16x16 hero is
    /// `spanning(2, 2)`, wearing the cell at the top left of its picture. One cell is the default,
    /// and what most of a cast is drawn from.
    ///
    /// It is the look and only the look. The rectangle the member is stepped by is still the one
    /// [`Member::builder`] gave it, and what everybody meets in it is still the flags of the one
    /// cell it wears.
    ///
    /// One to sixteen cells each way, the sheet being sixteen cells across; anything else is a bug
    /// in the cart, and panics saying so.
    pub fn spanning(mut self, width: u8, height: u8) -> Self {
        self.record.span = span_byte(width, height);

        self
    }

    /// The same member, with rules of its own about what means *wall* to it.
    ///
    /// The scene's word — [`World::with_solid`] — is what a member is stopped by unless it says
    /// otherwise here, and most of a cast never says otherwise, because what is a wall is usually a
    /// fact about the scene rather than about anybody in it. What is said here *replaces* the
    /// scene's word for this member alone, and the emptiest rule of all — `BitFlags::empty()` — is
    /// a member nothing anywhere stops, whatever the scene declares: a bullet, a bird, anything a
    /// cart wants told about the world rather than stopped by it.
    ///
    /// A member's *own* kind belongs here as readily as anything else, and is the usual reason to
    /// have rules of one's own at all: the world knows who is who and never asks a member about
    /// itself, so two crates wearing `CRATE`, each with `CRATE` solid to it, block each other and
    /// neither is ever its own wall.
    ///
    /// ```no_run
    /// # use pixel8::{physics::Member, SpriteFlag, SpriteId};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// # const CRATE: SpriteFlag = SpriteFlag::Flag1;
    /// # const CRATE_SPRITE: SpriteId = SpriteId(9);
    /// # fn f(world: &mut pixel8::physics::World<4>) {
    /// // The walls stop a crate like they stop everybody — and so does another crate.
    /// let crated = Member::builder(0.0, 0.0, 8, 8)
    ///     .wearing(CRATE_SPRITE)
    ///     .stopped_by(SOLID | CRATE)
    ///     .enlist(world)
    ///     .expect("a seat for the crate");
    /// # }
    /// ```
    pub fn stopped_by(mut self, solid: impl Into<BitFlags<SpriteFlag>>) -> Self {
        self.solid = Some(solid.into());

        self
    }

    /// The same member, told about `heeds` and nothing else.
    ///
    /// [`stopped_by`](Self::stopped_by) says what stops the member; this says what it wants to hear
    /// about, and everything else the world meets on its behalf it throws away without ever working
    /// out whether it was met. Everything is the default, and it is the honest one — a member that
    /// has not said otherwise is told about every flag it meets.
    ///
    /// Narrowing it is a promise the cart makes and the world takes at its word: a neighbour
    /// carrying nothing this member heeds is skipped before a single edge of it is worked out, and
    /// a tile's flags are dropped before they are collected. In a scene where everything is in one
    /// cast that is most of the work of an update, and it is spent on answers nobody was going to
    /// read.
    ///
    /// It cannot cost a member a wall: solid is heeded whatever this says, so a wall it never asked
    /// to hear about still stops it, and being stopped by it still reports it.
    pub fn heeding(mut self, heeds: impl Into<BitFlags<SpriteFlag>>) -> Self {
        self.record.heeds = heeds.into().bits();

        self
    }

    /// The same member, never let out of `confines`.
    ///
    /// The edge of the world, which is not a wall and is nowhere on the map: nothing else stops a
    /// member walking off the last tile and falling for ever.
    /// [`Bounds::screen`](super::Bounds::screen) is what most carts that want one mean; a level
    /// bigger than the screen hands over the level. The sides it is held at arrive in the same
    /// [`Contacts`](super::Contacts) as the walls, so a hold at the bottom of the level reads
    /// [`below`](super::Contacts::below) as a floor tile does.
    ///
    /// Saying nothing — the default — is a member free to leave, which is what a bullet or a spent
    /// enemy wants: it walks off the map, and the cart retires it when
    /// [`Bounds::on_screen`](super::Bounds::on_screen) says it has gone. A room the player walks
    /// into changes the limits with [`set_confines`](MemberMut::set_confines).
    pub fn confined_to(mut self, confines: Bounds) -> Self {
        self.record.meta |= wire::CONFINED;
        (self.record.cx, self.record.cy) = (confines.x(), confines.y());
        (self.record.cw, self.record.ch) = (confines.width(), confines.height());

        self
    }

    /// The same member, with its rectangle sitting `dx`, `dy` pixels from the pixel it draws at.
    ///
    /// For a hurtbox narrower than the sprite: the member is drawn from one corner and judged from
    /// another. The rectangle keeps that seat on the body wherever the step carries it, so what
    /// stops the member stops the rectangle, exactly where a cart drew it.
    ///
    /// `(0, 0)` — the default — is the rectangle over the sprite, which is what most of a cast
    /// wants.
    pub fn offset(mut self, dx: i16, dy: i16) -> Self {
        self.offset = (dx, dy);
        (self.record.bx, self.record.by) = corner((self.record.rx, self.record.ry), (dx, dy));

        self
    }

    /// The same member as a prop: in the cast to be met, never to be moved.
    ///
    /// A prop stands in everybody's way exactly as any member does — the rectangle it covers and
    /// the flags on the cell it wears — and is otherwise left alone: no force reaches it, nothing
    /// resolves it, and its contacts are never written. The cart drives it wherever it likes, on
    /// whatever rails it likes, with [`set_pos`](MemberMut::set_pos) before the world steps. A
    /// hazard patrolling a fixed beat, a lift on a track, a door: things the world must know about
    /// without being asked to drive them. It is [drawn](World::draw) like anybody else, wherever
    /// the cart last put it.
    pub fn prop(mut self) -> Self {
        self.record.meta |= wire::PROP;

        self
    }

    /// The same member, hidden: in the cast to be met, never to be drawn.
    ///
    /// The other half of what a [prop](Self::prop) is: a prop is met and never moved, and a hidden
    /// member is met and never drawn. It is in the cast like anybody else — stepped, stopping
    /// whoever its cell is a wall to, and told what it meets — and [`World::draw`] leaves it off
    /// the screen. An invisible wall, a trigger wearing a flagged cell, a hero blinking through the
    /// frames after a hit, with [`set_hidden`](MemberMut::set_hidden) on every other one of them.
    pub fn hidden(mut self) -> Self {
        self.record.meta |= wire::HIDDEN;

        self
    }

    /// The same member, weighing `mass`.
    ///
    /// How hard it is to push, relative to everything else in the scene: `1.0` is the default
    /// nobody has to think about, `4.0` takes four times the shove for the same movement and `0.25`
    /// a quarter of it. What to make of it is each [`Force`]'s business — [`Wind`](super::Wind) and
    /// [`Atmosphere`](super::Atmosphere) divide their grip by it, and [`Gravity`](super::Gravity)
    /// never reads it at all. See the [module docs](super#mass).
    pub fn weighing(mut self, mass: f32) -> Self {
        self.mass = mass;

        self
    }

    /// Closes the description, claims the lowest empty seat of `world`'s cast for it — standing
    /// where it was described to, covering the rectangle it was given, and looking and answering
    /// exactly as it was told to — and hands back the [`MemberId`] the cart asks the world about
    /// the seat with.
    ///
    /// The lowest empty seat, always: a cast seated once, in the order the scene works, keeps that
    /// order — and it is the order the step goes in, so a lift enlisted before its rider carries it
    /// the same update. A seat freed by [`retire`](MemberMut::retire) is the next one filled.
    ///
    /// `None` is a full house: all `N` seats are taken, and the scene has to make room before it
    /// can take anybody else on. A cart that spawns as it goes — bullets, sparks — either sizes `N`
    /// for its worst frame or takes `None` as *not this frame*.
    #[must_use = "a seat with no id kept to it can never be retired"]
    pub fn enlist<const N: usize, F>(self, world: &mut World<N, F>) -> Option<MemberId>
    where
        F: Force,
    {
        world.claim(self.record, self.solid, self.mass, self.offset)
    }
}
