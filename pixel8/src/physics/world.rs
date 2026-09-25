//! The scene's one mover, and the cast it keeps: everything seated, stepped where it stands.

use super::{
    collider::{far, Cast, Collider, Neighbour},
    wire, Bounds, Contact, Contacts, Force, Kinetic, Member, Subject, Velocity,
};
use crate::{BitFlags, Context, Graphics, SpriteFlag, SpriteId};

/// The thing that moves everything, holds it and draws it: `N` seats of cast, the scene's weather,
/// one call an update and one a frame.
///
/// A cart makes one, [enlists](super::MemberBuilder::enlist) everybody its scene is played by, and
/// calls [`step`](Self::step). What that does, for each member in turn, is what an update of a
/// moving thing has always had to do — run the forces over its velocity, stop whatever ran into the
/// map's tiles or into the rest of the cast, keep it inside the rectangle it may not leave, move it
/// by what survived, and write down what it met — except that nothing else in the cart does any of
/// it, and nothing else in the cart holds any of it. And once a frame, [`draw`](Self::draw) puts
/// the whole cast on screen where the step left it.
///
/// ```no_run
/// use pixel8::{
///     physics::{Bounds, Gravity, Member, World},
///     *,
/// };
///
/// # const SPIKES: SpriteFlag = SpriteFlag::Flag2;
/// # const BADIE_SPRITE: SpriteId = SpriteId(6);
/// # const HERO_SPRITE: SpriteId = SpriteId(1);
/// struct Level {
///     /// The scene: three seats under the level's pull, and every one of them a member the
///     /// handles below are the cart's grip on.
///     world: World<3, Gravity>,
///     hero: Member,
///     badies: [Member; 2],
///     /// And what is the cart's own, which the world has never heard of.
///     score: u16,
/// }
///
/// impl Level {
///     fn new() -> Self {
///         let mut world = World::new().with_solid(SpriteFlag::Flag0).with_forces(Gravity::new());
///         // Seat order is stepping order, so the badies go on before the hero that meets them.
///         let badies = [40.0, 90.0].map(|x| {
///             Member::builder(x, 96.0, 8, 8)
///                 .wearing(BADIE_SPRITE)
///                 .enlist(&mut world)
///                 .expect("a seat apiece for the badies")
///         });
///         let hero = Member::builder(16.0, 80.0, 8, 8)
///             .wearing(HERO_SPRITE)
///             .confined_to(Bounds::screen())
///             .enlist(&mut world)
///             .expect("a seat for the hero");
///
///         Self { world, hero, badies, score: 0 }
///     }
/// }
///
/// impl Game for Level {
///     fn update(&mut self, ctx: &mut Context) {
///         // Whatever each of them means to do this update — read the buttons, turn a patrol
///         // round — is written into its velocity first. Then the world moves the lot.
///         let mut velocity = self.hero.velocity(&self.world);
///         velocity.dx = if ctx.is_button_down(Button::Right) { 0.7 } else { 0.0 };
///         self.hero.set_velocity(&mut self.world, velocity);
///
///         self.world.step(ctx);
///
///         // And the answers are waiting in the seats.
///         let grounded = self.hero.contacts(&self.world).below();
///         let hurt = self.hero.contacts(&self.world).touches(SPIKES);
///     }
///
///     fn draw(&self, gfx: &mut Graphics) {
///         gfx.clear(Color::BLACK);
///         // The whole cast, where the step left it: the badies, and the hero seated after them
///         // drawn over them.
///         self.world.draw(gfx, BitFlags::empty());
///     }
/// }
/// ```
///
/// # What the world owns
///
/// Everything about a member that moves, is collided with or is drawn: the exact sub-pixel
/// position and the coherent pixel it draws at, the velocity, the rectangle it covers, the cell it
/// wears and the rest of its look — which way round, how many cells, whether it is shown at all —
/// what stops it, what it cares to hear about, how far it may go, what it weighs, and what its
/// last step ran into. A cart keeps a [`Member`] — two bytes — and its own game data beside it,
/// and asks the member, handed the world, for the rest.
///
/// It is not a saving in bytes and does not pretend to be one. A seat is the forty-four bytes of
/// its record plus nine the world keeps alongside, `N` of them for as long as the world lives,
/// where those same fields used to sit in the cart's own structs — some twenty-two bytes an entity
/// — and the wire's records were borrowed from the stack for the length of one call. What it buys
/// is that there is only ever *one* of everything: nothing is copied into a buffer before the
/// crossing and nothing is read back out of one after it, because the buffer is the state. A cart
/// no longer gathers a cast of borrows an update, and the old friction of a fixed-capacity vector
/// whose `Drop` kept those borrows alive to the end of the block — the reason a cart used to hand
/// its cast to a function of its own — is gone with them.
///
/// # The seats, and the order they are in
///
/// `N` is the whole cast a scene can have at once, and a cart names it: it is the size of the one
/// array the world keeps, and the sixty-four the wire carries is the ceiling — a bigger `N` is
/// refused at compile time, so a world that builds is a world that steps.
///
/// Members are stepped one at a time, in seat order, and each of them is resolved against the
/// others *where they now stand*. That is the first thing worth ordering a cast for: enlist a lift
/// before its rider and the rider is carried up the moment the lift moves; enlist it after, and
/// the rider spends the update on last update's platform. The other is what is drawn over what:
/// [`draw`](Self::draw) goes in seat order too, so a later seat lands on top of an earlier one.
/// [`enlist`](super::MemberBuilder::enlist) fills the lowest empty seat, so a cast seated once in
/// the order the scene works stays in it.
///
/// An empty seat is nothing to anybody: it goes over the wire as a prop covering no pixels, which
/// no force reaches, nothing is stopped by and nobody is told about. Retiring a member costs the
/// rest of the cast exactly nothing but the seat it leaves behind.
pub struct World<const N: usize, F: Force = ()> {
    /// Whether the map is part of the scene or only the picture behind it. See
    /// [`mapless`](Self::mapless).
    reads_map: bool,
    /// What the scene calls a wall, for every member that has no rule of its own. See
    /// [`with_solid`](Self::with_solid).
    pub(super) solid: BitFlags<SpriteFlag>,
    /// The scene's weather, the world's own. See [`with_forces`](Self::with_forces).
    forces: F,
    /// The cast itself, stored once and in the very layout the step crosses the ABI in. A seat
    /// nobody is in holds [`wire::VACANT`].
    pub(super) records: [wire::Record; N],
    /// What each member weighs — the forces' business, and nobody's on the other side of the wire,
    /// so it never crosses it.
    pub(super) masses: [f32; N],
    /// Where each member's rectangle sits relative to the pixel it draws at. The step moves the
    /// body and, honouring the wire's in/out split, leaves the rectangle's corner alone, so this
    /// is what puts it back on the body afterwards.
    pub(super) offsets: [(i16, i16); N],
    /// How many times each seat has been emptied, so a handle to somebody who has left can be told
    /// from a handle to whoever was seated there next.
    pub(super) generations: [u8; N],
    /// Which seats are taken, a bit apiece. A `u64` covers every `N` there can be, the wire's
    /// ceiling being sixty-four.
    seated: u64,
    /// And which of those members answered what is solid with a rule of their own, so that the
    /// scene changing its word ([`with_solid`](Self::with_solid)) reaches the members who go by it
    /// and leaves the others theirs.
    pub(super) own_solid: u64,
}

impl<const N: usize> World<N> {
    /// A world of `N` empty seats. It calls nothing solid until
    /// [`with_solid`](Self::with_solid) says otherwise, and its weather is the still air until
    /// [`with_forces`](Self::with_forces) gives it one.
    ///
    /// `const`, so a cart can spell its world out in its `game!` initializer, however it is
    /// configured:
    ///
    /// ```
    /// # use pixel8::physics::World;
    /// const SCENE: World<8> = World::new();
    /// ```
    ///
    /// `N` cannot exceed the sixty-four members the wire carries, and the check is a compile-time
    /// one — there is nothing about it left to go wrong while a cart runs:
    ///
    /// ```compile_fail
    /// # use pixel8::physics::World;
    /// const CROWD: World<65> = World::new();
    /// ```
    pub const fn new() -> Self {
        const {
            assert!(
                N <= wire::CAP,
                "a World cannot have more seats than the sixty-four records the wire carries"
            )
        };

        Self {
            reads_map: true,
            solid: BitFlags::empty(),
            forces: (),
            records: [wire::VACANT; N],
            masses: [1.0; N],
            offsets: [(0, 0); N],
            generations: [0; N],
            seated: 0,
            own_solid: 0,
        }
    }

    /// A world whose map is scenery: the tiles are drawn and nothing else.
    ///
    /// The map is the one thing in a step that everything is resolved against whether it asked to
    /// be or not — every member sweeps the tiles under it every update, so that a cart which
    /// flagged its walls gets them for nothing. A scene that flagged no tile at all pays for that
    /// anyway: a lookup per tile per axis per member, collecting an answer that is always empty.
    /// This is how a cart says not to bother. Shoot-'em-ups whose level scrolls past behind the
    /// fight are the case it is for; so is anything whose collisions are all between moving things.
    ///
    /// Nothing else changes. The cast still meets itself, its
    /// [confines](super::MemberBuilder::confined_to) still hold, and *solid* — the scene's
    /// ([`with_solid`](Self::with_solid)) and anybody's [own](super::MemberBuilder::stopped_by) —
    /// still means what it meant; there is simply nothing on the map for it to mean it against.
    ///
    /// `const`, like [`new`](Self::new), so a level's world is spelled out where it is made:
    ///
    /// ```
    /// # use pixel8::physics::World;
    /// let sky: World<16> = World::mapless();
    /// ```
    pub const fn mapless() -> Self {
        Self {
            reads_map: false,
            ..Self::new()
        }
    }

    /// The same world, owning `forces` as the scene's weather.
    ///
    /// One force, a tuple of them applied left to right — a tuple of [`Force`]s is itself a
    /// [`Force`] — or nothing at all, which is what a world starts with. The step runs them over
    /// every member it moves, before anything moves; the ones with state of their own — a gusting
    /// [`Wind`](super::Wind) — stay reachable through [`forces_mut`](Self::forces_mut), to be
    /// updated where they live.
    ///
    /// The cast comes along: a world already seated may be given its weather afterwards, and
    /// everybody in it keeps their seat.
    ///
    /// `const`, like the two constructors above and for the same reason — a cart's whole opening
    /// world, weather and all, is a constant its [`game!`](crate::game) initializer can be:
    ///
    /// ```
    /// # use pixel8::physics::{Gravity, World};
    /// const LEVEL: World<8, Gravity> = World::new().with_forces(Gravity::new());
    /// ```
    ///
    /// The weather is given to a world that has none, which is every world as it is made. One that
    /// already has weather keeps it and drives it through [`forces_mut`](Self::forces_mut) — the
    /// forces a scene is played under are a fact about the scene, and a `World<N, F>` says which in
    /// its own type.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Atmosphere, Gravity, World};
    /// let world: World<8, _> = World::new().with_forces((Gravity::new(), Atmosphere::new()));
    /// ```
    pub const fn with_forces<G: Force>(self, forces: G) -> World<N, G> {
        World {
            reads_map: self.reads_map,
            solid: self.solid,
            forces,
            records: self.records,
            masses: self.masses,
            offsets: self.offsets,
            generations: self.generations,
            seated: self.seated,
            own_solid: self.own_solid,
        }
    }
}

impl<const N: usize, F: Force> World<N, F> {
    /// The weather the world owns — see [`with_forces`](Self::with_forces).
    pub fn forces(&self) -> &F {
        &self.forces
    }

    /// The same weather, to be driven: a force with state of its own — a gusting
    /// [`Wind`](super::Wind) — is updated where it lives, between one step and the next.
    pub fn forces_mut(&mut self) -> &mut F {
        &mut self.forces
    }

    /// The same world, with `solid` as the scene's word for *wall*.
    ///
    /// What stops a member is usually a fact about the scene rather than about anybody in it: one
    /// flag on the level's walls and floors, and everything that moves stops at them. So it is said
    /// once, here, and every member is stopped by these flags — on a tile or on a neighbour —
    /// unless it named [rules of its own](super::MemberBuilder::stopped_by), which replace the
    /// scene's for that member alone.
    ///
    /// A scene that changes its mind mid-game — a level where the water turns to ice — says so
    /// here too, and the new word reaches everybody already seated who goes by the scene's.
    ///
    /// ```no_run
    /// # use pixel8::{physics::World, SpriteFlag};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// let world: World<8> = World::new().with_solid(SOLID);
    /// ```
    ///
    /// This is the builder, for a world being made. A world that already exists — the one a cart
    /// keeps in a `static`, which is where a scene lives — is told the same thing by
    /// [`declare_solid`](Self::declare_solid).
    pub fn with_solid(mut self, solid: impl Into<BitFlags<SpriteFlag>>) -> Self {
        self.declare_solid(solid);

        self
    }

    /// The scene's word for *wall*, said to a world that is already there.
    ///
    /// What [`with_solid`](Self::with_solid) says, said in place instead of on the way in: the two
    /// are one thing, and a cart uses whichever fits where it is standing. A world spelled out as a
    /// constant cannot take the builder — flags are worked out through [`Into`], and that is not
    /// something a constant may call — so a scene that ships placed declares its walls in
    /// [`Game::boot`](crate::Game::boot), in one line, before anybody is enlisted.
    ///
    /// ```no_run
    /// # use pixel8::{physics::World, SpriteFlag};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// # fn f(world: &mut World<8>) {
    /// world.declare_solid(SOLID);
    /// # }
    /// ```
    pub fn declare_solid(&mut self, solid: impl Into<BitFlags<SpriteFlag>>) {
        self.solid = solid.into();
        // Each seat carries the word already settled between the member's own rule and the
        // scene's, so that the step never has to ask whose it was — which leaves the members who
        // go by the scene's to be told when it changes.
        let word = self.solid.bits();
        let mut theirs = self.seated & !self.own_solid;
        while theirs != 0 {
            let slot = theirs.trailing_zeros() as usize;
            theirs &= theirs - 1;
            self.records[slot].solid = word;
        }
    }

    /// Claims the lowest empty seat for a whole description — the `record` a
    /// [`MemberBuilder`](super::MemberBuilder) built up, its own rule for solid if it gave one, its
    /// mass and its rectangle's offset — and hands back the [`Member`] handle, or `None` for a
    /// full house: the world's half of [`enlist`](super::MemberBuilder::enlist), since the seats
    /// are the world's to hand out.
    pub(super) fn claim(
        &mut self,
        mut record: wire::Record,
        solid: Option<BitFlags<SpriteFlag>>,
        mass: f32,
        offset: (i16, i16),
    ) -> Option<Member> {
        // The lowest empty seat. Past `N` the mask reads as empty, so a full house answers here
        // rather than needing a count of its own.
        let slot = (!self.seated).trailing_zeros() as usize;
        if slot >= N {
            return None;
        }

        self.seated |= 1 << slot;
        // Solid is settled here, between the description's own rule and the scene's word at the
        // moment of claiming, exactly as it is for a member told a new rule after it is seated.
        let word = solid.unwrap_or(self.solid);
        record.solid = word.bits();
        self.records[slot] = record;
        self.masses[slot] = mass;
        self.offsets[slot] = offset;
        if solid.is_some() {
            self.own_solid |= 1 << slot;
        } else {
            self.own_solid &= !(1 << slot);
        }

        Some(Member {
            slot: slot as u8,
            generation: self.generations[slot],
        })
    }

    /// Moves the whole cast one update: the forces the world owns, then the world, then the bodies.
    ///
    /// For each member in turn, in seat order: the world's own [forces](Self::with_forces) bend its
    /// velocity, in the order they were composed; whatever ran its [rectangle](Member::bounds) into
    /// something solid to it — the scene's word ([`with_solid`](Self::with_solid)), unless the
    /// member has [rules of its own](super::MemberBuilder::stopped_by) — is taken out of it, on
    /// each axis separately; the velocity that survives is stored back and the body moved by
    /// it; whatever that carried outside its [confines](super::MemberBuilder::confined_to) is
    /// put back; and the sides that stopped it together with the flags of everything it met are
    /// written into its [`contacts`](Member::contacts), where the cart reads them at its
    /// leisure.
    ///
    /// *Something* is the map and the rest of the cast alike. The tiles under the member are asked
    /// what they carry, and so is every other member, at the rectangle it covers right now and
    /// under the flags the cell it [wears](super::MemberBuilder::wearing) carries in the sprite
    /// editor. Flags shared with what is solid to the member stop it — a tile and a neighbour
    /// in the same one-axis pass, so landing on a lift reads [`below`](Contacts::below) exactly
    /// as landing on a floor tile does — and everything met, wall or not, comes back in
    /// [`Contacts::touched`]. A member is never asked about itself, so its own kind is a wall
    /// like anybody else's: two crates wearing `CRATE`, each with `CRATE` in solid, stop each
    /// other and neither is ever its own wall.
    ///
    /// A meeting between two members reaches both of them, whichever one's movement made it: the
    /// mover's own sweep answers the mover, and whoever it arrived on is told what arrived —
    /// filtered by that member's own [heeds](super::MemberBuilder::heeding) and solid, flags only,
    /// in the same update. So a ram is felt on both sides of it however the two were seated,
    /// and either party may be [retired](Member::retire) the moment the step returns without
    /// costing the other its news.
    ///
    /// The two halves of the answer are taken over different ground, and on purpose. A member is
    /// *stopped* where an axis was trying to go — the endpoint, which is where a wall has to be to
    /// be one — and it is told what it *met* over the whole of the step: where it began, the ground
    /// each axis swept across, and where it ended up. So the pond a member walks out of this update
    /// is reported, and so is a hazard crossed between one pixel and the next. One thing follows
    /// from the difference and is worth knowing: something thinner than an update's movement can be
    /// stepped clean over without stopping the member, and comes back in
    /// [`touched`](Contacts::touched) all the same. Keeping a fall from doing that to a floor is
    /// what [`Gravity`](super::Gravity)'s terminal velocity is for.
    ///
    /// A neighbour standing *on* a member pushes it out before anything moves — out the shallower
    /// way, out the side it was already nearer — and reports the side it pushed *from*: a lift that
    /// has just risen a pixel into the rider standing on it carries the rider up, and the rider
    /// still reads `below`. What the push could not fully separate is reported but cannot block, so
    /// a thing caught between two solids can still walk out of them. A tile can never do any of
    /// this; a member can.
    ///
    /// The edge of the world is the last thing to have its say, after the movement that took the
    /// member out there: one that named [confines](super::MemberBuilder::confined_to) is put back
    /// against them, the speed that carried it out is spent, and the sides it was held at join
    /// the walls' in the answer.
    ///
    /// An axis that was blocked is zeroed in the stored velocity too, not just in this update's
    /// movement: a fall that lands has been spent, and something that walked into a wall is not
    /// still walking. A member driven by the buttons writes its sideways speed afresh every update
    /// and never notices; one carrying its own momentum does, which is the point.
    ///
    /// How a member looks is none of the step's business: the cell it wears is read for the flags
    /// on it and nothing more, and which way round it is drawn, how many cells it spans and whether
    /// it is shown at all pass through every step untouched.
    ///
    /// The weather is the world's own — one value, handed over once in
    /// [`with_forces`](Self::with_forces) — so a step asks for nothing but the update's context. A
    /// gust the whole scene is bent by lives on the world and is [driven](Self::forces_mut) between
    /// steps.
    ///
    /// What a step costs a cart is the crossing, and nothing else. The world's own seats are handed
    /// to the console as they stand — one pointer, one call, nothing written down for the journey —
    /// and the console steps them natively, over its own map and sheet, answering into the very
    /// bytes the cast lives in. No fuel is spent on the walking and the stopping, nothing is
    /// allocated, and the engine the console runs is the one this module's tests drive. A member
    /// that should cost nothing is simply not enlisted.
    pub fn step(&mut self, ctx: &Context) {
        // The forces are the cart's own code, so their half of the step happens on the cart's side
        // whichever way the rest of it goes — and before anything is read, which is the one
        // snapshot point both halves share.
        self.weather();

        // In the console, the whole step is one crossing of the ABI and the console's own, native,
        // work; on the native builds the tests are, the SDK runs the same engine itself.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            self.step_over_the_wire();
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.step_natively(ctx);
    }

    /// Draws the whole cast, in seat order, where the last step left it.
    ///
    /// Each member is drawn from the cell it [wears](super::MemberBuilder::wearing), at the pixel
    /// it draws at ([`draw_pos`](Member::draw_pos)), as many cells as it
    /// [spans](super::MemberBuilder::spanning), and mirrored the way it is
    /// [flipped](super::MemberBuilder::flipped). A member that wears nothing is not drawn, nor
    /// is one that is [hidden](super::MemberBuilder::hidden), and an empty seat is nothing here as
    /// it is nothing everywhere else. Seat order is drawing order as it is stepping order: a later
    /// seat is drawn over an earlier one.
    ///
    /// `layers` is the very filter [`Graphics::map`] takes. An empty set draws everybody; anything
    /// else draws only the members whose worn cell carries one of those flags, so a member wearing
    /// a flagless cell is drawn only by an empty `layers`. A scene drawn in layers is one call a
    /// layer, and whatever draw state a layer needs — a transparency, a palette swap for a hurt
    /// flash — is set around its call, as it would be around a `map`:
    ///
    /// ```no_run
    /// # use pixel8::{physics::World, *};
    /// # const AIRCRAFT: SpriteFlag = SpriteFlag::Flag0;
    /// # const SHOT: SpriteFlag = SpriteFlag::Flag1;
    /// # fn f(sky: &World<32>, gfx: &mut Graphics) {
    /// gfx.clear(Color::DARK_BLUE);
    /// // The shots first, so that every one of them is under the aircraft, whichever seats they
    /// // were fired into.
    /// sky.draw(gfx, SHOT);
    /// // Then the aircraft, whose black outlines are drawn black rather than seen through.
    /// gfx.set_transparent_color(Color::BLACK, false);
    /// sky.draw(gfx, AIRCRAFT);
    /// gfx.reset_transparency();
    /// # }
    /// ```
    ///
    /// The look is the update's to decide — [`set_sprite`](Member::set_sprite),
    /// [`set_flip`](Member::set_flip) and the rest are written beside the velocity — and
    /// [`Game::draw`](crate::Game::draw) holds the game by `&self`, so nothing about the world can
    /// change here. What is set on the screen is honoured as it is for any sprite: the camera, the
    /// clip, the transparency and the palette. Anything that is not a member — the HUD, a particle
    /// effect, the line of a rotor — is the cart's, drawn before this call or after it.
    ///
    /// What a draw costs a cart is one crossing. The world's own seats are handed to the console
    /// where they stand, exactly as [`step`](Self::step) hands them, and drawn natively, so a cast
    /// of sixty-four costs a cart's draw the one call a cast of one does.
    pub fn draw(&self, gfx: &mut Graphics, layers: impl Into<BitFlags<SpriteFlag>>) {
        let layers = layers.into();
        // Everything up to the last seat taken, exactly as the step sends it: an empty seat in the
        // middle of it travels as `wire::VACANT`, which wears nothing and so shows nothing.
        let taken = self.high_water();

        // In the console, the whole draw is one crossing of the ABI and the console's own blits;
        // on the native builds the tests are, the SDK walks the very same looks itself.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = gfx;
            // SAFETY: the pointer and the length name the first `taken` of the world's own `N`
            // records, each `#[repr(C)]` and `wire::RECORD` bytes long — `high_water` never counts
            // past `N` — and the console only reads those `taken * wire::RECORD` bytes, while
            // `&self` keeps them unchanged for the length of the call.
            unsafe {
                crate::ffi::draw_cast(
                    self.records.as_ptr().cast(),
                    taken as u32,
                    u32::from(layers.bits()),
                );
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            // What the sheet says a cell carries, asked of the ABI the way
            // `Context::sprite_flags` asks it: a draw has no context to ask through.
            let carried = |sprite: SpriteId| {
                // SAFETY: `sprite_flags` takes the cell by value and reaches no memory of the
                // cart's; off the console it is the ABI's stub, which reads nothing at all.
                let bits = unsafe { crate::ffi::sprite_flags(u32::from(sprite.0)) };
                BitFlags::from_bits(bits as u8).expect(
                    "sprite_flags returned an unknown sprite-flag bit \
                     (pixel8 host/SDK ABI mismatch)",
                )
            };
            for look in wire::looks(&self.records[..taken], layers, carried) {
                gfx.sprite_ext(
                    look.sprite,
                    look.x,
                    look.y,
                    look.width,
                    look.height,
                    look.flip_x,
                    look.flip_y,
                )
                .expect("a look is at least one cell each way");
            }
        }
    }

    /// One update's worth of the world's own forces, over every member the world moves.
    ///
    /// Velocities only — a force never touches a position — and props are left alone: the cart
    /// drives them, weather and all. What a force is shown is a [`Subject`] built from the seat: a
    /// velocity to bend, and the mass and position it may read.
    fn weather(&mut self) {
        let mut seated = self.seated;
        while seated != 0 {
            let slot = seated.trailing_zeros() as usize;
            seated &= seated - 1;
            let record = &mut self.records[slot];
            if record.meta & wire::PROP != 0 {
                continue;
            }

            let mut subject = Subject::new(
                Velocity::new(record.dx, record.dy),
                self.masses[slot],
                (record.x, record.y),
            );
            self.forces.apply(&mut subject);
            let bent = subject.velocity();
            (record.dx, record.dy) = (bent.dx, bent.dy);
        }
    }

    /// The step, sent across the ABI: the world's own seats handed over, the answers written into
    /// them. The console's side runs the engine [`step_cast`](Self::step_cast) is — see [`wire`] —
    /// so what a cart spends here is the call, not the collisions.
    #[cfg(target_arch = "wasm32")]
    fn step_over_the_wire(&mut self) {
        // Everything up to the last seat taken. What is empty in the middle of it travels as the
        // inert prop `wire::VACANT` is, so the console sees the cast at the very indices the world
        // keeps it at and needs to know nothing about vacancy.
        let taken = self.high_water();
        unsafe {
            crate::ffi::step_cast(
                self.records.as_mut_ptr().cast(),
                taken as u32,
                self.reads_map as u32,
            );
        }

        self.reseat();
    }

    /// The step, run here: the seats decoded into the engine's own view of a cast, the very engine
    /// the console runs, and the answers reported back into the seats.
    ///
    /// What answers a cart on a native build — the SDK's tests, and anything that runs a cart's
    /// logic outside the console. The map and the sprite sheet are reached for a host call at a
    /// time here, where the console has them natively; everything else is the same code.
    #[cfg(not(target_arch = "wasm32"))]
    fn step_natively(&mut self, ctx: &Context) {
        let taken = self.high_water();
        let mut cast: [wire::Recast; N] =
            core::array::from_fn(|slot| wire::Recast::of(&self.records[slot]));
        {
            let mut members = cast.each_mut().map(|member| member as &mut dyn Kinetic);
            self.step_cast(
                &mut members[..taken],
                |x, y| {
                    ctx.map_tile(x, y)
                        .map_or(BitFlags::empty(), |tile| ctx.sprite_flags(tile))
                },
                |sprite| ctx.sprite_flags(sprite),
            );
        }
        for (member, record) in cast.iter().zip(self.records.iter_mut()).take(taken) {
            member.report(record);
        }

        self.reseat();
    }

    /// The console's half of [`step`](Self::step): the same engine, over the map and the sheet the
    /// console binds in natively. Hidden — a cart calls `step`, and this is what answers it on the
    /// other side of the wire.
    #[doc(hidden)]
    pub fn step_hosted(
        &self,
        cast: &mut [&mut dyn Kinetic],
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
        carried: impl Fn(SpriteId) -> BitFlags<SpriteFlag>,
    ) {
        self.step_cast(cast, tiles, carried);
    }

    /// Puts every rectangle back on the body the step has just moved.
    ///
    /// The wire's answers are the body, the velocity and the contacts, and nothing else: what the
    /// cart wrote comes back exactly as the cart wrote it, which is what lets the seats *be* the
    /// state. So the rectangle's corner is the one thing left to work out afterwards — wherever the
    /// member now draws, plus the offset it keeps from it.
    fn reseat(&mut self) {
        let mut seated = self.seated;
        while seated != 0 {
            let slot = seated.trailing_zeros() as usize;
            seated &= seated - 1;
            let offset = self.offsets[slot];
            let record = &mut self.records[slot];
            (record.bx, record.by) = corner((record.rx, record.ry), offset);
        }
    }

    /// How much of the cast a step has to carry: one past the last seat taken.
    ///
    /// The empty seats before it go along as props of no size; the ones after it are not sent at
    /// all, so a world with room to spare pays for the room it is using.
    fn high_water(&self) -> usize {
        (u64::BITS - self.seated.leading_zeros()) as usize
    }

    /// `member`'s seat, or a panic naming it.
    ///
    /// A handle to somebody who has left the cast — or to somebody who was never enlisted, which is
    /// what [`Member::NOBODY`] is — is a cart holding on to nobody, and there is no honest answer
    /// to give it: the seat is empty, or it has been let to somebody else who is nobody's hero.
    /// Loud, and exactly where it happened.
    ///
    /// The check is on every accessor a cart calls in an update, so its hot half is spelled to
    /// inline there — a compare and a taken seat — and the panic lives in [`stale`], cold and out
    /// of line, where it costs nothing until it fires.
    #[inline(always)]
    pub(super) fn seat(&self, member: Member) -> usize {
        if !self.holds(member) {
            stale(member.slot, N);
        }

        member.slot as usize
    }

    /// Whether `member` names somebody actually in the cast: a seat of this world's, taken, and
    /// taken by the very member the handle was made for.
    #[inline(always)]
    pub(super) fn holds(&self, member: Member) -> bool {
        let slot = member.slot as usize;

        slot < N && self.seated & (1 << slot) != 0 && self.generations[slot] == member.generation
    }

    /// The step itself, over a map and a sprite sheet handed in rather than reached for.
    ///
    /// Everything that makes a world a world is here — the order, the splitting of the cast, the
    /// resolution, the contacts — so a test can drive the whole of it against a map and a sheet it
    /// wrote down itself, and what it proves holds for the console running the very same engine.
    /// The weather is not here: the forces are the cart's own code and have already had their say,
    /// over the whole cast at once, before any of this was read.
    fn step_cast(
        &self,
        cast: &mut [&mut dyn Kinetic],
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
        carried: impl Fn(SpriteId) -> BitFlags<SpriteFlag>,
    ) {
        // A world's own cast fits by construction — its `N` is compile-checked against the wire's
        // ceiling — so this guards the ways in that are not a world's own: the console's entry, and
        // anything a test writes down. Loud, exactly where it happened, rather than a quiet step
        // onto some slower path.
        assert!(
            cast.len() <= wire::CAP,
            "a cast of {} was handed across a wire with a ceiling of {}",
            cast.len(),
            wire::CAP
        );

        // What every cast member is worth to everybody else, taken once at the top: the rectangle
        // it covers and the flags its cell carries. Each member is then resolved against the rest
        // of the cast several times over — once to be pushed out of it, once for each axis it
        // moves along — and every one of those questions used to go back through the cast's `dyn`
        // for a rectangle and a sheet lookup that had not changed since the last one. That is the
        // n-squared this takes out: one question a member a step, and plain loads after it.
        //
        // Two arrays rather than one of pairs, and the difference is measurable: a rectangle is
        // eight bytes and a flag set is one, so an array of pairs is a run of stores to clear
        // while two runs of nothing are two bulk fills. A cast too long for them is walked through
        // the `dyn` instead, exactly as it always was — slower, and identical in what it answers.
        let mut boxes = [EMPTY; SNAPSHOT];
        let mut carries = [BitFlags::empty(); SNAPSHOT];
        let mut wants = [BitFlags::empty(); SNAPSHOT];
        let members = cast.len();
        let fits = members <= SNAPSHOT;
        // Everything anybody in the cast is wearing, in one flag set. What it buys is the two
        // questions a member can settle without walking anybody: whether there is a wall of its
        // own out there to be pushed out of, and whether there is anything out there at all.
        let mut worn = BitFlags::empty();
        if fits {
            for (((rectangle, flagged), listening), member) in boxes
                .iter_mut()
                .zip(carries.iter_mut())
                .zip(wants.iter_mut())
                .zip(cast.iter())
            {
                // What the member is listening for — everything it heeds or calls solid — which
                // is what an arrival on it is judged against. A prop listens for nothing: its
                // contacts are never written, so there is nobody home to tell.
                if !member.prop() {
                    *listening = member.heeds() | member.solid().unwrap_or(self.solid);
                }
                // A member that wears nothing, or whose cell the cart flagged with nothing,
                // stands in nobody's way and tells nobody anything — but one that is listening
                // still keeps a rectangle here, so an arrival on it can be seen. Only a slot
                // neither wearing nor listening stays the nothing it was born as.
                let flags = member.sprite().map_or(BitFlags::empty(), &carried);
                if !flags.is_empty() || !listening.is_empty() {
                    *rectangle = member.bounds();
                    *flagged = flags;
                    worn = worn | flags;
                }
            }
        } else {
            // A cast too long to snapshot still gets the one flag set, since it is what says
            // whether any of the walking below is worth doing at all. One question a member
            // where the snapshot would have asked two.
            for member in cast.iter() {
                if let Some(sprite) = member.sprite() {
                    worn = worn | carried(sprite);
                }
            }
        }

        // The meetings each member's own step makes, delivered to the other party once the whole
        // cast has moved: a slot per member, holding the flags of everything that arrived on it.
        // Collected rather than written straight away, because the other party may not have been
        // stepped yet — and its own step overwrites its contacts whole.
        let mut arrived = [BitFlags::<SpriteFlag>::empty(); wire::CAP];
        for index in 0..cast.len() {
            // The cast without this member in it, in two pieces: everything stepped already, and
            // everything still to be. The split is what the fallback walks, and the index of the
            // member in the middle is what the snapshot skips — either way, the whole of how a
            // member comes to be skipped against itself is that no question ever reaches it.
            let (before, rest) = cast.split_at_mut(index);
            let Some((entity, after)) = rest.split_first_mut() else {
                break;
            };

            // A prop was placed by the cart and stays placed: its slot in the snapshot is where
            // everybody meets it, and the whole of the stepping below — walls, the hold — is for
            // things the world moves.
            if entity.prop() {
                continue;
            }

            let mine = if fits {
                carries[index]
            } else {
                entity.sprite().map_or(BitFlags::empty(), &carried)
            };
            let neighbours = Neighbours {
                // Lazily, since the slices would be indexed out of a snapshot that was never
                // filled for a cast too long to fit in one.
                taken: fits.then(|| (&boxes[..members], &carries[..members], &wants[..members])),
                worn,
                mine: index,
                before,
                after,
                carried: &carried,
                solid: self.solid,
                met: core::cell::Cell::new(0),
            };
            step_entity(
                &mut **entity,
                tiles,
                self.reads_map,
                self.solid,
                mine,
                &neighbours,
            );

            // Whoever this step arrived on is owed the news of it: what this member wears, into
            // the slot of each neighbour the resolution noted, for the delivery below.
            let mut met = neighbours.met.get();
            while met != 0 {
                let slot = met.trailing_zeros() as usize;
                met &= met - 1;
                arrived[slot] = arrived[slot] | mine;
            }

            // The member has just moved, so its slot follows it. That is what keeps the cast
            // sequential: everything stepped after this one meets it where it now is, and
            // everything already stepped met it where it was. Flags cannot change under a step, so
            // the one read of them at the top stands — and a slot that neither wears nor listens
            // was never a rectangle anybody was going to be shown.
            if fits && (!carries[index].is_empty() || !wants[index].is_empty()) {
                boxes[index] = entity.bounds();
            }
        }

        // The deliveries: every meeting reaches both of its parties in the one step, whoever's
        // movement made it. Each recipient hears only what it was listening for — the very filter
        // the notes were taken under — and hears it by having it joined onto the answer its own
        // step wrote, sides untouched: an arrival tells a member what reached it, never that it
        // was stopped.
        for (index, entity) in cast.iter_mut().enumerate() {
            let news = arrived[index];
            if news.is_empty() {
                continue;
            }
            let listening = if fits {
                wants[index]
            } else if entity.prop() {
                BitFlags::empty()
            } else {
                entity.heeds() | entity.solid().unwrap_or(self.solid)
            };
            let contacts = entity.contacts_mut();
            contacts.touched = contacts.touched | (news & listening);
        }
    }
}

impl<const N: usize, F: Force> World<N, F> {
    /// Empties `slot`, bookkeeping and all — what [`retire`](Member::retire) does short of ageing
    /// the seat: no handle was ever handed out for it, so there is nothing to age.
    pub(super) fn vacate(&mut self, slot: usize) {
        self.seated &= !(1 << slot);
        self.own_solid &= !(1 << slot);
        self.records[slot] = wire::VACANT;
        self.masses[slot] = 1.0;
        self.offsets[slot] = (0, 0);
    }
}

/// How long a cast is snapshotted rather than walked.
///
/// Thirty-two members: 288 bytes of the shadow stack between the two arrays, under one percent of
/// a cart's 32 KiB default reserve, and reclaimed the moment the step returns. Room to spare rather
/// than room measured out, because clearing them costs a bulk fill either way — a capacity of
/// sixty-four measures the same as a capacity of sixteen — so the number is chosen for the scenes
/// it covers and not for the stack it takes. More than thirty-two things moving at once on a
/// 128x128 screen is a scene with bigger problems than this array, and one that has them anyway is
/// answered exactly as it always was: the walk falls back to asking each member through the cast's
/// `dyn`.
const SNAPSHOT: usize = 32;

/// The rectangle a slot holds until something is written into it: nothing, which overlaps nothing.
const EMPTY: Bounds = Bounds::new(0, 0, 0, 0);

/// A world of `N` empty seats. See [`World::new`].
impl<const N: usize> Default for World<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// One member's update: the resolution, the movement, the edge of the world, and the answer
/// written back into the member's own slot. The forces have already had their say, over the whole
/// cast at once, before anybody was stepped.
///
/// The order is the whole of it. The resolution takes out of the velocity whatever ran into
/// something; the body is moved by what is left; and only then is the member held inside its
/// limits, since it is the movement just made that carried it out there.
#[inline(always)]
fn step_entity(
    entity: &mut dyn Kinetic,
    tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
    reads_map: bool,
    solid: BitFlags<SpriteFlag>,
    worn: BitFlags<SpriteFlag>,
    neighbours: &impl Cast,
) {
    // Everything the member describes is read before anything of it moves — the limits along
    // with the rest, so an answer worked out from where it stands means where the step
    // found it, on either side of the wire.
    let limits = entity.confines();
    // `Velocity` is `Copy`, so this reads out what the forces left behind without holding a borrow
    // of the member over the resolution below.
    let mut velocity = *entity.velocity_mut();
    let mut contacts = Contacts::empty();
    // A member covering no pixels has nothing to resolve — a hitbox switched off, a blast that has
    // shrunk to nothing — and is only moved.
    //
    // The scene's word for solid, unless this member has one of its own.
    let solid = entity.solid().unwrap_or(solid);
    let heeds = entity.heeds();
    if let Some(mut collider) = Collider::new(
        entity.body(),
        entity.bounds(),
        solid,
        heeds,
        worn,
        reads_map,
    ) {
        // Out of anything standing on it first, so the resolution starts from a box that is clear
        // — and only where there is something out there to be inside of. A member that calls
        // nothing solid can stand in anything, and one no neighbour is a wall to has nothing to be
        // pushed out of, so most casts settle the whole separating question here, without placing
        // a box, walking anybody, or making the call.
        if collider.could_be_inside_something(neighbours) {
            // Guarded like the hold below: `set_pos` re-snaps the drawn pixel, and a member that
            // was never overlapping anything must keep the coherent step `Body` is holding for it.
            let ((dx, dy), pushed) = collider.expel(neighbours);
            if (dx, dy) != (0.0, 0.0) {
                let (x, y) = entity.body().pos();
                entity.body_mut().set_pos(x + dx, y + dy);
            }
            contacts = pushed;
        }

        let (survived, resolved) = collider.resolve(velocity, tiles, neighbours);
        velocity = survived;
        contacts |= resolved;
    }

    entity.body_mut().move_by(velocity.dx, velocity.dy);

    // The edge of the world, last: it is the movement just made that carried the member out there,
    // and the sides it is held at read alongside the walls that stopped it. The velocity goes with
    // it rather than being fetched back out of the member, so what survives the whole step is
    // stored once, at the bottom, however many places had a say in it.
    if let Some(limits) = limits {
        contacts |= hold(entity, limits, &mut velocity).into();
    }

    *entity.velocity_mut() = velocity;
    *entity.contacts_mut() = contacts;
}

/// The snapshot's three answers about the whole cast, a slot each in cast order: the rectangle
/// each member covers, the flags it carries, and the flags it is listening for.
type Snapshot<'a> = (
    &'a [Bounds],
    &'a [BitFlags<SpriteFlag>],
    &'a [BitFlags<SpriteFlag>],
);

/// The cast without the member being stepped: everything already moved this update, and everything
/// still to be.
///
/// Two slices rather than one, because the member in the middle is the one holding the `&mut` — and
/// that is the whole of how a member comes to be skipped against itself. What each neighbour is
/// worth is answered as the resolution reaches its slot: the rectangle it covers *now*, and the
/// flags the cart wrote on the cell it says it wears.
struct Neighbours<'a, 'cast, F> {
    /// Every cast member's rectangle, the flags it carries and the flags it is listening for, as
    /// they stand right now, a slot each in cast order — or nothing at all for a cast too long to
    /// have been snapshotted, which is walked through the `dyn` below instead.
    taken: Option<Snapshot<'a>>,
    /// Everything anybody in the cast is wearing, the member being stepped included — see
    /// [`Cast::carried`]. Nothing at all for a cast too long to have been snapshotted, which
    /// answers every question the long way round.
    worn: BitFlags<SpriteFlag>,
    /// Which slot is the member being stepped, so the snapshot can leave it out.
    mine: usize,
    before: &'a [&'cast mut dyn Kinetic],
    after: &'a [&'cast mut dyn Kinetic],
    carried: F,
    /// The scene's word for solid, which is what a neighbour with no rule of its own is listening
    /// for a wall with — the long-cast fallback works a neighbour's listening out through the
    /// `dyn`, and needs the world's word to finish it.
    solid: BitFlags<SpriteFlag>,
    /// One bit per cast slot: the neighbours this member's step has [met](Cast::note) while they
    /// were listening. A `Cell` because the walks hold the whole cast by `&self`; a `u64` because
    /// the wire's ceiling is sixty-four, and a world's cannot exceed it.
    met: core::cell::Cell<u64>,
}

impl<F: Fn(SpriteId) -> BitFlags<SpriteFlag>> Cast for Neighbours<'_, '_, F> {
    #[inline(always)]
    fn carried(&self) -> BitFlags<SpriteFlag> {
        self.worn
    }

    fn note(&self, index: usize) {
        // The walk's index becomes the cast's slot: the snapshot indexes the whole cast, and the
        // fallback indexes it with the member being stepped taken out of the middle.
        let slot = match self.taken {
            Some(_) => index,
            None if index < self.mine => index,
            None => index + 1,
        };
        self.met.set(self.met.get() | 1 << slot);
    }

    #[inline(always)]
    fn len(&self) -> usize {
        // Nobody in the cast is wearing anything, so there is nobody in it to meet and no slot
        // worth asking about: the two slices below hold members the whole of whose part in this
        // is that they are moved.
        if self.worn.is_empty() {
            return 0;
        }

        match self.taken {
            Some((boxes, ..)) => boxes.len(),
            // The member in the middle is in neither half, so there is no slot of its own here to
            // count or to skip: the two slices are the cast without it already.
            None => self.before.len() + self.after.len(),
        }
    }

    #[inline(always)]
    fn at(&self, index: usize) -> Option<Neighbour> {
        let Some((boxes, carries, wants)) = self.taken else {
            return self.asked(index);
        };

        if index == self.mine {
            return None;
        }
        let flags = *carries.get(index)?;
        let listening = *wants.get(index)?;
        if flags.is_empty() && listening.is_empty() {
            return None;
        }

        Some((*boxes.get(index)?, flags, listening))
    }
}

impl<F: Fn(SpriteId) -> BitFlags<SpriteFlag>> Neighbours<'_, '_, F> {
    /// The long-cast fallback: the neighbour asked for itself, in the same order and with the same
    /// answer as the snapshot would have given, and a great deal more slowly.
    ///
    /// Kept out of line so that the snapshot path above — which is every cast a cart is likely to
    /// have — inlines into the loops that drive it.
    #[inline(never)]
    fn asked(&self, index: usize) -> Option<Neighbour> {
        let other = match self.before.get(index) {
            Some(other) => other,
            None => self.after.get(index - self.before.len())?,
        };
        // The same two answers the snapshot holds, worked out here instead: what the neighbour
        // wears, and what it is listening for. One that wears nothing and listens for nothing is
        // not there for anybody: it stops nobody, tells nobody, and nothing that reaches it is
        // news.
        let flags = other
            .sprite()
            .map_or(BitFlags::empty(), |sprite| (self.carried)(sprite));
        let listening = if other.prop() {
            BitFlags::empty()
        } else {
            other.heeds() | other.solid().unwrap_or(self.solid)
        };
        if flags.is_empty() && listening.is_empty() {
            return None;
        }

        Some((other.bounds(), flags, listening))
    }
}

/// Puts `entity` back inside `limits`, and answers with the sides it was held at.
///
/// What the resolution does with walls, for the edge of the world: a member whose rectangle has
/// left `limits` is put back against them, and the speed that took it there is spent rather than
/// left to build up while it leans on the edge.
///
/// It is the rectangle that is held, wherever the member put it, and the body follows by the same
/// amount — so a hurtbox inset into a sprite stops with its own edge against the limit. The exact
/// sub-pixel position is what moves, not the drawn one, so something leaning on an edge sits at it
/// precisely instead of being nudged a pixel at a time.
///
/// Speed pointing back into `limits` is left alone: a member that starts outside and is already
/// travelling home keeps what was bringing it. A rectangle with no room to fit — one wider than
/// the `limits` it is given — is held against their near edge rather than pushed out the far one.
///
/// `velocity` is the step's own, handed over and spent in place: the member's slot is written once
/// where the step ends rather than read back out and written again here.
#[inline(always)]
fn hold(entity: &mut dyn Kinetic, limits: Bounds, velocity: &mut Velocity) -> BitFlags<Contact> {
    let bounds = entity.bounds();
    // One look at the body for both of what it is asked, since every question a member is put
    // through the cast's `dyn` costs a call the resolution used to have inlined.
    let body = entity.body();
    let (x, y) = body.pos();

    // The rectangle's own corner, in the exact sub-pixel coordinates the body keeps: the
    // whole-pixel offset of the rectangle from where the body draws, carried onto the position the
    // body really has. Zero for a rectangle over the sprite, which is most of them.
    let (draw_x, draw_y) = body.draw_pos();
    let left = x + (bounds.x() - draw_x) as f32;
    let top = y + (bounds.y() - draw_y) as f32;

    // Saturating, as the far edges of a `Bounds` are, so limits at the end of the coordinate space
    // cannot overflow — and never past the near edge, so a rectangle too big to fit is held at the
    // near edge instead of being flung out of the far one. In `i32` throughout: the near edge the
    // subtraction is held above is never below the bottom of the coordinate space, so holding it
    // there first buys nothing and costs the narrowing and widening around it.
    let rightmost =
        (far(limits.x(), limits.width()) - bounds.width() as i32).max(limits.x() as i32) as f32;
    let lowest =
        (far(limits.y(), limits.height()) - bounds.height() as i32).max(limits.y() as i32) as f32;

    let (mut dx, mut dy) = (0.0, 0.0);
    // Sides alone: the edge of the world is not a thing with flags on it, so there is nothing for
    // the other half of a `Contacts` to say.
    let mut held = BitFlags::empty();
    if left < limits.x() as f32 {
        dx = limits.x() as f32 - left;
        held = held | Contact::Left;
    } else if left > rightmost {
        dx = rightmost - left;
        held = held | Contact::Right;
    }
    if top < limits.y() as f32 {
        dy = limits.y() as f32 - top;
        held = held | Contact::Above;
    } else if top > lowest {
        dy = lowest - top;
        held = held | Contact::Below;
    }

    // Only the speed that was carrying the member out is spent. One already heading back in —
    // something spawned off the edge, or knocked there — keeps what is bringing it home.
    if !held.is_empty() {
        if (held.contains(Contact::Left) && velocity.dx < 0.0)
            || (held.contains(Contact::Right) && velocity.dx > 0.0)
        {
            velocity.dx = 0.0;
        }
        if (held.contains(Contact::Above) && velocity.dy < 0.0)
            || (held.contains(Contact::Below) && velocity.dy > 0.0)
        {
            velocity.dy = 0.0;
        }
    }
    // Guarded, because `set_pos` re-snaps the drawn pixel: a member that was already inside would
    // lose the coherent step `Body` is holding for it, and shimmer for it.
    if (dx, dy) != (0.0, 0.0) {
        entity.body_mut().set_pos(x + dx, y + dy);
    }

    held
}

/// Where a member's rectangle sits: the pixel it draws at, plus the offset it keeps from it.
///
/// Saturating at the ends of the coordinate space, exactly where a rectangle's own edges saturate
/// — a corner past them was never a pixel anything could stand on — and worked out in the wider
/// type, because a body drawn at one end of the space wearing a rectangle at the other is a
/// strange member but a safe one, and must not wrap into a different geometry here.
pub(super) fn corner((rx, ry): (i16, i16), (dx, dy): (i16, i16)) -> (i16, i16) {
    fn along(pixel: i16, offset: i16) -> i16 {
        (pixel as i32 + offset as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    (along(rx, dx), along(ry, dy))
}

/// `meta` with its two mirror bits set for `flip_x` and `flip_y`, and every other bit — what the
/// step reads, and whether the member is shown — as it was.
pub(super) fn mirrored(meta: u8, flip_x: bool, flip_y: bool) -> u8 {
    let mut mirrored = meta & !(wire::FLIP_X | wire::FLIP_Y);
    if flip_x {
        mirrored |= wire::FLIP_X;
    }
    if flip_y {
        mirrored |= wire::FLIP_Y;
    }

    mirrored
}

/// The wire's `span` byte for a block of `width` x `height` cells — or a panic, for a block no
/// sheet holds.
///
/// A nibble each way, holding the cells less one: the single cell is zero, which is what every
/// seat starts as, and the whole sheet is `0xff`. Nothing outside one to sixteen fits a nibble
/// that way, and a cart asking for it has made a mistake better heard about where it was made
/// than drawn from some other block of the sheet.
pub(super) fn span_byte(width: u8, height: u8) -> u8 {
    assert!(
        (1..=16).contains(&width) && (1..=16).contains(&height),
        "a member is drawn from one to sixteen cells each way, not {width}x{height}"
    );

    (width - 1) | ((height - 1) << 4)
}

/// The cold half of [`World::seat`]'s check: the panic a stale handle is answered with, out of
/// every hot path so the check itself is a compare and nothing more.
#[cold]
#[inline(never)]
fn stale(slot: u8, seats: usize) -> ! {
    assert!(
        (slot as usize) < seats,
        "seat {slot} is no seat of this world: a member that was never enlisted is being asked about"
    );
    panic!("seat {slot} was retired: a member that has left the cast is still being asked about")
}

#[cfg(test)]
mod tests {
    use super::{
        super::{force::Mob, Atmosphere, Gravity, Velocity, Wind},
        *,
    };
    use crate::Body;

    /// A level's pull, as an ordinary constant of the cart's own — `Gravity::new` is `const`.
    const GRAVITY: Gravity = Gravity::new();

    /// The world the engine's own tests are stepped by: there is only ever the one, and it seats
    /// nobody — the cast comes in through [`World::step_cast`], written down by the test itself.
    const WORLD: World<0> = World::new();

    /// The update's context, which a native build answers out of the ABI's stubs: an empty map,
    /// and a sheet with nothing flagged on it. What the tests about the world's own cast are
    /// stepped with.
    const CTX: Context = Context { _private: () };

    /// A world with the walls declared on it, for the tests about the scene's own word.
    fn walled() -> World<0> {
        World::new().with_solid(WALL)
    }

    #[test]
    #[should_panic(expected = "ceiling")]
    fn a_cast_past_the_wire_s_ceiling_is_refused_loudly() {
        // A world's own cast cannot overrun the wire — its `N` is compile-checked — so this pins
        // the guard on the engine's own entry, which the console and the tests walk in through:
        // one member past the wire's sixty-four is refused where it happened, not quietly stepped
        // some dearer way.
        let mut things: Vec<Thing> = (0..wire::CAP + 1)
            .map(|i| Thing::at(i as f32 * 100.0, 0.0))
            .collect();
        let mut cast: Vec<&mut dyn Kinetic> = things.iter_mut().map(Thing::as_kinetic).collect();
        WORLD.step_cast(&mut cast, air, unflagged);
    }

    #[test]
    fn a_meeting_is_told_to_both_of_its_parties_whoever_made_it() {
        // The stander is stepped first, going nowhere: its own sweep is the box it stands in,
        // and the mover is nowhere near it yet. The mover, stepped second, arrives on it. The
        // meeting must reach both contacts slots in this same update — read one-sidedly, a ram
        // kills only the party that moved, and the dead one leaves the cast before the other
        // was ever told.
        let mut stander = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE);
        let mut mover = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        WORLD.step_cast(
            &mut [stander.as_kinetic(), mover.as_kinetic()],
            air,
            flagged,
        );
        assert!(
            mover.contacts.touches(CRATE),
            "the mover's own sweep missed the meeting"
        );
        assert!(
            stander.contacts.touches(WALL),
            "the one arrived upon was never told"
        );
    }

    #[test]
    fn an_arrival_is_heard_only_by_a_listener_and_only_for_what_it_heeds() {
        // The deaf stander heeds nothing, so the arrival is not its news — while the mover, its
        // own sweep making the meeting, is still told as ever.
        let mut deaf = Thing::at(0.0, 0.0)
            .wearing(CRATE_SPRITE)
            .heeding(BitFlags::empty());
        let mut mover = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        WORLD.step_cast(&mut [deaf.as_kinetic(), mover.as_kinetic()], air, flagged);
        assert!(mover.contacts.touches(CRATE));
        assert_eq!(deaf.contacts, Contacts::empty(), "the deaf one was told");

        // And the interest can run one way alone: a mover listening for nothing hears nothing of
        // its own sweep, and the one it arrives on is still told what arrived.
        let mut stander = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE);
        let mut oblivious = Thing::at(12.0, 0.0)
            .wearing(WALL_SPRITE)
            .heeding(BitFlags::empty())
            .moving(-6.0, 0.0);
        WORLD.step_cast(
            &mut [stander.as_kinetic(), oblivious.as_kinetic()],
            air,
            flagged,
        );
        assert_eq!(oblivious.contacts, Contacts::empty());
        assert!(
            stander.contacts.touches(WALL),
            "the mover's own disinterest cost the stander its news"
        );
    }

    #[test]
    fn an_arrival_reaches_an_entity_wearing_nothing_at_all() {
        // A hero drawn from unflagged cells is nobody's obstacle and nobody's news — and still
        // wants to hear what runs into it. Wearing nothing must not cost it its slot.
        let mut bare = Thing::at(0.0, 0.0).heeding(WALL.into());
        let mut mover = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        WORLD.step_cast(&mut [bare.as_kinetic(), mover.as_kinetic()], air, flagged);
        assert!(bare.contacts.touches(WALL));
        assert_eq!(
            mover.contacts,
            Contacts::empty(),
            "something wearing nothing was met"
        );
    }

    #[test]
    fn an_arrival_on_a_prop_is_nobody_s_news() {
        // A prop's contacts are never written — the cart drives it and nothing is home to read
        // them — so an arrival on one is dropped, not delivered.
        let mut door = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE).parked();
        let mut mover = Thing::at(12.0, 0.0)
            .wearing(WALL_SPRITE)
            .stopped_by(BitFlags::empty())
            .moving(-6.0, 0.0);
        WORLD.step_cast(&mut [door.as_kinetic(), mover.as_kinetic()], air, flagged);
        assert!(mover.contacts.touches(CRATE));
        assert_eq!(door.contacts, Contacts::empty());
    }

    #[test]
    fn a_meeting_reaches_both_parties_on_a_cast_too_long_to_snapshot() {
        // The same promise down the long-cast fallback, which walks the `dyn` instead of a
        // snapshot and counts its neighbours with the stepped member taken out of the middle —
        // both directions, so the slot arithmetic is pinned on either side of `mine`.
        let mut cast: Vec<Thing> = (0..SNAPSHOT + 2)
            .map(|i| Thing::at(3000.0 + i as f32 * 100.0, 0.0))
            .collect();
        // The first arrives on the last: mover at slot 0, stander past everybody else.
        cast[0] = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        let last = cast.len() - 1;
        cast[last] = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE);
        let mut handed: Vec<&mut dyn Kinetic> = cast.iter_mut().map(Thing::as_kinetic).collect();
        WORLD.step_cast(&mut handed, air, flagged);
        assert!(cast[0].contacts.touches(CRATE));
        assert!(
            cast[last].contacts.touches(WALL),
            "the stander after `mine`"
        );

        // And the other way round: the last arrives on the first.
        let mut cast: Vec<Thing> = (0..SNAPSHOT + 2)
            .map(|i| Thing::at(3000.0 + i as f32 * 100.0, 0.0))
            .collect();
        cast[0] = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE);
        let last = cast.len() - 1;
        cast[last] = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        let mut handed: Vec<&mut dyn Kinetic> = cast.iter_mut().map(Thing::as_kinetic).collect();
        WORLD.step_cast(&mut handed, air, flagged);
        assert!(cast[last].contacts.touches(CRATE));
        assert!(cast[0].contacts.touches(WALL), "the stander before `mine`");
    }

    #[test]
    fn a_cast_at_the_ceiling_of_boxes_over_everything_costs_two_map_s_worths_apiece() {
        // The fence in the collider cuts one sweep to the map; this pins the aggregate a whole
        // call can buy. A cast at the wire's ceiling, every member a rectangle over the entire
        // coordinate space and moving on both axes, is the most host work one step can be asked
        // for: two sweeps a member, each billed for the 128x64 map. No more — the space could
        // name a thousand times as many tiles — and no fewer, so a fence cut wrong cannot pass
        // here by visiting nothing.
        use core::cell::Cell;

        let asked = Cell::new(0u32);
        let counted = |_: i16, _: i16| {
            asked.set(asked.get() + 1);
            BitFlags::empty()
        };
        let mut things: Vec<Thing> = (0..64)
            .map(|_| {
                Thing::at(i16::MIN as f32, i16::MIN as f32)
                    .sized(u16::MAX, u16::MAX)
                    .moving(1.0, 1.0)
            })
            .collect();
        let mut cast: Vec<&mut dyn Kinetic> = things.iter_mut().map(Thing::as_kinetic).collect();
        walled().step_cast(&mut cast, counted, unflagged);

        let map = u32::from(crate::MAP_WIDTH_TILES) * u32::from(crate::MAP_HEIGHT_TILES);
        assert_eq!(asked.get(), 64 * 2 * map);
    }

    #[test]
    fn a_step_moves_the_body_along_the_velocity() {
        let mut mob = Mob::moving(1.5, -2.0);
        step(&mut [mob.as_kinetic()]);
        assert_eq!(mob.body.pos(), (1.5, -2.0));
        assert_eq!(mob.contacts, Contacts::empty());
        step(&mut [mob.as_kinetic()]);
        assert_eq!(mob.body.pos(), (3.0, -4.0));
    }

    #[test]
    fn the_world_s_forces_are_applied_to_every_member_of_the_cast() {
        let mut world: World<2, Gravity> = World::new().with_forces(GRAVITY);
        let one = pebble(&mut world, 0.0, 0.0);
        let two = pebble(&mut world, 40.0, 0.0);
        world.step(&CTX);
        // One update's pull, both in the velocity each kept and in how far each fell.
        for (member, x) in [(one, 0.0), (two, 40.0)] {
            assert_eq!(
                member.velocity(&world),
                Velocity::new(0.0, Gravity::DEFAULT_STRENGTH)
            );
            assert_eq!(member.pos(&world), (x, Gravity::DEFAULT_STRENGTH));
        }

        for _ in 0..1_000 {
            world.step(&CTX);
        }
        assert_eq!(
            one.velocity(&world).dy,
            Gravity::DEFAULT_TERMINAL_VELOCITY,
            "the pull never settled at its terminal velocity"
        );
    }

    #[test]
    fn a_world_owning_no_forces_applies_none() {
        // The weather belongs to the scene, not to the members, so a cast stepped by a world that
        // took none carries on at whatever it was already doing.
        let mut world: World<1> = World::new();
        let drifter = Member::builder(0.0, 0.0, 8, 8)
            .moving(0.0, 1.0)
            .enlist(&mut world)
            .unwrap();
        world.step(&CTX);
        assert_eq!(drifter.velocity(&world), Velocity::new(0.0, 1.0));
        assert_eq!(drifter.pos(&world), (0.0, 1.0));
    }

    #[test]
    fn forces_run_in_the_order_the_tuple_composes_them() {
        const PULL: Gravity = Gravity::new().with_terminal_velocity(f32::MAX);
        const AIR: Atmosphere = Atmosphere::new();

        let (pulled, pulled_first) = one_update_under((PULL, AIR));
        let (aired, aired_first) = one_update_under((AIR, PULL));

        // The drag that runs before the pull has not felt it yet, so the first update — and every
        // one after it — differs by exactly that much.
        assert_eq!(aired_first.velocity(&aired).dy, Gravity::DEFAULT_STRENGTH);
        assert!(
            pulled_first.velocity(&pulled).dy < aired_first.velocity(&aired).dy,
            "the order made no difference: {} against {}",
            pulled_first.velocity(&pulled).dy,
            aired_first.velocity(&aired).dy
        );
    }

    #[test]
    fn mass_is_read_off_the_seat_by_the_forces_the_world_runs() {
        // The same wind on two members that differ in nothing but what they weigh, and it is the
        // world that carries the one to the other.
        let mut world: World<2, Wind> = World::new().with_forces(Wind::new(2.0));
        let light = Member::builder(0.0, 0.0, 8, 8)
            .weighing(0.5)
            .enlist(&mut world)
            .unwrap();
        let heavy = Member::builder(0.0, 40.0, 8, 8)
            .weighing(4.0)
            .enlist(&mut world)
            .unwrap();
        world.step(&CTX);
        assert!(
            light.velocity(&world).dx > heavy.velocity(&world).dx,
            "the mass was not read: {} against {}",
            light.velocity(&world).dx,
            heavy.velocity(&world).dx
        );

        // And it is the seat's own, changed where it lives.
        heavy.set_mass(&mut world, 0.5);
        assert_eq!(heavy.mass(&world), 0.5);
    }

    #[test]
    fn forces_of_different_types_compose_into_one_weather() {
        struct Updraft;

        impl Force for Updraft {
            fn apply(&self, subject: &mut Subject) {
                subject.velocity_mut().dy -= 0.5;
            }
        }

        let (world, member) = one_update_under((GRAVITY, Wind::new(1.0), Updraft));
        assert_eq!(member.velocity(&world).dx, 0.05);
        assert_eq!(member.velocity(&world).dy, Gravity::DEFAULT_STRENGTH - 0.5);
    }

    #[test]
    fn no_force_reaches_a_prop_the_cart_drives_itself() {
        // The pull handed to the step reaches everything but the prop, which is the cart's to
        // move and the world's only to know about.
        let mut world: World<2, Gravity> = World::new().with_forces(GRAVITY);
        let faller = pebble(&mut world, 0.0, 0.0);
        let lift = Member::builder(0.0, 40.0, 8, 8)
            .prop()
            .enlist(&mut world)
            .unwrap();
        for _ in 0..8 {
            world.step(&CTX);
        }
        assert!(faller.pos(&world).1 > 0.0, "the pull missed the faller");
        assert_eq!(lift.pos(&world), (0.0, 40.0), "the prop was moved");
        assert_eq!(lift.velocity(&world), Velocity::default());
    }

    /// One update of `forces` over a single member, and the world it was stepped in.
    fn one_update_under<F: Force>(forces: F) -> (World<1, F>, Member) {
        let mut world: World<1, F> = World::new().with_forces(forces);
        let member = pebble(&mut world, 0.0, 0.0);
        world.step(&CTX);

        (world, member)
    }

    /// A sprite-sized member seated at (`x`, `y`), saying nothing about itself — what most of a
    /// cast is.
    fn pebble<const N: usize, F: Force>(world: &mut World<N, F>, x: f32, y: f32) -> Member {
        Member::builder(x, y, 8, 8).enlist(world).unwrap()
    }

    #[test]
    fn a_member_takes_the_lowest_empty_seat_and_a_full_house_turns_one_away() {
        let mut world: World<3> = World::new();
        let seats: Vec<Member> = (0..3)
            .map(|i| pebble(&mut world, i as f32 * 16.0, 0.0))
            .collect();
        assert_eq!(
            seats.iter().map(|m| m.seat()).collect::<Vec<_>>(),
            [0, 1, 2],
            "the seats were not filled in order"
        );
        assert!(
            Member::builder(0.0, 0.0, 8, 8).enlist(&mut world).is_none(),
            "a fourth member was seated in a world of three"
        );

        // And a seat freed in the middle is the next one filled, so a cast seated in the order the
        // scene works stays in it.
        seats[1].retire(&mut world);
        let understudy = pebble(&mut world, 64.0, 0.0);
        assert_eq!(understudy.seat(), 1);
        assert_eq!(understudy.pos(&world), (64.0, 0.0));
    }

    #[test]
    fn a_member_described_and_never_enlisted_takes_no_seat() {
        // Nothing is claimed until `enlist`: a description dropped on the way there — never
        // enlisted, or unwound out of by a builder that panicked — has touched no world at all,
        // so the next member enlisted still gets seat 0.
        let mut world: World<1> = World::new();
        let _ = Member::builder(10.0, 10.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .flipped(true, true)
            .spanning(2, 2)
            .hidden();

        // The world of one has its one seat still free — and nothing the dropped description
        // wrote is waiting in it for whoever is seated next, the look included.
        let member = Member::builder(0.0, 0.0, 8, 8)
            .enlist(&mut world)
            .expect("the seat was leaked");
        assert_eq!(member.seat(), 0);
        assert!(member.seated(&world));
        assert_eq!(member.pos(&world), (0.0, 0.0));
        assert_eq!(member.sprite(&world), None);
        assert_eq!(member.flip(&world), (false, false));
        assert_eq!(member.span(&world), (1, 1));
        assert!(!member.hidden(&world));
    }

    #[test]
    fn one_description_seats_as_many_members_as_it_is_enlisted() {
        // `MemberBuilder` is `Copy`, so one description seats as many members as a cart enlists
        // from it — a shot fired again and again — and each seat reads back exactly what was
        // described.
        let mut world: World<2> = World::new();
        let description = Member::builder(4.0, 8.0, 8, 8)
            .moving(1.0, -2.0)
            .wearing(CRATE_SPRITE)
            .flipped(true, false);

        let first = description
            .enlist(&mut world)
            .expect("a seat for the first");
        let second = description
            .enlist(&mut world)
            .expect("a seat for the second");
        assert_eq!((first.seat(), second.seat()), (0, 1));

        for member in [first, second] {
            assert_eq!(member.pos(&world), (4.0, 8.0));
            assert_eq!(member.velocity(&world), Velocity::new(1.0, -2.0));
            assert_eq!(member.sprite(&world), Some(CRATE_SPRITE));
            assert_eq!(member.flip(&world), (true, false));
        }
    }

    #[test]
    fn a_bare_description_is_a_whole_member_already() {
        let mut world: World<1> = World::new();
        let bare = Member::builder(4.5, -2.25, 8, 8)
            .enlist(&mut world)
            .unwrap();
        let record = &world.records[bare.seat()];
        // Where it stands, exactly and as it draws: the position floored, as a fresh body's is.
        assert_eq!((record.x, record.y), (4.5, -2.25));
        assert_eq!((record.rx, record.ry), (4, -3));
        // The rectangle over the sprite, and the whole of it.
        assert_eq!((record.bx, record.by, record.bw, record.bh), (4, -3, 8, 8));
        // Standing still, wearing nothing, told about everything, held nowhere, moved by the
        // world.
        assert_eq!((record.dx, record.dy), (0.0, 0.0));
        assert_eq!(record.sprite, wire::UNWORN);
        assert_eq!(record.heeds, BitFlags::<SpriteFlag>::all().bits());
        assert_eq!(record.meta, 0);
        // And of the weight nobody has to think about, with the rectangle over the sprite.
        assert_eq!(bare.mass(&world), 1.0);
        assert_eq!(world.offsets[bare.seat()], (0, 0));
    }

    #[test]
    fn the_scene_s_word_for_wall_is_a_member_s_unless_it_has_one_of_its_own() {
        let mut world: World<3> = World::new().with_solid(WALL);

        // Nothing said: the scene's word, whatever it is.
        let plain = pebble(&mut world, 0.0, 0.0);
        assert_eq!(
            world.records[plain.seat()].solid,
            BitFlags::from(WALL).bits()
        );
        assert_eq!(plain.solid(&world), None);

        // A rule of its own replaces it rather than adding to it — the empty one included.
        let own = Member::builder(0.0, 40.0, 8, 8)
            .stopped_by(CRATE)
            .enlist(&mut world)
            .unwrap();
        assert_eq!(
            world.records[own.seat()].solid,
            BitFlags::from(CRATE).bits()
        );
        let ghost = Member::builder(0.0, 80.0, 8, 8)
            .stopped_by(BitFlags::empty())
            .enlist(&mut world)
            .unwrap();
        assert_eq!(world.records[ghost.seat()].solid, 0);
        assert_eq!(ghost.solid(&world), Some(BitFlags::empty()));
    }

    #[test]
    fn everything_a_description_says_reaches_the_seat_it_fills() {
        const ROOM: Bounds = Bounds::new(-8, 4, 100, 64);

        let mut world: World<1> = World::new();
        let member = Member::builder(10.0, 20.0, 6, 4)
            .moving(1.5, -0.5)
            .wearing(SpriteId(9))
            .heeding(CRATE)
            .confined_to(ROOM)
            .offset(1, 2)
            .prop()
            .weighing(3.0)
            .enlist(&mut world)
            .unwrap();

        let record = &world.records[member.seat()];
        assert_eq!((record.dx, record.dy), (1.5, -0.5));
        assert_eq!(record.sprite, 9);
        assert_eq!(record.heeds, BitFlags::from(CRATE).bits());
        assert_eq!(
            (record.cx, record.cy, record.cw, record.ch),
            (-8, 4, 100, 64)
        );
        assert_eq!(record.meta, wire::PROP | wire::CONFINED);
        // The rectangle sits where the offset put it, and is the size it was given.
        assert_eq!((record.bx, record.by, record.bw, record.bh), (11, 22, 6, 4));
        assert_eq!(member.mass(&world), 3.0);
    }

    #[test]
    fn a_rectangle_at_the_end_of_the_coordinate_space_is_held_rather_than_wrapped() {
        assert_eq!(corner((i16::MAX, i16::MIN), (8, -8)), (i16::MAX, i16::MIN));
        assert_eq!(corner((0, 0), (-4, 6)), (-4, 6));
    }

    #[test]
    #[should_panic(expected = "seat 0 was retired")]
    fn a_retired_member_s_handle_is_nobody() {
        let mut world: World<2> = World::new();
        let member = pebble(&mut world, 0.0, 0.0);
        assert!(member.seated(&world));
        member.retire(&mut world);
        // The asking-first way round, for a cart that would rather not find out the hard way.
        assert!(!member.seated(&world));

        member.pos(&world);
    }

    #[test]
    #[should_panic(expected = "seat 1 was retired")]
    fn a_seat_let_again_does_not_answer_the_last_member_s_handle() {
        let mut world: World<2> = World::new();
        let _ = pebble(&mut world, 0.0, 0.0);
        let gone = pebble(&mut world, 16.0, 0.0);
        gone.retire(&mut world);
        let successor = pebble(&mut world, 32.0, 0.0);
        // The same seat, and not the same member: the handle to whoever left it must not answer
        // for whoever took it.
        assert_eq!(successor.seat(), gone.seat());
        assert_ne!(successor, gone);
        assert_eq!(successor.pos(&world), (32.0, 0.0));

        gone.set_velocity(&mut world, Velocity::new(1.0, 0.0));
    }

    #[test]
    fn an_empty_seat_is_nothing_to_anybody() {
        // The zero-ABI trick, pinned where it has to hold: an empty seat crosses the wire as
        // `wire::VACANT` and is stepped by the *unmodified* engine, which has never heard of
        // vacancy. So the two crates below must meet through the hole between them exactly as
        // they meet with nothing there — the empty seat wears nothing, listens for nothing, is
        // never moved and is never told anything.
        let mut hole = wire::Recast::of(&wire::VACANT);
        let mut left = Thing::at(0.0, 20.0).wearing(CRATE_SPRITE).stopped_by(CRATE);
        let mut right = Thing::at(16.0, 20.0)
            .wearing(CRATE_SPRITE)
            .stopped_by(CRATE);
        for _ in 0..3 {
            left.velocity = Velocity::new(2.0, 0.0);
            right.velocity = Velocity::new(-2.0, 0.0);
            WORLD.step_cast(
                &mut [left.as_kinetic(), &mut hole, right.as_kinetic()],
                air,
                flagged,
            );
        }

        // The very answers the same pair gives with nobody between them.
        assert_eq!(left.body.pos(), (4.0, 20.0));
        assert_eq!(right.body.pos(), (12.0, 20.0));
        assert!(left.contacts.right() && left.contacts.touches(CRATE));
        assert!(right.contacts.left() && right.contacts.touches(CRATE));
        // And the seat itself went nowhere and was told nothing.
        assert_eq!(hole.body().pos(), (0.0, 0.0));
        assert_eq!(*hole.contacts(), Contacts::empty());
    }

    #[test]
    fn a_rectangle_keeps_its_seat_on_the_body_the_step_moved() {
        // The step answers the body and leaves the rectangle's corner alone — the wire's in/out
        // split — so it is the world that has to put it back, every step, offset and all.
        let mut world: World<1> = World::new();
        let inset = Member::builder(0.0, 0.0, 4, 8)
            .offset(2, 0)
            .moving(3.0, 1.0)
            .enlist(&mut world)
            .unwrap();
        assert_eq!(inset.bounds(&world), Bounds::new(2, 0, 4, 8));
        world.step(&CTX);
        assert_eq!(inset.pos(&world), (3.0, 1.0));
        assert_eq!(inset.bounds(&world), Bounds::new(5, 1, 4, 8));

        // And a rectangle re-cut mid-animation is met at its new size from the next step on.
        inset.resize(&mut world, 8, 4);
        inset.set_offset(&mut world, 0, 4);
        assert_eq!(inset.bounds(&world), Bounds::new(3, 5, 8, 4));
    }

    #[test]
    fn a_teleport_re_snaps_the_drawn_pixel_and_takes_the_rectangle_with_it() {
        let mut world: World<1> = World::new();
        let prop = Member::builder(0.0, 0.0, 8, 8)
            .offset(1, -1)
            .prop()
            .enlist(&mut world)
            .unwrap();
        prop.set_pos(&mut world, 40.9, 12.2);
        assert_eq!(prop.pos(&world), (40.9, 12.2));
        assert_eq!(prop.draw_pos(&world), (40, 12));
        assert_eq!(prop.bounds(&world), Bounds::new(41, 11, 8, 8));
    }

    #[test]
    fn what_a_member_is_told_after_it_is_seated_lands_in_its_seat() {
        // The seat is where a member lives, so everything its description said once can be said
        // again — and what is said reaches the very bytes the next step is read out of.
        let mut world: World<1> = World::new().with_solid(WALL);
        let walker = pebble(&mut world, 0.0, 0.0);
        let seat = walker.seat();

        walker.set_sprite(&mut world, Some(CRATE_SPRITE));
        assert_eq!(world.records[seat].sprite, CRATE_SPRITE.0 as u16);
        walker.set_sprite(&mut world, None);
        assert_eq!(world.records[seat].sprite, wire::UNWORN);

        walker.set_heeds(&mut world, CRATE);
        assert_eq!(world.records[seat].heeds, BitFlags::from(CRATE).bits());

        // A rule of its own replaces the scene's; handing it back takes the scene's word as it
        // stands now.
        walker.set_solid(&mut world, Some(CRATE.into()));
        assert_eq!(world.records[seat].solid, BitFlags::from(CRATE).bits());
        walker.set_solid(&mut world, None);
        assert_eq!(world.records[seat].solid, BitFlags::from(WALL).bits());

        walker.set_confines(&mut world, Some(Bounds::new(-8, 4, 100, 64)));
        let record = &world.records[seat];
        assert_eq!(record.meta & wire::CONFINED, wire::CONFINED);
        assert_eq!(
            (record.cx, record.cy, record.cw, record.ch),
            (-8, 4, 100, 64)
        );
        walker.set_confines(&mut world, None);
        assert_eq!(world.records[seat].meta & wire::CONFINED, 0);
    }

    #[test]
    fn what_a_member_is_described_as_reads_back_through_its_handle() {
        // The world owns the description, so the cart asks rather than remembers: whoever draws
        // the member and whoever wonders what everybody else meets in it read the same answer,
        // through the same handle — once it is enlisted, and after every setter.
        let mut world: World<1> = World::new().with_solid(WALL);
        let walker = pebble(&mut world, 0.0, 0.0);

        assert_eq!(walker.sprite(&world), None);
        assert_eq!(walker.solid(&world), None, "no rule of its own yet");
        assert_eq!(walker.heeds(&world), BitFlags::all());
        assert_eq!(walker.confines(&world), None);

        walker.set_sprite(&mut world, Some(CRATE_SPRITE));
        walker.set_solid(&mut world, Some(WALL | CRATE));
        walker.set_heeds(&mut world, CRATE);
        walker.set_confines(&mut world, Some(Bounds::new(-8, 4, 100, 64)));
        assert_eq!(walker.sprite(&world), Some(CRATE_SPRITE));
        assert_eq!(walker.solid(&world), Some(WALL | CRATE));
        assert_eq!(walker.heeds(&world), CRATE.into());
        assert_eq!(walker.confines(&world), Some(Bounds::new(-8, 4, 100, 64)));

        // Handed back to the scene's word, the member has no rule of its own to report — the
        // scene's word is not its answer, however much it is stopped by it.
        walker.set_solid(&mut world, None);
        assert_eq!(walker.solid(&world), None);
    }

    #[test]
    fn the_scene_s_word_for_wall_reaches_everybody_who_goes_by_it() {
        // Said after the cast is seated, which a level that changes its mind does: the member with
        // no rule of its own takes the new word, and the one that named the empty rule keeps it.
        let mut world: World<2> = World::new();
        let walker = pebble(&mut world, 0.0, 0.0);
        let ghost = Member::builder(0.0, 40.0, 8, 8)
            .stopped_by(BitFlags::empty())
            .enlist(&mut world)
            .unwrap();
        let mut world = world.with_solid(WALL);
        assert_eq!(
            world.records[walker.seat()].solid,
            BitFlags::from(WALL).bits()
        );
        assert_eq!(world.records[ghost.seat()].solid, 0);

        // And said in place, which is how a world that was placed rather than built says it: the
        // same word reaching the same seats.
        world.declare_solid(CRATE);
        assert_eq!(
            world.records[walker.seat()].solid,
            BitFlags::from(CRATE).bits()
        );
        assert_eq!(world.records[ghost.seat()].solid, 0);
    }

    #[test]
    #[should_panic(expected = "seat 255 is no seat of this world")]
    fn a_handle_to_nobody_is_nobody_in_any_world() {
        // What a cart's placed state holds for an actor it has not enlisted yet: a seat no world
        // has. Asking is the same bug as asking about somebody who left, and says which it was.
        let mut world: World<2> = World::new();
        assert!(!Member::NOBODY.seated(&world));
        let _ = pebble(&mut world, 0.0, 0.0);
        assert!(!Member::NOBODY.seated(&world));

        Member::NOBODY.pos(&world);
    }

    #[test]
    fn a_whole_world_can_be_spelled_out_as_a_constant() {
        // What the `game!` preset needs of this module: a world, weather and all, placed by the
        // loader rather than built by anything that runs. A cast is seated in `Game::boot`, and
        // the scene's word for a wall is said there too — the one thing the builder cannot be.
        const LEVEL: World<2, Gravity> = World::new().with_forces(Gravity::new());
        // And it really does initialize a `static`, which is where a cart's scene lives.
        static SCENE: World<2, Gravity> = LEVEL;
        assert_eq!(SCENE.high_water(), 0);

        let mut world = LEVEL;
        world.declare_solid(WALL);
        let member = pebble(&mut world, 0.0, 0.0);
        assert_eq!(
            world.records[member.seat()].solid,
            BitFlags::from(WALL).bits()
        );
    }

    #[test]
    fn a_member_is_held_inside_the_limits_it_is_confined_to() {
        let mut world: World<1> = World::new();
        let walker = Member::builder(-4.0, 8.0, 8, 8)
            .moving(-2.0, 0.5)
            .confined_to(Bounds::screen())
            .enlist(&mut world)
            .unwrap();
        world.step(&CTX);
        assert!(walker.contacts(&world).left());
        assert_eq!(walker.pos(&world), (0.0, 8.5));

        // And limits taken away are a member let go.
        walker.set_confines(&mut world, None);
        walker.set_velocity(&mut world, Velocity::new(-2.0, 0.0));
        world.step(&CTX);
        assert_eq!(walker.pos(&world), (-2.0, 8.5));
        assert_eq!(walker.contacts(&world), Contacts::empty());
    }

    #[test]
    fn a_bare_member_is_drawn_from_one_cell_unflipped_and_shown() {
        let mut world: World<1> = World::new();
        let bare = pebble(&mut world, 4.5, -2.25);
        assert_eq!(bare.flip(&world), (false, false));
        assert_eq!(bare.span(&world), (1, 1));
        assert!(!bare.hidden(&world));
        let record = &world.records[bare.seat()];
        assert_eq!(
            record.meta & (wire::FLIP_X | wire::FLIP_Y | wire::HIDDEN),
            0
        );
        assert_eq!(record.span, 0);

        // And it wears nothing, so for all its look there is nothing of it to draw.
        let taken = &world.records[..world.high_water()];
        assert_eq!(wire::looks(taken, BitFlags::empty(), unflagged).count(), 0);
    }

    #[test]
    fn everything_a_description_says_about_the_look_reaches_the_seat() {
        let mut world: World<1> = World::new();
        let member = Member::builder(0.0, 0.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .flipped(true, false)
            .spanning(2, 3)
            .hidden()
            .enlist(&mut world)
            .unwrap();

        let record = &world.records[member.seat()];
        assert_eq!(record.meta, wire::FLIP_X | wire::HIDDEN);
        // Two across less one in the low nibble, three down less one in the high.
        assert_eq!(record.span, 0x21);
        assert_eq!(member.flip(&world), (true, false));
        assert_eq!(member.span(&world), (2, 3));
        assert!(member.hidden(&world));
    }

    #[test]
    fn what_a_member_looks_like_can_be_changed_after_it_is_seated() {
        // A prop held to the screen, so the bits the step reads are set all along and every
        // change to the look has to leave them standing.
        let mut world: World<1> = World::new();
        let door = Member::builder(0.0, 0.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .confined_to(Bounds::screen())
            .prop()
            .enlist(&mut world)
            .unwrap();
        let seat = door.seat();
        let steps_by = wire::PROP | wire::CONFINED;

        door.set_flip(&mut world, true, false);
        assert_eq!(world.records[seat].meta, steps_by | wire::FLIP_X);
        assert_eq!(door.flip(&world), (true, false));
        // Turned the other way round: across cleared, up and down set.
        door.set_flip(&mut world, false, true);
        assert_eq!(world.records[seat].meta, steps_by | wire::FLIP_Y);
        assert_eq!(door.flip(&world), (false, true));

        door.set_span(&mut world, 16, 16);
        assert_eq!(world.records[seat].span, 0xff);
        assert_eq!(door.span(&world), (16, 16));

        door.set_hidden(&mut world, true);
        assert_eq!(
            world.records[seat].meta,
            steps_by | wire::FLIP_Y | wire::HIDDEN
        );
        assert!(door.hidden(&world));
        assert_eq!(world.records[seat].look(), None);
        door.set_hidden(&mut world, false);
        assert_eq!(world.records[seat].meta, steps_by | wire::FLIP_Y);
        assert!(!door.hidden(&world));
        assert!(
            world.records[seat].look().is_some(),
            "it was never shown again"
        );
    }

    #[test]
    fn the_cast_is_drawn_where_it_stands_in_seat_order() {
        // Three members wearing three cells — one plain, one hidden, one turned round and drawn
        // from a block — and a seat retired from under the middle of them.
        let mut world: World<4> = World::new();
        let plain = Member::builder(3.5, 7.25, 8, 8)
            .wearing(SpriteId(5))
            .enlist(&mut world)
            .unwrap();
        let gone = Member::builder(20.0, 0.0, 8, 8)
            .wearing(SpriteId(6))
            .enlist(&mut world)
            .unwrap();
        let _ghost = Member::builder(40.0, 0.0, 8, 8)
            .wearing(WALL_SPRITE)
            .hidden()
            .enlist(&mut world)
            .unwrap();
        let turned = Member::builder(60.5, 30.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .flipped(true, false)
            .spanning(2, 3)
            .enlist(&mut world)
            .unwrap();
        gone.retire(&mut world);

        // Only the two that show, in the order they are seated, each at the pixel it draws at.
        let taken = &world.records[..world.high_water()];
        let (plain_x, plain_y) = plain.draw_pos(&world);
        let (turned_x, turned_y) = turned.draw_pos(&world);
        let shown: Vec<wire::Look> = wire::looks(taken, BitFlags::empty(), unflagged).collect();
        assert_eq!(
            shown,
            [
                wire::Look {
                    sprite: SpriteId(5),
                    x: plain_x,
                    y: plain_y,
                    width: 8,
                    height: 8,
                    flip_x: false,
                    flip_y: false,
                },
                wire::Look {
                    sprite: CRATE_SPRITE,
                    x: turned_x,
                    y: turned_y,
                    width: 16,
                    height: 24,
                    flip_x: true,
                    flip_y: false,
                },
            ]
        );

        // A layer draws only the member whose cell carries it: not the one wearing a flagless
        // cell, and not the hidden one, whatever its own cell carries.
        let layered: Vec<SpriteId> = wire::looks(taken, WALL | CRATE, flagged)
            .map(|look| look.sprite)
            .collect();
        assert_eq!(layered, [CRATE_SPRITE]);
    }

    #[test]
    fn a_step_moves_the_look_with_the_body_and_leaves_the_rest_of_it_alone() {
        let mut world: World<1> = World::new();
        let runner = Member::builder(0.0, 0.0, 8, 8)
            .moving(3.0, 2.0)
            .wearing(CRATE_SPRITE)
            .flipped(true, true)
            .spanning(2, 1)
            .hidden()
            .enlist(&mut world)
            .unwrap();
        world.step(&CTX);
        assert_eq!(runner.draw_pos(&world), (3, 2), "the step never moved it");

        // Nothing of the look went into the step, and nothing of it came back changed.
        assert_eq!(runner.flip(&world), (true, true));
        assert_eq!(runner.span(&world), (2, 1));
        assert!(runner.hidden(&world));

        // And shown again, it is drawn where the step left the body.
        runner.set_hidden(&mut world, false);
        let look = world.records[runner.seat()]
            .look()
            .expect("a member wearing a cell and shown has a look");
        assert_eq!((look.x, look.y), runner.draw_pos(&world));
    }

    #[test]
    fn a_retired_seat_shows_nothing_and_the_next_member_in_it_starts_plain() {
        let mut world: World<1> = World::new();
        let showy = Member::builder(0.0, 0.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .flipped(true, true)
            .spanning(3, 2)
            .enlist(&mut world)
            .unwrap();
        let seat = showy.seat();
        assert!(world.records[seat].look().is_some());
        showy.retire(&mut world);
        assert_eq!(world.records[seat].look(), None, "a retired seat was drawn");

        // Whoever is seated there next wears a cell of its own and nothing else of the last one's.
        let successor = Member::builder(8.0, 16.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .enlist(&mut world)
            .unwrap();
        assert_eq!(successor.flip(&world), (false, false));
        assert_eq!(successor.span(&world), (1, 1));
        assert!(!successor.hidden(&world));
        assert_eq!(
            world.records[seat].look(),
            Some(wire::Look {
                sprite: CRATE_SPRITE,
                x: 8,
                y: 16,
                width: 8,
                height: 8,
                flip_x: false,
                flip_y: false,
            })
        );
    }

    #[test]
    #[should_panic(expected = "one to sixteen cells")]
    fn a_member_drawn_from_no_cells_at_all_is_refused_loudly() {
        let mut world: World<1> = World::new();
        let _ = Member::builder(0.0, 0.0, 8, 8)
            .spanning(0, 1)
            .enlist(&mut world)
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "one to sixteen cells")]
    fn a_member_drawn_from_past_the_edge_of_the_sheet_is_refused_loudly() {
        let mut world: World<1> = World::new();
        let member = pebble(&mut world, 0.0, 0.0);
        member.set_span(&mut world, 1, 17);
    }

    #[test]
    fn a_draw_walks_the_seats_natively_whatever_the_layers() {
        // Off the console the draw walks the very looks the console would and blits each through
        // the ABI's stubs: nothing to see, and nothing on the way that may panic — not a gap in
        // the cast, not the widest block there is, and not a layer.
        let mut world: World<3> = World::new();
        let _shown = Member::builder(0.0, 0.0, 8, 8)
            .wearing(CRATE_SPRITE)
            .enlist(&mut world)
            .unwrap();
        let gone = pebble(&mut world, 16.0, 0.0);
        let _sheet = Member::builder(32.0, 0.0, 8, 8)
            .wearing(WALL_SPRITE)
            .flipped(true, true)
            .spanning(16, 16)
            .enlist(&mut world)
            .unwrap();
        gone.retire(&mut world);

        let mut gfx = Graphics { _private: () };
        world.draw(&mut gfx, BitFlags::empty());
        world.draw(&mut gfx, WALL);
    }

    #[test]
    fn an_entity_stopped_by_nothing_steps_straight_through_everything() {
        // Saying nothing is the default, and this world says nothing either: nothing anywhere is
        // a wall to it, and it is still told what it went through — a sensor, and nothing to opt
        // into for it.
        let mut sensor = Thing::at(0.0, 0.0).moving(4.0, 0.0);
        let mut wall = Thing::at(8.0, 0.0).wearing(WALL_SPRITE);
        assert!(sensor.solid().is_none());
        stepped(&mut [sensor.as_kinetic(), wall.as_kinetic()]);
        assert_eq!(sensor.body.pos(), (4.0, 0.0));
        assert!(sensor.contacts.touches(WALL) && sensor.contacts.sides().is_empty());
    }

    #[test]
    fn an_entity_alone_in_an_empty_world_meets_nothing() {
        let mut walker = Thing::at(8.0, 8.0).stopped_by(WALL).moving(1.0, 1.0);
        stepped(&mut [walker.as_kinetic()]);
        assert_eq!(walker.contacts, Contacts::empty());
        assert_eq!(walker.body.pos(), (9.0, 9.0));
        assert_eq!(walker.velocity, Velocity::new(1.0, 1.0));
    }

    #[test]
    fn the_map_stops_an_entity_that_answers_to_walls() {
        // The tiles are the level standing still, and the world asks them exactly as it asks the
        // cast: a floor along row 2, and a fall that lands on it.
        let mut faller = Thing::at(0.0, 7.0).stopped_by(WALL).moving(0.0, 4.0);
        WORLD.step_cast(
            &mut [faller.as_kinetic()],
            map(&["....", "....", "####"]),
            unflagged,
        );
        assert!(faller.contacts.below() && faller.contacts.touches(WALL));
        assert_eq!(faller.velocity.dy, 0.0);
    }

    #[test]
    fn the_world_s_solid_stops_every_entity_that_has_no_rule_of_its_own() {
        // The scene's word for wall, said once on the world. The walker says nothing about what
        // stops it and is stopped all the same — by the tile, and by the cast member wearing the
        // same flag.
        let mut walker = Thing::at(0.0, 0.0).moving(4.0, 0.0);
        assert_eq!(walker.solid(), None);
        walled().step_cast(&mut [walker.as_kinetic()], map(&[".#"]), flagged);
        assert_eq!(walker.body.pos(), (0.0, 0.0));
        assert!(walker.contacts.right() && walker.contacts.touches(WALL));

        let mut lift = Thing::at(30.0, 0.0).wearing(WALL_SPRITE).parked();
        let mut other = Thing::at(20.0, 0.0).moving(4.0, 0.0);
        walled().step_cast(&mut [lift.as_kinetic(), other.as_kinetic()], air, flagged);
        assert_eq!(other.body.pos(), (20.0, 0.0), "it walked into the lift");
        assert!(other.contacts.right() && other.contacts.touches(WALL));
    }

    #[test]
    fn an_entity_with_rules_of_its_own_replaces_the_world_s_rather_than_adds_to_them() {
        // The world calls walls solid; this one answers with crates alone. The wall tile lets it
        // through — and is still reported, heeding being another question — and the crate does
        // not.
        let mut walker = Thing::at(0.0, 0.0).stopped_by(CRATE).moving(4.0, 0.0);
        walled().step_cast(&mut [walker.as_kinetic()], map(&[".#"]), flagged);
        assert_eq!(walker.body.pos(), (4.0, 0.0), "the world's wall stopped it");
        assert!(walker.contacts.touches(WALL) && walker.contacts.sides().is_empty());

        // And the emptiest rule of all: stopped by nothing whatever the scene declares — the
        // ghost an entity can only be by saying so itself now that the world has a word.
        let mut ghost = Thing::at(0.0, 0.0)
            .stopped_by(BitFlags::empty())
            .moving(4.0, 0.0);
        walled().step_cast(&mut [ghost.as_kinetic()], map(&[".#"]), flagged);
        assert_eq!(ghost.body.pos(), (4.0, 0.0));
        assert!(ghost.contacts.touches(WALL) && ghost.contacts.sides().is_empty());
    }

    #[test]
    fn an_entity_is_stopped_by_the_cast_member_in_its_way() {
        // A crate standing still and a walker with `CRATE` in its `solid`: the crate is a wall
        // wherever it happens to be, and nothing about it was handed over to say so.
        let mut walker = Thing::at(0.0, 0.0).stopped_by(CRATE).moving(4.0, 0.0);
        let mut boxed = Thing::at(10.0, 0.0).wearing(CRATE_SPRITE);
        stepped(&mut [walker.as_kinetic(), boxed.as_kinetic()]);
        assert_eq!(walker.body.pos(), (0.0, 0.0), "it walked through the crate");
        assert!(walker.contacts.right() && walker.contacts.touches(CRATE));
        // And the crate, which calls nothing solid, was neither stopped nor moved.
        assert_eq!(boxed.body.pos(), (10.0, 0.0));
    }

    #[test]
    fn two_entities_of_one_kind_stop_each_other_and_neither_is_its_own_wall() {
        // The crates, which flags alone could never manage: one sprite, one flag, and that flag in
        // both their `solid`. The world knows who is who — an entity is simply not in its own
        // walk — so there is nothing to declare and no ghost to leave out.
        let mut left = Thing::at(0.0, 20.0)
            .wearing(CRATE_SPRITE)
            .stopped_by(CRATE)
            .moving(2.0, 0.0);
        let mut right = Thing::at(16.0, 20.0)
            .wearing(CRATE_SPRITE)
            .stopped_by(CRATE)
            .moving(-2.0, 0.0);

        // Two updates close the eight pixels of daylight between them — two pixels each an update
        // — and leave the pair flush, 4 to 11 and 12 to 19.
        for _ in 0..2 {
            left.velocity = Velocity::new(2.0, 0.0);
            right.velocity = Velocity::new(-2.0, 0.0);
            stepped(&mut [left.as_kinetic(), right.as_kinetic()]);
        }
        assert_eq!(left.body.pos(), (4.0, 20.0));
        assert_eq!(right.body.pos(), (12.0, 20.0));
        assert_eq!(left.contacts, Contacts::empty());
        assert_eq!(right.contacts, Contacts::empty());

        // No update after that moves either of them: two more pixels would put each inside the
        // other. Both are stopped, both are told what they met, and neither has been shoved off
        // its own row — which is what an entity mistaken for itself would be, every update, for
        // ever.
        for _ in 0..4 {
            left.velocity = Velocity::new(2.0, 0.0);
            right.velocity = Velocity::new(-2.0, 0.0);
            stepped(&mut [left.as_kinetic(), right.as_kinetic()]);
        }
        assert_eq!(left.body.pos(), (4.0, 20.0));
        assert_eq!(right.body.pos(), (12.0, 20.0));
        assert!(left.contacts.right() && left.contacts.touches(CRATE));
        assert!(right.contacts.left() && right.contacts.touches(CRATE));
    }

    #[test]
    fn a_cast_too_long_to_snapshot_is_answered_exactly_as_a_short_one() {
        // The crates above, run three times over in casts of three lengths: one the snapshot
        // holds, one exactly filling it, and one past it — which is walked through the cast's
        // `dyn` instead. The padding wears nothing and stops at nothing, so it is nothing to
        // anybody and the only thing it changes is how the neighbours are read. All three must
        // agree to the pixel, or the fallback is a different world.
        let snapshotted = crates_meeting(0);
        let brimming = crates_meeting(SNAPSHOT - 2);
        let walked = crates_meeting(SNAPSHOT);
        assert_eq!(snapshotted, brimming, "the full snapshot disagreed");
        assert_eq!(snapshotted, walked, "the fallback disagreed");

        // And it is the right answer rather than three matching wrong ones.
        assert_eq!(snapshotted.0, (4.0, 20.0));
        assert_eq!(snapshotted.1, (12.0, 20.0));
        assert!(snapshotted.2.right() && snapshotted.2.touches(CRATE));
        assert!(snapshotted.3.left() && snapshotted.3.touches(CRATE));
    }

    /// The two crates of the test above, stepped three times in a cast padded out with `padding`
    /// entities that carry nothing: where the pair ended up, and what each was told.
    fn crates_meeting(padding: usize) -> ((f32, f32), (f32, f32), Contacts, Contacts) {
        let mut left = Thing::at(0.0, 20.0).wearing(CRATE_SPRITE).stopped_by(CRATE);
        let mut right = Thing::at(16.0, 20.0)
            .wearing(CRATE_SPRITE)
            .stopped_by(CRATE);
        // Well away from the crates and wearing nothing, so the only thing the padding does is
        // make the cast longer.
        let mut crowd: Vec<Thing> = (0..padding).map(|_| Thing::at(100.0, 100.0)).collect();

        for _ in 0..3 {
            left.velocity = Velocity::new(2.0, 0.0);
            right.velocity = Velocity::new(-2.0, 0.0);
            let mut cast: Vec<&mut dyn Kinetic> = Vec::with_capacity(padding + 2);
            cast.push(left.as_kinetic());
            cast.push(right.as_kinetic());
            cast.extend(crowd.iter_mut().map(Thing::as_kinetic));
            WORLD.step_cast(&mut cast, air, flagged);
        }

        (
            left.body.pos(),
            right.body.pos(),
            left.contacts,
            right.contacts,
        )
    }

    #[test]
    fn an_entity_of_a_kind_all_on_its_own_is_never_stopped_by_itself() {
        // The same crate with nobody else in the world: its own flag is solid to it and its own
        // sprite is what it wears, and it walks and falls exactly as if it carried nothing.
        for velocity in [
            Velocity::new(2.0, 0.0),
            Velocity::new(0.0, 2.0),
            Velocity::new(-2.0, -2.0),
        ] {
            let mut boxed = Thing::at(8.0, 8.0)
                .wearing(CRATE_SPRITE)
                .stopped_by(CRATE)
                .moving(velocity.dx, velocity.dy);
            stepped(&mut [boxed.as_kinetic()]);
            assert_eq!(
                boxed.body.pos(),
                (8.0 + velocity.dx, 8.0 + velocity.dy),
                "{velocity:?} ran into itself"
            );
            assert_eq!(boxed.contacts, Contacts::empty(), "{velocity:?} met itself");
        }
    }

    #[test]
    fn an_entity_that_wears_nothing_is_there_for_nobody() {
        // The default: it is stopped by the cast, and the cast is never stopped by it — nor even
        // told about it. A hero drawn from unflagged cells, and everything a cart would rather
        // handle by holding it than by flagging it.
        let mut ghost = Thing::at(0.0, 0.0).stopped_by(CRATE).moving(4.0, 0.0);
        let mut boxed = Thing::at(10.0, 0.0)
            .wearing(CRATE_SPRITE)
            .stopped_by(CRATE)
            .moving(-4.0, 0.0);
        assert_eq!(ghost.sprite(), None);
        stepped(&mut [ghost.as_kinetic(), boxed.as_kinetic()]);

        // The one that wears nothing was stopped by the crate.
        assert_eq!(ghost.body.pos(), (0.0, 0.0));
        assert!(ghost.contacts.right() && ghost.contacts.touches(CRATE));
        // And the crate walked straight through where it stands, having been told nothing at all.
        assert_eq!(boxed.body.pos(), (6.0, 0.0));
        assert_eq!(boxed.contacts, Contacts::empty());
    }

    #[test]
    fn a_lift_stepped_before_its_rider_carries_it_the_same_update() {
        // Twenty updates of the case a tile can never make, and of the ordering that makes the
        // world worth having. The lift rises a pixel an update and the rider stands flush on it,
        // so every update begins with the lift already inside the rider: the push puts the rider
        // back on top and reports it as standing there. The lift is stepped first, so what the
        // rider meets is where the lift is *now* — no lag, and no falling through it.
        let mut lift = Thing::at(0.0, 32.0)
            .sized(24, 8)
            .wearing(WALL_SPRITE)
            .moving(0.0, -1.0);
        let mut rider = Thing::at(0.0, 24.0).stopped_by(WALL);

        for update in 1..=20 {
            lift.velocity = Velocity::new(0.0, -1.0);
            rider.velocity = Velocity::new(0.0, 1.0);
            stepped(&mut [lift.as_kinetic(), rider.as_kinetic()]);

            assert!(
                rider.contacts.below(),
                "the rider let go on update {update}"
            );
            assert!(rider.contacts.touches(WALL));
            assert_eq!(
                rider.body.draw_y() + 8,
                lift.body.draw_y(),
                "the rider is not standing on the lift on update {update}"
            );
        }
    }

    #[test]
    fn a_lift_stepped_after_its_rider_is_a_frame_behind() {
        // The same scene with the cast the other way round, so that what the ordering buys is on
        // the record rather than merely claimed. The rider is stepped first and meets the lift
        // where it was when the update began — a pixel below where it ends up — so it spends every
        // update being caught up with rather than carried.
        let mut lift = Thing::at(0.0, 32.0)
            .sized(24, 8)
            .wearing(WALL_SPRITE)
            .moving(0.0, -1.0);
        let mut rider = Thing::at(0.0, 24.0).stopped_by(WALL);

        for _ in 0..4 {
            lift.velocity = Velocity::new(0.0, -1.0);
            rider.velocity = Velocity::new(0.0, 1.0);
            stepped(&mut [rider.as_kinetic(), lift.as_kinetic()]);
        }
        assert!(
            rider.body.draw_y() + 8 > lift.body.draw_y(),
            "the rider kept up with a lift stepped after it"
        );
    }

    #[test]
    fn an_entity_with_no_rectangle_at_all_is_only_moved() {
        // A hitbox switched off — the frames something is invulnerable, a blast that has shrunk to
        // nothing. There is nothing to resolve, so nothing resolves it, and it travels.
        let mut nothing = Thing::at(0.0, 0.0)
            .sized(0, 8)
            .stopped_by(WALL)
            .moving(4.0, 0.0);
        let mut wall = Thing::at(2.0, 0.0).wearing(WALL_SPRITE);
        stepped(&mut [nothing.as_kinetic(), wall.as_kinetic()]);
        assert_eq!(nothing.body.pos(), (4.0, 0.0));
        assert_eq!(nothing.contacts, Contacts::empty());
    }

    #[test]
    fn an_entity_that_confines_nothing_is_free_to_walk_off_the_map() {
        // The default, and what a bullet or a spent enemy wants: nothing holds it anywhere, so
        // it steps clean off the screen and goes on going.
        let mut walker = Thing::at(120.0, 120.0).stopped_by(WALL).moving(4.0, 4.0);
        assert_eq!(walker.confines(), None);
        for _ in 0..4 {
            walker.velocity = Velocity::new(4.0, 4.0);
            stepped(&mut [walker.as_kinetic()]);
            assert_eq!(walker.contacts, Contacts::empty());
        }
        assert_eq!(walker.body.pos(), (136.0, 136.0));
        assert!(!walker.bounds().on_screen(), "something held it back");
    }

    #[test]
    fn an_entity_is_held_inside_the_limits_it_confines_itself_to() {
        // Off the left edge and still travelling: the step takes it further out, then puts it
        // back against the edge, and the speed that took it there is gone.
        let mut walker = confined(-4.0, 8.0).moving(-2.0, 0.5);
        stepped(&mut [walker.as_kinetic()]);
        let held = walker.contacts;
        assert!(held.left() && !held.right() && !held.above() && !held.below());
        assert_eq!(walker.body.pos(), (0.0, 8.5));
        assert_eq!(walker.velocity, Velocity::new(0.0, 0.5));

        // And the far edges, which the entity's own size is taken off: the walker is a sprite
        // square, and it is held that far short of each.
        let mut walker = confined(200.0, 200.0).moving(1.0, 4.0);
        stepped(&mut [walker.as_kinetic()]);
        assert!(walker.contacts.right() && walker.contacts.below());
        assert_eq!(walker.body.pos(), (120.0, 120.0));
        assert_eq!(walker.velocity, Velocity::default());
    }

    #[test]
    fn an_entity_is_held_at_the_top_edge_and_reports_it() {
        // The one edge the case above does not reach, and the one whose contact a platformer
        // must not confuse with the floor.
        let mut walker = confined(8.0, -4.0).moving(0.5, -2.0);
        stepped(&mut [walker.as_kinetic()]);
        let held = walker.contacts;
        assert!(held.above() && !held.below() && !held.left() && !held.right());
        assert_eq!(walker.body.pos(), (8.5, 0.0));
        assert_eq!(walker.velocity, Velocity::new(0.5, 0.0));
    }

    #[test]
    fn an_entity_within_the_limits_is_left_alone() {
        let mut walker = confined(64.0, 64.0).moving(0.5, 0.5);
        stepped(&mut [walker.as_kinetic()]);
        assert_eq!(walker.contacts, Contacts::empty());
        assert_eq!(walker.body.pos(), (64.5, 64.5));
        assert_eq!(walker.velocity, Velocity::new(0.5, 0.5));
    }

    #[test]
    fn an_entity_flush_against_an_edge_is_not_held() {
        // Exactly at each far edge, which is inside: nothing was stopped, so nothing is reported
        // — a cart reading `below` as *grounded* must not get one from merely being there.
        for (x, y) in [(0.0, 0.0), (120.0, 120.0)] {
            let mut walker = confined(x, y);
            stepped(&mut [walker.as_kinetic()]);
            assert_eq!(
                walker.contacts,
                Contacts::empty(),
                "held at ({x}, {y}), which is inside the screen"
            );
            assert_eq!(walker.body.pos(), (x, y));
        }
    }

    #[test]
    fn an_entity_inside_the_limits_keeps_the_pixel_it_draws_at() {
        // `set_pos` re-snaps the drawn pixel, so a hold that fired on an entity that never left
        // would throw away the coherent step `Body` is holding back — the shimmer it exists to
        // stop.
        let mut walker = confined(64.0, 64.0);
        for _ in 0..3 {
            walker.velocity = Velocity::new(0.5, 0.4);
            stepped(&mut [walker.as_kinetic()]);
        }
        // Standing still from here, so the only thing left that could move the drawn pixel is
        // the hold at the end of the step.
        walker.velocity = Velocity::default();
        let drawn = walker.body.draw_pos();
        assert_ne!(drawn.1, walker.body.y() as i16, "expected a held-back row");
        stepped(&mut [walker.as_kinetic()]);
        assert_eq!(walker.contacts, Contacts::empty());
        assert_eq!(walker.body.draw_pos(), drawn);
    }

    #[test]
    fn speed_carrying_an_entity_back_inside_is_not_spent() {
        // Something spawned off the edge and flying in. It is put where it belongs, but the
        // velocity bringing it home is not the velocity that took it out there.
        let mut walker = confined(140.0, 8.0).moving(-1.0, 0.0);
        stepped(&mut [walker.as_kinetic()]);
        assert!(walker.contacts.right());
        assert_eq!(walker.body.pos(), (120.0, 8.0));
        assert_eq!(walker.velocity, Velocity::new(-1.0, 0.0));
    }

    #[test]
    fn limits_that_do_not_start_at_the_origin_hold_at_their_own_edges() {
        // A room, rather than the screen: the near edges are the ones that are easy to write as
        // a bare zero and never notice.
        let room = Bounds::new(-64, 32, 32, 48);
        for (start, expected, side) in [
            ((-100.0, 40.0), (-64.0, 40.0), Contact::Left),
            ((0.0, 40.0), (-40.0, 40.0), Contact::Right),
            ((-50.0, 0.0), (-50.0, 32.0), Contact::Above),
            ((-50.0, 100.0), (-50.0, 72.0), Contact::Below),
        ] {
            let mut walker = Thing::at(start.0, start.1).within(room);
            stepped(&mut [walker.as_kinetic()]);
            assert_eq!(walker.contacts, side.into(), "from {start:?}");
            assert_eq!(walker.body.pos(), expected, "from {start:?}");
        }
    }

    #[test]
    fn limits_too_small_to_hold_the_entity_do_not_throw_it_out() {
        // A rectangle with no room to fit. Holding it against the far edge would put it further
        // outside than it started, so it is held against the near one and stays put after that.
        let cramped = Bounds::new(0, 0, 4, 4);
        let mut walker = Thing::at(8.0, 8.0).within(cramped);
        stepped(&mut [walker.as_kinetic()]);
        assert!(walker.contacts.right());
        assert_eq!(walker.body.pos(), (0.0, 0.0));

        // And the next update agrees with the one before rather than pushing it back the other
        // way, which is what an unclamped far edge would do, once per update, for ever.
        stepped(&mut [walker.as_kinetic()]);
        assert_eq!(walker.contacts, Contacts::empty());
        assert_eq!(walker.body.pos(), (0.0, 0.0));
    }

    #[test]
    fn no_limits_anywhere_can_be_made_to_hold_an_entity_badly() {
        // `Bounds` has no size to get wrong, so any rectangle at all can be confined to. Nothing
        // may panic — in debug, where the arithmetic that would wrap panics instead — and whatever
        // comes back, an update must agree with the one before rather than shunt the entity back
        // the other way for ever.
        const CORNERS: [i16; 6] = [i16::MIN, -129, -1, 0, 127, i16::MAX - 1];
        const SIDES: [u16; 6] = [0, 1, 8, 128, 32768, u16::MAX];
        for &x in &CORNERS {
            for &y in &CORNERS {
                for &width in &SIDES {
                    for &height in &SIDES {
                        let limits = Bounds::new(x, y, width, height);
                        for start in [(0.0, 0.0), (-300.0, 60.0), (300.0, -60.0)] {
                            let mut walker =
                                Thing::at(start.0, start.1).within(limits).moving(1.0, 1.0);
                            stepped(&mut [walker.as_kinetic()]);
                            // Standing still from here, so what the two updates compare is two
                            // holds rather than two moves.
                            walker.velocity = Velocity::default();
                            stepped(&mut [walker.as_kinetic()]);
                            let settled = walker.body.pos();
                            stepped(&mut [walker.as_kinetic()]);
                            assert_eq!(
                                walker.body.pos(),
                                settled,
                                "{limits:?} could not settle an entity from {start:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn limits_at_the_end_of_the_coordinate_space_do_not_overflow() {
        // `Bounds` saturates its far edges; taking the entity's size off them must too. Limits
        // smaller than the walker at the very start of the space is where the subtraction runs
        // off the end of `i16` — a panic in debug, and a wrapped clamp in release.
        let mut near = Thing::at(0.0, 0.0).within(Bounds::new(i16::MIN, i16::MIN, 4, 4));
        stepped(&mut [near.as_kinetic()]);
        let mut far = Thing::at(0.0, 0.0).within(Bounds::new(i16::MAX - 4, i16::MAX - 4, 4, 4));
        stepped(&mut [far.as_kinetic()]);

        // A rectangle wider than the space it is measured in is still only held, never wrapped.
        let mut walker = Thing::at(0.0, 0.0).within(Bounds::new(0, 0, 40000, 8));
        stepped(&mut [walker.as_kinetic()]);
        assert_eq!(walker.contacts, Contacts::empty());
    }

    #[test]
    fn limits_of_a_cart_s_own_hold_as_the_screen_does() {
        // A level wider than the screen, which is the case `Bounds::screen` does not cover.
        let level = Bounds::new(0, 0, 256, 128);
        let mut walker = Thing::at(300.0, 8.0).within(level);
        stepped(&mut [walker.as_kinetic()]);
        assert!(walker.contacts.right());
        assert_eq!(walker.body.pos(), (248.0, 8.0));
    }

    #[test]
    fn an_entity_is_held_by_the_rectangle_it_covers_wherever_it_put_it() {
        // A four-pixel rectangle inset two pixels into an eight-pixel sprite, which is what a cart
        // writes when it wants to be judged by less than it draws — and held by less, too. The
        // rectangle lands flush against the edge, so the body ends up two pixels further out.
        let mut inset = Thing::at(200.0, 8.0).inset(2, 4, 8).moving(2.0, 0.0);
        stepped(&mut [inset.as_kinetic()]);
        assert!(inset.contacts.right());
        assert_eq!(inset.bounds().right(), Bounds::screen().right());
        assert_eq!(inset.body.pos(), (122.0, 8.0));

        // And the near edge, where holding the body instead would leave the rectangle two pixels
        // short of an edge it can never reach.
        let mut inset = Thing::at(-10.0, 8.0).inset(2, 4, 8).moving(-2.0, 0.0);
        stepped(&mut [inset.as_kinetic()]);
        assert!(inset.contacts.left());
        assert_eq!(inset.bounds().x(), 0);
        assert_eq!(inset.body.pos(), (-2.0, 8.0));

        // Something long and thin, so the two axes cannot be mistaken for each other: held sixteen
        // pixels short of the right edge and four short of the bottom.
        let mut plank = Thing::at(200.0, 200.0)
            .sized(16, 4)
            .within(Bounds::screen());
        stepped(&mut [plank.as_kinetic()]);
        assert!(plank.contacts.right() && plank.contacts.below());
        assert_eq!(plank.body.pos(), (112.0, 124.0));
    }

    #[test]
    fn a_wall_and_an_edge_in_one_update_are_both_reported() {
        // The two halves of the answer come off different things and land in the same slot: a
        // crate stopping the walker sideways, the bottom of the level holding it below.
        let mut walker = Thing::at(100.0, 200.0)
            .within(Bounds::screen())
            .stopped_by(CRATE)
            .moving(4.0, 4.0);
        let mut boxed = Thing::at(110.0, 200.0).wearing(CRATE_SPRITE);
        stepped(&mut [walker.as_kinetic(), boxed.as_kinetic()]);
        assert!(walker.contacts.right() && walker.contacts.below());
        assert!(walker.contacts.touches(CRATE));
    }

    #[test]
    fn a_prop_is_met_where_the_cart_parked_it_and_never_moved() {
        // The patrolling hazard: the cart walks it on rails of its own, and the world only has to
        // know it is there.
        let mut hazard = Thing::at(16.0, 20.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 20.0).stopped_by(CRATE);
        for _ in 0..8 {
            walker.velocity = Velocity::new(2.0, 0.0);
            WORLD.step_cast(
                &mut [hazard.as_kinetic(), walker.as_kinetic()],
                air,
                flagged,
            );
        }
        // The prop has neither been moved nor written on.
        assert_eq!(hazard.body.pos(), (16.0, 20.0));
        assert_eq!(*hazard.velocity_mut(), Velocity::default());
        assert_eq!(hazard.contacts, Contacts::empty());
        // And it stood in the walker's way all the same: flush at its edge, told what it hit.
        assert_eq!(walker.body.pos().0, 8.0);
        assert!(walker.contacts.right() && walker.contacts.touches(CRATE));
    }

    #[test]
    fn a_prop_the_cart_moves_is_met_where_it_now_is() {
        // The cart drives its prop between updates, and the same update's cast meets it there —
        // the snapshot is taken at the top of the step, after the cart has done its driving.
        let mut hazard = Thing::at(40.0, 20.0).wearing(CRATE_SPRITE).parked();
        let mut sensor = Thing::at(20.0, 20.0);
        stepped(&mut [hazard.as_kinetic(), sensor.as_kinetic()]);
        assert!(!sensor.contacts.touches(CRATE));

        hazard.body.set_pos(21.0, 20.0);
        stepped(&mut [hazard.as_kinetic(), sensor.as_kinetic()]);
        assert!(sensor.contacts.touches(CRATE));
    }

    #[test]
    fn an_entity_hears_about_everything_it_meets_unless_it_says_otherwise() {
        // The default: nothing was said, so everything met is reported — the crate it walked
        // through and the wall tile it is standing on alike.
        let mut crate_ = Thing::at(8.0, 0.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 0.0);
        walker.velocity = Velocity::new(4.0, 0.0);
        WORLD.step_cast(
            &mut [crate_.as_kinetic(), walker.as_kinetic()],
            map(&["##"]),
            flagged,
        );
        assert!(walker.contacts.touches(CRATE), "the crate went unreported");
        assert!(walker.contacts.touches(WALL), "the tile went unreported");
    }

    #[test]
    fn a_neighbour_carrying_nothing_heeded_is_never_met() {
        // The same scene, with the walker saying it only cares about walls. The crate is still
        // there and still overlapped; it is simply not this walker's business, and the world never
        // works out that they touched.
        let mut crate_ = Thing::at(8.0, 0.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 0.0).heeding(WALL.into());
        walker.velocity = Velocity::new(4.0, 0.0);
        WORLD.step_cast(
            &mut [crate_.as_kinetic(), walker.as_kinetic()],
            air,
            flagged,
        );
        assert!(!walker.contacts.touches(CRATE));
        assert_eq!(walker.contacts, Contacts::empty());
        // And it went straight through, since nothing it heeds was ever solid to it.
        assert_eq!(walker.body.pos().0, 4.0);
    }

    #[test]
    fn a_tile_carrying_nothing_heeded_is_never_reported() {
        // The map speaks the same vocabulary as the cast and is masked by the same word: a walker
        // that heeds only crates is told nothing about the wall tiles it is sweeping across.
        let mut walker = Thing::at(0.0, 0.0).heeding(CRATE.into());
        walker.velocity = Velocity::new(4.0, 0.0);
        step_over(&mut [walker.as_kinetic()], map(&["##"]));
        assert!(!walker.contacts.touches(WALL));
        assert_eq!(walker.contacts, Contacts::empty());
    }

    #[test]
    fn a_wall_it_never_asked_to_hear_about_still_stops_it_and_is_still_reported() {
        // Heeding cannot cost an entity a wall. This one is stopped by `WALL` and asks to hear
        // about `CRATE` alone — and the wall stops it all the same, and says so, because what
        // stops an entity is heeded whether it was named or not.
        let mut walker = Thing::at(0.0, 0.0).stopped_by(WALL).heeding(CRATE.into());
        walker.velocity = Velocity::new(4.0, 0.0);
        step_over(&mut [walker.as_kinetic()], map(&[".#"]));
        assert_eq!(walker.body.pos().0, 0.0, "the wall let it through");
        assert!(walker.contacts.right());
        assert!(walker.contacts.touches(WALL), "the wall went unreported");
    }

    #[test]
    fn a_neighbour_that_stops_it_is_reported_though_it_heeds_nothing_at_all() {
        // The same, off a cast member rather than a tile, and heeding nothing whatsoever.
        let mut crate_ = Thing::at(8.0, 0.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 0.0)
            .stopped_by(CRATE)
            .heeding(BitFlags::empty());
        walker.velocity = Velocity::new(4.0, 0.0);
        WORLD.step_cast(
            &mut [crate_.as_kinetic(), walker.as_kinetic()],
            air,
            flagged,
        );
        assert_eq!(walker.body.pos().0, 0.0);
        assert!(walker.contacts.right() && walker.contacts.touches(CRATE));
    }

    #[test]
    fn a_mapless_world_leaves_the_tiles_to_the_picture() {
        // The same walker across the same wall, under the two worlds. The one that reads the map
        // is stopped by it; the one whose map is scenery walks straight through and is told
        // nothing, because nothing on it was ever asked.
        let mut walker = Thing::at(0.0, 0.0).stopped_by(WALL);
        walker.velocity = Velocity::new(4.0, 0.0);
        let mapful: World<0> = World::new();
        mapful.step_cast(&mut [walker.as_kinetic()], map(&[".#"]), unflagged);
        assert_eq!(walker.body.pos().0, 0.0);
        assert!(walker.contacts.right() && walker.contacts.touches(WALL));

        let mut drifter = Thing::at(0.0, 0.0).stopped_by(WALL);
        drifter.velocity = Velocity::new(4.0, 0.0);
        let scenery: World<0> = World::mapless();
        scenery.step_cast(&mut [drifter.as_kinetic()], map(&[".#"]), unflagged);
        assert_eq!(drifter.body.pos().0, 4.0);
        assert_eq!(drifter.contacts, Contacts::empty());
    }

    #[test]
    fn a_mapless_world_still_has_a_cast_and_still_has_edges() {
        // Only the map goes. The rest of the scene meets itself exactly as it did, and what an
        // entity confines itself to still holds it.
        let mut crate_ = Thing::at(8.0, 0.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 0.0)
            .stopped_by(CRATE)
            .within(Bounds::new(0, 0, 64, 64));
        walker.velocity = Velocity::new(4.0, 0.0);
        let scenery: World<0> = World::mapless();
        scenery.step_cast(
            &mut [crate_.as_kinetic(), walker.as_kinetic()],
            map(&["##"]),
            flagged,
        );
        assert_eq!(walker.body.pos().0, 0.0, "the crate let it through");
        assert!(walker.contacts.touches(CRATE));

        let mut leaver = Thing::at(60.0, 0.0).within(Bounds::new(0, 0, 64, 64));
        leaver.velocity = Velocity::new(8.0, 0.0);
        scenery.step_cast(&mut [leaver.as_kinetic()], air, unflagged);
        assert_eq!(leaver.body.pos().0, 56.0);
        assert!(leaver.contacts.right());
    }

    /// One step of `cast` over a map the test writes down, and a sheet with the crate and the wall
    /// on it.
    fn step_over(
        cast: &mut [&mut dyn Kinetic],
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
    ) {
        WORLD.step_cast(cast, tiles, flagged);
    }

    /// One step of `cast` through the world, over an empty map and a sheet where the crate and the
    /// wall are flagged.
    fn stepped(cast: &mut [&mut dyn Kinetic]) {
        WORLD.step_cast(cast, air, flagged);
    }

    /// The same with nothing flagged anywhere, for the tests that are only about movement.
    fn step(cast: &mut [&mut dyn Kinetic]) {
        WORLD.step_cast(cast, air, unflagged);
    }

    /// A sprite-sized entity held inside the screen, which is the limit most carts mean.
    fn confined(x: f32, y: f32) -> Thing {
        Thing::at(x, y).within(Bounds::screen())
    }

    /// A tile map written down: `#` is a wall, anything else is air. Row 0 is the top.
    fn map(rows: &'static [&'static str]) -> impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy {
        move |tx: i16, ty: i16| {
            if tx < 0 || ty < 0 {
                return BitFlags::empty();
            }
            match rows
                .get(ty as usize)
                .and_then(|row| row.as_bytes().get(tx as usize))
            {
                Some(b'#') => WALL.into(),
                _ => BitFlags::empty(),
            }
        }
    }

    /// A map with nothing on it, which is what a cart with no walls in its level has.
    fn air(_: i16, _: i16) -> BitFlags<SpriteFlag> {
        BitFlags::empty()
    }

    /// The sprite sheet as the cart flagged it: the wall cell is a wall, the crate cell is a
    /// crate, and every other cell carries nothing.
    fn flagged(sprite: SpriteId) -> BitFlags<SpriteFlag> {
        match sprite {
            WALL_SPRITE => WALL.into(),
            CRATE_SPRITE => CRATE.into(),
            _ => BitFlags::empty(),
        }
    }

    /// A sheet nobody has flagged anything on.
    fn unflagged(_: SpriteId) -> BitFlags<SpriteFlag> {
        BitFlags::empty()
    }

    /// A cast member a test can say everything about: where it is, how big, what it wears, what
    /// stops it and how far it may go.
    struct Thing {
        body: Body,
        velocity: Velocity,
        contacts: Contacts,
        size: (u16, u16),
        offset: i16,
        sprite: Option<SpriteId>,
        solid: Option<BitFlags<SpriteFlag>>,
        heeds: BitFlags<SpriteFlag>,
        limits: Option<Bounds>,
        prop: bool,
    }

    impl Thing {
        /// One sprite's worth, standing still, wearing nothing and stopped by nothing.
        fn at(x: f32, y: f32) -> Self {
            Self {
                body: Body::new(x, y),
                velocity: Velocity::default(),
                contacts: Contacts::default(),
                size: (8, 8),
                offset: 0,
                sprite: None,
                solid: None,
                heeds: BitFlags::all(),
                limits: None,
                prop: false,
            }
        }

        /// One of a size of its own — a lift, a plank, a hitbox switched off.
        fn sized(mut self, width: u16, height: u16) -> Self {
            self.size = (width, height);

            self
        }

        /// One whose rectangle sits `offset` pixels into what it draws.
        fn inset(mut self, offset: i16, width: u16, height: u16) -> Self {
            self.offset = offset;

            self.sized(width, height).within(Bounds::screen())
        }

        /// One already travelling.
        fn moving(mut self, dx: f32, dy: f32) -> Self {
            self.velocity = Velocity::new(dx, dy);

            self
        }

        /// One drawn from a cell the cart has flagged, which is how everybody else knows what it
        /// is.
        fn wearing(mut self, sprite: SpriteId) -> Self {
            self.sprite = Some(sprite);

            self
        }

        /// One with something to answer to.
        /// Told about these and nothing else, where the default is told about everything.
        fn heeding(mut self, heeds: BitFlags<SpriteFlag>) -> Self {
            self.heeds = heeds;
            self
        }

        /// One with rules of its own about what stops it, whatever the world declares.
        fn stopped_by(mut self, flags: impl Into<BitFlags<SpriteFlag>>) -> Self {
            self.solid = Some(flags.into());

            self
        }

        /// One that may never leave a rectangle.
        fn within(mut self, limits: Bounds) -> Self {
            self.limits = Some(limits);

            self
        }

        /// One the cart drives itself, in the cast only to be met.
        fn parked(mut self) -> Self {
            self.prop = true;

            self
        }

        /// This one as the trait object the engine's cast is made of.
        fn as_kinetic(&mut self) -> &mut dyn Kinetic {
            self
        }
    }

    impl Kinetic for Thing {
        fn body(&self) -> &Body {
            &self.body
        }

        fn body_mut(&mut self) -> &mut Body {
            &mut self.body
        }

        fn velocity_mut(&mut self) -> &mut Velocity {
            &mut self.velocity
        }

        fn contacts(&self) -> &Contacts {
            &self.contacts
        }

        fn contacts_mut(&mut self) -> &mut Contacts {
            &mut self.contacts
        }

        fn bounds(&self) -> Bounds {
            let (x, y) = self.body.draw_pos();

            Bounds::new(x + self.offset, y, self.size.0, self.size.1)
        }

        fn prop(&self) -> bool {
            self.prop
        }

        fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
            self.solid
        }

        fn heeds(&self) -> BitFlags<SpriteFlag> {
            self.heeds
        }

        fn sprite(&self) -> Option<SpriteId> {
            self.sprite
        }

        fn confines(&self) -> Option<Bounds> {
            self.limits
        }
    }

    /// What this cart flags its walls with — its tiles, and the lift it drags around.
    const WALL: SpriteFlag = SpriteFlag::Flag0;

    /// And its crates, which are walls to each other and to nothing else.
    const CRATE: SpriteFlag = SpriteFlag::Flag1;

    /// The cells those two flags are written on.
    const WALL_SPRITE: SpriteId = SpriteId(1);
    const CRATE_SPRITE: SpriteId = SpriteId(9);
}
