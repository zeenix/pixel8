//! The pixels an entity covers, and what two of them have to say to each other.

use crate::{Body, SCREEN_HEIGHT, SCREEN_WIDTH};

/// The rectangle something covers, and what a cart's own collisions are judged against.
///
/// One entity against another, either of them against the edges of the screen, or a door or a
/// trigger the level puts down once and never moves. [`Kinetic::bounds`] is where an entity says
/// which rectangle is its own, and every question about two of them is asked here.
///
/// Whole pixels, measured from where a [`Body`] *draws* rather than the sub-pixel position it
/// tracks: two things that overlap on screen overlap here, which is the only answer a player will
/// accept.
///
/// Every rectangle is a rectangle. There is no size to get wrong and nothing to unwrap: a side of
/// zero gives the [empty](Self::is_empty) rectangle, which covers no pixels and so overlaps
/// nothing, is nowhere on screen, and holds nothing inside it.
///
/// [`Body`]: crate::Body
/// [`Kinetic::bounds`]: super::Kinetic::bounds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

impl Bounds {
    /// A rectangle `width` x `height` pixels with its top-left corner at (`x`, `y`).
    ///
    /// `const`, so a rectangle a cart knows at compile time — a level, a doorway, the area a
    /// trigger covers — can be written down once as one:
    ///
    /// ```
    /// # use pixel8::physics::Bounds;
    /// // The level, which is bigger than the screen, and the doorway out of it.
    /// const LEVEL: Bounds = Bounds::new(0, 0, 256, 128);
    /// const EXIT: Bounds = Bounds::new(112, 96, 16, 32);
    /// ```
    pub const fn new(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The rectangle a `body` covers, `width` x `height` pixels from where it draws.
    ///
    /// [`new`](Self::new) with the body's own position filled in, which is how an entity almost
    /// always answers [`Kinetic::bounds`](super::Kinetic::bounds):
    ///
    /// ```
    /// # use pixel8::{physics::{Bounds, Kinetic}, Body};
    /// # struct Hero { body: Body }
    /// # impl Hero {
    /// fn bounds(&self) -> Bounds {
    ///     Bounds::of(&self.body, 8, 8)
    /// }
    /// # }
    /// ```
    pub fn of(body: &Body, width: u16, height: u16) -> Self {
        let (x, y) = body.draw_pos();

        Self::new(x, y, width, height)
    }

    /// The screen, as a rectangle.
    ///
    /// The limit most carts hold their entities inside — see
    /// [`Kinetic::keep_within`](super::Kinetic::keep_within) — and what
    /// [`on_screen`](Self::on_screen) is measured against. A cart with a level bigger than the
    /// screen writes down the level instead; this knows nothing of a
    /// [`camera`](crate::Graphics::camera).
    pub const fn screen() -> Self {
        Self::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    /// The left edge: the first pixel column the rectangle covers.
    pub const fn x(&self) -> i16 {
        self.x
    }

    /// The top edge: the first pixel row the rectangle covers.
    pub const fn y(&self) -> i16 {
        self.y
    }

    /// How wide the rectangle is, in pixels.
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// How tall the rectangle is, in pixels.
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Whether the rectangle covers no pixels at all, either side of it being zero.
    ///
    /// An empty rectangle is nothing rather than a point: it [overlaps](Self::overlaps) nothing,
    /// including itself, and it is not [on screen](Self::on_screen). A cart working its sizes out
    /// at run time — a blast radius that has shrunk to nothing, a hitbox switched off for the
    /// frames an entity is invulnerable — gets that answer instead of an error to handle.
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The first pixel column to the *right* of the rectangle.
    ///
    /// The far edges are one past the last pixel, the way a width is: a rectangle at `x` 0 that is
    /// eight wide covers 0 to 7 and its `right` is 8. So two rectangles that share an edge are
    /// side by side rather than overlapping.
    ///
    /// Saturating, so a rectangle at the end of the coordinate space does not wrap around to the
    /// start of it. One with nowhere left to extend into is squeezed flat instead, and a flat
    /// rectangle overlaps nothing, exactly as an [empty](Self::is_empty) one does.
    pub const fn right(&self) -> i16 {
        self.x.saturating_add_unsigned(self.width)
    }

    /// The first pixel row *below* the rectangle. See [`right`](Self::right).
    pub const fn bottom(&self) -> i16 {
        self.y.saturating_add_unsigned(self.height)
    }

    /// Whether the two rectangles have any pixel in common.
    ///
    /// One shared pixel is a hit, and a shared edge is not — see [`right`](Self::right). An
    /// [empty](Self::is_empty) rectangle has no pixels to share and so never hits anything.
    pub const fn overlaps(&self, other: Bounds) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Whether any pixel of the rectangle is on the screen.
    ///
    /// This is what a cart drops a stray bullet or a spent enemy on: nothing here keeps an entity
    /// on the screen — it is free to travel right off it — and this is how the cart notices it has
    /// gone. One that should not be allowed to leave is held there instead, by
    /// [`Kinetic::keep_within`](super::Kinetic::keep_within).
    ///
    /// Measured against [`screen`](Self::screen), so it means the first screenful of the world.
    /// A cart that scrolls with a [`camera`](crate::Graphics::camera) is asking about somewhere
    /// else and should say so, with [`overlaps`](Self::overlaps) against a rectangle of its own.
    pub const fn on_screen(&self) -> bool {
        self.overlaps(Self::screen())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One sprite's worth of rectangle, which is what most entities are.
    fn sprite(x: i16, y: i16) -> Bounds {
        Bounds::new(x, y, 8, 8)
    }

    #[test]
    fn a_rectangle_keeps_the_corner_and_the_size_it_was_given() {
        let bounds = Bounds::new(-3, 4, 16, 7);
        assert_eq!((bounds.x(), bounds.y()), (-3, 4));
        assert_eq!((bounds.width(), bounds.height()), (16, 7));
        assert!(!bounds.is_empty());

        // And it can be written down at compile time, which is the point of asking for nothing
        // that has to be checked.
        const LEVEL: Bounds = Bounds::new(0, 0, 256, 128);
        assert_eq!(LEVEL.right(), 256);
    }

    #[test]
    fn a_rectangle_with_a_side_of_nothing_is_nothing() {
        // No error to unwrap, and no pixels either: whichever side went to zero, what comes back
        // covers nothing, so it hits nothing and is nowhere.
        for empty in [
            Bounds::new(0, 0, 0, 8),
            Bounds::new(0, 0, 8, 0),
            Bounds::new(0, 0, 0, 0),
        ] {
            assert!(empty.is_empty(), "{empty:?}");
            assert!(!empty.overlaps(sprite(0, 0)), "{empty:?} hit something");
            assert!(!sprite(0, 0).overlaps(empty), "something hit {empty:?}");
            assert!(!empty.overlaps(empty), "{empty:?} hit itself");
            assert!(!empty.on_screen(), "{empty:?} was on screen");
        }

        // The far edges are still the near ones, so nothing has grown a pixel from being empty.
        let empty = Bounds::new(5, 6, 0, 0);
        assert_eq!((empty.right(), empty.bottom()), (5, 6));
    }

    #[test]
    fn the_far_edges_are_one_past_the_last_pixel() {
        // Deliberately not square, and not at the origin: a rectangle that reads its height for
        // its right edge, or vice versa, has to fail this.
        let bounds = Bounds::new(3, 100, 16, 4);
        assert_eq!(bounds.right(), 19);
        assert_eq!(bounds.bottom(), 104);
    }

    #[test]
    fn rectangles_overlap_on_the_pixels_they_share() {
        // Squarely on top of one another, and a single corner pixel's worth.
        assert!(sprite(0, 0).overlaps(sprite(0, 0)));
        assert!(sprite(0, 0).overlaps(sprite(7, 7)));

        // A shared edge is not an overlap, on either axis.
        assert!(!sprite(0, 0).overlaps(sprite(8, 0)));
        assert!(!sprite(0, 0).overlaps(sprite(0, 8)));

        // Lined up on one axis and clear of each other on the other.
        assert!(!sprite(0, 0).overlaps(sprite(0, 9)));
        assert!(!sprite(0, 0).overlaps(sprite(9, 0)));

        // And one swallowed whole by another.
        let wide = Bounds::new(-4, -4, 32, 32);
        assert!(wide.overlaps(sprite(0, 0)));
    }

    #[test]
    fn overlapping_is_mutual() {
        let wide = Bounds::new(-4, -4, 32, 32);
        for (ours, theirs) in [
            (sprite(0, 0), sprite(7, 7)),
            (sprite(0, 0), sprite(8, 0)),
            (sprite(0, 0), wide),
            (sprite(40, 40), sprite(0, 0)),
        ] {
            assert_eq!(
                ours.overlaps(theirs),
                theirs.overlaps(ours),
                "{ours:?} and {theirs:?} disagree about their overlap"
            );
        }
    }

    #[test]
    fn a_rectangle_is_on_screen_until_the_last_pixel_of_it_leaves() {
        assert!(sprite(0, 0).on_screen());

        // Off each edge in turn, by the pixel that decides it.
        assert!(sprite(-7, 60).on_screen());
        assert!(!sprite(-8, 60).on_screen());
        assert!(sprite(SCREEN_WIDTH as i16 - 1, 60).on_screen());
        assert!(!sprite(SCREEN_WIDTH as i16, 60).on_screen());
        assert!(sprite(60, -7).on_screen());
        assert!(!sprite(60, -8).on_screen());
        assert!(sprite(60, SCREEN_HEIGHT as i16 - 1).on_screen());
        assert!(!sprite(60, SCREEN_HEIGHT as i16).on_screen());
    }

    #[test]
    fn a_body_covers_a_rectangle_from_where_it_draws() {
        let mut body = Body::new(10.0, 20.0);
        assert_eq!(Bounds::of(&body, 8, 8), sprite(10, 20));
        assert!(Bounds::of(&body, 0, 8).is_empty());

        // The drawn position, not the exact one — and the two are only told apart where the
        // coherent pixel is deliberately holding a step back, which a sub-pixel diagonal does.
        for _ in 0..3 {
            body.move_by(0.5, 0.4);
        }
        assert_eq!(
            body.y() as i16,
            21,
            "the exact position is already a row on"
        );
        assert_eq!(
            body.draw_pos(),
            (11, 20),
            "and the drawn one is holding back"
        );
        assert_eq!(Bounds::of(&body, 8, 8), sprite(11, 20));
    }

    #[test]
    fn the_screen_is_the_rectangle_everything_is_drawn_in() {
        const PLAY_AREA: Bounds = Bounds::screen();
        assert_eq!((PLAY_AREA.x(), PLAY_AREA.y()), (0, 0));
        assert_eq!(
            (PLAY_AREA.right(), PLAY_AREA.bottom()),
            (SCREEN_WIDTH as i16, SCREEN_HEIGHT as i16)
        );
        assert!(PLAY_AREA.on_screen());
    }

    /// Corners and sizes worth being suspicious of: both ends of the coordinate space, both
    /// sides of the origin and of the screen, nothing, everything, and the point where a size
    /// stops fitting in the signed coordinate it is added to.
    const CORNERS: [i16; 9] = [
        i16::MIN,
        i16::MIN + 1,
        -129,
        -1,
        0,
        1,
        127,
        i16::MAX - 1,
        i16::MAX,
    ];
    const SIDES: [u16; 7] = [0, 1, 8, 127, 128, 32768, u16::MAX];

    #[test]
    fn no_rectangle_anywhere_can_be_asked_a_question_it_cannot_answer() {
        // There is no size to get wrong, so there is nothing to report and nothing may panic —
        // in debug, where the arithmetic that would have wrapped panics instead.
        for &x in &CORNERS {
            for &y in &CORNERS {
                for &width in &SIDES {
                    for &height in &SIDES {
                        let bounds = Bounds::new(x, y, width, height);
                        assert_eq!((bounds.x(), bounds.y()), (x, y));
                        assert_eq!((bounds.width(), bounds.height()), (width, height));
                        assert_eq!(bounds.is_empty(), width == 0 || height == 0);

                        // A far edge is never behind its near one, however big the side.
                        assert!(bounds.right() >= x, "{bounds:?} wrapped its right edge");
                        assert!(bounds.bottom() >= y, "{bounds:?} wrapped its bottom edge");

                        // It shares a pixel with itself exactly when it has one to share: an
                        // empty rectangle has none, and neither has one saturated flat against
                        // the end of the coordinate space.
                        let has_pixels = bounds.right() > x && bounds.bottom() > y;
                        assert_eq!(bounds.overlaps(bounds), has_pixels, "{bounds:?}");
                        if !has_pixels {
                            assert!(!bounds.on_screen(), "{bounds:?} was on screen");
                        }

                        // And two of them always agree about each other.
                        for other in [Bounds::screen(), sprite(0, 0), Bounds::new(y, x, 4, 4)] {
                            assert_eq!(
                                bounds.overlaps(other),
                                other.overlaps(bounds),
                                "{bounds:?} and {other:?} disagree"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_rectangle_at_the_end_of_the_world_does_not_wrap_around_it() {
        let far = Bounds::new(i16::MAX - 1, i16::MAX - 1, 8, 8);
        assert_eq!((far.right(), far.bottom()), (i16::MAX, i16::MAX));
        assert!(!far.on_screen());
        assert!(!far.overlaps(sprite(0, 0)));

        // The far corner itself, where the edges saturate onto the position: the rectangle has
        // been squeezed to nothing, and the one thing it must still do is not claim a hit.
        let corner = Bounds::new(i16::MAX, i16::MAX, 8, 8);
        assert_eq!((corner.right(), corner.bottom()), (i16::MAX, i16::MAX));
        assert!(!corner.overlaps(sprite(0, 0)));

        // And the other end of the world, which saturation never reaches.
        let near = Bounds::new(i16::MIN, i16::MIN, 8, 8);
        assert_eq!((near.right(), near.bottom()), (i16::MIN + 8, i16::MIN + 8));
        assert!(!near.on_screen());
    }
}
