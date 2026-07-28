//! [`Smoke`]: a plume with no fire under it.

use core::ops::Range;

use super::{scale::SUBPIXELS, stream::Plume, DEFAULT_LIFETIME, FULL_SCALE};
#[cfg(feature = "physics")]
use crate::physics::Wind;
use crate::{Color, Context, Direction, Graphics};

/// Smoke: a plume of grey particles that darken as they disperse.
///
/// This is smoke with no fire under it — an exhaust or a smouldering wreck. For smoke that
/// continues a fire, use [`SmokingFire`].
///
///
/// `SCALE` sizes the plume (see the [module docs](super#size)) and `LIFETIME` is how many updates
/// each particle lives for. Smoke drifts at half a fire's pace, so its default lifetime is twice
/// as long: a plume the length of a fire's flames, but thinner and slower.
///
/// [`SmokingFire`]: super::SmokingFire
#[derive(Debug)]
pub struct Smoke<const SCALE: usize = FULL_SCALE, const LIFETIME: usize = SMOKE_LIFETIME> {
    plume: Plume<SCALE, LIFETIME>,
}

impl<const SCALE: usize, const LIFETIME: usize> Smoke<SCALE, LIFETIME> {
    /// A new smoke plume based at the pixel position (`x`, `y`) — the middle of the bed it rises
    /// from — billowing upwards. Point it another way with
    /// [`with_direction`](Self::with_direction).
    pub fn new(x: i16, y: i16) -> Self {
        Self {
            plume: Plume::new(x, y, SMOKE_SPEED, None),
        }
    }

    /// Sets which way the smoke billows, to chain onto [`new`](Self::new).
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.set_direction(direction);
        self
    }

    /// Builds the plume out of `puffs` puffs rather than one an update, to chain onto
    /// [`new`](Self::new).
    ///
    /// This is what turns the smallest scales from a solid lump into a wisp — see
    /// [Thinning a small plume](super#thinning-a-small-plume). `puffs` is clamped to
    /// `1..=LIFETIME`, and `LIFETIME` (the default) is a puff an update.
    pub fn with_puffs(mut self, puffs: usize) -> Self {
        self.plume.set_puffs(puffs);
        self
    }

    /// Turns the smoke to billow towards `direction`.
    ///
    /// Smoke already in the air keeps drifting the way it was, so the trail bends behind a
    /// source that turns rather than swinging around all at once.
    pub fn set_direction(&mut self, direction: Direction) {
        self.plume.set_direction(direction);
    }

    /// Lets the source go on puffing, or shuts it off without clearing the screen of it.
    ///
    /// Smoke already in the air is left to drift, fade and thin out on its own, so a source that
    /// shuts off empties over a `LIFETIME` rather than blinking away — which is what a cigarette
    /// between draws, or an engine that cuts out, actually looks like. Open it again and the next
    /// puff comes off the base as usual. See the [module docs](super#starting-and-stopping).
    pub fn set_puffing(&mut self, puffing: bool) {
        self.plume.set_puffing(puffing);
    }

    /// Moves the source of the smoke to (`x`, `y`), for a trail off something that moves.
    ///
    /// Smoke already in the air stays where it was let go — which is what makes it a trail
    /// rather than a cloud dragged along. Move it before [`update`](Self::update) and the next
    /// puff comes off the new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
    }

    /// Hands the smoke over to `wind`, which from here on is the only thing that leans it.
    ///
    /// Smoke drifts from side to side on its own, having no weather of its own to answer to; this
    /// gives it some, and that drifting stands down rather than adding to it. So a wind gusting
    /// either side of nothing wanders the trail about as much as it wandered alone, a steady one
    /// holds it at a lean, and a wind blowing at nothing sends it straight up. See the
    /// [module docs](super#wind).
    ///
    /// The wind's speed is read as it stands, so this belongs in
    /// [`Game::update`](crate::Game::update) just after [`Wind::update`]; what it reads holds
    /// until the next call. A puff leaves the source already in a share of the wind and is taken
    /// by all of it a little under half a second later, so the trail bends away along its length
    /// rather than shearing off as a block.
    ///
    /// Only what the wind [`blow`](Wind::blow)s is read — its direction and its speed, and not
    /// its exposure: a plume sets its own pace against a wind through that ramp. Either axis of
    /// it past [`MAX_WIND_SPEED`] is clamped to it.
    ///
    /// [`MAX_WIND_SPEED`]: super::MAX_WIND_SPEED
    #[cfg(feature = "physics")]
    pub fn blown_by(&mut self, wind: &Wind) {
        self.plume.blown_by(wind);
    }

    /// Advances the smoke by one frame. Call this from [`Game::update`](crate::Game::update).
    pub fn update(&mut self, ctx: &mut Context) {
        self.plume.update(ctx);
    }

    /// Draws the smoke. Call this from [`Game::draw`](crate::Game::draw).
    pub fn draw(&self, gfx: &mut Graphics) {
        self.plume.draw(gfx, SMOKE_BIRTH_COLOR, &SMOKE_COLOR_STOPS);
    }
}

/// The default `LIFETIME` of [`Smoke`]: twice [`DEFAULT_LIFETIME`], which at half a fire's pace
/// spans the same length.
pub const SMOKE_LIFETIME: usize = 2 * DEFAULT_LIFETIME;

/// The color a smoke particle is born with.
const SMOKE_BIRTH_COLOR: Color = Color::WHITE;

/// What a smoke particle turns into, and at what age. The ages are twice a fire's, matching the
/// halved pace, so each band of color covers the same distance.
const SMOKE_COLOR_STOPS: [(usize, Color); 2] = [(8, Color::LIGHT_GREY), (20, Color::DARK_GREY)];

/// How fast smoke particles drift, in sub-pixel units per update at [`FULL_SCALE`] — half a
/// fire's pace.
const SMOKE_SPEED: Range<i32> = SUBPIXELS / 4..SUBPIXELS * 3 / 4;
