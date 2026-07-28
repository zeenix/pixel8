//! Stopping at the map: the hitbox, the sides that touched, and the resolution between them.

use core::iter;

use super::Velocity;
use crate::{flags::bitflag_enum, BitFlags, Dim, SpriteFlag, ZeroSize};

/// The solid rectangle an entity occupies, and what counts as a wall to it.
///
/// The rectangle is in pixels and anchored at the [`Body`](crate::Body)'s position, top-left, so
/// it lines up with where the entity's sprite draws: `Collider::new(8, 8, ..)` is exactly one
/// sprite's worth. `solid` is the sprite flags that stop it — a tile is a wall when its own flags
/// have any of them — which is the same flag a cart marks its walls with for
/// [`Graphics::map`](crate::Graphics::map). A collider with no flags in it collides with nothing.
///
/// An entity hands one to [`Kinetic::collider`](super::Kinetic::collider) and
/// [`Kinetic::step`](super::Kinetic::step) does the rest. See the [module docs](super#collision).
///
/// ```no_run
/// # use pixel8::{physics::Collider, SpriteFlag};
/// const SOLID: SpriteFlag = SpriteFlag::Flag0;
/// // One sprite's worth of hitbox. A size of nothing is no hitbox at all, so this is fallible in
/// // the same way the drawing calls are.
/// let hitbox = Collider::new(8, 8, SOLID).unwrap();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collider {
    width: u16,
    height: u16,
    solid: BitFlags<SpriteFlag>,
}

impl Collider {
    /// A hitbox `width` x `height` pixels, stopping at tiles flagged `solid`.
    ///
    /// `Err(ZeroSize)` for a size that is not strictly positive, exactly as the drawing calls
    /// report one: a hitbox with no pixels in it would pass through everything, which is never
    /// what a cart meant to ask for.
    pub fn new(
        width: impl Dim,
        height: impl Dim,
        solid: impl Into<BitFlags<SpriteFlag>>,
    ) -> Result<Self, ZeroSize> {
        Ok(Self {
            width: width.to_nonzero().ok_or(ZeroSize)?.get(),
            height: height.to_nonzero().ok_or(ZeroSize)?.get(),
            solid: solid.into(),
        })
    }

    /// How wide the hitbox is, in pixels.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// How tall the hitbox is, in pixels.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The sprite flags that mean *wall* to this entity. A tile stops it when its own flags have
    /// any of them.
    pub fn solid(&self) -> BitFlags<SpriteFlag> {
        self.solid
    }

    /// Whether a tile carrying `flags` is a wall to this entity.
    ///
    /// Any flag in common is enough, which is what lets one map carry a cart's walls, its water
    /// and its ladders on separate flags and each entity stop at the ones that concern it. A
    /// collider that names no flags at all is stopped by nothing.
    pub(super) fn stops_at(&self, flags: BitFlags<SpriteFlag>) -> bool {
        self.solid.intersects(flags)
    }

    /// One update of `velocity` for an entity of this shape at `position`, with whatever ran into
    /// the map taken out of it — and which sides that was.
    ///
    /// `solid` answers, for a pair of *tile* coordinates, whether that tile is a wall. It is a
    /// closure rather than the map itself so that the resolution can be exercised against a map
    /// written down in a test.
    ///
    /// One axis at a time, `x` first and then `y` from where `x` ended up, which is what lets an
    /// entity slide along a wall it is pressed into instead of stopping dead in the corner. A
    /// blocked axis is zeroed outright: this update's movement along it is dropped *and* so is the
    /// speed behind it, since a fall that lands has been spent and a walk into a wall is over.
    /// Anything driven by input writes its sideways speed afresh every update anyway, so the only
    /// entity that notices is one carrying its own momentum — which is the one that should.
    pub(super) fn resolve(
        &self,
        position: (f32, f32),
        velocity: Velocity,
        solid: impl Fn(i16, i16) -> bool,
    ) -> (Velocity, Contacts) {
        let (x, y) = position;
        let mut moved = velocity;
        let mut contacts = Contacts::empty();

        if self.covers_solid(x + moved.dx, y, &solid) {
            if moved.dx > 0.0 {
                contacts = contacts | Contact::Right;
            } else if moved.dx < 0.0 {
                contacts = contacts | Contact::Left;
            }
            moved.dx = 0.0;
        }
        // From the `x` that survived, so a diagonal move that is blocked sideways still falls.
        if self.covers_solid(x + moved.dx, y + moved.dy, &solid) {
            if moved.dy > 0.0 {
                contacts = contacts | Contact::Below;
            } else if moved.dy < 0.0 {
                contacts = contacts | Contact::Above;
            }
            moved.dy = 0.0;
        }

        (moved, contacts)
    }

    /// Whether the hitbox, placed with its top-left at the pixel (`x`, `y`), covers a tile `solid`
    /// calls a wall.
    ///
    /// The float position is truncated to a pixel first, and the pixel to the tile it falls in.
    fn covers_solid(&self, x: f32, y: f32, solid: &impl Fn(i16, i16) -> bool) -> bool {
        let (x, y) = (x as i16, y as i16);
        for py in edge(y, self.height) {
            for px in edge(x, self.width) {
                if solid(px.div_euclid(TILE), py.div_euclid(TILE)) {
                    return true;
                }
            }
        }
        false
    }
}

bitflag_enum! {
    /// One side of an entity that ran into something solid.
    ///
    /// Screen space, like everything else: [`Below`](Self::Below) is the floor an entity landed
    /// on and [`Above`](Self::Above) the ceiling it bumped its head on.
    pub enum Contact {
        /// Something solid under the entity — it landed. A platformer's *grounded*.
        Below = 1 << 0,
        /// Something solid over the entity — it bumped its head.
        Above = 1 << 1,
        /// A wall to the entity's left.
        Left = 1 << 2,
        /// A wall to the entity's right.
        Right = 1 << 3,
    }
}

/// The sides an entity touched during the update [`Kinetic::step`](super::Kinetic::step) just
/// resolved — none of them, one, or two at once for something wedged into a corner.
///
/// A side is only reported when the entity was *moving* that way and something stopped it, so an
/// entity resting against a wall it is not pushing into reports nothing.
pub type Contacts = BitFlags<Contact>;

impl Contacts {
    /// Something solid stopped the entity falling: it is standing on something.
    ///
    /// The one a platformer reads every update, to know whether a jump is allowed.
    pub fn below(self) -> bool {
        self.contains(Contact::Below)
    }

    /// Something solid stopped the entity rising: it bumped its head.
    pub fn above(self) -> bool {
        self.contains(Contact::Above)
    }

    /// A wall stopped the entity moving left.
    pub fn left(self) -> bool {
        self.contains(Contact::Left)
    }

    /// A wall stopped the entity moving right.
    pub fn right(self) -> bool {
        self.contains(Contact::Right)
    }
}

/// The pixels to sample along one edge of a `size`-pixel hitbox starting at `start`: a tile's
/// worth apart, and the far edge whatever size the box is.
///
/// A sprite-sized 8-pixel side is its two ends, so the four corners are all an ordinary entity
/// ever costs — the same four the hand-rolled version of this in every platformer checks. Bigger
/// boxes get the middles as well, because a tile is eight pixels and anything wider could
/// otherwise straddle one without either corner landing in it.
fn edge(start: i16, size: u16) -> impl Iterator<Item = i16> {
    let last = size.saturating_sub(1) as i16;
    (0..last)
        .step_by(TILE as usize)
        .chain(iter::once(last))
        .map(move |offset| start.saturating_add(offset))
}

/// A map tile is eight pixels square, as everything else in the console is.
const TILE: i16 = 8;

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile map written down: `#` is a wall, anything else is air. Row 0 is the top, and a
    /// coordinate off the edges is air, exactly as [`Context::map_tile`](crate::Context::map_tile)
    /// reports one off the map.
    fn map(rows: &'static [&'static str]) -> impl Fn(i16, i16) -> bool {
        move |tx: i16, ty: i16| {
            if tx < 0 || ty < 0 {
                return false;
            }
            rows.get(ty as usize)
                .and_then(|row| row.as_bytes().get(tx as usize))
                .is_some_and(|&tile| tile == b'#')
        }
    }

    /// One sprite's worth of hitbox, stopping at the first sprite flag — the ordinary case.
    fn hitbox() -> Collider {
        Collider::new(8, 8, SpriteFlag::Flag0).unwrap()
    }

    #[test]
    fn a_collider_wants_a_size_to_have() {
        let hitbox = Collider::new(8, 6, SpriteFlag::Flag0 | SpriteFlag::Flag3).unwrap();
        assert_eq!((hitbox.width(), hitbox.height()), (8, 6));
        assert!(hitbox.solid().contains(SpriteFlag::Flag3));
        // Nothing panics on a size that means nothing; it is reported, like a drawing call's.
        assert_eq!(Collider::new(0, 8, SpriteFlag::Flag0), Err(ZeroSize));
        assert_eq!(Collider::new(8, -1, SpriteFlag::Flag0), Err(ZeroSize));
    }

    #[test]
    fn a_fall_stops_on_the_floor_and_reports_it() {
        // Air with a floor along the bottom; the entity sits one pixel above it, falling fast.
        let floor = map(&["....", "....", "####"]);
        let (moved, contacts) = hitbox().resolve((0.0, 7.0), Velocity::new(0.0, 4.0), floor);
        assert_eq!(moved, Velocity::default(), "it fell through the floor");
        assert!(contacts.below());
        assert!(!contacts.above() && !contacts.left() && !contacts.right());
    }

    #[test]
    fn a_fall_that_clears_the_floor_is_left_alone() {
        let floor = map(&["....", "....", "####"]);
        let velocity = Velocity::new(0.5, 1.0);
        let (moved, contacts) = hitbox().resolve((0.0, 0.0), velocity, floor);
        assert_eq!(moved, velocity);
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn each_side_is_reported_from_the_way_it_was_moving() {
        // A box of walls with a one-tile hollow at (1, 1), and an entity sitting in it.
        let room = map(&["###", "#.#", "###"]);
        let inside = (8.0, 8.0);
        for (velocity, expected) in [
            (Velocity::new(0.0, 2.0), Contact::Below),
            (Velocity::new(0.0, -2.0), Contact::Above),
            (Velocity::new(-2.0, 0.0), Contact::Left),
            (Velocity::new(2.0, 0.0), Contact::Right),
        ] {
            let (moved, contacts) = hitbox().resolve(inside, velocity, map(&["###", "#.#", "###"]));
            assert_eq!(moved, Velocity::default(), "it left the room");
            assert_eq!(
                contacts,
                expected.into(),
                "{velocity:?} touched the wrong side"
            );
        }

        // Into a corner: both sides at once, and the entity stays put.
        let (moved, contacts) = hitbox().resolve(inside, Velocity::new(-2.0, 2.0), room);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.left() && contacts.below());
    }

    #[test]
    fn a_blocked_axis_leaves_the_other_one_moving() {
        // A wall on the left of the hollow only: pressed into it while falling, the entity slides
        // down it rather than stopping dead.
        let wall = map(&["#..", "#..", "#.."]);
        let (moved, contacts) = hitbox().resolve((8.0, 0.0), Velocity::new(-2.0, 1.0), wall);
        assert_eq!(moved, Velocity::new(0.0, 1.0));
        assert!(contacts.left() && !contacts.below());
    }

    #[test]
    fn a_wall_hit_sideways_is_not_mistaken_for_a_floor() {
        // A wall to the right and nothing at all underneath. The vertical check runs from the `x`
        // that survived the horizontal one, so the entity is stopped by the wall and goes on
        // falling past it; checking from where it *tried* to go would have found the same wall
        // under it and stood it on thin air.
        let wall = map(&["...", ".#.", "..."]);
        let (moved, contacts) = hitbox().resolve((0.0, 8.0), Velocity::new(8.0, 1.0), wall);
        assert_eq!(moved, Velocity::new(0.0, 1.0));
        assert!(contacts.right());
        assert!(!contacts.below(), "it landed on a wall");
    }

    #[test]
    fn a_standstill_against_a_wall_reports_nothing() {
        // A side is reported for being moved into, so an entity resting against a wall it is not
        // pushing on has touched nothing this update.
        let room = map(&["###", "#.#", "###"]);
        let (moved, contacts) = hitbox().resolve((8.0, 8.0), Velocity::default(), room);
        assert_eq!(moved, Velocity::default());
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn a_wide_hitbox_cannot_straddle_a_tile() {
        // Three tiles wide, with a single wall tile under its middle: sampling the corners alone
        // would miss it entirely and drop the entity through.
        let spike = map(&["....", "....", ".#.."]);
        let wide = Collider::new(24, 8, SpriteFlag::Flag0).unwrap();
        let (moved, contacts) = wide.resolve((0.0, 8.0), Velocity::new(0.0, 1.0), spike);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.below());

        // And the same box over clear ground is not stopped by anything.
        let (moved, _) = wide.resolve(
            (0.0, 8.0),
            Velocity::new(0.0, 1.0),
            map(&["....", "....", "...."]),
        );
        assert_eq!(moved, Velocity::new(0.0, 1.0));
    }

    #[test]
    fn a_hitbox_smaller_than_a_tile_samples_its_own_corners() {
        // A one-pixel entity is one sample, and a small one its own four corners: the sampling
        // never reaches past the box it was given.
        let mote = Collider::new(1, 1, SpriteFlag::Flag0).unwrap();
        let wall = map(&[".#"]);
        let (moved, contacts) = mote.resolve((7.0, 0.0), Velocity::new(1.0, 0.0), wall);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());

        // Seven pixels to the left of the same wall, the mote is clear of it.
        let (moved, _) = mote.resolve((0.0, 0.0), Velocity::new(1.0, 0.0), map(&[".#"]));
        assert_eq!(moved, Velocity::new(1.0, 0.0));
    }

    #[test]
    fn the_edge_samples_cover_every_tile_a_side_crosses() {
        // Never more than a tile apart, always both ends, and never outside the box.
        for size in [1u16, 2, 8, 9, 16, 17, 24, 40] {
            let samples: Vec<i16> = edge(100, size).collect();
            assert_eq!(samples.first(), Some(&100), "{size} missed its near edge");
            assert_eq!(
                samples.last(),
                Some(&(100 + size as i16 - 1)),
                "{size} missed its far edge"
            );
            for pair in samples.windows(2) {
                assert!(pair[1] - pair[0] <= TILE, "{size} skipped a tile");
            }
        }
    }

    #[test]
    fn a_tile_stops_an_entity_by_any_flag_they_share() {
        let walls = Collider::new(8, 8, SpriteFlag::Flag0 | SpriteFlag::Flag1).unwrap();
        assert!(walls.stops_at(SpriteFlag::Flag1.into()));
        assert!(walls.stops_at(SpriteFlag::Flag1 | SpriteFlag::Flag7));
        assert!(!walls.stops_at(SpriteFlag::Flag7.into()));
        assert!(!walls.stops_at(BitFlags::empty()));

        // A collider that names no flags is stopped by nothing — not, as "contains" would have
        // it, by every tile on the map.
        let ghost = Collider::new(8, 8, BitFlags::empty()).unwrap();
        assert!(!ghost.stops_at(SpriteFlag::Flag0.into()));
        assert!(!ghost.stops_at(BitFlags::empty()));
    }
}
