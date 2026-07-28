//! Fire, smoke and explosions, as particles.
//!
//! A *plume* is a stream of particles that spawn around a base point, travel away from it and
//! fade out on the way: the shape shared by a campfire, the smoke above it, a rocket's exhaust
//! and the trail from a damaged engine. [`Fire`] and [`Smoke`] are the two this module ships.
//! An [`Explosion`] is the other thing particles do: not a stream but a burst, everything it has
//! thrown out at once and gone in a quarter of a second. None of them cost anything but code —
//! no sprites, no map, no assets of any kind.
//!
//! They are sized at compile time and never allocate: an effect owns a fixed-capacity buffer of
//! particles, six bytes each. A full-size [`Fire`] holds 260 of them (about 1.5 KiB), a
//! [`SmokingFire`] twice that, and an [`Explosion`] 50.
//!
//! They are not free, though, and the bill lands in `draw`: every particle is one filled circle,
//! costing about 0.05% of the draw budget. A full-size [`Fire`] runs at roughly 6% of `update`
//! and 12% of `draw`, and a [`SmokingFire`] or a [`Smoke`] — twice the particles — about 10% and
//! 24%. An [`Explosion`] at its default size is 50 particles, so about 2% of `draw` for the
//! quarter-second it lasts and nothing at all of `update` — a burst carries no simulation, only
//! its age. Scale a plume down and both fall away with the particle count.
//! Budget for the ones on screen at once, and reach for a smaller `SCALE` before giving up on
//! the effect.
//!
//! Those are budget figures, which count the cart's side of each circle and not the console's
//! rasterizing of it — see [`Context::cpu_draw`]. Particle radii are small, so the two track
//! each other closely here, but on a slow device trust [`Context::fps`] over the percentages.
//!
//! Everything here stays in this module — a cart's `use pixel8::*;` does not reach it, so name
//! what the effect needs:
//!
//! ```no_run
//! use pixel8::{plume::SmokingFire, *};
//!
//! struct Camp {
//!     fire: SmokingFire,
//! }
//!
//! impl Game for Camp {
//!     fn update(&mut self, ctx: &mut Context) {
//!         self.fire.update(ctx);
//!     }
//!
//!     fn draw(&self, gfx: &mut Graphics) {
//!         gfx.clear(Color::BLACK);
//!         self.fire.draw(gfx);
//!     }
//! }
//! ```
//!
//! # Size
//!
//! The `SCALE` parameter sizes a plume. [`FULL_SCALE`] is a fire about 30 pixels tall, a `2` or
//! a `3` the flame of a candle or a torch, and anything up to [`MAX_SCALE`] grows it further:
//!
//! ```no_run
//! # use pixel8::plume::Fire;
//! let candle: Fire<2> = Fire::new(64, 100);
//! let bonfire: Fire<20> = Fire::new(64, 100);
//! ```
//!
//! `SCALE` is also how many particles go in each puff, and that is the one part of a plume that
//! cannot keep shrinking: a puff holds at least one particle however small the plume is, while
//! the ground it covers shrinks with the square. So the smaller a plume gets the more crowded it
//! is — a `Fire<1>` is around twelve times as dense as a `Fire<10>` — which is what
//! [`with_puffs`](Fire::with_puffs) is for.
//!
//! # Thinning a small plume
//!
//! A plume puffs once an update by default, which at [`FULL_SCALE`] is what makes it look like
//! something billowing. Far below that the puffs land on top of each other and it reads as a
//! solid lump instead. Building the plume out of fewer, further-apart puffs fixes it:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, Direction};
//! // A cigarette: the smallest plume there is, and thinned, or it is a blob on someone's face.
//! let wisp: Smoke<1> = Smoke::new(64, 100)
//!     .with_direction(Direction::UpLeft)
//!     .with_puffs(8);
//! ```
//!
//! Particles still move every update, so the plume keeps the reach, pace and direction it had —
//! there is simply less in it, spaced further apart. The colors spread out with the puffs too,
//! so a thinned plume greys along its length instead of in its first pixel, and it costs
//! proportionally less to update and draw. What it does not give back is memory: the buffer is
//! sized for a puff an update whether or not the plume uses them.
//!
//! # Direction
//!
//! Plumes travel in one of eight [`Direction`]s. They rise unless told otherwise, which is what
//! a fire wants; smoke pouring from a damaged aircraft flying up-screen wants
//! [`Down`], and a plume in a crosswind one of the diagonals. Particles sway
//! from side to side as they travel, so the sway follows the direction too.
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, Direction};
//! let exhaust: Smoke<4> = Smoke::new(64, 40).with_direction(Direction::Down);
//! ```
//!
//! A plume can be turned as it runs, with [`Fire::set_direction`] / [`Smoke::set_direction`].
//! Particles already in the air carry on the way they were going, so a plume that turns bends
//! rather than swinging around all at once.
// Everything this section documents — `blown_by`, the `Wind` it takes — exists only with the
// `physics` feature, and it links to all of it. So the section is gated too, or a cart that
// linked the plumes alone would document itself with links that resolve to nothing. Doc
// attributes render in source order, so it stays where it reads.
#![cfg_attr(
    feature = "physics",
    doc = r#"
# Wind

A plume leans from side to side as it travels, a slow wander that never repeats itself. That sway
is the weather a plume invents for itself, having none — and it is exactly what a [`Wind`] takes
the place of. With the `physics` feature on as well, [`Fire::blown_by`] and [`Smoke::blown_by`]
hand a plume the real thing, and from then on the wind is the only thing pushing it sideways. The
two never add up, so a scene that gives its plumes a gentle wind gets the sway it always had,
blowing the way the rest of the scene blows.

That leaves the shape of the wind the shape of the plume:

- A gusty wind wandering a range that straddles nothing sways a plume much as it swayed on its
  own — [`with_gusts`] wanders by the same trick the sway does, reversing at a random point so
  that neither settles into a rhythm.
- A steady wind holds the plume at a lean, with no wander left in it at all.
- A wind blowing at nothing stills it, and it travels dead straight.

A gust reaches the whole plume at once, but not evenly: a particle leaves the source already in a
share of the wind and is in all of it by the time it is old. So a column bends away along its
length instead of sliding sideways in one piece, and a gust shakes a fire where it stands while
carrying the smoke above it clean off.

```no_run
# use pixel8::{physics::Wind, plume::SmokingFire, *};
struct Camp {
    fire: SmokingFire,
    wind: Wind,
}

impl Game for Camp {
    fn update(&mut self, ctx: &mut Context) {
        // The gust first, so everything the wind touches this frame is bent by the same one.
        self.wind.update(ctx);
        self.fire.blown_by(&self.wind);
        self.fire.update(ctx);
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);
        self.fire.draw(gfx);
    }
}
```

The one place the swap shows is which way sideways is. A plume's own sway lies across its travel,
wherever it is pointed, while a wind blows its own way across the screen — so a plume that is not
pointed square across the wind takes part of it along its own length as well, as the headwind or
the tailwind it ought to be.

A [`Wind`] has a [`Direction`] of its own, and it is not this one. A plume's says where it is
pointed; a wind's says the side it comes *from*, weather being named that way round. The two are
about different things besides: the direction belongs to the source, while the wind bends what has
already left it. A fire in a gale still burns upwards; it is its smoke that ends up sideways.

[`Wind`]: crate::physics::Wind
[`with_gusts`]: crate::physics::Wind::with_gusts
"#
)]
//! # Starting and stopping
//!
//! [`Fire::set_puffing`] / [`Smoke::set_puffing`] turn the source off and on. A plume that has
//! stopped is not a plume that has vanished: it keeps everything it has already let go of, and
//! that carries on rising, greying and thinning out until it ages away. So the plume empties over
//! a `LIFETIME` from the base up, which is how a real one goes out.
//!
//! Simply not drawing it would blink the whole thing away instead — the difference between a
//! cigarette between draws and one that stops existing:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, *};
//! # struct Smoker { smoke: Smoke<1>, inhaling: bool }
//! impl Smoker {
//!     fn update(&mut self, ctx: &mut Context) {
//!         // Nothing new off the cigarette while it is at their lips; the last of it drifts off.
//!         self.smoke.set_puffing(!self.inhaling);
//!         self.smoke.update(ctx);
//!     }
//! }
//! ```
//!
//! # Trails
//!
//! A plume is not pinned to where it started: [`Fire::move_to`] and [`Smoke::move_to`] move the
//! point it spawns from. Particles already in the air keep the base they came off, so moving a
//! plume trails it rather than dragging everything it has emitted along — which is what makes a
//! plume a trail. That damaged aircraft is a `Smoke` moved to the plane every frame:
//!
//! ```no_run
//! # use pixel8::{plume::Smoke, *};
//! struct Plane {
//!     x: i16,
//!     y: i16,
//!     smoke: Smoke<4>,
//! }
//!
//! impl Plane {
//!     fn update(&mut self, ctx: &mut Context) {
//!         self.y -= 1;
//!         // Move first, and this frame's puff comes off where the plane is now.
//!         self.smoke.move_to(self.x, self.y);
//!         self.smoke.update(ctx);
//!     }
//! }
//! ```
//!
//! # Smoke from fire
//!
//! [`Smoke`] on its own is a plume that starts wherever it is placed. A fire that *turns into*
//! smoke is a different thing: its particles have to keep the position and the sway they had as
//! flames, or the two effects read as unrelated. That is what [`SmokingFire`] is — one plume
//! whose particles live twice as long, spending the second half of their life grey and drifting
//! at half the pace.
//!
//! # Bursts
//!
//! An [`Explosion`] is the same particles gone the other way about. There is no source to point,
//! to move or to stop: the sparks are thrown by the first [`update`](Explosion::update), each on
//! its own heading and at its own pace, and from then on the burst only thins. When
//! [`finished`](Explosion::finished) says so there is nothing left of it, which is a cart's cue
//! to drop it:
//!
//! ```no_run
//! # use pixel8::{plume::Explosion, *};
//! # struct Mine { blast: Option<Explosion> }
//! impl Mine {
//!     fn update(&mut self, ctx: &mut Context) {
//!         let Some(blast) = &mut self.blast else { return };
//!
//!         blast.update(ctx);
//!         if blast.finished() {
//!             self.blast = None;
//!         }
//!     }
//! }
//! ```
//!
//! A spark costs what any other particle costs to draw and nothing at all to update: it carries a
//! heading rather than a position, so where it is now is that heading times its age, and ageing
//! the whole burst is one addition.
//!
//! [`Context::cpu_draw`]: crate::Context::cpu_draw
//! [`Context::fps`]: crate::Context::fps
//! [`Direction`]: crate::Direction
//! [`Down`]: crate::Direction::Down

mod explosion;
mod fire;
mod scale;
mod smoke;
mod stream;

pub use explosion::{Explosion, DEFAULT_SPARKS};
pub use fire::{Fire, SmokingFire, SMOKING_FIRE_LIFETIME};
pub use scale::{FULL_SCALE, MAX_SCALE};
pub use smoke::{Smoke, SMOKE_LIFETIME};
pub use stream::{DEFAULT_LIFETIME, MAX_LIFETIME};

#[cfg(feature = "physics")]
pub use stream::MAX_WIND_SPEED;
