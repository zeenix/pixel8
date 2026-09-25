//! The handle a cart keeps: which seat in the cast, and which occupancy of it.

use super::{
    wire,
    world::{corner, mirrored, span_byte},
    Bounds, Contacts, Force, Velocity, World,
};
use crate::{motion::floor_i16, BitFlags, SpriteFlag, SpriteId};

/// One member of a [`World`](super::World)'s cast: a seat, and the right to ask about whoever is
/// in it.
///
/// What [`MemberBuilder::enlist`] hands back and the cart keeps beside its own game data.
/// Two bytes, [`Copy`], and nothing else: the position, the velocity, the rectangle, the contacts
/// and the look are all the world's, and every one of them is asked of the handle, handed the
/// world it was enlisted into to reach them — `hero.pos(&world)`, `hero.contacts(&world)`,
/// `hero.set_velocity(&mut world, v)`.
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
///     member: Member::builder(16.0, 80.0, 8, 8).enlist(world)?,
///     coins: 0,
/// };
/// # Some(hero) }
/// ```
///
/// A member is only ever as good as its seat. [`retire`](Self::retire) empties the seat and the
/// handle to it goes stale on the spot: asking the world anything with a stale one is a bug in the
/// cart, and it is answered with a panic naming the seat rather than with somebody else's
/// position. A cart that would rather ask than know asks [`seated`](Self::seated).
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
    /// [`enlist`](MemberBuilder::enlist) can ever hand it out. It is a `const`, which is the whole
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
    /// over the handle the cart asks about the seat with.
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

    /// Which seat of the cast this is, counting from zero.
    ///
    /// The stepping order is seat order — see [`enlist`](MemberBuilder::enlist) — so this is
    /// the one thing a cart can read off a handle: who moves before whom.
    pub const fn seat(self) -> usize {
        self.slot as usize
    }

    /// Empties the member's seat: it leaves the cast, and the handle to it goes stale on the spot.
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
    /// Retiring a member twice, or asking the world anything with its handle afterwards, is a bug
    /// in the cart and panics saying so.
    pub fn retire<const N: usize, F>(self, world: &mut World<N, F>)
    where
        F: Force,
    {
        let slot = world.seat(self);
        world.vacate(slot);
        // The seat is let again to somebody else, and the handle to whoever has just left it must
        // not answer for them.
        world.generations[slot] = world.generations[slot].wrapping_add(1);
    }

    /// Whether the member is still in the cast.
    ///
    /// The question to ask instead of finding out the hard way: everything else here panics on a
    /// handle whose member has [retired](Self::retire), because a stale handle is a cart holding on
    /// to somebody who left. A cart that would rather ask than know asks here.
    pub fn seated<const N: usize, F>(self, world: &World<N, F>) -> bool
    where
        F: Force,
    {
        world.holds(self)
    }

    /// Where the member is: its exact sub-pixel position.
    ///
    /// The truth for a cart's own arithmetic — which tile it is over, how far it is from something.
    /// What to *draw* at is [`draw_pos`](Self::draw_pos).
    pub fn pos<const N: usize, F>(self, world: &World<N, F>) -> (f32, f32)
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        (record.x, record.y)
    }

    /// The coherent pixel the member draws at.
    ///
    /// Where [`draw`](World::draw) puts the member — the top left of the block it is drawn from —
    /// and the pixel for anything a cart draws at it on its own: a rotor, a shadow, a name over its
    /// head. It is [`Body`](crate::Body)'s phase-coherent pixel — a sub-pixel diagonal climbs a
    /// clean staircase through it instead of shimmering — and the step keeps it coherent across the
    /// wire, so a running jump climbs that same staircase in the console.
    pub fn draw_pos<const N: usize, F>(self, world: &World<N, F>) -> (i16, i16)
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        (record.rx, record.ry)
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
    pub fn set_pos<const N: usize, F>(self, world: &mut World<N, F>, x: f32, y: f32)
    where
        F: Force,
    {
        let slot = world.seat(self);
        let offset = world.offsets[slot];
        let record = &mut world.records[slot];
        (record.x, record.y) = (x, y);
        (record.rx, record.ry) = (floor_i16(x), floor_i16(y));
        (record.bx, record.by) = corner((record.rx, record.ry), offset);
    }

    /// What the member is travelling at, in pixels per update.
    ///
    /// After a step, what survived it: an axis that ran into something has been spent, so a fall
    /// that landed reads zero and something that walked into a wall is not still walking.
    pub fn velocity<const N: usize, F>(self, world: &World<N, F>) -> Velocity
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        Velocity::new(record.dx, record.dy)
    }

    /// Sets what the member is travelling at: what the cart means it to do this update.
    ///
    /// Where the buttons, the patrol and the jump all end up. It is written before
    /// [`step`](World::step), which is what turns it into movement — and written afresh every
    /// update by anything that leans on a wall, since the step spends the speed that ran into one.
    pub fn set_velocity<const N: usize, F>(self, world: &mut World<N, F>, velocity: Velocity)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        (record.dx, record.dy) = (velocity.dx, velocity.dy);
    }

    /// What the member's last step ran into: the sides it was stopped at, and the flags of
    /// everything it met.
    ///
    /// The whole answer, walls and the edge of the world together, so a cart standing a member on
    /// the bottom of the level, on a floor tile and on a moving platform reads all three the same
    /// way. A [prop](MemberBuilder::prop) is never given contacts: the cart drives it, and there
    /// is nobody home to tell.
    pub fn contacts<const N: usize, F>(self, world: &World<N, F>) -> Contacts
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        Contacts::from_wire(record.sides, record.touched)
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
    /// # use pixel8::physics::{Member, World};
    /// # fn f(world: &World<4>, hero: Member, badie: Member) -> bool {
    /// // Level with the badie is a ram; anything else is the hero coming down on it.
    /// hero.bounds(world).y() == badie.bounds(world).y()
    /// # }
    /// ```
    pub fn bounds<const N: usize, F>(self, world: &World<N, F>) -> Bounds
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        Bounds::new(record.bx, record.by, record.bw, record.bh)
    }

    /// Sets how big the member's rectangle is.
    ///
    /// For a hitbox that follows the animation — a crouch, a blast that grows, a hurtbox switched
    /// off by giving it no size at all, which is a member nothing resolves and everything lets
    /// through. Where the rectangle sits on the body is [`set_offset`](Self::set_offset).
    pub fn resize<const N: usize, F>(self, world: &mut World<N, F>, width: u16, height: u16)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        (record.bw, record.bh) = (width, height);
    }

    /// Sets where the member's rectangle sits relative to the pixel it draws at — see
    /// [`MemberBuilder::offset`].
    pub fn set_offset<const N: usize, F>(self, world: &mut World<N, F>, dx: i16, dy: i16)
    where
        F: Force,
    {
        let slot = world.seat(self);
        world.offsets[slot] = (dx, dy);
        let record = &mut world.records[slot];
        (record.bx, record.by) = corner((record.rx, record.ry), (dx, dy));
    }

    /// The rectangle the member may not leave, if it named one — see
    /// [`MemberBuilder::confined_to`].
    pub fn confines<const N: usize, F>(self, world: &World<N, F>) -> Option<Bounds>
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];

        (record.meta & wire::CONFINED != 0)
            .then(|| Bounds::new(record.cx, record.cy, record.cw, record.ch))
    }

    /// Sets the rectangle the member may not leave, or takes its limits away — see
    /// [`MemberBuilder::confined_to`].
    ///
    /// The room the player has just walked into, an arena closing in, a level that grows. `None`
    /// is a member let go: free to walk off the map, which is what a bullet or a spent enemy wants.
    pub fn set_confines<const N: usize, F>(self, world: &mut World<N, F>, confines: Option<Bounds>)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        match confines {
            Some(limits) => {
                record.meta |= wire::CONFINED;
                (record.cx, record.cy) = (limits.x(), limits.y());
                (record.cw, record.ch) = (limits.width(), limits.height());
            }
            None => record.meta &= !wire::CONFINED,
        }
    }

    /// The cell the member wears, if any — see [`MemberBuilder::wearing`].
    ///
    /// One cell for both halves of a member's part in the scene: [`draw`](World::draw) draws the
    /// member from it and [`step`](World::step) steps it by the flags on it, so what is drawn and
    /// what is met can never be two different sprites.
    pub fn sprite<const N: usize, F>(self, world: &World<N, F>) -> Option<SpriteId>
    where
        F: Force,
    {
        let record = &world.records[world.seat(self)];
        match record.sprite {
            wire::UNWORN => None,
            id => Some(SpriteId(id as u8)),
        }
    }

    /// Sets the cell the member wears, or takes it off — see [`MemberBuilder::wearing`].
    ///
    /// How an animation is shown: the next frame of a walk cycle, written in the update that took
    /// the step. Cells carrying the same flags change how the member looks and nothing about what
    /// everybody meets, which is what a walk cycle wants; a badie that turns into a puff of smoke
    /// changes both.
    pub fn set_sprite<const N: usize, F>(self, world: &mut World<N, F>, sprite: Option<SpriteId>)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        record.sprite = match sprite {
            Some(sprite) => sprite.0 as u16,
            None => wire::UNWORN,
        };
    }

    /// Which way round the member is drawn: mirrored across, and mirrored up and down — see
    /// [`MemberBuilder::flipped`].
    pub fn flip<const N: usize, F>(self, world: &World<N, F>) -> (bool, bool)
    where
        F: Force,
    {
        let meta = world.records[world.seat(self)].meta;

        (meta & wire::FLIP_X != 0, meta & wire::FLIP_Y != 0)
    }

    /// Sets which way round the member is drawn — see [`MemberBuilder::flipped`].
    ///
    /// The walker turning round: written in the update that turned it, beside the velocity that
    /// sends it back the way it came.
    pub fn set_flip<const N: usize, F>(self, world: &mut World<N, F>, flip_x: bool, flip_y: bool)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        record.meta = mirrored(record.meta, flip_x, flip_y);
    }

    /// How many cells the member is drawn from, across and down — see [`MemberBuilder::spanning`].
    pub fn span<const N: usize, F>(self, world: &World<N, F>) -> (u8, u8)
    where
        F: Force,
    {
        let span = world.records[world.seat(self)].span;

        ((span & 0x0f) + 1, (span >> 4) + 1)
    }

    /// Sets how many cells the member is drawn from, across and down — see
    /// [`MemberBuilder::spanning`].
    ///
    /// The pose that needs more room than the rest: a sword swung out a cell in front, a stretch a
    /// cell taller. It is the look alone — the rectangle the member is met by is
    /// [`resize`](Self::resize)'s, if it changes at all — and a block no sheet holds panics here as
    /// it does in [`spanning`](MemberBuilder::spanning).
    pub fn set_span<const N: usize, F>(self, world: &mut World<N, F>, width: u8, height: u8)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        record.span = span_byte(width, height);
    }

    /// Whether the member is left off the screen — see [`MemberBuilder::hidden`].
    pub fn hidden<const N: usize, F>(self, world: &World<N, F>) -> bool
    where
        F: Force,
    {
        world.records[world.seat(self)].meta & wire::HIDDEN != 0
    }

    /// Hides the member, or shows it again — see [`MemberBuilder::hidden`].
    ///
    /// The blink: a hero flickering through the frames after a hit is hidden on every other one
    /// of them, and stepped, met and told on all of them alike.
    pub fn set_hidden<const N: usize, F>(self, world: &mut World<N, F>, hidden: bool)
    where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        if hidden {
            record.meta |= wire::HIDDEN;
        } else {
            record.meta &= !wire::HIDDEN;
        }
    }

    /// The member's own answer to what means *wall* to it, where it gave one — see
    /// [`MemberBuilder::stopped_by`].
    ///
    /// `None` is a member that goes by the scene's word, whatever
    /// [`with_solid`](World::with_solid) declared it to be.
    pub fn solid<const N: usize, F>(self, world: &World<N, F>) -> Option<BitFlags<SpriteFlag>>
    where
        F: Force,
    {
        let slot = world.seat(self);

        (world.own_solid & (1 << slot) != 0).then(|| {
            BitFlags::from_bits(world.records[slot].solid)
                .expect("a seat's solid was written from real flags")
        })
    }

    /// Sets what means *wall* to the member, or hands it back to the scene's word — see
    /// [`MemberBuilder::stopped_by`].
    ///
    /// `None` is the scene's word as it stands now ([`with_solid`](World::with_solid)), and the
    /// member follows it from here on.
    pub fn set_solid<const N: usize, F>(
        self,
        world: &mut World<N, F>,
        solid: Option<BitFlags<SpriteFlag>>,
    ) where
        F: Force,
    {
        let slot = world.seat(self);
        let word = match solid {
            Some(solid) => {
                world.own_solid |= 1 << slot;
                solid
            }
            None => {
                world.own_solid &= !(1 << slot);
                world.solid
            }
        };
        world.records[slot].solid = word.bits();
    }

    /// Which flags the member cares to be told about — see [`MemberBuilder::heeding`].
    pub fn heeds<const N: usize, F>(self, world: &World<N, F>) -> BitFlags<SpriteFlag>
    where
        F: Force,
    {
        BitFlags::from_bits(world.records[world.seat(self)].heeds)
            .expect("a seat's heeds was written from real flags")
    }

    /// Sets which flags the member cares to be told about — see [`MemberBuilder::heeding`].
    pub fn set_heeds<const N: usize, F>(
        self,
        world: &mut World<N, F>,
        heeds: impl Into<BitFlags<SpriteFlag>>,
    ) where
        F: Force,
    {
        let record = &mut world.records[world.seat(self)];
        record.heeds = heeds.into().bits();
    }

    /// What the member weighs — see [`MemberBuilder::weighing`].
    pub fn mass<const N: usize, F>(self, world: &World<N, F>) -> f32
    where
        F: Force,
    {
        world.masses[world.seat(self)]
    }

    /// Sets what the member weighs: a crate that fills with water, a ship that burns its fuel off.
    pub fn set_mass<const N: usize, F>(self, world: &mut World<N, F>, mass: f32)
    where
        F: Force,
    {
        let slot = world.seat(self);
        world.masses[slot] = mass;
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
/// empty seat and hands back the [`Member`] handle.
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
    /// every update with [`set_velocity`](Member::set_velocity), out of the buttons or a patrol or
    /// whatever else the cart is thinking.
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
    /// [`set_sprite`](Member::set_sprite); two walk-cycle cells carrying the same flag change
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
    /// turns with [`set_flip`](Member::set_flip), in the update that turned it.
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
    /// into changes the limits with [`set_confines`](Member::set_confines).
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
    /// whatever rails it likes, with [`set_pos`](Member::set_pos) before the world steps. A hazard
    /// patrolling a fixed beat, a lift on a track, a door: things the world must know about without
    /// being asked to drive them. It is [drawn](World::draw) like anybody else, wherever the cart
    /// last put it.
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
    /// frames after a hit, with [`set_hidden`](Member::set_hidden) on every other one of them.
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
    /// exactly as it was told to — and hands back the [`Member`] handle the cart asks about the
    /// seat with.
    ///
    /// The lowest empty seat, always: a cast seated once, in the order the scene works, keeps that
    /// order — and it is the order the step goes in, so a lift enlisted before its rider carries it
    /// the same update. A seat freed by [`retire`](Member::retire) is the next one filled.
    ///
    /// `None` is a full house: all `N` seats are taken, and the scene has to make room before it
    /// can take anybody else on. A cart that spawns as it goes — bullets, sparks — either sizes `N`
    /// for its worst frame or takes `None` as *not this frame*.
    #[must_use = "a seat with no handle kept to it can never be retired"]
    pub fn enlist<const N: usize, F>(self, world: &mut World<N, F>) -> Option<Member>
    where
        F: Force,
    {
        world.claim(self.record, self.solid, self.mass, self.offset)
    }
}
