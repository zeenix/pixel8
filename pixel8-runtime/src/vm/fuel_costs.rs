//! What one frame's fuel budget actually buys.
//!
//! `docs/LIMITS.md` makes concrete promises to cart authors about the per-frame work budget:
//! that a host call is billed at a flat one unit no matter how many arguments it takes, that a
//! `draw` written in normal Rust fits something like ten thousand of them, and that a tight loop
//! doing nothing else fits roughly thirty thousand. Those sentences are measurements of wasmi's
//! instruction pricing, and nothing but this module keeps them honest: a wasmi upgrade or a
//! `Config` change could quietly turn any of them into fiction.
//!
//! **A failure here is not a test to relax.** Every bound below is quoted, in words, in
//! `docs/LIMITS.md`. If wasmi genuinely re-prices something, the fix is to re-measure, decide
//! whether the *advice* the doc gives still holds, and update the doc and the bound together —
//! never to widen the bound until CI goes green. Each assertion carries the number it measured
//! so a red run is diagnosable from the CI log alone.
//!
//! The bounds are deliberately relational or ranged where a re-pricing could move a figure
//! without changing the advice, and exact only where the exact number *is* the promise.

use super::*;
use crate::{assets::Assets, audio::AudioHandle, storage::Storage};

/// The flat price of crossing into the host, in fuel. `docs/LIMITS.md` states it as "a flat one
/// unit of the 128 K — the same price whether it takes ten arguments or none".
const HOST_CALL_FUEL: u64 = 1;

/// Generous ceiling on what an empty `update` or `draw` may cost. Measured: 1 fuel, i.e. 0.0008%
/// of the budget.
const EMPTY_FRAME_CEILING: u64 = 8;

#[test]
fn an_empty_frame_costs_next_to_nothing() {
    let empty = cart("", "", "");
    let update = warm_update(&empty);
    let draw = warm_draw(&empty);
    // A property, not a price: the two phases run under one meter and one budget, so whatever
    // entering a cart function costs, it must cost the same either side. A split here means the
    // phases stopped being interchangeable and `cpu_update`/`cpu_draw` are no longer comparable.
    assert_eq!(
        update, draw,
        "update and draw must cost the same to enter: {update} vs {draw} fuel"
    );
    // Ranged: the exact floor (1) is not promised anywhere, but "real game logic uses a tiny
    // fraction of the budget" (docs/LIMITS.md) needs the fixed overhead to round to nothing. If
    // this trips, entering a frame acquired a real cost and the doc's framing needs revisiting.
    assert!(
        update <= EMPTY_FRAME_CEILING,
        "an empty frame cost {update} fuel of the {FUEL_PER_CALL} budget"
    );
}

#[test]
fn a_host_call_costs_one_fuel_at_every_arity() {
    // The headline claim of docs/LIMITS.md's per-frame-work section, and the one most likely to
    // be assumed away: the console's side of a draw call is not metered, so `sprite_stretch` with
    // ten arguments is billed exactly like `buttons_down` with none. Carts are told to budget by
    // call count alone; if that stops being true they need a different rule of thumb.
    let costs = arity_costs();
    // The sweep has to reach both ends of the claim, or "at every arity" passes on a table that
    // no longer spans one. If an ABI signature changes, fix the signature here — do not drop the
    // row, which would leave the ten-argument end of the doc's promise pinned by nothing.
    let widest = costs.iter().map(|&(_, arity, ..)| arity).max().unwrap_or(0);
    assert!(
        costs.iter().any(|&(_, arity, ..)| arity == 0) && widest == 10,
        "the sweep must span arity 0 to 10, the range docs/LIMITS.md quotes, but tops out at \
         {widest} over {} measurements",
        costs.len()
    );
    let (_, _, baseline_shape, baseline) = *costs
        .first()
        .expect("the sweep covers at least one import, the zero-argument one");
    for (name, arity, shape, fuel) in costs {
        // Relational first, because it survives a re-pricing: whatever a host call costs, arity
        // must not enter into it. A failure means the price now depends on how many arguments
        // cross, and docs/LIMITS.md's "the same price whether it takes ten arguments or none"
        // is false as written. The shape is in the message because a divergence in only one of
        // them says something quite different — that argument *encoding*, not count, is priced.
        assert_eq!(
            fuel, baseline,
            "{name} (arity {arity}, {shape}) cost {fuel} fuel a call against arity 0 with \
             {baseline_shape}'s {baseline}"
        );
    }
    // Exact, because this number *is* the published promise: one call, one unit of the 128 K.
    // Re-pricing it changes every "how many draws fit a frame" figure in docs/LIMITS.md, so it
    // must be re-measured there rather than absorbed here.
    assert_eq!(
        baseline, HOST_CALL_FUEL,
        "a warm host call cost {baseline} fuel, not the documented {HOST_CALL_FUEL}"
    );
}

#[test]
fn a_host_call_costs_the_same_however_much_work_it_asks_for() {
    // The other half of "only the call itself is billed", and the half the arity sweep above
    // cannot see, since it passes 1 for every argument and so measures every primitive at its
    // cheapest. docs/LIMITS.md: "`circle_fill` costs the same whether it paints one pixel or
    // twelve thousand, and drawing the whole tilemap costs the same as drawing one tile...
    // Asking `sprite_stretch` to blow an 8x8 sprite up to 4096x4096, or `map` for a hundred
    // thousand cels, costs no more than the 128x128 that can actually be seen."
    //
    // Each row is one import measured at trivial extents and again at the absurd extents the doc
    // names. A failure means the meter started billing the console's own work — most likely a
    // deliberate surcharge added to rein in draw-call abuse. That is a defensible change, but it
    // makes "budget by call count" wrong, so it belongs in docs/LIMITS.md before it belongs here.
    const EXTENTS: &[(&str, &str, &[i32], &[i32])] = &[
        // x, y, r, color: one pixel against a circle far wider than the screen.
        (
            "circle_fill",
            "(param i32 i32 i32 i32)",
            &[64, 64, 1, 9],
            &[64, 64, 100_000, 9],
        ),
        // cel_x, cel_y, sx, sy, cel_w, cel_h, layers: one tile against a hundred thousand cels.
        (
            "map",
            "(param i32 i32 i32 i32 i32 i32 i32)",
            &[0, 0, 0, 0, 1, 1, 1],
            &[0, 0, 0, 0, 100_000, 100_000, 1],
        ),
        // sx, sy, sw, sh, dx, dy, dw, dh, flip_x, flip_y: 8x8 one to one against 8x8 to 4096x4096.
        (
            "sprite_stretch",
            "(param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)",
            &[0, 0, 8, 8, 0, 0, 8, 8, 0, 0],
            &[0, 0, 8, 8, 0, 0, 4096, 4096, 0, 0],
        ),
    ];

    for &(name, signature, small, large) in EXTENTS {
        let import = format!("(import \"pixel8\" \"{name}\" (func ${name} {signature}))");
        let cost = |extents: &[i32]| {
            let one = format!("(call ${name} {})", i32_args(extents));
            // Few repetitions: the host really does paint at these extents, and the point is the
            // meter reading, not throughput. Three points still guard against a folded-away body.
            cost_per_repetition(
                warm_update,
                |reps| cart(&import, &one.repeat(reps as usize), ""),
                2,
                2,
            )
        };
        let (cheap, expensive) = (cost(small), cost(large));
        // Relational, and exact for the same reason the arity sweep is: the promise is that the
        // two are the same call at the same flat price, not merely that both are small.
        assert_eq!(
            cheap, expensive,
            "{name} cost {expensive} fuel a call at {large:?} against {cheap} at {small:?}"
        );
        assert_eq!(
            expensive, HOST_CALL_FUEL,
            "{name} at {large:?} cost {expensive} fuel, not the documented {HOST_CALL_FUEL}"
        );
    }
}

#[test]
fn the_measured_call_shapes_really_run() {
    // Guards every measurement above and below. Fuel readings only mean something if the work
    // being priced survived translation: a body that folds away costs nothing and would read as
    // a wonderfully cheap host call. Each shape the sweeps use paints a distinct run of pixels,
    // so the framebuffer proves the calls happened.
    let import = "(import \"pixel8\" \"set_pixel\" (func $ps (param i32 i32 i32)))";
    // Locals come first: wasm declares them all before the body's first instruction.
    let mut straight =
        String::from("(local $c i32) (local $i i32)\n(local.set $c (i32.const 9))\n");
    for x in 0..8 {
        // The constant-argument shape, and the local-argument shape one row down.
        straight.push_str(&format!(
            "(call $ps (i32.const {x}) (i32.const 0) (i32.const 9))\n\
             (call $ps (i32.const {x}) (i32.const 1) (local.get $c))\n"
        ));
    }
    let looped = "(local.set $i (i32.const 8))\n\
                  (loop $l (call $ps (i32.add (local.get $i) (i32.const -1)) (i32.const 2) \
                    (i32.const 9))\n\
                    (local.set $i (i32.add (local.get $i) (i32.const -1)))\n\
                    (br_if $l (local.get $i)))";
    let mut vm = vm_of(&cart(import, &format!("{straight}\n{looped}"), ""));
    vm.call_update().unwrap();
    for (row, shape) in [
        (0, "constant arguments"),
        (1, "local arguments"),
        (2, "a loop"),
    ] {
        let painted = (0..8).filter(|x| vm.state().fb.pget(*x, row) == 9).count();
        assert_eq!(
            painted, 8,
            "only {painted} of 8 host calls with {shape} reached the framebuffer, so the fuel \
             sweeps in this module are pricing a body that folded away"
        );
    }
}

#[test]
fn cart_side_arithmetic_costs_a_few_fuel_an_iteration() {
    // What sets the real ceiling on a frame is the cart's own code, not the calls it makes, so
    // these two are the units docs/LIMITS.md's "real game logic uses a tiny fraction" rests on.
    let integer = cost_per_repetition(warm_update, i32_loop, 1_000, 1_000);
    let float = cost_per_repetition(warm_update, f32_loop, 1_000, 1_000);
    // Ranged: the exact 3 and 5 are not published, the orders of magnitude they imply are. A
    // frame has to afford tens of thousands of iterations of ordinary arithmetic — if either
    // number grows enough to break that, the budget buys a different kind of cart and the doc's
    // reassurance needs rewriting rather than the bound loosening.
    assert!(
        (2..=6).contains(&integer),
        "a bare integer loop iteration cost {integer} fuel (measured: 3)"
    );
    assert!(
        (integer..=integer + 6).contains(&float),
        "an f32 multiply-add iteration cost {float} fuel against a bare loop's {integer} \
         (measured: 5 against 3)"
    );
    let integer_iterations = FUEL_PER_CALL / integer;
    let float_iterations = FUEL_PER_CALL / float;
    assert!(
        integer_iterations >= 20_000 && float_iterations >= 12_000,
        "one frame affords {integer_iterations} integer and {float_iterations} float iterations \
         (measured: 43,690 and 26,214)"
    );
}

#[test]
fn a_tight_loop_of_draw_calls_fits_thirty_thousand_a_frame() {
    // docs/LIMITS.md: "a tight loop that does nothing else roughly thirty thousand". The loop
    // costs 3 fuel an iteration on top of the call's 1, so the budget affords 131_072 / 4 =
    // 32_768 of them — 32_767 in practice, the loop's own setup taking the last one.
    let per_iteration = cost_per_repetition(warm_draw, sprite_loop_draw, 1_000, 1_000);
    let fits = FUEL_PER_CALL / per_iteration;
    // Ranged, two-sided: cheaper would mean the doc undersells the budget, dearer that it
    // oversells it. Either way the sentence needs re-measuring, not the range widening.
    assert!(
        (30_000..=40_000).contains(&fits),
        "a tight draw loop fits {fits} host calls a frame at {per_iteration} fuel an iteration \
         (measured: 32,768 at 4)"
    );
    // The arithmetic above is only worth trusting if the budget really does cut in around there,
    // so bracket it with a run either side rather than taking the division on faith.
    assert!(
        draw_completes(&sprite_loop_draw(30_000)),
        "30,000 host calls from a tight loop must fit one draw"
    );
    assert!(
        !draw_completes(&sprite_loop_draw(45_000)),
        "45,000 host calls from a tight loop must overrun one draw"
    );
}

#[test]
fn a_rust_shaped_draw_fits_ten_thousand_calls() {
    // docs/LIMITS.md: "a draw written in normal Rust fits something like ten thousand of them".
    // `sprite_grid_draw` is that shape in WAT — a loop that derives each sprite's position before
    // drawing it — and costs 10 fuel a call, so 13,107 fit one draw.
    //
    // What a real cart costs depends entirely on how much arithmetic it wraps around the call,
    // and the spread is wide: `#![no_std]` builds against the SDK have measured 7 fuel a call
    // (18,724 a draw) for this same derive-and-draw shape and 25 (5,242 a draw) for an entity
    // array with f32 motion. The doc's "something like ten thousand" sits inside that spread,
    // which is the claim — not any single cart's figure. So this bounds the model, which is
    // reproducible from this file, and a failure means re-measuring the model and asking whether
    // the doc's sentence still spans real carts. It does not mean widening the range.
    let per_call = cost_per_repetition(warm_draw, sprite_grid_draw, 100, 100);
    let fits = FUEL_PER_CALL / per_call;
    // Ranged two-sided: dearer and the budget no longer reaches the doc's ten thousand for any
    // plausible cart shape; cheaper by a lot and the doc is underselling the budget.
    assert!(
        (10_000..=20_000).contains(&fits),
        "a Rust-shaped draw fits {fits} sprite calls a frame at {per_call} fuel each \
         (measured: 13,107 at 10)"
    );
    // And the promise end to end: ten thousand sprites really do get drawn inside one budget.
    let mut vm = vm_of(&sprite_grid_draw(10_000));
    vm.call_draw().unwrap();
    vm.call_draw()
        .expect("a draw issuing 10,000 sprite calls must fit the frame budget");
    let spent = FUEL_PER_CALL - vm.store.get_fuel().unwrap();
    assert!(
        spent > 10_000,
        "10,000 sprite calls cost only {spent} fuel, so the loop cannot have run"
    );
}

/// Every arity the `"pixel8"` import set spans, priced per call: `(name, arity, shape, fuel)`.
/// Arities 6, 8 and 9 have no ABI function to sweep, so the walk goes 0..5, 7, 10.
///
/// Each import is measured twice, once with constant arguments and once with arguments read from
/// a local, because a register machine can encode the two differently; both must land on the same
/// price for the "count your calls" advice to hold. The shape rides along in the tuple so a
/// failure can name which of the two moved.
fn arity_costs() -> Vec<(&'static str, u32, &'static str, u64)> {
    const ABI_ARITIES: &[(&str, &str, u32, bool)] = &[
        ("buttons_down", "(result i32)", 0, true),
        ("storage_clear", "", 0, false),
        ("clear", "(param i32)", 1, false),
        ("is_button_down", "(param i32) (result i32)", 1, true),
        ("camera", "(param i32 i32)", 2, false),
        ("set_pixel", "(param i32 i32 i32)", 3, false),
        ("circle_fill", "(param i32 i32 i32 i32)", 4, false),
        ("rect_fill", "(param i32 i32 i32 i32 i32)", 5, false),
        ("print", "(param i32 i32 i32 i32 i32) (result i32)", 5, true),
        ("sprite", "(param i32 i32 i32 i32 i32 i32 i32)", 7, false),
        ("map", "(param i32 i32 i32 i32 i32 i32 i32)", 7, false),
        (
            "sprite_stretch",
            "(param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32)",
            10,
            false,
        ),
    ];

    let mut costs = Vec::new();
    for &(name, signature, arity, has_result) in ABI_ARITIES {
        let import = format!("(import \"pixel8\" \"{name}\" (func ${name} {signature}))");
        let call = |args: &str| match has_result {
            true => format!("(drop (call ${name} {args}))"),
            false => format!("(call ${name} {args})"),
        };
        let shapes = [
            (
                "constant args",
                "",
                call(&"(i32.const 1) ".repeat(arity as usize)),
            ),
            (
                "local args",
                "(local $a i32)\n",
                call(&"(local.get $a) ".repeat(arity as usize)),
            ),
        ];
        for (shape, locals, one) in shapes {
            let repeated = |reps: u32| {
                cart(
                    &import,
                    &format!("{locals}{}", one.repeat(reps as usize)),
                    "",
                )
            };
            costs.push((
                name,
                arity,
                shape,
                cost_per_repetition(warm_update, repeated, 10, 10),
            ));
        }
    }
    costs
}

/// A call's argument list in WAT, one `i32.const` per value.
fn i32_args(values: &[i32]) -> String {
    values
        .iter()
        .map(|v| format!("(i32.const {v}) "))
        .collect::<String>()
}

/// A `draw` that calls `sprite` `iterations` times from a loop that does nothing else.
fn sprite_loop_draw(iterations: u32) -> String {
    cart(
        SPRITE_IMPORT,
        "",
        &format!(
            "(local $i i32) (local.set $i (i32.const {iterations}))\n\
             (loop $l {SPRITE_CALL}\n\
               (local.set $i (i32.add (local.get $i) (i32.const -1)))\n\
               (br_if $l (local.get $i)))"
        ),
    )
}

/// A `draw` shaped like one written in Rust: a loop over `sprites` entities that works out each
/// one's screen position before drawing it, so the cart's own arithmetic is in the price too.
fn sprite_grid_draw(sprites: u32) -> String {
    cart(
        SPRITE_IMPORT,
        "",
        &format!(
            "(local $i i32) (local $x i32) (local $y i32)\n\
             (local.set $i (i32.const {sprites}))\n\
             (loop $l\n\
               (local.set $x (i32.and (i32.add (i32.mul (local.get $i) (i32.const 7)) \
                 (i32.const 3)) (i32.const 127)))\n\
               (local.set $y (i32.and (i32.add (i32.mul (local.get $i) (i32.const 5)) \
                 (i32.const 9)) (i32.const 127)))\n\
               (call $spr (i32.const 1) (local.get $x) (local.get $y) (i32.const 8) \
                 (i32.const 8) (i32.const 0) (i32.const 0))\n\
               (local.set $i (i32.add (local.get $i) (i32.const -1)))\n\
               (br_if $l (local.get $i)))"
        ),
    )
}

const SPRITE_IMPORT: &str =
    "(import \"pixel8\" \"sprite\" (func $spr (param i32 i32 i32 i32 i32 i32 i32)))";

const SPRITE_CALL: &str = "(call $spr (i32.const 1) (i32.const 2) (i32.const 3) (i32.const 8) \
                           (i32.const 8) (i32.const 0) (i32.const 0))";

/// An `update` that counts down from `iterations`, the cheapest loop wasm can express.
fn i32_loop(iterations: u32) -> String {
    cart(
        "",
        &format!(
            "(local $i i32) (local.set $i (i32.const {iterations}))\n\
             (loop $l (local.set $i (i32.add (local.get $i) (i32.const -1)))\n\
               (br_if $l (local.get $i)))"
        ),
        "",
    )
}

/// The same loop with a float multiply-add carried across iterations, so neither the multiply nor
/// the add can be folded into a constant.
fn f32_loop(iterations: u32) -> String {
    cart(
        "",
        &format!(
            "(local $i i32) (local $x f32) (local.set $x (f32.const 1.5))\n\
             (local.set $i (i32.const {iterations}))\n\
             (loop $l\n\
               (local.set $x (f32.add (f32.mul (local.get $x) (f32.const 1.0001)) \
                 (f32.const 0.5)))\n\
               (local.set $i (i32.add (local.get $i) (i32.const -1)))\n\
               (br_if $l (local.get $i)))"
        ),
        "",
    )
}

/// Fuel one repetition of a shape costs: the slope of frame cost against repetition count, which
/// cancels the fixed cost of entering the frame.
///
/// Three points rather than two, because the slope has to be *linear* to mean anything. A body
/// that const-folded away, or a loop the translator hoisted, reads as suspiciously cheap from two
/// points and as non-linear from three — so it fails here instead of quietly pinning a figure
/// measured off work that never ran.
fn cost_per_repetition<M, B>(measure: M, build: B, base: u32, step: u32) -> u64
where
    M: Fn(&str) -> u64,
    B: Fn(u32) -> String,
{
    let low = measure(&build(base));
    let mid = measure(&build(base + step));
    let high = measure(&build(base + 2 * step));
    assert!(
        low < mid && mid < high,
        "cost must grow with the repetition count, got {low}, {mid}, {high} — the body under \
         measurement is not running"
    );
    assert_eq!(
        mid - low,
        high - mid,
        "cost must be linear in the repetition count, got {low}, {mid}, {high}"
    );
    let per_step = mid - low;
    assert_eq!(
        per_step % u64::from(step),
        0,
        "{step} repetitions cost {per_step} fuel, which is not a whole number each"
    );
    per_step / u64::from(step)
}

/// Fuel a warm `pixel8_update` call costs — warm meaning the frame is not the first, so nothing
/// one-off (translation, lazy init) is folded into the reading. Two consecutive frames must agree,
/// or the figure is not a steady-state cost at all.
fn warm_update(src: &str) -> u64 {
    let mut vm = vm_of(src);
    vm.call_update().unwrap();
    vm.call_update().unwrap();
    let first = FUEL_PER_CALL - vm.store.get_fuel().unwrap();
    vm.call_update().unwrap();
    let second = FUEL_PER_CALL - vm.store.get_fuel().unwrap();
    assert_eq!(
        first, second,
        "an update's steady-state cost must be stable, got {first} then {second}"
    );
    first
}

/// Fuel a warm `pixel8_draw` call costs. See [`warm_update`].
fn warm_draw(src: &str) -> u64 {
    let mut vm = vm_of(src);
    vm.call_draw().unwrap();
    vm.call_draw().unwrap();
    let first = FUEL_PER_CALL - vm.store.get_fuel().unwrap();
    vm.call_draw().unwrap();
    let second = FUEL_PER_CALL - vm.store.get_fuel().unwrap();
    assert_eq!(
        first, second,
        "a draw's steady-state cost must be stable, got {first} then {second}"
    );
    first
}

/// Whether a warm `pixel8_draw` finishes inside its budget.
fn draw_completes(src: &str) -> bool {
    let mut vm = vm_of(src);
    let _ = vm.call_draw();
    vm.call_draw().is_ok()
}

/// A cart with the given imports and lifecycle bodies, loaded and ready to run.
fn vm_of(src: &str) -> GameVm {
    let wasm = wat::parse_str(src).unwrap();
    GameVm::load(
        &wasm,
        &Assets::default(),
        AudioHandle::dummy(),
        Storage::default(),
    )
    .unwrap()
}

fn cart(imports: &str, update: &str, draw: &str) -> String {
    format!(
        "(module {imports}\n  (memory (export \"memory\") 1)\n\
         (func (export \"pixel8_init\"))\n\
         (func (export \"pixel8_update\") {update})\n\
         (func (export \"pixel8_draw\") {draw}))"
    )
}
