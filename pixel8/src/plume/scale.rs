//! What sizes an effect: the `SCALE` every one of them takes, and the sub-pixel units they all
//! work in.

/// The `SCALE` of a full-size plume: a fire about 30 pixels tall, spawning 10 particles an
/// update.
pub const FULL_SCALE: usize = 10;

/// The largest `SCALE` a plume supports, twice [`FULL_SCALE`]. Exceeding it fails the build.
///
/// A plume this big already spans the screen, and it is as far as a particle's six bytes stretch:
/// speeds stop fitting the byte they are stored in a little past it. Radii run out sooner — from
/// around `14` the fattest particles saturate at four pixels instead of growing with the rest.
pub const MAX_SCALE: usize = 2 * FULL_SCALE;

/// Spatial values are fixed-point, in units of 1/64th of a pixel: multiply by this to go from
/// pixels to sub-pixel units, divide to go back.
pub(super) const SUBPIXELS: i32 = 64;

/// A `value` of the [`FULL_SCALE`] plume, adjusted for the given scale.
pub(super) const fn scaled(value: i32, scale: i32) -> i32 {
    value * scale / FULL_SCALE as i32
}

/// A scaled value capped to what a particle's bytes hold, so that the biggest plumes
/// saturate their fattest particles instead of overflowing them.
pub(super) fn capped(value: i32) -> u8 {
    value.clamp(0, u8::MAX as i32) as u8
}
