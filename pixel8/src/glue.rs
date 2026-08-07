//! Lifecycle glue between the `game!` macro exports and the game trait.

use core::cell::Cell;

use crate::{Context, Game, Graphics};

/// The cart's declared frame rate while [`Game::boot`] runs, and zero the rest of the time —
/// what lets [`Context::fps`] keep its promise before the host has ever asked `pixel8_fps`.
pub(crate) fn boot_rate() -> u32 {
    BOOT_RATE.0.get()
}

/// See [`boot_rate`]. A `Cell` behind the same single-threaded justification as `Slot`: the host
/// drives one wasm instance sequentially, and the sandbox exposes no way to spawn threads.
struct BootRate(Cell<u32>);

unsafe impl Sync for BootRate {}

static BOOT_RATE: BootRate = BootRate(Cell::new(0));

/// Implementation details of the [`game!`](crate::game) macro. Not part
/// of the public API; do not call directly.
#[doc(hidden)]
pub mod __internal {
    use super::*;
    use core::cell::UnsafeCell;

    pub use crate::fmt::{format_args_to_buf, FmtBuf, LINE_CAP};

    /// Typed storage for the one game instance a cart declares.
    ///
    /// The [`game!`](crate::game) macro creates a `static Slot<G>` for the
    /// cart's concrete game type, so the instance is stored by value with
    /// no heap allocation or trait object.
    pub struct Slot<G>(UnsafeCell<Option<G>>);

    // Carts are single-threaded by construction: the host calls
    // pixel8_init/update/draw sequentially on one wasm instance, and the
    // sandbox exposes no way to spawn threads.
    unsafe impl<G> Sync for Slot<G> {}

    impl<G> Slot<G>
    where
        G: Game,
    {
        /// A slot with the game already in it, spelled out at compile time.
        ///
        /// The state ships as data — placed by the module's own memory image,
        /// the way any other initialized `static` is — so nothing is built and
        /// nothing is copied when the cart starts.
        pub const fn preset(game: G) -> Self {
            Slot(UnsafeCell::new(Some(game)))
        }

        /// An empty slot, filled later by [`init`](Slot::init).
        ///
        /// Prefer this over [`Default`]: it is `const`, so the slot can
        /// initialize a `static`.
        pub const fn new() -> Self {
            Slot(UnsafeCell::new(None))
        }

        /// Start a cart whose game is already in the slot: hook up panics and
        /// let the game boot.
        pub fn start(&self) {
            hook();
            self.boot();
        }

        /// Construct and store the game instance, then boot it. The build
        /// happens on the stack and is copied into the slot, which is what
        /// [`preset`](Slot::preset) exists to avoid.
        pub fn init(&self, make: impl FnOnce() -> G) {
            hook();
            *self.get() = Some(make());
            self.boot();
        }

        /// The cart's selected frame rate, as a frames-per-second number.
        /// The host queries this once after `init` to set its update/draw
        /// cadence. It depends only on the type, not the instance.
        pub fn fps(&self) -> u32 {
            G::FRAME_RATE.fps()
        }

        /// Advance the world one frame.
        pub fn update(&self) {
            if let Some(game) = self.get() {
                game.update(&mut Context { _private: () });
            }
        }

        /// Draw the world.
        pub fn draw(&self) {
            if let Some(game) = self.get() {
                game.draw(&mut Graphics { _private: () });
            }
        }

        /// The game's one chance to do what a constant cannot: enlist a cast,
        /// read the store, ask the clock. Once, before the first update.
        fn boot(&self) {
            if let Some(game) = self.get() {
                // The host only asks `pixel8_fps` once `pixel8_init` has returned — the
                // documented order, which a hand-written cart computing its rate in init
                // depends on — so for the length of `boot` the SDK answers `Context::fps`
                // itself, from the rate the game type already declares.
                super::BOOT_RATE.0.set(G::FRAME_RATE.fps());
                game.boot(&mut Context { _private: () });
                super::BOOT_RATE.0.set(0);
            }
        }

        #[allow(clippy::mut_from_ref)]
        fn get(&self) -> &mut Option<G> {
            unsafe { &mut *self.0.get() }
        }
    }

    impl<G> Default for Slot<G>
    where
        G: Game,
    {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Forward panics to the console so carts die with a readable error screen
    /// instead of a silent trap. On `no_std` the crate-level panic handler
    /// below does the same job, so there is nothing to install.
    fn hook() {
        #[cfg(feature = "std")]
        std::panic::set_hook(std::boxed::Box::new(|info| {
            let msg = info.to_string();
            unsafe { crate::ffi::panic(msg.as_ptr(), msg.len() as u32) };
        }));
    }
}

/// Capture the panic message and hand it to the host, then trap. Mirrors the
/// `std` `set_hook` path so both kinds of cart show the same error screen.
#[cfg(not(feature = "std"))]
#[panic_handler]
fn handle_panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    let mut buf = crate::fmt::FmtBuf::<256>::new();
    let _ = write!(buf, "{info}");
    let msg = buf.as_str();
    unsafe { crate::ffi::panic(msg.as_ptr(), msg.len() as u32) };
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::unreachable();
    #[cfg(not(target_arch = "wasm32"))]
    loop {}
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU32, Ordering};

    use super::__internal::Slot;
    use crate::{Context, FrameRate, Game, Graphics};

    /// Where the game below parks what `boot` was answered, since nothing reads a `Slot` back.
    static SEEN_AT_BOOT: AtomicU32 = AtomicU32::new(u32::MAX);

    struct HalfRate;

    impl Game for HalfRate {
        const FRAME_RATE: FrameRate = FrameRate::Fps30;

        fn boot(&mut self, ctx: &mut Context) {
            SEEN_AT_BOOT.store(ctx.fps() as u32, Ordering::Relaxed);
        }

        fn update(&mut self, _: &mut Context) {}

        fn draw(&self, _: &mut Graphics) {}
    }

    #[test]
    fn boot_is_answered_the_carts_own_rate() {
        // The host does not ask `pixel8_fps` until `pixel8_init` returns, so while `boot` runs
        // the answer cannot come from it: the glue parks the game's own declared rate for
        // exactly that long. A 30-fps cart that read the host's default here would mistune
        // everything it works out at boot.
        let slot: Slot<HalfRate> = Slot::new();
        slot.init(|| HalfRate);
        assert_eq!(SEEN_AT_BOOT.load(Ordering::Relaxed), 30);
        // And the parking spot is empty again: after boot the question is the host's, whose
        // native stub answers zero.
        assert_eq!(super::boot_rate(), 0, "the boot rate outlived boot");
        assert_eq!(Context { _private: () }.fps(), 0.0);
    }
}
