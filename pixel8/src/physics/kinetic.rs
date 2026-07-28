//! The entity forces act on, and the one call that moves it.

use super::{collide::Contacts, Collider, Force, Velocity};
use crate::{Body, Context};

/// An entity a [`Force`] can push: a [`Body`] and the [`Velocity`] it travels at.
///
/// Implementing it is two accessors, and it is what lets one gravity or wind be applied to a
/// whole cast of entities that otherwise have nothing in common. Everything past those two is
/// optional and describes the entity: what it weighs, and what shape it is when it hits a wall.
///
/// [`step`](Self::step) is what a cart calls each update, handing over the forces of the moment;
/// it is the only thing here that moves anything.
///
/// ```no_run
/// # use pixel8::{physics::{Gravity, Kinetic, Velocity}, *};
/// struct Crate {
///     body: Body,
///     velocity: Velocity,
/// }
///
/// impl Kinetic for Crate {
///     fn body_mut(&mut self) -> &mut Body {
///         &mut self.body
///     }
///
///     fn velocity_mut(&mut self) -> &mut Velocity {
///         &mut self.velocity
///     }
///
///     // A crate is heavy: the wind hardly shifts it, and it still falls like everything else.
///     fn mass(&self) -> f32 {
///         6.0
///     }
/// }
///
/// fn update(crates: &mut [Crate], gravity: &Gravity, ctx: &Context) {
///     for entity in crates {
///         entity.step(ctx, &[gravity]);
///     }
/// }
/// ```
pub trait Kinetic: dynamic::AsKinetic {
    /// The body the entity is drawn from and collides with.
    fn body_mut(&mut self) -> &mut Body;

    /// The velocity forces act on.
    fn velocity_mut(&mut self) -> &mut Velocity;

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

    /// The solid rectangle the entity occupies, if it collides with the map at all.
    ///
    /// `None` — the default — is an entity that goes wherever its velocity takes it: a particle, a
    /// bird, a bullet the cart checks itself. Anything that has to stop at walls returns a
    /// [`Collider`], and [`step`](Self::step) resolves the map for it.
    ///
    /// ```no_run
    /// # use pixel8::{physics::Collider, SpriteFlag};
    /// # const SOLID: SpriteFlag = SpriteFlag::Flag0;
    /// # struct Hero;
    /// # impl Hero {
    /// fn collider(&self) -> Option<Collider> {
    ///     Collider::new(8, 8, SOLID).ok()
    /// }
    /// # }
    /// ```
    fn collider(&self) -> Option<Collider> {
        None
    }

    /// Moves the entity one update: the `forces` acting on it, then the map, then the body.
    ///
    /// In order: every force in `forces` bends the velocity, in slice order; a
    /// [`collider`](Self::collider) — if there is one — has whatever ran into a wall taken out of
    /// it, on each axis separately; the velocity that survives is stored back on the entity and
    /// the [`Body`] is moved by it. What comes back is the sides that were touched, which is where
    /// a platformer reads its *grounded* from:
    ///
    /// ```no_run
    /// # use pixel8::{physics::{Force, Kinetic}, Context};
    /// # fn f(entity: &mut impl Kinetic, ctx: &Context, weather: &[&dyn Force], grounded: &mut bool) {
    /// *grounded = entity.step(ctx, weather).below();
    /// # }
    /// ```
    ///
    /// The forces are passed rather than kept, so the scene owns its own weather: a gust the whole
    /// cast is bent by is one [`Wind`](super::Wind) the world holds and hands to each entity in
    /// turn, and an entity that answers to something of its own is stepped with a slice of its
    /// own. Nothing is stored on the entity between updates but its velocity.
    ///
    /// An axis that was blocked is zeroed in the stored velocity too, not just in this update's
    /// movement: a fall that lands has been spent, and something that walked into a wall is not
    /// still walking. An entity driven by the buttons writes its sideways speed afresh every
    /// update and never notices; one carrying its own momentum does, which is the point.
    fn step(&mut self, ctx: &Context, forces: &[&dyn Force]) -> Contacts {
        for force in forces {
            // `as_kinetic` is the coercion a default method cannot write for itself, `Self` being
            // unsized as far as this body knows. It costs nothing and there is nothing to write:
            // every `Kinetic` has it already.
            force.apply(self.as_kinetic());
        }

        // `Velocity` is `Copy`, so this reads out what the forces left behind without holding a
        // borrow of the entity over the resolution below.
        let mut velocity = *self.velocity_mut();
        let contacts = match self.collider() {
            Some(collider) => {
                let position = self.body_mut().pos();
                let (survived, contacts) = collider.resolve(position, velocity, |x, y| {
                    ctx.map_tile(x, y)
                        .is_some_and(|tile| collider.stops_at(ctx.sprite_flags(tile)))
                });
                velocity = survived;
                contacts
            }
            None => Contacts::empty(),
        };

        *self.velocity_mut() = velocity;
        self.body_mut().move_by(velocity.dx, velocity.dy);
        contacts
    }
}

/// The coercion [`Kinetic::step`] needs to hand an entity to a [`Force`], which takes one as a
/// trait object.
///
/// A default method's `Self` is not known to be sized, so it cannot cast itself to
/// `&mut dyn Kinetic` on its own. This says it for every entity there is and asks nothing of a
/// cart: implementing [`Kinetic`] implements this, and no cart ever names it.
mod dynamic {
    use super::Kinetic;

    /// See the [module docs](self).
    pub trait AsKinetic {
        /// This entity as the trait object a [`Force`](super::Force) takes.
        fn as_kinetic(&mut self) -> &mut dyn Kinetic;
    }

    impl<T: Kinetic> AsKinetic for T {
        fn as_kinetic(&mut self) -> &mut dyn Kinetic {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{force::Mob, Atmosphere, Gravity, Wind},
        *,
    };
    use crate::SpriteFlag;

    /// A level's pull, as an ordinary constant of the cart's own — `Gravity::new` is `const`.
    const GRAVITY: Gravity = Gravity::new();

    #[test]
    fn an_entity_weighs_one_and_collides_with_nothing_unless_it_says_otherwise() {
        /// A [`Kinetic`] that implements only the two accessors it has to — the shape of every
        /// entity in a cart that has never heard of mass.
        struct Pebble {
            body: Body,
            velocity: Velocity,
        }

        impl Kinetic for Pebble {
            fn body_mut(&mut self) -> &mut Body {
                &mut self.body
            }

            fn velocity_mut(&mut self) -> &mut Velocity {
                &mut self.velocity
            }
        }

        let ctx = Context { _private: () };
        let mut pebble = Pebble {
            body: Body::new(0.0, 0.0),
            velocity: Velocity::new(1.0, 0.5),
        };
        assert_eq!(pebble.mass(), 1.0);
        assert_eq!(pebble.collider(), None);

        // Handed nothing and with nothing in its way, a step is exactly its velocity.
        assert_eq!(pebble.step(&ctx, &[]), Contacts::empty());
        assert_eq!(pebble.body.pos(), (1.0, 0.5));
        assert_eq!(pebble.velocity, Velocity::new(1.0, 0.5));
    }

    #[test]
    fn step_moves_the_body_along_the_velocity() {
        let ctx = Context { _private: () };
        let mut mob = Mob::moving(1.5, -2.0);
        assert_eq!(mob.step(&ctx, &[]), Contacts::empty());
        assert_eq!(mob.body.pos(), (1.5, -2.0));
        mob.step(&ctx, &[]);
        assert_eq!(mob.body.pos(), (3.0, -4.0));
    }

    #[test]
    fn the_forces_handed_to_a_step_are_applied_to_the_entity() {
        let ctx = Context { _private: () };
        let mut mob = Mob::new();
        mob.step(&ctx, &[&GRAVITY]);
        // One update's pull, both in the velocity it kept and in where it ended up.
        assert_eq!(mob.velocity, Velocity::new(0.0, Gravity::DEFAULT_STRENGTH));
        assert_eq!(mob.body.pos(), (0.0, Gravity::DEFAULT_STRENGTH));

        for _ in 0..1_000 {
            mob.step(&ctx, &[&GRAVITY]);
        }
        assert_eq!(mob.velocity.dy, Gravity::DEFAULT_TERMINAL_VELOCITY);
    }

    #[test]
    fn a_step_handed_nothing_applies_nothing() {
        // The forces belong to the scene, not to the entity, so an entity left out of the
        // weather one update carries on at whatever it was already doing.
        let ctx = Context { _private: () };
        let mut mob = Mob::moving(0.0, 1.0);
        mob.step(&ctx, &[]);
        assert_eq!(mob.velocity, Velocity::new(0.0, 1.0));
        assert_eq!(mob.body.pos(), (0.0, 1.0));
    }

    #[test]
    fn forces_run_in_the_order_the_slice_puts_them() {
        const PULL: Gravity = Gravity::new().with_terminal_velocity(f32::MAX);
        const AIR: Atmosphere = Atmosphere::new();

        let ctx = Context { _private: () };
        let (mut pulled, mut aired) = (Mob::new(), Mob::new());
        pulled.step(&ctx, &[&PULL, &AIR]);
        aired.step(&ctx, &[&AIR, &PULL]);

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
    fn a_force_applied_on_its_own_only_changes_the_velocity() {
        // Nothing moves until the entity is stepped, whoever applied what to it.
        let mut mob = Mob::new();
        GRAVITY.apply(&mut mob);
        assert_eq!(mob.velocity, Velocity::new(0.0, Gravity::DEFAULT_STRENGTH));
        assert_eq!(mob.body.pos(), (0.0, 0.0));
    }

    #[test]
    fn forces_of_different_types_go_in_one_slice() {
        struct Updraft;

        impl Force for Updraft {
            fn apply(&self, entity: &mut dyn Kinetic) {
                entity.velocity_mut().dy -= 0.5;
            }
        }

        let ctx = Context { _private: () };
        let wind = Wind::new(1.0);
        let weather: [&dyn Force; 3] = [&GRAVITY, &wind, &Updraft];

        let mut mob = Mob::new();
        mob.step(&ctx, &weather);
        assert_eq!(mob.velocity.dx, 0.05);
        assert_eq!(mob.velocity.dy, Gravity::DEFAULT_STRENGTH - 0.5);
    }

    #[test]
    fn mass_is_read_off_the_entity_by_the_forces_a_step_runs() {
        // The same wind on two entities that differ in nothing but what they weigh, and it is
        // `step` that carries the one to the other.
        let ctx = Context { _private: () };
        let wind = Wind::new(2.0);
        let (mut light, mut heavy) = (Mob::with_mass(0.5), Mob::with_mass(4.0));
        light.step(&ctx, &[&wind]);
        heavy.step(&ctx, &[&wind]);
        assert!(
            light.velocity.dx > heavy.velocity.dx,
            "the mass was not read: {} against {}",
            light.velocity.dx,
            heavy.velocity.dx
        );
    }

    #[test]
    fn a_collider_less_entity_steps_straight_through_everything() {
        // With no collider there is no map lookup at all, which is what keeps a particle cheap.
        let ctx = Context { _private: () };
        let mut mob = Mob::moving(-4.0, 4.0);
        assert_eq!(mob.step(&ctx, &[]), Contacts::empty());
        assert_eq!(mob.body.pos(), (-4.0, 4.0));
    }

    #[test]
    fn an_entity_with_a_collider_asks_the_map() {
        /// One that does have walls to answer to. The native map is empty — every tile reads as
        /// sprite 0 with no flags — so nothing stops it here; what the resolution does when
        /// something *is* in the way is tested against a written-down map in `collide`.
        struct Walker {
            body: Body,
            velocity: Velocity,
        }

        impl Kinetic for Walker {
            fn body_mut(&mut self) -> &mut Body {
                &mut self.body
            }

            fn velocity_mut(&mut self) -> &mut Velocity {
                &mut self.velocity
            }

            fn collider(&self) -> Option<Collider> {
                Collider::new(8, 8, SpriteFlag::Flag0).ok()
            }
        }

        let ctx = Context { _private: () };
        let mut walker = Walker {
            body: Body::new(16.0, 16.0),
            velocity: Velocity::new(0.5, 1.0),
        };
        assert_eq!(walker.collider().unwrap().width(), 8);
        assert_eq!(walker.step(&ctx, &[]), Contacts::empty());
        assert_eq!(walker.body.pos(), (16.5, 17.0));
    }

    #[test]
    fn both_traits_survive_being_trait_objects() {
        // Carts keep casts of entities that have nothing in common but this trait, and weather
        // whose types have nothing in common either. Both have to work through a `dyn`.
        let ctx = Context { _private: () };
        let mut mob = Mob::moving(1.0, 0.0);
        let entity: &mut dyn Kinetic = &mut mob;
        let forces: [&dyn Force; 1] = [&GRAVITY];
        entity.step(&ctx, &forces);
        assert_eq!(mob.body.pos(), (1.0, Gravity::DEFAULT_STRENGTH));
    }
}
