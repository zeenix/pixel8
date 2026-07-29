//! Gravity, air, wind, and force fields of your own.
//!
//! Anything that falls in a cart is written the same three ways: a `vy` kept next to the entity's
//! [`Body`](crate::Body), a constant added to it every update, and a cap so that a long drop does
//! not end with the entity a screen below the floor. This module gives those pieces names. A
//! [`Velocity`] is the per-update movement an entity carries, a [`Force`] is anything that bends
//! one, and a [`Kinetic`] is an entity that has both — so one [`Gravity`] can be the level's
//! gravity rather than a constant repeated in every entity's update, and a cart's own force fields
//! are just another `impl Force`.
//!
//! Forces act on velocity, never on position. [`Kinetic::step`] is where the two meet: hand it the
//! forces of the moment and it runs them over the entity, stops whatever ran into the map, moves
//! the [`Body`](crate::Body) with what survives, and reports which sides were touched. One call an
//! entity, an update.
//!
//! The weather belongs to the scene rather than to the things in it. Nothing is stored on an
//! entity between updates but its velocity, so the same gust that bends the whole cast is one
//! [`Wind`] the world holds and hands round — and an entity that answers to something of its own
//! is stepped with a slice of its own.
//!
//! Nothing here allocates or pulls in a dependency; a force costs a couple of floats and a
//! multiplication or two an update.
//!
//! Everything here stays in this module — a cart's `use pixel8::*;` does not reach it, so name
//! what the game needs:
//!
//! ```no_run
//! use pixel8::{
//!     physics::{Atmosphere, Bounds, Gravity, Kinetic, Velocity, Wind},
//!     *,
//! };
//!
//! struct Leaf {
//!     body: Body,
//!     velocity: Velocity,
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
//!     // The scene's weather, held once: the pull, the air it falls through, and a wind that
//!     // gusts and so cannot be a constant.
//!     gravity: Gravity,
//!     air: Atmosphere,
//!     wind: Wind,
//! }
//!
//! impl Game for Autumn {
//!     fn update(&mut self, ctx: &mut Context) {
//!         // The gust first, so every leaf is bent by the same one.
//!         self.wind.update(ctx);
//!         for leaf in &mut self.leaves {
//!             leaf.step(ctx, &[&self.gravity, &self.air, &self.wind]);
//!         }
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
//! Every entity says what rectangle it covers — [`Kinetic::bounds`] — and that is what the cart's
//! own collisions are judged against: [`Kinetic::overlaps`] for one entity against another, and
//! [`Bounds::on_screen`] for one that has left the screen altogether.
//!
//! ```no_run
//! # use pixel8::physics::{Bounds, Kinetic};
//! /// A bullet against everything it might have hit — and nothing at all once it is off screen.
//! fn hit(bullet: &dyn Kinetic, enemies: &[Bounds]) -> bool {
//!     if !bullet.bounds().on_screen() {
//!         return false;
//!     }
//!
//!     enemies.iter().any(|enemy| bullet.overlaps(*enemy))
//! }
//! ```
//!
//! A rectangle is all [`Kinetic::overlaps`] wants of the other party, so the thing collided with
//! need not be an entity at all: a door, a trigger, a patrolling sprite nothing ever pushes.
//! Which pairs are worth asking about is the cart's: it is the one that knows a bullet has no
//! quarrel with another bullet, and what a hit costs each of the two.
//!
//! The map is the other thing an entity can run into, and that one is opt-in. An entity that names
//! a sprite flag in [`Kinetic::solid`] stops at the level instead of drifting through it: every
//! tile carrying that flag is a wall to it, over the same rectangle it covers everywhere else. It
//! is the flag a cart already marks its walls with for [`Graphics::map`](crate::Graphics::map).
//!
//! [`Kinetic::step`] then resolves one axis at a time and hands back a [`Contacts`]: which sides
//! ran into something this update. [`below`](Contacts::below) is what a platformer calls
//! *grounded*.
//!
//! ```no_run
//! # use pixel8::{physics::{Bounds, Gravity, Kinetic, Velocity}, *};
//! /// Walls and floors are whatever the cart flagged as such in the sprite editor.
//! const SOLID: SpriteFlag = SpriteFlag::Flag0;
//! /// The level's pull. Nothing gusts here, so it is a constant.
//! const GRAVITY: Gravity = Gravity::new();
//!
//! struct Walker {
//!     body: Body,
//!     velocity: Velocity,
//!     grounded: bool,
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
//!     // One sprite's worth: what a wall stops, and what everything else judges it by.
//!     fn bounds(&self) -> Bounds {
//!         Bounds::of(&self.body, 8, 8)
//!     }
//!
//!     // And the tiles that are walls to it.
//!     fn solid(&self) -> BitFlags<SpriteFlag> {
//!         SOLID.into()
//!     }
//! }
//!
//! impl Walker {
//!     fn update(&mut self, ctx: &mut Context) {
//!         if self.grounded && ctx.is_button_pressed(Button::O) {
//!             self.velocity.dy = -3.25;
//!         }
//!         self.velocity.dx = if ctx.is_button_down(Button::Left) {
//!             -0.7
//!         } else if ctx.is_button_down(Button::Right) {
//!             0.7
//!         } else {
//!             0.0
//!         };
//!         // The pull, the walls and the movement, in one call.
//!         self.grounded = self.step(ctx, &[&GRAVITY]).below();
//!     }
//! }
//! ```
//!
//! One rectangle does both jobs, so a wall stops an entity exactly where another entity would have
//! hit it. An entity nothing on the map was ever going to stop — a bullet, a pickup — names no
//! flag at all, and never troubles the map for it.
//!
//! The edge of the world is the last thing an entity runs into, and it is not a wall: nothing
//! stops an entity walking straight off the map and falling for ever. [`Kinetic::keep_within`]
//! holds one inside a rectangle — [`Bounds::screen`], or the level itself where that is the
//! bigger of the two — putting it back against the edge and spending the speed that took it
//! there, which is what a wall would have done.
//!
//! # Forces of your own
//!
//! A [`Force`] is one method, so a cart's own force fields — a current, a magnet, the drag of deep
//! water — work everywhere the ones here do, and go in the same slice alongside them:
//!
//! ```no_run
//! # use pixel8::{physics::{Force, Gravity, Kinetic}, Context};
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
//! fn sink(diver: &mut impl Kinetic, ctx: &Context) {
//!     let water = Water { drag: 0.1 };
//!     diver.step(ctx, &[&SINKING, &water]);
//! }
//! ```
//!
//! Which order they run in is the cart's to choose, and it is the slice's: [`Kinetic::step`] takes
//! the forces it is handed front to back. The difference is one update's worth either way — a drag
//! that runs before this update's pull has not felt it yet — so it shows in where a fall settles,
//! not in how it looks.

mod atmosphere;
mod bounds;
mod contact;
mod force;
mod gravity;
mod kinetic;
mod map;
mod velocity;
mod wind;

pub use atmosphere::Atmosphere;
pub use bounds::Bounds;
pub use contact::{Contact, Contacts};
pub use force::Force;
pub use gravity::Gravity;
pub use kinetic::Kinetic;
pub use velocity::Velocity;
pub use wind::Wind;
