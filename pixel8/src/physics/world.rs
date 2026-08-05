//! The scene's one mover: the whole cast, stepped where it stands.

use super::{
    collider::{far, Cast, Collider, Neighbour},
    wire, Bounds, Contact, Contacts, Force, Kinetic, Velocity,
};
use crate::{BitFlags, Context, SpriteFlag, SpriteId};

/// The thing that moves everything: hand it the weather and the cast, once an update.
///
/// A cart holds one and calls [`step`](Self::step). What that does, for each entity in turn, is
/// what an update of a moving thing has always had to do — run the forces over its velocity, stop
/// whatever ran into the map's tiles or into the rest of the cast, keep it inside the rectangle it
/// says it may not leave, move its [`Body`](crate::Body) with what survived, and write down what it
/// met — except that no entity does any of it. Entities only [describe](Kinetic) themselves; the
/// world is where the collisions live.
///
/// ```no_run
/// use pixel8::{
///     physics::{Bounds, Contacts, Gravity, Kinetic, Velocity, World},
///     *,
/// };
///
/// # struct Hero { body: Body, velocity: Velocity, contacts: Contacts }
/// # struct Badie { body: Body, velocity: Velocity, contacts: Contacts }
/// # impl Kinetic for Hero {
/// #     fn body(&self) -> &Body { &self.body }
/// #     fn body_mut(&mut self) -> &mut Body { &mut self.body }
/// #     fn velocity_mut(&mut self) -> &mut Velocity { &mut self.velocity }
/// #     fn contacts(&self) -> &Contacts { &self.contacts }
/// #     fn contacts_mut(&mut self) -> &mut Contacts { &mut self.contacts }
/// #     fn bounds(&self) -> Bounds { Bounds::of(&self.body, 8, 8) }
/// # }
/// # impl Kinetic for Badie {
/// #     fn body(&self) -> &Body { &self.body }
/// #     fn body_mut(&mut self) -> &mut Body { &mut self.body }
/// #     fn velocity_mut(&mut self) -> &mut Velocity { &mut self.velocity }
/// #     fn contacts(&self) -> &Contacts { &self.contacts }
/// #     fn contacts_mut(&mut self) -> &mut Contacts { &mut self.contacts }
/// #     fn bounds(&self) -> Bounds { Bounds::of(&self.body, 8, 8) }
/// # }
/// # const SPIKES: SpriteFlag = SpriteFlag::Flag2;
/// struct Level {
///     // A cast of three under the level's pull, owned like everything else about the scene:
///     // `World::new().with_forces(Gravity::new())` is the whole of making one.
///     world: World<3, Gravity>,
///     hero: Hero,
///     badies: [Badie; 2],
/// }
///
/// impl Game for Level {
///     fn update(&mut self, ctx: &mut Context) {
///         // Whatever each entity means to do this update — read the buttons, turn a patrol
///         // round — is written into its velocity first. Then the world moves the lot.
///         let [first, second] = &mut self.badies;
///         self.world.step(
///             ctx,
///             &mut [
///                 self.hero.as_kinetic(),
///                 first.as_kinetic(),
///                 second.as_kinetic(),
///             ],
///         );
///
///         // And the answers are waiting on the entities themselves.
///         let grounded = self.hero.contacts().below();
///         let hurt = self.hero.contacts().touches(SPIKES);
///     }
///
///     fn draw(&self, gfx: &mut Graphics) {
///         gfx.clear(Color::BLACK);
///     }
/// }
/// ```
///
/// # The cast, and the order it is in
///
/// The cast is a slice of `&mut dyn Kinetic`, gathered fresh each update — a cart's entities live
/// wherever the cart keeps them, in fields and arrays and `heapless::Vec`s that have no type in
/// common. [`Kinetic::as_kinetic`] is the one word that turns each of them into a cast member, and
/// a `heapless::Vec` of those is how a cart with a variable cast gathers one without allocating:
///
/// ```ignore
/// let mut cast: heapless::Vec<&mut dyn Kinetic, 16> = heapless::Vec::new();
/// let _ = cast.push(self.hero.as_kinetic());
/// for badie in &mut self.badies {
///     let _ = cast.push(badie.as_kinetic());
/// }
/// self.world.step(ctx, &mut cast);
/// ```
///
/// Entities are stepped one at a time, in the order the slice puts them, and each of them is
/// resolved against the others *where they now stand*. So an entity stepped early is met at the
/// position it began the update at by nobody, and at the position it ended the update at by
/// everybody after it. That is a feature, and the one worth ordering a cast for: put a lift before
/// its rider and the rider is carried up the moment the lift moves, with no lag at all; put it
/// after and the rider spends the update on last update's platform.
///
/// Nothing is lagged, and nothing has to linger. An entity that dies this update has still been
/// met this update by everything stepped after it, and the cart may drop it the moment the step
/// returns.
///
/// A cast member that declares itself a [prop](Kinetic::prop) is met and never moved: its
/// rectangle and flags stand in everybody's way from wherever the cart last put it, and the
/// forces, the walls and the contacts all pass it by. Rails the cart drives — a patrol, a lift
/// on a track — cost the cast no more than being seen.
/// # The cast ceiling
///
/// `CAST` is the most members one step takes at full speed, and with it the size of the buffer
/// the cast crosses the ABI in: `CAST` records of forty-four bytes each, carried inside the
/// world. Sixty-four — the default — is the most the wire itself takes; a cart that knows its
/// scene is smaller says so the way it says every other capacity in this console, and pays for
/// no more records than it means to fill:
///
/// ```
/// # use pixel8::physics::World;
/// /// A hero and a badie: two moving things, so two records' worth of world.
/// const MAX_CAST: usize = 2;
/// let world: World<MAX_CAST> = World::new();
/// ```
///
/// The ceiling is a declaration, and it is held to: a cast handed past it is a cart bug and the
/// step says so at once, rather than quietly stepping some other, dearer way. The number that
/// bounds the cast's own `heapless::Vec` is the number to put here — one constant, owning both.
pub struct World<const CAST: usize = 64, F: Force = ()> {
    /// Whether the map is part of the scene or only the picture behind it. See
    /// [`mapless`](Self::mapless).
    reads_map: bool,
    /// What the scene calls a wall, for every entity that has no rule of its own. See
    /// [`with_solid`](Self::with_solid).
    solid: BitFlags<SpriteFlag>,
    /// The scene's weather, the world's own. See [`with_forces`](Self::with_forces).
    forces: F,
    /// The buffer the cast crosses the ABI in — see [`step`](Self::step). Only a cart has a wire
    /// to cross, so only the cart's build carries it.
    #[cfg(target_arch = "wasm32")]
    records: [wire::Record; CAST],
}

impl<const CAST: usize> World<CAST> {
    /// The world every cart starts with. It calls nothing solid until
    /// [`with_solid`](Self::with_solid) says otherwise.
    ///
    /// `const`, so a cart can spell its world out in its `game!` initializer, however it is
    /// configured.
    pub const fn new() -> Self {
        const {
            assert!(
                CAST <= wire::CAP,
                "a World's cast ceiling cannot exceed the sixty-four records the wire carries"
            )
        };

        Self {
            reads_map: true,
            solid: BitFlags::empty(),
            forces: (),
            #[cfg(target_arch = "wasm32")]
            records: [wire::EMPTY; CAST],
        }
    }

    /// A world whose map is scenery: the tiles are drawn and nothing else.
    ///
    /// The map is the one thing in a step that everything is resolved against whether it asked to
    /// be or not — every entity sweeps the tiles under it every update, so that a cart which
    /// flagged its walls gets them for nothing. A scene that flagged no tile at all pays for that
    /// anyway: a host call per tile per axis per entity, collecting an answer that is always
    /// empty. This is how a cart says not to bother. Shoot-'em-ups whose level scrolls past behind
    /// the fight are the case it is for; so is anything whose collisions are all between moving
    /// things.
    ///
    /// Nothing else changes. The cast still meets itself, [`confines`](Kinetic::confines) still
    /// holds, and *solid* — the scene's ([`with_solid`](Self::with_solid)) and anybody's
    /// [own](Kinetic::solid) — still means what it meant; there is simply nothing on
    /// the map for it to mean it against.
    ///
    /// `const`, like [`new`](Self::new), so a level's world is spelled out where it is made:
    ///
    /// ```
    /// # use pixel8::physics::World;
    /// let world: World = World::mapless();
    /// ```
    pub const fn mapless() -> Self {
        Self {
            reads_map: false,
            ..Self::new()
        }
    }
}

impl<const CAST: usize, F: Force> World<CAST, F> {
    /// The same world, owning `forces` as the scene's weather.
    ///
    /// One force, a tuple of them applied left to right — a tuple of [`Force`]s is itself a
    /// [`Force`] — or nothing at all, which is what a world starts with. The step runs them over
    /// every entity it moves, before anything moves; the ones with state of their own — a gusting
    /// [`Wind`](super::Wind) — stay reachable through [`forces_mut`](Self::forces_mut), to be
    /// updated where they live.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Atmosphere, Gravity, World};
    /// let world: World<64, _> = World::new().with_forces((Gravity::new(), Atmosphere::new()));
    /// ```
    pub fn with_forces<G: Force>(self, forces: G) -> World<CAST, G> {
        World {
            reads_map: self.reads_map,
            solid: self.solid,
            forces,
            #[cfg(target_arch = "wasm32")]
            records: self.records,
        }
    }

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
    /// What stops an entity is usually a fact about the scene rather than about anybody in it: one
    /// flag on the level's walls and floors, and everything that moves stops at them. So it is
    /// said once, here, and every cast member is stopped by these flags — on a tile or on a
    /// neighbour — unless it answers [`Kinetic::solid`] with rules of its own, which replace the
    /// scene's for that entity alone.
    ///
    /// ```no_run
    /// # use pixel8::{physics::World, SpriteFlag};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// let world: World = World::new().with_solid(SOLID);
    /// ```
    pub fn with_solid(mut self, solid: impl Into<BitFlags<SpriteFlag>>) -> Self {
        self.solid = solid.into();

        self
    }

    /// Moves the whole `cast` one update: the forces the world owns, then the world, then the
    /// bodies.
    ///
    /// For each entity in turn, in cast order: the world's own [forces](Self::with_forces) bend
    /// its velocity, in the order they were composed; whatever ran its
    /// [`bounds`](Kinetic::bounds) into something solid to it — the
    /// scene's word ([`with_solid`](Self::with_solid)), unless the entity has
    /// [rules of its own](Kinetic::solid) — is taken out of it, on each axis separately; the
    /// velocity that survives is stored back and the [`Body`](crate::Body) moved by it;
    /// whatever that carried outside its [`confines`](Kinetic::confines) is put back; and the
    /// sides that stopped it together with the flags of everything it met are written into its
    /// [`contacts`](Kinetic::contacts) slot, where the cart reads them at its leisure.
    ///
    /// *Something* is the map and the rest of the cast alike. The tiles under the entity are asked
    /// what they carry, and so is every other cast member, at the rectangle its
    /// [`bounds`](Kinetic::bounds) covers right now and under the flags its
    /// [`sprite`](Kinetic::sprite) carries in the sprite editor. Flags shared with what is solid
    /// to the entity stop it — a tile and a neighbour in the same one-axis
    /// pass, so landing on a lift reads [`below`](Contacts::below) exactly as landing on a floor
    /// tile does — and everything met, wall or not, comes back in [`Contacts::touched`]. An entity
    /// is never asked about itself, so its own kind is a wall like anybody else's: two crates
    /// flagged `CRATE`, each with `CRATE` in `solid`, stop each other and neither is ever its own
    /// wall.
    ///
    /// A meeting between cast members reaches both of them, whichever one's movement made it: the
    /// mover's own sweep answers the mover, and whoever it arrived on is told what arrived —
    /// filtered by that entity's own [`heeds`](Kinetic::heeds) and solid, flags only, in the same
    /// update. So a ram is felt on both sides of it however the two were ordered, and either party
    /// may be dropped the moment the step returns without costing the other its news.
    ///
    /// The two halves of the answer are taken over different ground, and on purpose. An entity is
    /// *stopped* where an axis was trying to go — the endpoint, which is where a wall has to be to
    /// be one — and it is told what it *met* over the whole of the step: where it began, the ground
    /// each axis swept across, and where it ended up. So the pond an entity walks out of this
    /// update is reported, and so is a hazard crossed between one pixel and the next. One thing
    /// follows from the difference and is worth knowing: something thinner than an update's
    /// movement can be stepped clean over without stopping the entity, and comes back in
    /// [`touched`](Contacts::touched) all the same. Keeping a fall from doing that to a floor is
    /// what [`Gravity`](super::Gravity)'s terminal velocity is for.
    ///
    /// A neighbour standing *on* an entity pushes it out before anything moves — out the shallower
    /// way, out the side it was already nearer — and reports the side it pushed *from*: a lift that
    /// has just risen a pixel into the rider standing on it carries the rider up, and the rider
    /// still reads `below`. What the push could not fully separate is reported but cannot block, so
    /// a thing caught between two solids can still walk out of them. A tile can never do any of
    /// this; a cast member can.
    ///
    /// The edge of the world is the last thing to have its say, after the movement that took the
    /// entity out there: one that named a rectangle in [`confines`](Kinetic::confines) is put back
    /// against it, the speed that carried it out is spent, and the sides it was held at join the
    /// walls' in the answer.
    ///
    /// An axis that was blocked is zeroed in the stored velocity too, not just in this update's
    /// movement: a fall that lands has been spent, and something that walked into a wall is not
    /// still walking. An entity driven by the buttons writes its sideways speed afresh every update
    /// and never notices; one carrying its own momentum does, which is the point.
    ///
    /// The weather is the world's own — one value, handed over once in
    /// [`with_forces`](Self::with_forces) — so a step asks for nothing but the cast. A gust the
    /// whole scene is bent by lives on the world and is [driven](Self::forces_mut) between steps;
    /// nothing is stored on an entity between updates but its velocity and its contacts.
    ///
    /// What a step costs a cart is the crossing, not the collisions. The cast is written down
    /// once — everything each entity describes, one fixed-size record apiece — and handed to the
    /// console in a single call; the console steps it natively, over its own map and sheet, and
    /// the answers are read back out of the same buffer. Nothing is allocated, no fuel is spent
    /// on the walking and the stopping, and the engine the console runs is the very one this
    /// module's tests drive. A cast past the world's own [ceiling](World#the-cast-ceiling)
    /// panics — the ceiling is the cart's own declaration, and holding it to it costs one
    /// comparison where a quiet slower path would cost an order of magnitude of fuel. An entity
    /// that should cost nothing is simply left out of the cast.
    pub fn step(&mut self, ctx: &Context, cast: &mut [&mut dyn Kinetic]) {
        // In the console, the whole step is one crossing of the ABI and the console's own,
        // native, work; on the native builds the tests are, the SDK runs the same engine itself.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            self.step_over_the_wire(cast);
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.step_cast(
            cast,
            |x, y| {
                ctx.map_tile(x, y)
                    .map_or(BitFlags::empty(), |tile| ctx.sprite_flags(tile))
            },
            |sprite| ctx.sprite_flags(sprite),
        );
    }

    /// The step, sent across the ABI: the cast written down, one `step_cast` import, the answers
    /// read back. The console's side runs the engine [`step_cast`](Self::step_cast) is — see
    /// [`wire`] — so what a cart spends here is the writing and the reading, not the collisions.
    #[cfg(target_arch = "wasm32")]
    fn step_over_the_wire(&mut self, cast: &mut [&mut dyn Kinetic]) {
        // The forces are the cart's own code, so their half of the step happens on the cart's
        // side — before the cast is written down, which is the one snapshot point both halves of
        // `step` share.
        self.weather(cast);

        // The buffer is the world's own, so the record a cast member crosses in costs the cart
        // exactly the ceiling it declared.
        let records = &mut self.records;
        for (record, entity) in records.iter_mut().zip(cast.iter_mut()) {
            let velocity = *entity.velocity_mut();
            *record = wire::Record::of(&**entity, self.solid, velocity);
        }

        unsafe {
            crate::ffi::step_cast(
                records.as_mut_ptr().cast(),
                cast.len() as u32,
                self.reads_map as u32,
            );
        }

        for (record, entity) in records.iter().zip(cast.iter_mut()) {
            // A prop went along only to be met: nothing was decided about it, so nothing of it
            // is touched.
            if entity.prop() {
                continue;
            }
            entity
                .body_mut()
                .set_wire((record.x, record.y, record.rx, record.ry));
            *entity.velocity_mut() = Velocity::new(record.dx, record.dy);
            *entity.contacts_mut() = Contacts::from_wire(record.sides, record.touched);
        }
    }

    /// The console's half of [`step`](Self::step): the same engine, over the map and the sheet
    /// the console binds in natively. Hidden — a cart calls `step`, and this is what answers it
    /// on the other side of the wire.
    #[doc(hidden)]
    pub fn step_hosted(
        &self,
        cast: &mut [&mut dyn Kinetic],
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
        carried: impl Fn(SpriteId) -> BitFlags<SpriteFlag>,
    ) {
        self.step_cast(cast, tiles, carried);
    }

    /// One update's worth of the world's own forces, over every entity the world moves.
    ///
    /// Velocities only — a force never touches a position — and props are left alone: the cart
    /// drives them, weather and all.
    fn weather(&self, cast: &mut [&mut dyn Kinetic]) {
        // The ceiling is the cart's own declaration, so a cast past it is a cart bug — and a
        // loud one, exactly where it happened, rather than a quiet step onto some slower path.
        // Both halves of `step` begin here, so the one guard covers them both.
        assert!(
            cast.len() <= CAST,
            "a cast of {} was handed to a world with a ceiling of {}",
            cast.len(),
            CAST
        );

        for entity in cast.iter_mut() {
            if entity.prop() {
                continue;
            }
            self.forces.apply(&mut **entity);
        }
    }

    /// The step itself, over a map and a sprite sheet handed in rather than reached for.
    ///
    /// [`step`](Self::step) is this with the console bound into the two closures — bound in
    /// natively on the console's own side of the wire, and through a host call apiece in the
    /// SDK's fall-back. Everything that makes a world a world is here — the order, the splitting
    /// of the cast, the resolution, the contacts slot — so a test can drive the whole of it
    /// against a map and a sheet it wrote down itself, and what it proves holds for the console
    /// running the very same engine.
    fn step_cast(
        &self,
        cast: &mut [&mut dyn Kinetic],
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
        carried: impl Fn(SpriteId) -> BitFlags<SpriteFlag>,
    ) {
        // The weather first, over the whole cast, so that everything an entity describes is read
        // after the forces have had their say — one defined moment, and the same one the wire
        // crossing samples at, so the two halves of `step` can never disagree about what a
        // velocity-dependent description answered.
        self.weather(cast);

        // What every cast member is worth to everybody else, taken once at the top: the rectangle
        // it covers and the flags its cell carries. Each entity is then resolved against the rest
        // of the cast several times over — once to be pushed out of it, once for each axis it
        // moves along — and every one of those questions used to go back through the cast's `dyn`
        // for a rectangle and a sheet lookup that had not changed since the last one. That is the
        // n-squared this takes out: one question an entity a step, and plain loads after it.
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
        // questions an entity can settle without walking anybody: whether there is a wall of its
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
                // An entity that wears nothing, or whose cell the cart flagged with nothing,
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
            // whether any of the walking below is worth doing at all. One question an entity
            // where the snapshot would have asked two.
            for member in cast.iter() {
                if let Some(sprite) = member.sprite() {
                    worn = worn | carried(sprite);
                }
            }
        }

        // The meetings each entity's own step makes, delivered to the other party once the whole
        // cast has moved: a slot per member, holding the flags of everything that arrived on it.
        // Collected rather than written straight away, because the other party may not have been
        // stepped yet — and its own step overwrites its contacts whole.
        let mut arrived = [BitFlags::<SpriteFlag>::empty(); CAST];
        for index in 0..cast.len() {
            // The cast without this entity in it, in two pieces: everything stepped already, and
            // everything still to be. The split is what the fallback walks, and the index of the
            // entity in the middle is what the snapshot skips — either way, the whole of how an
            // entity comes to be skipped against itself is that no question ever reaches it.
            let (before, rest) = cast.split_at_mut(index);
            let Some((entity, after)) = rest.split_first_mut() else {
                break;
            };

            // A prop was placed by the cart and stays placed: its slot in the snapshot is where
            // everybody meets it, and the whole of the stepping below — forces, walls, the hold —
            // is for things the world moves.
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

            // Whoever this step arrived on is owed the news of it: what this entity wears, into
            // the slot of each neighbour the resolution noted, for the delivery below.
            let mut met = neighbours.met.get();
            while met != 0 {
                let slot = met.trailing_zeros() as usize;
                met &= met - 1;
                arrived[slot] = arrived[slot] | mine;
            }

            // The entity has just moved, so its slot follows it. That is what keeps the cast
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
        // step wrote, sides untouched: an arrival tells an entity what reached it, never that it
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

/// How long a cast is snapshotted rather than walked.
///
/// Thirty-two entities: 288 bytes of the shadow stack between the two arrays, under one percent of
/// a cart's 32 KiB default reserve, and reclaimed the moment the step returns. Room to spare rather
/// than room measured out, because clearing them costs a bulk fill either way — a capacity of
/// sixty-four measures the same as a capacity of sixteen — so the number is chosen for the scenes
/// it covers and not for the stack it takes. More than thirty-two things moving at once on a
/// 128x128 screen is a scene with bigger problems than this array, and one that has them anyway is
/// answered exactly as it always was: the walk falls back to asking each entity through the cast's
/// `dyn`.
const SNAPSHOT: usize = 32;

/// The rectangle a slot holds until something is written into it: nothing, which overlaps nothing.
const EMPTY: Bounds = Bounds::new(0, 0, 0, 0);

/// The world every cart starts with. See [`World::new`].
impl<const CAST: usize> Default for World<CAST> {
    fn default() -> Self {
        Self::new()
    }
}

/// One entity's update: the resolution, the movement, the edge of the world, and the answer
/// written back into the entity's own slot. The forces have already had their say, over the whole
/// cast at once, before anybody was stepped.
///
/// The order is the whole of it. The resolution takes out of the velocity whatever ran into
/// something; the body is moved by what is left; and only then is the entity held inside its
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
    // Everything the entity describes is read before anything of it moves — the limits along
    // with the rest, so an answer worked out from where the entity stands means where the step
    // found it, on either side of the wire.
    let limits = entity.confines();
    // `Velocity` is `Copy`, so this reads out what the forces left behind without holding a borrow
    // of the entity over the resolution below.
    let mut velocity = *entity.velocity_mut();
    let mut contacts = Contacts::empty();
    // An entity covering no pixels has nothing to resolve — a hitbox switched off, a blast that has
    // shrunk to nothing — and is only moved.
    //
    // The scene's word for solid, unless this entity has one of its own.
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
        // — and only where there is something out there to be inside of. An entity that calls
        // nothing solid can stand in anything, and one no neighbour is a wall to has nothing to be
        // pushed out of, so most casts settle the whole separating question here, without placing
        // a box, walking anybody, or making the call.
        if collider.could_be_inside_something(neighbours) {
            // Guarded like the hold below: `set_pos` re-snaps the drawn pixel, and an entity that
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

    // The edge of the world, last: it is the movement just made that carried the entity out there,
    // and the sides it is held at read alongside the walls that stopped it. The velocity goes with
    // it rather than being fetched back out of the entity, so what survives the whole step is
    // stored once, at the bottom, however many places had a say in it.
    if let Some(limits) = limits {
        contacts |= hold(entity, limits, &mut velocity).into();
    }

    *entity.velocity_mut() = velocity;
    *entity.contacts_mut() = contacts;
}

/// The cast without the entity being stepped: everything already moved this update, and everything
/// still to be.
///
/// Two slices rather than one, because the entity in the middle is the one holding the `&mut` — and
/// that is the whole of how an entity comes to be skipped against itself. What each neighbour is
/// worth is answered as the resolution reaches its slot: the rectangle it covers *now*, and the
/// flags the cart wrote on the cell it says it wears.
/// The snapshot's three answers about the whole cast, a slot each in cast order: the rectangle
/// each member covers, the flags it carries, and the flags it is listening for.
type Snapshot<'a> = (
    &'a [Bounds],
    &'a [BitFlags<SpriteFlag>],
    &'a [BitFlags<SpriteFlag>],
);

struct Neighbours<'a, 'cast, F> {
    /// Every cast member's rectangle, the flags it carries and the flags it is listening for, as
    /// they stand right now, a slot each in cast order — or nothing at all for a cast too long to
    /// have been snapshotted, which is walked through the `dyn` below instead.
    taken: Option<Snapshot<'a>>,
    /// Everything anybody in the cast is wearing, the entity being stepped included — see
    /// [`Cast::carried`]. Nothing at all for a cast too long to have been snapshotted, which
    /// answers every question the long way round.
    worn: BitFlags<SpriteFlag>,
    /// Which slot is the entity being stepped, so the snapshot can leave it out.
    mine: usize,
    before: &'a [&'cast mut dyn Kinetic],
    after: &'a [&'cast mut dyn Kinetic],
    carried: F,
    /// The scene's word for solid, which is what a neighbour with no rule of its own is listening
    /// for a wall with — the long-cast fallback works a neighbour's listening out through the
    /// `dyn`, and needs the world's word to finish it.
    solid: BitFlags<SpriteFlag>,
    /// One bit per cast slot: the neighbours this entity's step has [met](Cast::note) while they
    /// were listening. A `Cell` because the walks hold the whole cast by `&self`; a `u64` because
    /// the wire's ceiling is sixty-four, and the world's cannot exceed it.
    met: core::cell::Cell<u64>,
}

impl<F: Fn(SpriteId) -> BitFlags<SpriteFlag>> Cast for Neighbours<'_, '_, F> {
    #[inline(always)]
    fn carried(&self) -> BitFlags<SpriteFlag> {
        self.worn
    }

    fn note(&self, index: usize) {
        // The walk's index becomes the cast's slot: the snapshot indexes the whole cast, and the
        // fallback indexes it with the entity being stepped taken out of the middle.
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
        // worth asking about: the two slices below hold entities the whole of whose part in this
        // is that they are moved.
        if self.worn.is_empty() {
            return 0;
        }

        match self.taken {
            Some((boxes, ..)) => boxes.len(),
            // The entity in the middle is in neither half, so there is no slot of its own here to
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
/// What the resolution does with walls, for the edge of the world: an entity whose
/// [`bounds`](Kinetic::bounds) have left `limits` is put back against them, and the speed that took
/// it there is spent rather than left to build up while it leans on the edge.
///
/// It is the rectangle that is held, wherever the entity put it, and the body follows by the same
/// amount — so a hurtbox inset into a sprite stops with its own edge against the limit. The exact
/// sub-pixel position is what moves, not the drawn one, so something leaning on an edge sits at it
/// precisely instead of being nudged a pixel at a time.
///
/// Speed pointing back into `limits` is left alone: an entity that starts outside and is already
/// travelling home keeps what was bringing it. A rectangle with no room to fit — one wider than
/// the `limits` it is given — is held against their near edge rather than pushed out the far one.
///
/// `velocity` is the step's own, handed over and spent in place: the entity's slot is written once
/// where the step ends rather than read back out and written again here.
#[inline(always)]
fn hold(entity: &mut dyn Kinetic, limits: Bounds, velocity: &mut Velocity) -> BitFlags<Contact> {
    let bounds = entity.bounds();
    // One look at the body for both of what it is asked, since every question an entity is put
    // through the cast's `dyn` costs a call the resolution used to have inlined.
    let body = entity.body();
    let (x, y) = body.pos();

    // The rectangle's own corner, in the exact sub-pixel coordinates the body keeps: the
    // whole-pixel offset of the rectangle from where the body draws, carried onto the position the
    // body really has. Zero for a `Bounds::of`, which is most of them.
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

    // Only the speed that was carrying the entity out is spent. One already heading back in —
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
    // Guarded, because `set_pos` re-snaps the drawn pixel: an entity that was already inside would
    // lose the coherent step `Body` is holding for it, and shimmer for it.
    if (dx, dy) != (0.0, 0.0) {
        entity.body_mut().set_pos(x + dx, y + dy);
    }

    held
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

    /// The world every one of these tests is stepped by: there is only ever the one.
    const WORLD: World = World::new();

    /// A world with the walls declared on it, for the tests about the scene's own word. Spelled
    /// as a function because a test wants it inline, and spelled with its type because a bare
    /// `World::new()` in expression position has no cast ceiling to infer.
    fn walled() -> World {
        World::new().with_solid(WALL)
    }

    #[test]
    #[should_panic(expected = "ceiling")]
    fn a_cast_past_the_world_s_ceiling_is_refused_loudly() {
        // The ceiling is the cart's own declaration; overrunning it is a bug the cart wants told
        // about, not a quiet step onto a dearer path.
        let (mut one, mut two, mut three) = (
            Thing::at(0.0, 0.0),
            Thing::at(20.0, 0.0),
            Thing::at(40.0, 0.0),
        );
        let low: World<2> = World::new();
        low.step_cast(
            &mut [one.as_kinetic(), two.as_kinetic(), three.as_kinetic()],
            air,
            unflagged,
        );
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
        // snapshot and counts its neighbours with the stepped entity taken out of the middle —
        // both directions, so the slot arithmetic is pinned on either side of `mine`.
        let mut cast: Vec<Thing> = (0..SNAPSHOT + 2)
            .map(|i| Thing::at(3000.0 + i as f32 * 100.0, 0.0))
            .collect();
        // The first arrives on the last: mover at slot 0, stander past everybody else.
        cast[0] = Thing::at(12.0, 0.0).wearing(WALL_SPRITE).moving(-6.0, 0.0);
        let last = cast.len() - 1;
        cast[last] = Thing::at(0.0, 0.0).wearing(CRATE_SPRITE);
        let mut handed: Vec<&mut dyn Kinetic> = cast.iter_mut().map(|t| t.as_kinetic()).collect();
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
        let mut handed: Vec<&mut dyn Kinetic> = cast.iter_mut().map(|t| t.as_kinetic()).collect();
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
        let mut cast: Vec<&mut dyn Kinetic> = things.iter_mut().map(|t| t.as_kinetic()).collect();
        walled().step_cast(&mut cast, counted, unflagged);

        let map = u32::from(crate::MAP_WIDTH_TILES) * u32::from(crate::MAP_HEIGHT_TILES);
        assert_eq!(asked.get(), 64 * 2 * map);
    }

    #[test]
    fn what_an_entity_describes_is_read_after_the_weather_has_had_its_say() {
        // An entity whose sprite answer depends on its velocity — falling reads as flagged. The
        // snapshot everyone else meets it through must be taken after the world's forces have
        // bent the velocities: one defined moment, the same one the wire crossing samples at, so
        // the two halves of `step` can never disagree about it.
        struct Faller {
            body: Body,
            velocity: Velocity,
            contacts: Contacts,
        }

        impl Kinetic for Faller {
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
                Bounds::of(&self.body, 8, 8)
            }

            // Wearing the crate cell exactly while it falls: a description worked out from the
            // velocity, which is what pins when the velocity was read.
            fn sprite(&self) -> Option<SpriteId> {
                (self.velocity.dy > 0.0).then_some(CRATE_SPRITE)
            }
        }

        let mut faller = Faller {
            body: Body::new(0.0, 0.0),
            velocity: Velocity::default(),
            contacts: Contacts::default(),
        };
        let mut sensor = Thing::at(4.0, 0.0);
        pulled().step_cast(
            &mut [faller.as_kinetic(), sensor.as_kinetic()],
            air,
            flagged,
        );
        assert!(
            sensor.contacts.touches(CRATE),
            "the snapshot was taken before the pull made the faller a crate"
        );
    }

    /// A world under the default pull, for the tests about what the weather does.
    fn pulled() -> World<64, Gravity> {
        World::new().with_forces(GRAVITY)
    }

    /// A world owning whatever weather a test composes.
    fn weathered<F: Force>(forces: F) -> World<64, F> {
        World::new().with_forces(forces)
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
    fn the_world_s_forces_are_applied_to_every_entity_in_the_cast() {
        let world = pulled();
        let (mut one, mut two) = (Mob::new(), Mob::new());
        world.step_cast(&mut [one.as_kinetic(), two.as_kinetic()], air, unflagged);
        // One update's pull, both in the velocity each kept and in where each ended up.
        for mob in [&one, &two] {
            assert_eq!(mob.velocity, Velocity::new(0.0, Gravity::DEFAULT_STRENGTH));
            assert_eq!(mob.body.pos(), (0.0, Gravity::DEFAULT_STRENGTH));
        }

        for _ in 0..1_000 {
            world.step_cast(&mut [one.as_kinetic()], air, unflagged);
        }
        assert_eq!(one.velocity.dy, Gravity::DEFAULT_TERMINAL_VELOCITY);
    }

    #[test]
    fn a_world_owning_no_forces_applies_none() {
        // The weather belongs to the scene, not to the entity, so a cast stepped by a world that
        // took none carries on at whatever it was already doing.
        let mut mob = Mob::moving(0.0, 1.0);
        step(&mut [mob.as_kinetic()]);
        assert_eq!(mob.velocity, Velocity::new(0.0, 1.0));
        assert_eq!(mob.body.pos(), (0.0, 1.0));
    }

    #[test]
    fn forces_run_in_the_order_the_tuple_composes_them() {
        const PULL: Gravity = Gravity::new().with_terminal_velocity(f32::MAX);
        const AIR: Atmosphere = Atmosphere::new();

        let (mut pulled, mut aired) = (Mob::new(), Mob::new());
        weathered((PULL, AIR)).step_cast(&mut [pulled.as_kinetic()], air, unflagged);
        weathered((AIR, PULL)).step_cast(&mut [aired.as_kinetic()], air, unflagged);

        // The drag that runs before the pull has not felt it yet, so the first update — and every
        // one after it — differs by exactly that much.
        assert_eq!(aired.velocity.dy, Gravity::DEFAULT_STRENGTH);
        assert!(
            pulled.velocity.dy < aired.velocity.dy,
            "the order made no difference: {} against {}",
            pulled.velocity.dy,
            aired.velocity.dy
        );
    }

    #[test]
    fn mass_is_read_off_the_entity_by_the_forces_the_world_runs() {
        // The same wind on two entities that differ in nothing but what they weigh, and it is the
        // world that carries the one to the other.
        let (mut light, mut heavy) = (Mob::with_mass(0.5), Mob::with_mass(4.0));
        weathered(Wind::new(2.0)).step_cast(
            &mut [light.as_kinetic(), heavy.as_kinetic()],
            air,
            unflagged,
        );
        assert!(
            light.velocity.dx > heavy.velocity.dx,
            "the mass was not read: {} against {}",
            light.velocity.dx,
            heavy.velocity.dx
        );
    }

    #[test]
    fn forces_of_different_types_compose_into_one_weather() {
        struct Updraft;

        impl Force for Updraft {
            fn apply(&self, entity: &mut dyn Kinetic) {
                entity.velocity_mut().dy -= 0.5;
            }
        }

        let mut mob = Mob::new();
        weathered((GRAVITY, Wind::new(1.0), Updraft)).step_cast(
            &mut [mob.as_kinetic()],
            air,
            unflagged,
        );
        assert_eq!(mob.velocity.dx, 0.05);
        assert_eq!(mob.velocity.dy, Gravity::DEFAULT_STRENGTH - 0.5);
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
            cast.extend(crowd.iter_mut().map(Kinetic::as_kinetic));
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
        // know it is there. The pull handed to the step reaches everything but the prop.
        let mut hazard = Thing::at(16.0, 20.0).wearing(CRATE_SPRITE).parked();
        let mut walker = Thing::at(0.0, 20.0).stopped_by(CRATE);
        let world = pulled();
        for _ in 0..8 {
            walker.velocity = Velocity::new(2.0, 0.0);
            world.step_cast(
                &mut [hazard.as_kinetic(), walker.as_kinetic()],
                air,
                flagged,
            );
        }
        // The prop has neither fallen nor drifted, and nothing was ever written on it.
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
        let mapful: World = World::new();
        mapful.step_cast(&mut [walker.as_kinetic()], map(&[".#"]), unflagged);
        assert_eq!(walker.body.pos().0, 0.0);
        assert!(walker.contacts.right() && walker.contacts.touches(WALL));

        let mut drifter = Thing::at(0.0, 0.0).stopped_by(WALL);
        drifter.velocity = Velocity::new(4.0, 0.0);
        let scenery: World = World::mapless();
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
        let scenery: World = World::mapless();
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
