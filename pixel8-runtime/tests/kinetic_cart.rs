//! A real cart, compiled to wasm, whose cast collides with itself.
//!
//! The `physics` half of the SDK asks the console two things and nothing else:
//! what a map tile is, and what flags a sprite carries. Both sides of that are
//! covered against their own idea of the other — the runtime's unit tests call
//! its assets directly, the SDK's resolve against a map and a sheet written down
//! in Rust — so an ABI that drifted between them would leave both green and every
//! cart in the world wrong.
//!
//! This is the crossing: the fixture in `tests/carts/kinetic` is built with cargo
//! for `wasm32-unknown-unknown` against the SDK in this working tree, loaded into
//! a real [`GameVm`], and run for five frames. It steps one `World` over four
//! entities — two crates of one kind walking into each other, a sensor walking
//! into a flagged hazard — and writes the world's answers back out as pixels;
//! what is asserted here is those pixels.
//!
//! Nothing in the fixture is drawn but the report. Collision comes off what an
//! entity says it wears and what the *host* flagged that cell with, so the flags
//! below are set here rather than in the cart — and the cart still collides
//! without painting a thing.

use std::{path::PathBuf, process::Command};

use pixel8_runtime::{
    assets::Assets,
    audio::AudioHandle,
    fb::{Framebuffer, WIDTH},
    storage::Storage,
    vm::GameVm,
};

/// The rows and columns the cart reports through, from its `src/lib.rs`. A lit
/// pixel is a yes; the position rows carry a single lit pixel at the coordinate
/// they are reporting.
const LEFT_CRATE_ROW: i32 = 126;
const RIGHT_CRATE_ROW: i32 = 125;
const CRATE_Y_ROW: i32 = 124;
const SENSOR_X_ROW: i32 = 123;
const ANSWER_ROW: i32 = 127;
const LEFT_STOPPED: i32 = 0;
const RIGHT_STOPPED: i32 = 2;
const MET_HAZARD: i32 = 4;

/// The color the cart reports in.
const LIT: u8 = 7;

/// What the crates do, worked out from the geometry the cart sets up rather than
/// from what it printed the first time it ran.
///
/// Their corners start sixteen pixels apart on a row of their own — eight pixels
/// of daylight between two cell-square boxes — and each closes two pixels an
/// update. Both wear the crate cell and both call the crate flag solid, so each
/// is a wall to the other, and to nobody else — itself least of all.
///
/// | frame | left x | right x | left y | left stopped | right stopped |
/// |-------|--------|---------|--------|--------------|---------------|
/// | 0     | 10     | 22      | 20     | no           | no            |
/// | 1     | 12     | 20      | 20     | no           | no            |
/// | 2     | 12     | 20      | 20     | yes          | yes           |
/// | 3     | 12     | 20      | 20     | yes          | yes           |
/// | 4     | 12     | 20      | 20     | yes          | yes           |
///
/// Two frames close the eight pixels and leave the pair flush: 12 to 19 and 20
/// to 27. From frame 2 neither can go further — two more pixels would put each
/// inside the other — so the left reads `right` and the right reads `left`, both
/// of them for ever, which is what a cart renewing its push every update is for.
///
/// The `left y` column is the negative half of the same claim, and the one that
/// would break loudest. An entity mistaken for itself would be found inside
/// itself every update and shoved out — up eight pixels, that being the shallower
/// way out of a square that covers you exactly — so a crate still standing on row
/// 20 five frames later was never once its own wall.
const CRATES: [(i32, i32, bool, bool); 5] = [
    (10, 22, false, false),
    (12, 20, false, false),
    (12, 20, true, true),
    (12, 20, true, true),
    (12, 20, true, true),
];

/// And what the sensor does: it calls nothing solid, so nothing anywhere stops
/// it, and it is told what it met all the same.
///
/// It starts at x 20 and walks four pixels an update along a row of its own; the
/// hazard is the eight pixels from 40, standing still and wearing a flagged cell.
/// Meeting is taken over the whole strip a step swept, not over its endpoints, so
/// the frame the sensor's stride first *reaches* into the hazard is the frame it
/// is told about — 32 to 43, on frame 3.
///
/// | frame | sensor x | met the hazard |
/// |-------|----------|----------------|
/// | 0     | 24       | no             |
/// | 1     | 28       | no             |
/// | 2     | 32       | no             |
/// | 3     | 36       | yes            |
/// | 4     | 40       | yes            |
const SENSOR: [(i32, bool); 5] = [
    (24, false),
    (28, false),
    (32, false),
    (36, true),
    (40, true),
];

#[test]
fn a_cart_collides_with_the_cast_it_handed_the_world() {
    let mut vm = GameVm::load(
        &fixture_wasm(),
        &probe_assets(),
        AudioHandle::dummy(),
        Storage::default(),
    )
    .expect("the fixture cart loads");

    for (frame, (&(left, right, stopped, pushed), &(sensor, met))) in
        CRATES.iter().zip(SENSOR.iter()).enumerate()
    {
        vm.call_update()
            .unwrap_or_else(|e| panic!("frame {frame}: update: {}", e.message));
        // The step is the console's work, so what the cart pays is the crossing: a handful of
        // hundreds of fuel a frame, not the thousands the same collisions cost stepped in cart
        // wasm. The ceiling is deliberately generous — it is here to catch the step sliding back
        // into the cart, not to pin the marshaling to the instruction.
        let fuel = vm.cpu_update() * 131_072.0;
        assert!(
            fuel < 2_000.0,
            "frame {frame}: the update burned {fuel} fuel — is the cast being stepped in cart \
             wasm again?"
        );
        vm.call_draw()
            .unwrap_or_else(|e| panic!("frame {frame}: draw: {}", e.message));
        let fb = &vm.state().fb;
        let lit = |column: i32| fb.pget(column, ANSWER_ROW) == LIT;

        assert_eq!(
            reported(fb, LEFT_CRATE_ROW),
            Some(left),
            "frame {frame}: left crate x"
        );
        assert_eq!(
            reported(fb, RIGHT_CRATE_ROW),
            Some(right),
            "frame {frame}: right crate x"
        );
        assert_eq!(
            reported(fb, CRATE_Y_ROW),
            Some(20),
            "frame {frame}: the left crate was shoved off its own row"
        );
        assert_eq!(
            lit(LEFT_STOPPED),
            stopped,
            "frame {frame}: the left crate was stopped"
        );
        assert_eq!(
            lit(RIGHT_STOPPED),
            pushed,
            "frame {frame}: the right crate was stopped"
        );

        assert_eq!(
            reported(fb, SENSOR_X_ROW),
            Some(sensor),
            "frame {frame}: sensor x"
        );
        assert_eq!(lit(MET_HAZARD), met, "frame {frame}: met the hazard");
    }
}

/// The single coordinate a report row is lit at, or `None` for a row that says
/// nothing. Two lit pixels would mean the cart drew something across the row it
/// reports in, and the answer would be worth no more than a guess.
fn reported(fb: &Framebuffer, row: i32) -> Option<i32> {
    let mut lit = (0..WIDTH).filter(|&x| fb.pget(x, row) == LIT);
    let first = lit.next()?;

    lit.next().is_none().then_some(first)
}

/// The sheet the fixture's entities wear: one cell flagged as the crates' own
/// kind, one flagged as the hazard, and nothing else.
///
/// Flags only, and no pixels at all. Flagging is the console's half of the
/// arrangement — it is the host that says what a sprite *means* — and what an
/// entity is made of no longer has anything to do with what was painted.
fn probe_assets() -> Assets {
    let mut assets = Assets::default();
    // Sprite 1 is the crate, sprite 2 the hazard, matching the fixture's own
    // `CRATE_SPRITE` and `HAZARD_SPRITE`.
    assets.sprites.set_flag(1, 0, true);
    assets.sprites.set_flag(2, 1, true);

    assets
}

/// Build the fixture cart and read back its wasm.
///
/// `cargo build --release --target wasm32-unknown-unknown`, the same line the
/// console's builder runs and the same one a cart author types: the fixture is
/// an ordinary crate depending on the in-tree SDK by path, so what comes back
/// is built against the ABI this very runtime links.
fn fixture_wasm() -> Vec<u8> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/carts/kinetic");
    // Out of the way of both the workspace's target directory (whose lock this
    // test is already holding, having been started by cargo) and the fixture's
    // own, so a test run leaves the source tree as it found it.
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/tests/kinetic-cart");
    let build = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_TERM_COLOR", "never")
        // An ambient `RUSTFLAGS` replaces the fixture's `.cargo/config.toml`
        // rather than adding to it, and what it would drop is the 32 KiB
        // shadow-stack reserve that keeps the cart inside the 128 K memory cap.
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("cargo runs");
    assert!(
        build.status.success(),
        "building the fixture cart failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let wasm = target.join("wasm32-unknown-unknown/release/kinetic_cart.wasm");

    std::fs::read(&wasm).unwrap_or_else(|e| panic!("reading {}: {e}", wasm.display()))
}
