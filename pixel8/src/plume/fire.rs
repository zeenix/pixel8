//! [`Fire`], and the [`SmokingFire`] whose flames carry on as smoke.

use core::ops::Range;

use super::{scale::SUBPIXELS, stream::Plume, DEFAULT_LIFETIME, FULL_SCALE};
#[cfg(feature = "physics")]
use crate::physics::Wind;
use crate::{Color, Context, Direction, Graphics};

/// A fire: a plume of flame particles, white-hot at the base and fading to orange at the tips.
///
/// `SCALE` sizes the fire (see the [module docs](super#size)) and `LIFETIME` is how many updates
/// each particle lives for, which is what makes the flames as long as they are. Particles that
/// outlive [`DEFAULT_LIFETIME`] turn to smoke, so a `LIFETIME` past it — [`SmokingFire`] — is a
/// fire that smokes.
#[derive(Debug)]
pub struct Fire<const SCALE: usize = FULL_SCALE, const LIFETIME: usize = DEFAULT_LIFETIME> {
    plume: Plume<SCALE, LIFETIME>,
}

impl<const SCALE: usize, const LIFETIME: usize> Fire<SCALE, LIFETIME> {
    /// A new fire based at the pixel position (`x`, `y`), burning upwards. Point it another way
    /// with [`with_direction`](Self::with_direction).
    ///
    /// The base is the middle of the bed the flames rise from — for a campfire, the logs — and
    /// is where they are widest; they narrow as they travel.
    pub fn new(x: i16, y: i16) -> Self {
        Self {
            plume: Plume::new(x, y, FIRE_SPEED, Some(DEFAULT_LIFETIME)),
        }
    }

    /// Sets which way the flames burn, to chain onto [`new`](Self::new).
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.set_direction(direction);
        self
    }

    /// Builds the fire out of `puffs` puffs of flame rather than one an update, to chain onto
    /// [`new`](Self::new).
    ///
    /// This is what keeps a small fire from burning as a solid lump — see
    /// [Thinning a small plume](super#thinning-a-small-plume). `puffs` is clamped to
    /// `1..=LIFETIME`, and `LIFETIME` (the default) is a puff an update.
    pub fn with_puffs(mut self, puffs: usize) -> Self {
        self.plume.set_puffs(puffs);
        self
    }

    /// Turns the fire to burn towards `direction`.
    ///
    /// Flames already in the air keep burning the way they were, so a fire that turns bends
    /// instead of swinging around all at once.
    pub fn set_direction(&mut self, direction: Direction) {
        self.plume.set_direction(direction);
    }

    /// Lets the fire go on throwing off flames, or stops it without clearing the screen of it.
    ///
    /// A fire that has stopped keeps what is already alight: the last flames rise, fade and go
    /// out on their own, so it dies down over a `LIFETIME` rather than blinking away. Start it
    /// again and it picks straight back up. See the [module docs](super#starting-and-stopping).
    pub fn set_puffing(&mut self, puffing: bool) {
        self.plume.set_puffing(puffing);
    }

    /// Moves the fire to (`x`, `y`), for one carried or burning on something that moves.
    ///
    /// Flames already in the air stay where they were let go, so a fire on the move trails
    /// behind itself. Move it before [`update`](Self::update) and the next flames come off the
    /// new position.
    pub fn move_to(&mut self, x: i16, y: i16) {
        self.plume.move_to(x, y);
    }

    /// Hands the flames over to `wind`, which from here on is the only thing that leans them.
    ///
    /// A fire sways on its own, having no weather of its own to answer to; this gives it some, and
    /// the sway stands down rather than adding to it. So a wind gusting either side of nothing
    /// sways the fire about as much as it swayed alone, a steady one holds it at a lean, and a
    /// wind blowing at nothing stands it up straight. See the [module docs](super#wind).
    ///
    /// The wind's speed is read as it stands, so this belongs in
    /// [`Game::update`](crate::Game::update) just after [`Wind::update`]; what it reads holds
    /// until the next call. A gust takes hold of the whole fire, though not evenly: the youngest
    /// flames are in a share of it and the spent ones above go with it entirely, so the fire
    /// flickers where it stands while a [`SmokingFire`]'s smoke is carried off.
    ///
    /// Only what the wind [`blow`](Wind::blow)s is read — its direction and its speed, and not
    /// its exposure: a plume sets its own pace against a wind through that age ramp. Either axis
    /// of it past [`MAX_WIND_SPEED`] is clamped to it.
    ///
    /// [`MAX_WIND_SPEED`]: super::MAX_WIND_SPEED
    #[cfg(feature = "physics")]
    pub fn blown_by(&mut self, wind: &Wind) {
        self.plume.blown_by(wind);
    }

    /// Advances the flames by one frame. Call this from [`Game::update`](crate::Game::update).
    pub fn update(&mut self, ctx: &mut Context) {
        self.plume.update(ctx);
    }

    /// Draws the flames. Call this from [`Game::draw`](crate::Game::draw).
    pub fn draw(&self, gfx: &mut Graphics) {
        self.plume.draw(gfx, FIRE_BIRTH_COLOR, &FIRE_COLOR_STOPS);
    }
}

/// A fire whose burnt-out particles turn into smoke instead of vanishing.
///
/// The smoke picks up exactly where the flames end — same positions, same sway — and drifts on
/// at half their pace, so the two read as one column rather than two effects sharing a screen.
pub type SmokingFire<const SCALE: usize = FULL_SCALE> = Fire<SCALE, SMOKING_FIRE_LIFETIME>;

/// The `LIFETIME` of a [`SmokingFire`]: a full flame life followed by an equally long smoky one.
pub const SMOKING_FIRE_LIFETIME: usize = 2 * DEFAULT_LIFETIME;

/// The color a flame particle is born with.
const FIRE_BIRTH_COLOR: Color = Color::WHITE;

/// What a flame particle turns into, and at what age: yellow, then orange, then — for a
/// [`SmokingFire`], whose particles live long enough to reach them — the two greys of smoke.
const FIRE_COLOR_STOPS: [(usize, Color); 4] = [
    (4, Color::YELLOW),
    (10, Color::ORANGE),
    (DEFAULT_LIFETIME, Color::LIGHT_GREY),
    (DEFAULT_LIFETIME + 10, Color::DARK_GREY),
];

/// How fast flame particles rise, in sub-pixel units per update at [`FULL_SCALE`].
pub(super) const FIRE_SPEED: Range<i32> = SUBPIXELS / 2..SUBPIXELS * 3 / 2;
