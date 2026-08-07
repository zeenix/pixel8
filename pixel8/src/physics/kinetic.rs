//! The engine's view of one cast member: what it is, and nothing about what it does.
//!
//! Not a cart's business, and no longer anything a cart writes. A cart describes its members once
//! as it [enlists](super::World::enlist) them and the [`World`](super::World) keeps them; this
//! trait is the seam the engine is written against, so that the same
//! [`step_cast`](super::World::step_hosted) can be run over the world's own
//! [`Recast`](super::wire::Recast)s on the console's side of the wire and over whatever the
//! module's own tests write down. It is `pub` because the console reaches it
//! across the crate boundary, and hidden because nothing outside these two callers should.

use super::{Bounds, Contacts, Velocity};
use crate::{BitFlags, Body, SpriteFlag, SpriteId};

/// Something the engine can step: a [`Body`], the [`Velocity`] it travels at, the [`Contacts`]
/// slot it is told what it met in, and the handful of descriptions a resolution reads.
///
/// Every method here *describes*. Nothing on this trait moves anything, asks the console anything,
/// or detects anything: an implementor says where it is, how big it is, what stops it, what it
/// wears and how far it is let go, and the step does the rest for the whole cast at once.
///
/// Every answer is read once, as the step begins — after the world's forces have bent the
/// velocities, before anything moves — and holds for the whole of that step; the one exception is
/// [`bounds`](Self::bounds), whose rectangle keeps its seat on the body wherever the step carries
/// it.
#[doc(hidden)]
pub trait Kinetic {
    /// Where it is: the body it is drawn from and collides with.
    fn body(&self) -> &Body;

    /// The same body, for the world that moves it.
    fn body_mut(&mut self) -> &mut Body;

    /// The velocity forces act on and the world spends.
    fn velocity_mut(&mut self) -> &mut Velocity;

    /// What the last step ran into.
    fn contacts(&self) -> &Contacts;

    /// The same slot, for the world that fills it.
    fn contacts_mut(&mut self) -> &mut Contacts;

    /// The rectangle it covers, in the coordinates its [`Body`] is in.
    ///
    /// The one rectangle a member has, and everything about where it *is* rather than what is
    /// pushing it goes through it: what the map's tiles stop, what the rest of the cast meets, and
    /// what [`confines`](Self::confines) holds inside the world.
    fn bounds(&self) -> Bounds;

    /// What means *wall* to it, where the world's word is not its word — `None` for the world's.
    fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
        None
    }

    /// Which sprite flags it cares to be told about. Everything, unless it says otherwise.
    fn heeds(&self) -> BitFlags<SpriteFlag> {
        BitFlags::all()
    }

    /// What it is made of, as far as everybody else is concerned: the sprite it wears.
    fn sprite(&self) -> Option<SpriteId> {
        None
    }

    /// The rectangle it is never let out of, if there is one.
    fn confines(&self) -> Option<Bounds> {
        None
    }

    /// Whether it is a prop: in the cast to be met, never to be moved.
    fn prop(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Contact, *};

    #[test]
    fn a_member_says_nothing_about_itself_unless_it_wants_to() {
        /// A [`Kinetic`] that implements only what it has to — the smallest thing the engine can
        /// be asked to step.
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
        // No rule of its own about what stops it: the scene's word is its word.
        assert!(pebble.solid().is_none());
        // And it carries nothing itself: it is stopped and it senses, and nobody else is ever told
        // about it.
        assert_eq!(pebble.sprite(), None);
        // Nor is it held anywhere: one that names no limits is free to leave the map.
        assert_eq!(pebble.confines(), None);
        // It is told about everything it meets, and it is moved rather than merely met.
        assert_eq!(pebble.heeds(), BitFlags::all());
        assert!(!pebble.prop());
        // And it has met nothing, never having been stepped.
        assert_eq!(*pebble.contacts(), Contacts::empty());
    }

    #[test]
    fn the_contacts_slot_is_where_a_step_leaves_its_answer() {
        // The engine writes it; the world reads it back out into the record it keeps.
        let mut walker = Walker::at(0.0, 0.0);
        assert_eq!(*walker.contacts(), Contacts::empty());
        *walker.contacts_mut() = Contact::Below.into();
        assert!(walker.contacts().below());
        assert!(!walker.contacts().above());
    }

    #[test]
    fn a_member_covers_the_rectangle_it_says_it_does() {
        let walker = Walker::at(16.0, 16.0);
        let bounds = walker.bounds();
        assert_eq!((bounds.x(), bounds.y()), (16, 16));
        assert_eq!((bounds.width(), bounds.height()), (8, 8));

        // And through the `dyn` the engine keeps its cast in, which is where a member is least
        // itself and the question still has to work.
        let member: &dyn Kinetic = &walker;
        assert_eq!(member.bounds(), Bounds::new(16, 16, 8, 8));
    }

    /// A member with walls to answer to and a kind of its own — one sprite's worth of everything,
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

    /// What this cart flags its walls with — on the map, and on every member that is one.
    const SOLID: SpriteFlag = SpriteFlag::Flag0;

    /// The cell a member of the test's own kind is drawn from, whose flags are what everybody else
    /// meets when they meet one.
    const WALKER_SPRITE: SpriteId = SpriteId(9);
}
