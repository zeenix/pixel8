//! Gravity, air, wind, collision, and force fields of your own.
//!
//! Anything that falls in a cart is written the same three ways: a `vy` kept next to the entity's
//! [`Body`](crate::Body), a constant added to it every update, and a cap so that a long drop does
//! not end with the entity a screen below the floor. This module gives those pieces names. A
//! [`Velocity`] is the per-update movement an entity carries, a [`Force`] is anything that bends
//! one, and a [`Kinetic`] is an entity that has both — so one [`Gravity`] can be the level's
//! gravity rather than a constant repeated in every entity's update, and a cart's own force fields
//! are just another `impl Force`.
//!
//! Forces act on velocity, never on position, and no entity moves itself. [`World`] is where it all
//! meets: it owns the scene's weather, and handed the whole cast once an update it runs those
//! forces over each entity, stops whatever ran into the map's tiles or into the rest of the cast,
//! holds each inside the rectangle it may not leave, moves the [`Body`](crate::Body) with what
//! survives, and writes what was met into the entity's own [`Contacts`] slot. One call a scene, an
//! update.
//!
//! The weather belongs to the scene rather than to the things in it. Nothing is stored on an
//! entity between updates but its velocity and its contacts: the same gust that bends the whole
//! cast is one [`Wind`] the world [owns](World::with_forces), driven where it lives.
//!
//! Nothing here allocates or pulls in a dependency; a force costs a couple of floats and a
//! multiplication or two an update.
//!
//! Everything here stays in this module — a cart's `use pixel8::*;` does not reach it, so name
//! what the game needs:
//!
//! ```no_run
//! use pixel8::{
//!     physics::{Atmosphere, Bounds, Contacts, Gravity, Kinetic, Velocity, Wind, World},
//!     *,
//! };
//!
//! struct Leaf {
//!     body: Body,
//!     velocity: Velocity,
//!     contacts: Contacts,
//! }
//!
//! impl Kinetic for Leaf {
//!     fn body(&self) -> &Body {
//!         &self.body
//!     }
//!
//!     fn body_mut(&mut self) -> &mut Body {
//!         &mut self.body
//!     }
//!
//!     fn velocity_mut(&mut self) -> &mut Velocity {
//!         &mut self.velocity
//!     }
//!
//!     fn contacts(&self) -> &Contacts {
//!         &self.contacts
//!     }
//!
//!     fn contacts_mut(&mut self) -> &mut Contacts {
//!         &mut self.contacts
//!     }
//!
//!     fn bounds(&self) -> Bounds {
//!         Bounds::of(&self.body, 4, 4)
//!     }
//!
//!     // A leaf weighs next to nothing, so the wind has three times the grip on it — and the
//!     // still air three times the drag. Gravity does not read this: everything falls alike.
//!     fn mass(&self) -> f32 {
//!         0.3
//!     }
//! }
//!
//! struct Autumn {
//!     leaves: [Leaf; 16],
//!     // The one thing that moves any of them, owning the scene's whole weather: the pull, the
//!     // air the leaves fall through, and a wind that gusts and so cannot be a constant.
//!     world: World<16, (Gravity, Atmosphere, Wind)>,
//! }
//!
//! impl Game for Autumn {
//!     fn update(&mut self, ctx: &mut Context) {
//!         // The gust first, where it lives, so every leaf is bent by the same one.
//!         self.world.forces_mut().2.update(ctx);
//!         // The cast, gathered where it lives and handed over whole.
//!         let mut cast = self.leaves.each_mut().map(Kinetic::as_kinetic);
//!         self.world.step(ctx, &mut cast);
//!     }
//!
//!     fn draw(&self, gfx: &mut Graphics) {
//!         gfx.clear(Color::BLACK);
//!         for leaf in &self.leaves {
//!             gfx.sprite(SpriteId(1), leaf.body.draw_x(), leaf.body.draw_y());
//!         }
//!     }
//! }
//! ```
//!
//! # Units
//!
//! Velocities are in pixels per update and accelerations in pixels per update squared — the units
//! [`Body::move_by`](crate::Body::move_by) already speaks. They are per *update* and not per
//! second, so a 30 fps cart tunes its constants, exactly as it already must for everything else it
//! moves.
//!
//! # Mass
//!
//! [`Kinetic::mass`] is how hard an entity is to push, relative to everything else in the scene:
//! `1.0` is the default nobody has to think about, `4.0` takes four times the shove for the same
//! movement and `0.25` a quarter of it. An entity opts in by overriding the one method, and a
//! cart that never mentions mass carries on exactly as it did.
//!
//! What to make of it is each force's business. [`Wind`] divides its grip by mass, so the gale
//! that carries a leaf off barely stirs a boulder beside it, and [`Atmosphere`] divides its drag
//! by it in the same way. [`Gravity`] never reads it at all: everything falls alike, whatever it
//! weighs. Mass is how hard a thing is to push, not how hard it falls — the feather and the anvil
//! are told apart by the air between them, not by the pull.
//!
//! # Gravity
//!
//! [`Gravity::new`] is the pull a platformer usually arrives at by trial and error: a quarter of a
//! pixel per update squared, and a fall that tops out at four pixels an update. The terminal
//! velocity is the part worth keeping even when the strength is retuned — without it, a long fall
//! ends with the entity moving further in one update than a wall is thick, and it goes straight
//! through.
//!
//! It pulls whichever way it is pointed, so a cart is not stuck with down:
//!
//! ```no_run
//! # use pixel8::{physics::Gravity, Direction};
//! // The moon: an eighth of the pull, and nothing falls very fast there.
//! let moon = Gravity::new().with_strength(0.03).with_terminal_velocity(1.5);
//! // A station spinning the other way up, or a room the player has walked into upside down.
//! let ceiling = Gravity::new().with_direction(Direction::Up);
//! ```
//!
//! # Atmosphere
//!
//! Gravity's cap is the cheap way to keep a fall in hand. [`Atmosphere`] is the honest one: air
//! that takes a share of whatever moves through it, every update, on every axis. A fall under it
//! settles because the drag grows with the speed until it matches the pull, and sideways motion
//! slows too — which the cap never did.
//!
//! Where gravity is blind to [`mass`](Kinetic::mass), the air is not, and that is what finally
//! tells the feather from the anvil: the same air takes a great share of the light thing and
//! almost nothing of the heavy one, so the feather settles to a drift while the anvil goes on
//! gaining. [`Atmosphere::new`] is air at sea level, tuned so that a `1.0`-mass body settles
//! exactly where the default [`Gravity`] would have capped it.
//!
//! The two work together — a cart wanting the air alone to decide its terminal velocity puts
//! gravity's cap out of the way with `with_terminal_velocity(f32::MAX)` — and
//! [`Atmosphere::vacuum`] is the airless version, where nothing drags and everything falls
//! forever.
//!
//! # Wind
//!
//! A [`Wind`] is named for the side it comes *from*, the way weather always is: the default blows
//! in over the left edge of the screen and pushes things to the right. Unlike gravity it does not
//! accelerate what it pushes forever — velocity eases *towards* the wind's speed and stops there,
//! which is the drag a real wind has. How fast it gets there is
//! [`with_exposure`](Wind::with_exposure) — a leaf takes the wind almost at once, a boulder barely
//! notices it — divided by what the entity itself weighs, so something heavy enough shrugs off a
//! wind it is fully exposed to.
//!
//! A steady wind is a constant. [`with_gusts`](Wind::with_gusts) makes its speed wander inside a
//! range instead, never quite repeating itself, which is what stops a windy scene from reading as
//! a scrolling texture. Gusty wind needs [`update`](Wind::update) once an update, before it is
//! handed to anything.
//!
//! # Collision
//!
//! Nothing here asks a cart to walk its own pairs, and nothing asks an entity to detect anything.
//! A [`Kinetic`] only *describes*: the rectangle it covers, the flags that stop it, the sprite it
//! wears, how far it is let go. [`World::step`] is handed the whole cast and does the rest — it
//! stops each entity at everything solid to it, tiles and cast alike, and writes the flags of
//! everything it ran into into that entity's own [`Contacts`] slot. The cart registers and reads;
//! it never detects.
//!
//! Every entity still says what rectangle it covers — [`Kinetic::bounds`] — because that is what
//! the step is judged over, and the same rectangle answers the two questions a cart asks off its
//! own bat: [`Kinetic::overlaps`] against a rectangle it knows about already, and
//! [`Bounds::on_screen`] for an entity that has left the screen altogether.
//!
//! ```no_run
//! # use pixel8::physics::{Bounds, Kinetic};
//! /// A bullet against the doors the level put down once — and nothing at all once it is off
//! /// screen.
//! fn hit(bullet: &dyn Kinetic, doors: &[Bounds]) -> bool {
//!     bullet.bounds().on_screen() && doors.iter().any(|door| bullet.overlaps(*door))
//! }
//! ```
//!
//! ## What is in the way
//!
//! The map is the level standing still, and the console has always answered for it: what a scene
//! calls a wall is a sprite flag, named once on the world in [`World::with_solid`], and every
//! entity stops at every tile carrying it, over the rectangle it covers everywhere else. It is
//! the flag a cart already marks its walls with for [`Graphics::map`](crate::Graphics::map). An
//! entity the scene's word does not fit answers [`Kinetic::solid`] with rules of its own, which
//! replace the world's for that entity alone.
//!
//! The tiles taught the console one vocabulary — a thing *is* whatever flags it carries, written
//! once in the sprite editor — and the cast speaks it too. [`Kinetic::sprite`] is where an entity
//! says which cell it wears, and the flags on that cell are what everybody else meets when they
//! meet it. So one [`World::step`] settles both: a flag shared with `solid` stops the entity,
//! whether it is on a tile or on a neighbour, in the same one-axis-at-a-time pass; a rising lift
//! pushes the rider standing on it and the rider goes on reading [`below`](Contacts::below); and
//! everything met, wall or not, tile or entity, comes back in [`Contacts::touched`]. One step
//! answers the three questions an update asks at once: am I grounded, am I in the water, did I walk
//! into the badie.
//!
//! Three flags, three directions, and they are the whole of it:
//!
//! * [`solid`](Kinetic::solid) — which flags are a **wall to me**. The world's word
//!   ([`World::with_solid`]) by default, and rules of this entity's own where it gives them; under
//!   a world that declared nothing, nothing anywhere stops it.
//! * [`sprite`](Kinetic::sprite) — which cell I **wear**, and so which flags others meet in me.
//!   `None` by default: nobody is stopped by it, and nobody is told about it. It is still stopped
//!   by everything, and still told everything — a sensor needs no flag of its own.
//! * [`heeds`](Kinetic::heeds) — which flags I **care to be told about**. Everything by default,
//!   which is why a cart never has to think about it until it wants to.
//!
//! ## What an entity cares to meet
//!
//! An entity describes what it wants to hear about, and the world spends nothing on the rest. That
//! is [`heeds`](Kinetic::heeds), and it is the one place a cart can make a scene cheaper by saying
//! something true about it: a bullet fired at the enemy cares about the enemy and about nothing
//! else in the sky — not the other bullets beside it, not the tiles scrolling past behind it. Say
//! so, and a neighbour carrying nothing it heeds is refused before a single edge of that neighbour
//! is worked out, and a tile's flags are dropped before they are collected. In a scene where
//! everything is in one cast, that is most of the work of an update, and it is spent on answers
//! nobody was going to read.
//!
//! The promise stays one sentence, whatever a cart narrows: **you are told what you heed, and you
//! are stopped by what you call solid.** `solid` is heeded whether it was named or not, so
//! narrowing this can never cost an entity a wall — one it never asked to hear about still stops
//! it, and being stopped by it still reports it. And it reads the same off a tile as off a
//! neighbour: the mask is one word, applied to the one vocabulary.
//!
//! A whole scene can say the same thing about its map. [`World::mapless`] is a world whose tiles
//! are the picture behind the fight and nothing else — a shoot-'em-up scrolling a landscape past,
//! anything whose collisions are all between moving things — and its steps never ask the map a
//! question. [`World::new`] is unchanged and reads the map as it always has.
//!
//! ```no_run
//! # use pixel8::{physics::{Kinetic, World}, *};
//! # const AIRCRAFT: SpriteFlag = SpriteFlag::Flag0;
//! # const ENEMY_SHOT: SpriteFlag = SpriteFlag::Flag1;
//! // The level scrolls past behind the fight; nothing on it is in anybody's way.
//! const WORLD: World = World::mapless();
//!
//! # struct Lady;
//! # impl Lady {
//! // And she is rammed and she is shot, and the rest of the sky is somebody else's business.
//! fn heeds(&self) -> BitFlags<SpriteFlag> {
//!     AIRCRAFT | ENEMY_SHOT
//! }
//! # }
//! ```
//!
//! Stopping and meeting are answered over different ground. An entity is *stopped* where an axis
//! was trying to go — the endpoint, which is where a wall has to be to be one — and it is told what
//! it *met* over the whole of the step: where it began, the ground each axis swept across, and
//! where it ended up. So an entity that walks out of the water this update is told it was in the
//! water, and a hazard crossed between one pixel and the next is named rather than missed. The
//! difference has one consequence, and it is better said than discovered: something thinner than an
//! update's movement can be stepped clean over without stopping the entity, and is reported all the
//! same. [`Gravity`]'s terminal velocity is the guard — nothing moving slower than a wall is thick
//! can pass through one — which is exactly why it is there.
//!
//! ```no_run
//! # use pixel8::{physics::{Gravity, Kinetic, World}, *};
//! # const GRAVITY: Gravity = Gravity::new();
//! # const WATER: SpriteFlag = SpriteFlag::Flag3;
//! # fn f(world: &mut World<64, Gravity>, ctx: &Context, hero: &mut impl Kinetic) {
//! world.step(ctx, &mut [hero.as_kinetic()]);
//! let (grounded, swimming) = (hero.contacts().below(), hero.contacts().touches(WATER));
//! # }
//! ```
//!
//! ## The cast, and the order it is in
//!
//! The cast is whatever the cart hands over: a slice of `&mut dyn Kinetic`, gathered fresh each
//! update out of the fields, arrays and `heapless::Vec`s the cart keeps its entities in.
//! [`Kinetic::as_kinetic`] is the one word that makes an entity a cast member.
//!
//! Three things follow from that, and they are the whole of the contract:
//!
//! * **Same frame.** Everything is where it is. An entity meets its neighbours at the rectangles
//!   they cover *now*, not at a picture of them taken earlier, so a shot that lands is a hit its
//!   target feels in the same update, and something that dies on arrival can be dropped the moment
//!   the step returns.
//! * **Cast order.** Entities are stepped one at a time, front to back, and each of them meets the
//!   ones stepped before it where they have *just* moved to. So a lift put before its rider carries
//!   the rider with no lag at all, and one put after it is a frame behind. Order the cast the way
//!   the scene works.
//! * **Never itself.** The world knows who is who — an entity is simply left out of its own
//!   questions — so an entity's own kind is a wall like anybody else's. Two crates whose
//!   [`sprite`](Kinetic::sprite) carries `CRATE`, each with `CRATE` [solid](Kinetic::solid) to it,
//!   block each other and neither is ever shoved off its own feet.
//!
//! And one refinement for the things a cart drives itself: a cast member that says it is a
//! [prop](Kinetic::prop) is met and never moved. Its rectangle and its flags stand in everybody's
//! way from wherever the cart last put it — a hazard patrolling a fixed beat, a lift on a track —
//! and the forces, the walls and the contacts all pass it by.
//!
//! ```no_run
//! # use pixel8::{physics::{Bounds, Contacts, Gravity, Kinetic, Velocity, World}, *};
//! /// Walls and floors are whatever the cart flagged as such in the sprite editor.
//! const SOLID: SpriteFlag = SpriteFlag::Flag0;
//! /// And this walker's own sprite is flagged `WALKER`, which is how everybody else's step reports
//! /// having met one — and how two walkers come to be walls to each other.
//! const WALKER: SpriteFlag = SpriteFlag::Flag1;
//! /// The cell it is drawn from, which is where that flag is written.
//! const WALKER_SPRITE: SpriteId = SpriteId(4);
//! /// The level's pull. Nothing gusts here, so it is a constant.
//! const GRAVITY: Gravity = Gravity::new();
//!
//! struct Walker {
//!     body: Body,
//!     velocity: Velocity,
//!     contacts: Contacts,
//! }
//!
//! impl Kinetic for Walker {
//!     fn body(&self) -> &Body {
//!         &self.body
//!     }
//!
//!     fn body_mut(&mut self) -> &mut Body {
//!         &mut self.body
//!     }
//!
//!     fn velocity_mut(&mut self) -> &mut Velocity {
//!         &mut self.velocity
//!     }
//!
//!     // Where the world writes what this walker ran into, and where the cart reads it.
//!     fn contacts(&self) -> &Contacts {
//!         &self.contacts
//!     }
//!
//!     fn contacts_mut(&mut self) -> &mut Contacts {
//!         &mut self.contacts
//!     }
//!
//!     // One sprite's worth: what a wall stops, and what everything else judges it by.
//!     fn bounds(&self) -> Bounds {
//!         Bounds::of(&self.body, 8, 8)
//!     }
//!
//!     // What it is made of, so that everybody who meets it is told they met a walker.
//!     fn sprite(&self) -> Option<SpriteId> {
//!         Some(WALKER_SPRITE)
//!     }
//!
//!     // And what is a wall to it — a rule of its own rather than the scene's, because its own
//!     // kind is in it, which no world-wide word could say for walkers alone.
//!     fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
//!         Some(SOLID | WALKER)
//!     }
//!
//!     // The edge of the world, which is not a wall and is on no tile: said once here, and the
//!     // world never lets the walker out of it.
//!     fn confines(&self) -> Option<Bounds> {
//!         Some(Bounds::screen())
//!     }
//! }
//!
//! impl Walker {
//!     /// What the buttons ask for, written into the velocity before the world runs.
//!     fn steer(&mut self, ctx: &Context) {
//!         if self.contacts.below() && ctx.is_button_pressed(Button::O) {
//!             self.velocity.dy = -3.25;
//!         }
//!         self.velocity.dx = if ctx.is_button_down(Button::Left) {
//!             -0.7
//!         } else if ctx.is_button_down(Button::Right) {
//!             0.7
//!         } else {
//!             0.0
//!         };
//!     }
//! }
//!
//! fn update(world: &mut World<3, Gravity>, ctx: &mut Context, walkers: &mut [Walker; 3]) {
//!     for walker in walkers.iter_mut() {
//!         walker.steer(ctx);
//!     }
//!     // The pull, the walls, the walkers and the movement, in one call.
//!     world.step(ctx, &mut walkers.each_mut().map(Kinetic::as_kinetic));
//! }
//! ```
//!
//! One rectangle does both jobs, so a wall stops an entity exactly where another entity would have
//! hit it.
//!
//! Flags say what *kind* of thing was met and never which one — two patches of water read as one
//! patch of water, and one badie reads like another. A cart that must know which, because
//! something has to happen to it, already holds the thing: it looks at its own state, and asks
//! [`Kinetic::overlaps`] if it must ask a rectangle anything at all. The step says *the hero met
//! a badie*; which badie, and what it costs the hero, is the cart's and was never anybody else's.
//!
//! ## The edge of the world
//!
//! The last thing an entity runs into is not a wall and is nowhere on the map: nothing stops it
//! walking off the last tile and falling for ever. [`Kinetic::confines`] is where an entity says
//! it may not — [`Bounds::screen`], or the level itself where that is the bigger of the two — and
//! [`World::step`] holds it there, putting the rectangle back against the edge and spending the
//! speed that took it out, exactly as a wall would have. It is declared once, with everything else
//! the entity says about itself, rather than enforced by a call an update can forget; the sides it
//! was held at arrive in the same [`Contacts`], so a hold at the bottom of the level reads
//! [`below`](Contacts::below) as a floor tile does.
//!
//! ```no_run
//! # use pixel8::physics::{Bounds, Kinetic};
//! # struct Hero;
//! # impl Hero {
//! fn confines(&self) -> Option<Bounds> {
//!     Some(Bounds::screen())
//! }
//! # }
//! ```
//!
//! Saying nothing — the default — is an entity free to leave, which is what a bullet or a spent
//! enemy wants: it walks off the map, and the cart drops it when [`Bounds::on_screen`] says it
//! has gone.
//!
//! # Forces of your own
//!
//! A [`Force`] is one method, so a cart's own force fields — a current, a magnet, the drag of deep
//! water — work everywhere the ones here do: hand [`with_forces`](World::with_forces) a tuple with
//! them in it and the world owns the lot, applying it to every cast member it moves — a
//! [prop](Kinetic::prop) steers itself, so no force bends one — before any of them takes a step:
//!
//! ```no_run
//! # use pixel8::{physics::{Force, Gravity, Kinetic, World}, Context};
//! /// Water: it drags whatever moves through it, and it holds it up a little.
//! struct Water {
//!     drag: f32,
//! }
//!
//! impl Force for Water {
//!     fn apply(&self, entity: &mut dyn Kinetic) {
//!         // Something heavier carries its momentum through the water further, so the drag eases
//!         // it less; the clamp is what stops a light enough diver being dragged past a halt.
//!         // Read before the velocity is taken, or the two borrows of the diver overlap.
//!         let drag = (self.drag / entity.mass()).clamp(0.0, 1.0);
//!         let velocity = entity.velocity_mut();
//!         velocity.dx -= velocity.dx * drag;
//!         velocity.dy -= velocity.dy * drag;
//!     }
//! }
//!
//! /// The pull down there, which is the level's own with the fall taken out of it.
//! const SINKING: Gravity = Gravity::new().with_terminal_velocity(0.8);
//!
//! /// The pool: its own pull and its own drag, owned by the world that will do the stepping.
//! fn pool() -> World<8, (Gravity, Water)> {
//!     World::new().with_forces((SINKING, Water { drag: 0.3 }))
//! }
//!
//! fn sink(world: &mut World<8, (Gravity, Water)>, divers: &mut [&mut dyn Kinetic], ctx: &Context) {
//!     world.step(ctx, divers);
//! }
//! ```
//!
//! Which order they run in is the cart's to choose, and it is the tuple's: a tuple of forces is
//! one [`Force`], applied front to back. The difference is one update's worth either way — a drag
//! that runs before this update's pull has not felt it yet — so it shows in where a fall settles,
//! not in how it looks.

mod atmosphere;
mod bounds;
mod collider;
mod contact;
mod force;
mod gravity;
mod kinetic;
mod velocity;
mod wind;
#[doc(hidden)]
pub mod wire;
mod world;

pub use atmosphere::Atmosphere;
pub use bounds::Bounds;
pub use contact::{Contact, Contacts};
pub use force::Force;
pub use gravity::Gravity;
pub use kinetic::Kinetic;
pub use velocity::Velocity;
pub use wind::Wind;
pub use world::World;
