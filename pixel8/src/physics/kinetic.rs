//! The entity the world moves: what it is, and nothing about what it does.

use super::{Bounds, Contacts, Velocity};
use crate::{BitFlags, Body, SpriteFlag, SpriteId};

/// An entity a [`Force`](super::Force) can push and a [`World`](super::World) can move: a [`Body`],
/// the [`Velocity`] it travels at, and the [`Contacts`] slot it is told what it met in.
///
/// Every method here *describes*. Nothing on this trait moves anything, asks the console anything,
/// or detects anything: an entity says where it is, how big it is, what it weighs, what stops it,
/// what it wears and how far it is let go, and [`World::step`](super::World::step) does the rest
/// for the whole cast at once. Which is the point — the cart registers its entities and reads their
/// contacts, and never walks a pair.
///
/// Implementing it is five accessors and a rectangle. Everything past those has an answer already:
/// a thing that weighs one, stops at whatever the scene calls solid, wears nothing and may go
/// anywhere.
///
/// Every answer is read once, as the step begins — after the world's forces have bent the
/// velocities, before anything moves — and holds for the whole of that step; the one exception is
/// [`bounds`](Self::bounds), whose rectangle keeps its seat on the body wherever the step carries
/// it. An answer worked out from the entity's state is welcome, and it is the next update's step
/// that sees a change.
///
/// ```no_run
/// # use pixel8::{physics::{Bounds, Contacts, Kinetic, Velocity}, *};
/// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
/// # const CRATE: SpriteFlag = SpriteFlag::Flag2;
/// # const CRATE_SPRITE: SpriteId = SpriteId(9);
/// struct Crate {
///     body: Body,
///     velocity: Velocity,
///     contacts: Contacts,
/// }
///
/// impl Kinetic for Crate {
///     fn body(&self) -> &Body {
///         &self.body
///     }
///
///     fn body_mut(&mut self) -> &mut Body {
///         &mut self.body
///     }
///
///     fn velocity_mut(&mut self) -> &mut Velocity {
///         &mut self.velocity
///     }
///
///     fn contacts(&self) -> &Contacts {
///         &self.contacts
///     }
///
///     fn contacts_mut(&mut self) -> &mut Contacts {
///         &mut self.contacts
///     }
///
///     fn bounds(&self) -> Bounds {
///         Bounds::of(&self.body, 8, 8)
///     }
///
///     // A crate is heavy: the wind hardly shifts it, and it still falls like everything else.
///     fn mass(&self) -> f32 {
///         6.0
///     }
///
///     // What it is drawn from, so that everybody who meets it is told they met a crate — and,
///     // that flag being in its own `solid` below, so that crates stack.
///     fn sprite(&self) -> Option<SpriteId> {
///         Some(CRATE_SPRITE)
///     }
///
///     // Rules of its own, which replace the world's — so the scene's walls are named again
///     // here, and its own kind beside them: crates stop at the walls and stack on each other.
///     fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
///         Some(SOLID | CRATE)
///     }
/// }
/// ```
pub trait Kinetic {
    /// Where the entity is: the body it is drawn from and collides with.
    ///
    /// [`Game::draw`](crate::Game::draw) is handed a `&self`, so this is the one a cart reads
    /// there.
    fn body(&self) -> &Body;

    /// The same body, for the world that moves it.
    fn body_mut(&mut self) -> &mut Body;

    /// The velocity forces act on and the world spends.
    fn velocity_mut(&mut self) -> &mut Velocity;

    /// What the entity's last step ran into.
    ///
    /// A field like the body and the velocity, and it belongs to the entity for the same reason:
    /// the world writes it each step and the cart reads it whenever it likes, in the same update
    /// or in the draw after it. `Contacts::default()` is the right thing to start one at — a thing
    /// that has not been stepped yet has met nothing.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Contacts, Kinetic};
    /// # struct Hero { contacts: Contacts }
    /// # impl Hero {
    /// fn grounded(&self) -> bool {
    ///     self.contacts.below()
    /// }
    /// # }
    /// ```
    fn contacts(&self) -> &Contacts;

    /// The same slot, for the world that fills it.
    fn contacts_mut(&mut self) -> &mut Contacts;

    /// The rectangle the entity covers, in the coordinates its [`Body`] is in.
    ///
    /// The one rectangle an entity has, and everything about where it *is* rather than what is
    /// pushing it goes through it: what the map's tiles stop, what the rest of the cast meets when
    /// it meets this entity, what [`confines`](Self::confines) holds inside the world, what
    /// [`overlaps`](Self::overlaps) compares, and what [`Bounds::on_screen`] answers for. There is
    /// no separate hitbox anywhere: this rectangle is the entity's body, full stop.
    ///
    /// [`Bounds::of`] is the whole of the usual answer, and a rectangle offset from the body — a
    /// hurtbox narrower than the sprite — is met and held exactly where the entity put it.
    ///
    /// ```no_run
    /// # use pixel8::{physics::{Bounds, Kinetic}, Body};
    /// # struct Hero { body: Body }
    /// # impl Hero {
    /// fn bounds(&self) -> Bounds {
    ///     Bounds::of(&self.body, 8, 8)
    /// }
    /// # }
    /// ```
    fn bounds(&self) -> Bounds;

    /// How hard the entity is to push, relative to everything else in the scene.
    ///
    /// `1.0` is the default nobody has to think about, and an entity opts out of it by overriding
    /// this one method: `4.0` is four times as hard to shove and `0.25` a quarter. Forces that
    /// care divide their grip by it — twice the mass, half the shove — and forces that do not,
    /// [`Gravity`](super::Gravity) among them, pull everything alike whatever it weighs. Mass is
    /// how hard a thing is to push, not how hard it falls; what tells a feather from an anvil as
    /// they fall is the [`Atmosphere`](super::Atmosphere) between them.
    ///
    /// A mass that means nothing — zero, negative, or `NaN` — is read as `1.0` wherever it is
    /// divided by, so a bad number gives an ordinary entity rather than one flung off the screen.
    fn mass(&self) -> f32 {
        1.0
    }

    /// The entity's own answer to what means *wall* to it, where the world's word is not its word.
    ///
    /// `None` — the default — is an entity that goes by what the scene says: it is stopped by
    /// whatever its world declared in [`with_solid`](super::World::with_solid), and by nothing at
    /// all under a world that declared nothing. Most of a cast answers this way, because what is a
    /// wall is usually a fact about the scene rather than about anybody in it.
    ///
    /// `Some` is an entity with rules of its own, and they replace the world's rather than add to
    /// them. `Some(BitFlags::empty())` is the furthest they go: nothing anywhere stops this one,
    /// whatever the scene declares — a bullet, a bird, anything a cart wants told about the world
    /// rather than stopped by it. It is still told everything it walked through; answering with
    /// nothing buys a sensor, not a saving.
    ///
    /// Whoever names the flags, they mean the same thing: [`World::step`](super::World::step)
    /// stops the entity at everything carrying one of them — a map tile or another cast member
    /// alike — over the rectangle [`bounds`](Self::bounds) gives. It is the same flag the cart
    /// already marks its walls with for [`Graphics::map`](crate::Graphics::map), and the same one
    /// it writes on the sprite its lifts and its crates are drawn from. Any flag in common is
    /// enough, so one cart can carry its walls, its water and its ladders on flags of their own
    /// and each entity stop at the ones that concern it.
    ///
    /// An entity's *own* kind belongs here as readily as anything else, and is the usual reason to
    /// have rules of one's own at all: the world knows who is who and never asks an entity about
    /// itself, so two crates whose [`sprite`](Self::sprite) carries `CRATE`, each with `CRATE`
    /// solid to it, block each other and neither is ever its own wall.
    ///
    /// ```no_run
    /// # use pixel8::{BitFlags, SpriteFlag};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// # const CRATE: SpriteFlag = SpriteFlag::Flag1;
    /// # struct Crate;
    /// # impl Crate {
    /// // The walls stop a crate like they stop everybody — and so does another crate.
    /// fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
    ///     Some(SOLID | CRATE)
    /// }
    /// # }
    /// ```
    fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
        None
    }

    /// Which sprite flags the entity cares to be *told* about.
    ///
    /// [`solid`](Self::solid) says what stops the entity; this says what it wants to hear about,
    /// and everything else the world meets on its behalf it throws away without ever working out
    /// whether it was met. Only flags in here — or in `solid`, which is always heeded whether it is
    /// named here or not — reach [`Contacts::touched`], and they reach it the same way from a tile
    /// as from another cast member — and the same way again when the meeting was the *other*
    /// party's doing, an arrival on this entity while it stood. So the promise reads in one line:
    /// *you are told what you heed, whoever's movement brought it, and you are stopped by what you
    /// call solid.*
    ///
    /// [Everything](BitFlags::all) is the default, and it is the honest one: an entity that has not
    /// said otherwise is told about every flag it meets, which is what makes a `Contacts` worth
    /// reading before a cart knows what it will want from it. Narrowing it is a promise the cart
    /// makes and the world takes at its word — a neighbour carrying nothing this entity heeds is
    /// skipped before a single edge of it is worked out, and a tile's flags are dropped before they
    /// are collected. In a scene where everything is in one cast that is most of the work of an
    /// update, and it is spent on answers nobody was going to read.
    ///
    /// It cannot cost an entity a wall. `solid` is heeded whatever this says, so a wall an entity
    /// never asked to hear about still stops it, and being stopped by it still reports it.
    ///
    /// ```no_run
    /// # use pixel8::{BitFlags, SpriteFlag};
    /// # const AIRCRAFT: SpriteFlag = SpriteFlag::Flag0;
    /// # const ENEMY_SHOT: SpriteFlag = SpriteFlag::Flag1;
    /// # struct Hero;
    /// # impl Hero {
    /// // This one is shot at and rammed, and nothing else in the scene concerns it.
    /// fn heeds(&self) -> BitFlags<SpriteFlag> {
    ///     AIRCRAFT | ENEMY_SHOT
    /// }
    /// # }
    /// ```
    fn heeds(&self) -> BitFlags<SpriteFlag> {
        BitFlags::all()
    }

    /// What the entity is made of, as far as everybody else is concerned: the sprite it wears.
    ///
    /// The other side of [`solid`](Self::solid). That says which flags stop *me*; this says which
    /// flags I carry, and they are the flags the cart wrote on that sprite in the sprite editor —
    /// the same one vocabulary the map's tiles already speak. So a badie is a badie because its
    /// cell is flagged `BADIE`, and every entity that meets it is told `BADIE` in
    /// [`Contacts::touched`], whether or not it was stopped.
    ///
    /// `None` — the default — is an entity that carries nothing. It is still stopped by tiles, by
    /// the rest of the cast and by its own [`confines`](Self::confines), and it still senses
    /// everything it meets; it is simply not there for anybody else. Which is right for a hero
    /// drawn from unflagged cells, and for anything the cart handles by holding it rather than by
    /// flagging it.
    ///
    /// An entity whose look changes with its state returns whichever cell it is wearing now; two
    /// walk-cycle cells carrying the same flag make the answer moot, which is the usual case.
    ///
    /// ```no_run
    /// # use pixel8::SpriteId;
    /// # const BADIE_SPRITE: SpriteId = SpriteId(6);
    /// # struct Badie;
    /// # impl Badie {
    /// fn sprite(&self) -> Option<SpriteId> {
    ///     Some(BADIE_SPRITE)
    /// }
    /// # }
    /// ```
    fn sprite(&self) -> Option<SpriteId> {
        None
    }

    /// The rectangle the entity is never let out of, if there is one.
    ///
    /// The edge of the world, said once rather than enforced by a call an update can forget:
    /// [`World::step`](super::World::step) holds the entity inside whatever comes back here, and
    /// reports the sides it was held at in the same [`Contacts`] as the walls. [`Bounds::screen`]
    /// is what most carts that want one mean; a level bigger than the screen hands over the level.
    ///
    /// `None` — the default — is an entity nothing holds: free to walk off the last tile and fall
    /// for ever, which is what a bullet or a spent enemy wants. The cart drops it when
    /// [`Bounds::on_screen`] says it has gone.
    ///
    /// It takes a `&self`, so limits that move are worked out rather than written down: the room
    /// the player has just walked into, a level that grows, an arena closing in. The answer is
    /// read once as the step begins, so a limit worked out from where the entity stands means
    /// where it stood as the step took it up.
    ///
    /// ```no_run
    /// # use pixel8::physics::{Bounds, Kinetic};
    /// # const LEVEL: Bounds = Bounds::new(0, 0, 256, 128);
    /// # struct Hero;
    /// # impl Hero {
    /// fn confines(&self) -> Option<Bounds> {
    ///     Some(LEVEL)
    /// }
    /// # }
    /// ```
    fn confines(&self) -> Option<Bounds> {
        None
    }

    /// Whether the entity is a prop: in the cast to be met, never to be moved.
    ///
    /// A prop stands in everybody else's way exactly as any cast member does — the rectangle its
    /// [`bounds`](Self::bounds) cover and the flags on the sprite it [wears](Self::sprite) — and
    /// is otherwise left alone: no force reaches it, nothing resolves it, and its contacts are
    /// never written. The cart moves it however it likes, on whatever rails it likes, before the
    /// world steps. A hazard patrolling a fixed beat, a lift on a track, a door: things the world
    /// must know about without being asked to drive them.
    ///
    /// `false` — the default — is an ordinary cast member, moved by the world. What a prop saves
    /// is exactly the moving: everything else in the cast still meets it, stops at it, and is
    /// told about it, the same update it stands wherever the cart put it.
    fn prop(&self) -> bool {
        false
    }

    /// Whether the entity is on any of the same pixels as `other`.
    ///
    /// A rectangle rather than another entity, so the thing collided with does not have to be one:
    /// a door, a trigger the level puts down once and never moves, the area a switch covers. The
    /// cast is the world's business and needs none of this; see the [module docs](super#collision).
    ///
    /// ```no_run
    /// # use pixel8::physics::{Bounds, Kinetic};
    /// # fn f(bullet: &dyn Kinetic, doors: &[Bounds]) -> bool {
    /// doors.iter().any(|door| bullet.overlaps(*door))
    /// # }
    /// ```
    fn overlaps(&self, other: Bounds) -> bool {
        self.bounds().overlaps(other)
    }

    /// This entity as the trait object the world takes.
    ///
    /// [`World::step`](super::World::step) is handed the whole cast as a slice of
    /// `&mut dyn Kinetic`, and a cast is heterogeneous by nature — a hero, a badie, a lift, none of
    /// which share a type. This is the coercion, spelled once so that gathering them reads as a
    /// list of entities rather than as a list of casts:
    ///
    /// ```no_run
    /// # use pixel8::{physics::{Force, Gravity, Kinetic, World}, Context};
    /// # fn f(world: &mut World<64, Gravity>, ctx: &Context, hero: &mut impl Kinetic, badie: &mut impl Kinetic) {
    /// world.step(ctx, &mut [hero.as_kinetic(), badie.as_kinetic()]);
    /// # }
    /// ```
    ///
    /// There is nothing to implement and nothing to override: every entity has it already.
    fn as_kinetic(&mut self) -> &mut dyn Kinetic
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Contact, *};

    #[test]
    fn an_entity_weighs_one_and_says_nothing_else_about_itself_unless_it_wants_to() {
        /// A [`Kinetic`] that implements only what it has to — the shape of every entity in a
        /// cart that has never heard of mass.
        struct Pebble {
            body: Body,
            velocity: Velocity,
            contacts: Contacts,
        }

        impl Kinetic for Pebble {
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
                Bounds::of(&self.body, 1, 1)
            }
        }

        let pebble = Pebble {
            body: Body::new(0.0, 0.0),
            velocity: Velocity::new(1.0, 0.5),
            contacts: Contacts::default(),
        };
        assert_eq!(pebble.mass(), 1.0);
        // No rule of its own about what stops it: the scene's word is its word.
        assert!(pebble.solid().is_none());
        // And it carries nothing itself: it is stopped and it senses, and nobody else is ever told
        // about it.
        assert_eq!(pebble.sprite(), None);
        // Nor is it held anywhere: an entity that names no limits is free to leave the map.
        assert_eq!(pebble.confines(), None);
        // And it has met nothing, never having been stepped.
        assert_eq!(*pebble.contacts(), Contacts::empty());
    }

    #[test]
    fn the_contacts_slot_is_the_entitys_own_to_read_whenever_it_likes() {
        // The world writes it; everything else reads it, in the update or in the draw after.
        let mut walker = Walker::at(0.0, 0.0);
        assert_eq!(*walker.contacts(), Contacts::empty());
        *walker.contacts_mut() = Contact::Below.into();
        assert!(walker.contacts().below());
        assert!(!walker.contacts().above());
    }

    #[test]
    fn an_entity_covers_the_rectangle_it_says_it_does() {
        let walker = Walker::at(16.0, 16.0);
        let bounds = walker.bounds();
        assert_eq!((bounds.x(), bounds.y()), (16, 16));
        assert_eq!((bounds.width(), bounds.height()), (8, 8));
    }

    #[test]
    fn an_entity_overlaps_the_rectangles_it_shares_a_pixel_with() {
        let walker = Walker::at(16.0, 16.0);
        assert!(walker.overlaps(Walker::at(20.0, 20.0).bounds()));
        assert!(walker.overlaps(Walker::at(16.0, 23.0).bounds()));

        // A shared edge is not an overlap, on either axis.
        assert!(!walker.overlaps(Walker::at(24.0, 16.0).bounds()));
        assert!(!walker.overlaps(Walker::at(16.0, 24.0).bounds()));

        // The other party is a rectangle and need not be an entity at all — a door, a trigger,
        // a patrolling sprite nothing pushes. A tall one catches the walker its own width away.
        let door = Bounds::new(20, 0, 4, 64);
        assert!(walker.overlaps(door));
        assert!(!Walker::at(0.0, 16.0).overlaps(door));

        // And through the `dyn` the world keeps its cast in, which is where an entity is least
        // itself and the question still has to work.
        let entity: &dyn Kinetic = &walker;
        assert!(entity.overlaps(door));
    }

    #[test]
    fn an_entity_hands_itself_over_as_the_trait_object_a_cast_is_made_of() {
        // What a cart writes to gather a cast of things with nothing in common but this trait.
        let (mut walker, mut other) = (Walker::at(0.0, 0.0), Walker::at(16.0, 0.0));
        let cast: [&mut dyn Kinetic; 2] = [walker.as_kinetic(), other.as_kinetic()];
        assert_eq!(cast[0].body().pos(), (0.0, 0.0));
        assert_eq!(cast[1].body().pos(), (16.0, 0.0));

        // And the same entity through a shared `dyn`, which is all a cart's `draw` is handed.
        let entity: &dyn Kinetic = &walker;
        assert_eq!(entity.bounds(), Bounds::new(0, 0, 8, 8));
    }

    /// An entity with walls to answer to and a kind of its own — one sprite's worth of everything,
    /// which is what most of a cast is.
    struct Walker {
        body: Body,
        velocity: Velocity,
        contacts: Contacts,
    }

    impl Walker {
        /// One standing still at a pixel position.
        fn at(x: f32, y: f32) -> Self {
            Self {
                body: Body::new(x, y),
                velocity: Velocity::default(),
                contacts: Contacts::default(),
            }
        }
    }

    impl Kinetic for Walker {
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

        fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
            Some(SOLID.into())
        }

        fn sprite(&self) -> Option<SpriteId> {
            Some(WALKER_SPRITE)
        }
    }

    /// What this cart flags its walls with — on the map, and on every entity that is one.
    const SOLID: SpriteFlag = SpriteFlag::Flag0;

    /// The cell an entity of the test's own kind is drawn from, whose flags are what everybody
    /// else meets when they meet one.
    const WALKER_SPRITE: SpriteId = SpriteId(9);
}
