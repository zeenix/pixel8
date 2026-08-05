//! A cart whose cast collides with itself, and spells out what the world said about it.
//!
//! The fixture behind `pixel8-runtime/tests/kinetic_cart.rs`, and the one thing neither half of the
//! `physics` module can check on its own: the SDK's own tests drive the resolution against a map
//! and a sprite sheet they wrote down themselves, and the runtime's drive its sprite flags and its
//! map in Rust, so only a real cart, compiled to wasm and run in a real `GameVm`, crosses the ABI
//! between them. An import renamed, a flag word packed one way and unpacked the other, and both
//! halves stay green while every cart in the world goes dark.
//!
//! One [`World`] of four seats and no forces at all: what moves the cast is a velocity written
//! afresh every update, so a member stopped by something goes on leaning into it instead of
//! settling down and reporting nothing.
//!
//! * the two *crates* are of one kind: one sprite, one flag, and that flag is what the world calls
//!   solid — said once, on the world, and neither crate says anything of its own. They walk into
//!   each other and stop, which is the arrangement flags alone could never buy — the world skips
//!   each of them against itself, so neither is ever its own wall.
//! * the *sensor* is stopped by a rule of its own — the empty one, stopped by
//!   nothing whatever the world declares — and walks right through it all, and the *hazard* stands
//!   still wearing a flagged cell. The sensor is never stopped and is told exactly when it reached
//!   the hazard.
//!
//! Nothing is drawn but the report. What a member is made of is the cell it wears and what the
//! cart flagged that cell with in the sprite editor — never
//! what was painted where — so a cart that draws nothing at all still collides.
//!
//! The answers go back to the host as pixels, since the framebuffer is the one thing a headless
//! test can read out of a running cart: a lit pixel at a member's x on a row of its own, and a row
//! of lit or dark answers underneath. What the world reports is only ever read here through the SDK
//! — `Contacts::right`, `Contacts::left`, `Contacts::touches` — so the reading is the cart's own,
//! not the test's.

#![no_std]

use pixel8::{
    physics::{Member, Velocity, World},
    *,
};

/// What the crates are known by — their own kind's flag, and the one flag the world calls solid.
/// Safe because the world never asks a member about itself.
const CRATE: SpriteFlag = SpriteFlag::Flag0;
/// What the hazard carries: something to be told about and walked straight through, which is the
/// half of an answer no stopped side ever reports.
const HAZARD: SpriteFlag = SpriteFlag::Flag1;

/// The cell both crates wear, and so the one flag every crate meets in every other.
const CRATE_SPRITE: SpriteId = SpriteId(1);
/// And the one the hazard wears. The host flags both; the cart only names them.
const HAZARD_SPRITE: SpriteId = SpriteId(2);

/// Every member here is a whole cell square, which is what makes the arithmetic in the test's
/// table something a reader can follow.
const SIDE: u16 = 8;

/// The row the crates walk along, and where they start: sixteen pixels of daylight, closed two
/// pixels each an update.
const CRATE_ROW: f32 = 20.0;
const LEFT_CRATE_AT: f32 = 8.0;
const RIGHT_CRATE_AT: f32 = 24.0;
const CRATE_SPEED: f32 = 2.0;

/// The row the sensor walks along, where it starts, how fast, and where the hazard stands.
const SENSOR_ROW: f32 = 40.0;
const SENSOR_AT: f32 = 20.0;
const SENSOR_SPEED: f32 = 4.0;
const HAZARD_AT: f32 = 40.0;

/// The rows each member's position is reported on, as the one lit pixel in the row: the left
/// crate's x, the right crate's x, the left crate's y — which says whether it was ever mistaken for
/// itself and shoved off its own row — and the sensor's x.
const LEFT_CRATE_ROW: i16 = 126;
const RIGHT_CRATE_ROW: i16 = 125;
const CRATE_Y_ROW: i16 = 124;
const SENSOR_X_ROW: i16 = 123;
/// The row the three answers are reported on, one column each: lit for yes.
const ANSWER_ROW: i16 = 127;
const LEFT_STOPPED: i16 = 0;
const RIGHT_STOPPED: i16 = 2;
const MET_HAZARD: i16 = 4;

game!(Probe = Probe::new());

struct Probe {
    /// The one thing that holds or moves any of them. Four seats — the cast this fixture pins is
    /// one a cart actually configures — and the crossing carries all four.
    world: World<4>,
    /// The two of one kind, walking into each other. Seat order is stepping order, and this is
    /// the order they are enlisted in.
    left: Member,
    right: Member,
    /// And the one that is stopped by nothing, walking into the one that stops nobody.
    sensor: Member,
    hazard: Member,
    /// What each of them means to do, written back into its velocity every update: a step that
    /// ran into something spends the speed that carried it there, so a member that did not renew
    /// it would stop reporting the wall it is leaning on.
    pushes: [Velocity; 4],
}

impl Probe {
    fn new() -> Self {
        let mut world = World::new().with_solid(CRATE);
        // A crate: wearing the crate cell, and stopped by whatever the world calls solid —
        // anything wearing that same cell, its own kind, which is only safe because the world
        // knows which crate this is.
        let crated = |world: &mut World<4>, x: f32| {
            world
                .enlist(x, CRATE_ROW, SIDE, SIDE)
                .unwrap()
                .wearing(CRATE_SPRITE)
                .member()
        };
        let left = crated(&mut world, LEFT_CRATE_AT);
        let right = crated(&mut world, RIGHT_CRATE_AT);
        // The sensor: wearing nothing, told everything, and stopped by nothing — a rule of its
        // own, held against a world that declares otherwise.
        let sensor = world
            .enlist(SENSOR_AT, SENSOR_ROW, SIDE, SIDE)
            .unwrap()
            .stopped_by(BitFlags::<SpriteFlag>::empty())
            .member();
        // And the hazard: standing still, wearing a flagged cell, stopped by nothing.
        let hazard = world
            .enlist(HAZARD_AT, SENSOR_ROW, SIDE, SIDE)
            .unwrap()
            .wearing(HAZARD_SPRITE)
            .stopped_by(BitFlags::<SpriteFlag>::empty())
            .member();

        Self {
            world,
            left,
            right,
            sensor,
            hazard,
            pushes: [
                Velocity::new(CRATE_SPEED, 0.0),
                Velocity::new(-CRATE_SPEED, 0.0),
                Velocity::new(SENSOR_SPEED, 0.0),
                Velocity::default(),
            ],
        }
    }
}

impl Game for Probe {
    fn update(&mut self, ctx: &mut Context) {
        // The cast in the order it was seated, every push renewed.
        let cast = [self.left, self.right, self.sensor, self.hazard];
        for (member, push) in cast.into_iter().zip(self.pushes) {
            self.world.set_velocity(member, push);
        }

        // The whole cast, in one call: the crates against each other, the sensor against the
        // hazard, and every one of them against the map.
        self.world.step(ctx);
    }

    fn draw(&self, gfx: &mut Graphics) {
        gfx.clear(Color::BLACK);

        // The report, and nothing else: the cast collided with no help from anything drawn.
        let (left_x, left_y) = self.world.draw_pos(self.left);
        gfx.pset(left_x, LEFT_CRATE_ROW, Color::WHITE);
        gfx.pset(
            self.world.draw_pos(self.right).0,
            RIGHT_CRATE_ROW,
            Color::WHITE,
        );
        gfx.pset(left_y, CRATE_Y_ROW, Color::WHITE);
        gfx.pset(
            self.world.draw_pos(self.sensor).0,
            SENSOR_X_ROW,
            Color::WHITE,
        );
        answer(gfx, LEFT_STOPPED, self.world.contacts(self.left).right());
        answer(gfx, RIGHT_STOPPED, self.world.contacts(self.right).left());
        answer(
            gfx,
            MET_HAZARD,
            self.world.contacts(self.sensor).touches(HAZARD),
        );
    }
}

/// One answer, lit for yes and left dark for no.
fn answer(gfx: &mut Graphics, column: i16, yes: bool) {
    if yes {
        gfx.pset(column, ANSWER_ROW, Color::WHITE);
    }
}
