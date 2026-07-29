//! Stopping at the map: the box the tiles stop, and the resolution between them.

use core::iter;

use super::{Bounds, Contact, Contacts, Velocity};
use crate::{BitFlags, Body, SpriteFlag};

/// An entity's own rectangle, placed exactly, and the tiles that are walls to it.
///
/// What [`Kinetic::step`](super::Kinetic::step) builds each update out of the two things an entity
/// says about itself — the [`bounds`](super::Kinetic::bounds) it covers and the
/// [`solid`](super::Kinetic::solid) flags that stop it — and throws away again once the update is
/// resolved. A cart never sees one: it describes the shape, and this is the shape doing the work.
pub(super) struct MapCollider {
    /// The top-left corner, in the exact sub-pixel coordinates the resolution keeps to.
    position: (f32, f32),
    /// How big the box is, in pixels.
    size: (u16, u16),
    /// The sprite flags that mean *wall*. Never empty: an entity that named none has no collider
    /// at all.
    solid: BitFlags<SpriteFlag>,
}

impl MapCollider {
    /// The box `bounds` is stopped at on the map, or `None` when nothing on it does.
    ///
    /// Two ways of being stopped by nothing, told apart here once so the map is never asked
    /// about them again: an entity that names no solid flags, and one whose rectangle is
    /// [empty](Bounds::is_empty) and so covers no pixel a tile could be under.
    pub(super) fn new(body: &Body, bounds: Bounds, solid: BitFlags<SpriteFlag>) -> Option<Self> {
        if solid.is_empty() || bounds.is_empty() {
            return None;
        }

        // The rectangle is whole pixels measured from where the body draws, and the resolution
        // below keeps to the exact sub-pixel position: it adds this update's movement *before*
        // truncating to a pixel, so a body a fraction short of a tile still enters it. Carrying
        // the rectangle across as an offset from the drawn corner keeps both — the fraction, and
        // wherever the entity chose to put its rectangle.
        let (x, y) = body.pos();
        let (draw_x, draw_y) = body.draw_pos();

        Some(Self {
            position: (
                x + (bounds.x() - draw_x) as f32,
                y + (bounds.y() - draw_y) as f32,
            ),
            size: (bounds.width(), bounds.height()),
            solid,
        })
    }

    /// Whether a tile carrying `flags` is a wall to this entity.
    ///
    /// Any flag in common is enough, which is what lets one map carry a cart's walls, its water
    /// and its ladders on separate flags and each entity stop at the ones that concern it.
    pub(super) fn stops_at(&self, flags: BitFlags<SpriteFlag>) -> bool {
        self.solid.intersects(flags)
    }

    /// One update of `velocity` with whatever ran into the map taken out of it — and which sides
    /// that was.
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
        velocity: Velocity,
        solid: impl Fn(i16, i16) -> bool,
    ) -> (Velocity, Contacts) {
        let (x, y) = self.position;
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

    /// Whether the box, placed with its top-left at the pixel (`x`, `y`), covers a tile `solid`
    /// calls a wall.
    ///
    /// The float position is floored to a pixel first, and the pixel to the tile it falls in —
    /// floored, and not truncated, so that the pixel a box is on is the one it would draw at on
    /// both sides of the origin.
    fn covers_solid(&self, x: f32, y: f32, solid: &impl Fn(i16, i16) -> bool) -> bool {
        let (x, y) = (crate::motion::floor_i16(x), crate::motion::floor_i16(y));
        let (width, height) = self.size;
        for py in edge(y, height) {
            for px in edge(x, width) {
                if solid(px.div_euclid(TILE), py.div_euclid(TILE)) {
                    return true;
                }
            }
        }
        false
    }
}

/// The pixels to sample along one edge of a `size`-pixel box starting at `start`: a tile's worth
/// apart, and the far edge whatever size the box is. `size` is never zero — an empty box has no
/// collider at all.
///
/// A sprite-sized 8-pixel side is its two ends, so the four corners are all an ordinary entity
/// ever costs — the same four the hand-rolled version of this in every platformer checks. Bigger
/// boxes get the middles as well, because a tile is eight pixels and anything wider could
/// otherwise straddle one without either corner landing in it.
fn edge(start: i16, size: u16) -> impl Iterator<Item = i16> {
    // Held inside the coordinate space the samples are taken in: a side longer than an `i16` can
    // count would otherwise put its far edge to the *left* of its near one, and a box that big
    // would sample somewhere it does not cover and be stopped by nothing at all.
    let last = size.saturating_sub(1).min(i16::MAX as u16) as i16;

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

    /// One sprite's worth of box at a position, stopping at the first sprite flag — the ordinary
    /// entity.
    fn hitbox(x: f32, y: f32) -> MapCollider {
        sized(x, y, 8, 8)
    }

    /// One of a size of its own, for the boxes that are not a sprite.
    fn sized(x: f32, y: f32, width: u16, height: u16) -> MapCollider {
        let body = Body::new(x, y);
        let bounds = Bounds::of(&body, width, height);

        MapCollider::new(&body, bounds, SpriteFlag::Flag0.into()).unwrap()
    }

    #[test]
    fn an_entity_stopped_by_no_flag_at_all_has_no_collider() {
        // The one an entity says by naming nothing solid: no box is built, so `step` never asks
        // the map about it.
        let body = Body::new(0.0, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        assert!(MapCollider::new(&body, bounds, BitFlags::empty()).is_none());
        assert!(MapCollider::new(&body, bounds, SpriteFlag::Flag0.into()).is_some());
    }

    #[test]
    fn a_fall_stops_on_the_floor_and_reports_it() {
        // Air with a floor along the bottom; the entity sits one pixel above it, falling fast.
        let floor = map(&["....", "....", "####"]);
        let (moved, contacts) = hitbox(0.0, 7.0).resolve(Velocity::new(0.0, 4.0), floor);
        assert_eq!(moved, Velocity::default(), "it fell through the floor");
        assert!(contacts.below());
        assert!(!contacts.above() && !contacts.left() && !contacts.right());
    }

    #[test]
    fn a_fall_that_clears_the_floor_is_left_alone() {
        let floor = map(&["....", "....", "####"]);
        let velocity = Velocity::new(0.5, 1.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, floor);
        assert_eq!(moved, velocity);
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn the_sub_pixel_position_is_kept_rather_than_the_drawn_one() {
        // A fraction short of the wall at tile 1, moving half a pixel: the movement is added
        // before the truncation, so the entity is stopped this update. Resolving from the drawn
        // pixel — a whole number — would have it a pixel short and let it through.
        let wall = map(&[".#"]);
        let body = Body::new(0.5, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        let collider = MapCollider::new(&body, bounds, SpriteFlag::Flag0.into()).unwrap();
        assert_eq!(
            bounds.x(),
            0,
            "the drawn pixel is the floor of the exact one"
        );
        let (moved, contacts) = collider.resolve(Velocity::new(0.5, 0.0), wall);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());
    }

    #[test]
    fn a_rectangle_with_no_pixels_in_it_has_no_collider() {
        // It covers nothing, so there is nothing for a tile to be under — the same answer
        // `Bounds::overlaps` gives it, rather than the single stray pixel a zero-length side
        // would otherwise be sampled at.
        let body = Body::new(0.0, 0.0);
        let solid = SpriteFlag::Flag0.into();
        for empty in [Bounds::new(0, 0, 0, 8), Bounds::new(0, 0, 8, 0)] {
            assert!(MapCollider::new(&body, empty, solid).is_none(), "{empty:?}");
        }
    }

    #[test]
    fn a_box_too_long_to_measure_still_samples_inside_itself() {
        // A side longer than an `i16` can count. Left alone, its far edge wraps round to the
        // left of its near one and the box is stopped by nothing at all; held inside the
        // coordinate space, it is still a box with a wall to its right.
        for size in [32768u16, 32769, 40000, u16::MAX] {
            let samples: Vec<i16> = edge(0, size).collect();
            assert_eq!(samples.first(), Some(&0), "{size} lost its near edge");
            assert!(
                samples.windows(2).all(|p| p[1] > p[0]),
                "{size} sampled backwards: {:?}..",
                &samples[..samples.len().min(4)]
            );
        }

        let wall = map(&[".#"]);
        let (moved, contacts) = sized(0.0, 0.0, 40000, 8).resolve(Velocity::new(1.0, 0.0), wall);
        assert_eq!(moved, Velocity::default(), "it walked through the wall");
        assert!(contacts.right());
    }

    #[test]
    fn a_box_over_the_origin_is_sampled_at_the_pixel_it_draws_on() {
        // Half a pixel to the left of zero covers pixel -1, which floors to -1 and truncates to
        // 0. Truncating puts the box a pixel to the right of where it is, and it is stopped by
        // a wall it has not reached.
        let wall = map(&["..#"]);
        let body = Body::new(-0.5, 0.0);
        let collider = MapCollider::new(&body, Bounds::of(&body, 9, 8), SpriteFlag::Flag0.into());
        let (moved, contacts) = collider.unwrap().resolve(Velocity::default(), wall);
        assert_eq!(moved, Velocity::default());
        assert_eq!(
            contacts,
            Contacts::empty(),
            "stopped by a wall two tiles off"
        );
    }

    #[test]
    fn a_rectangle_is_stopped_where_the_entity_put_it() {
        // A four-pixel box inset two pixels into an eight-pixel sprite, which is how an entity
        // asks for a hitbox narrower than what it draws. A wall the sprite's own corner would
        // have reached is two pixels short of the box, and the box goes past it.
        let wall = map(&[".#"]);
        let body = Body::new(0.0, 0.0);
        let inset = Bounds::new(body.draw_x() + 2, body.draw_y(), 4, 8);
        let collider = MapCollider::new(&body, inset, SpriteFlag::Flag0.into()).unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(2.0, 0.0), wall);
        assert_eq!(
            moved,
            Velocity::new(2.0, 0.0),
            "the inset box was stopped early"
        );
        assert_eq!(contacts, Contacts::empty());

        // Two pixels further and the box itself reaches the wall, so it is stopped there.
        let collider = MapCollider::new(&body, inset, SpriteFlag::Flag0.into()).unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(4.0, 0.0), map(&[".#"]));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());

        // And the sprite-sized box at the same body, which reaches the wall two pixels sooner.
        let whole = Bounds::of(&body, 8, 8);
        let collider = MapCollider::new(&body, whole, SpriteFlag::Flag0.into()).unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(2.0, 0.0), map(&[".#"]));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());
    }

    #[test]
    fn each_side_is_reported_from_the_way_it_was_moving() {
        // A box of walls with a one-tile hollow at (1, 1), and an entity sitting in it.
        let room = map(&["###", "#.#", "###"]);
        for (velocity, expected) in [
            (Velocity::new(0.0, 2.0), Contact::Below),
            (Velocity::new(0.0, -2.0), Contact::Above),
            (Velocity::new(-2.0, 0.0), Contact::Left),
            (Velocity::new(2.0, 0.0), Contact::Right),
        ] {
            let (moved, contacts) = hitbox(8.0, 8.0).resolve(velocity, map(&["###", "#.#", "###"]));
            assert_eq!(moved, Velocity::default(), "it left the room");
            assert_eq!(
                contacts,
                expected.into(),
                "{velocity:?} touched the wrong side"
            );
        }

        // Into a corner: both sides at once, and the entity stays put.
        let (moved, contacts) = hitbox(8.0, 8.0).resolve(Velocity::new(-2.0, 2.0), room);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.left() && contacts.below());
    }

    #[test]
    fn a_blocked_axis_leaves_the_other_one_moving() {
        // A wall on the left of the hollow only: pressed into it while falling, the entity slides
        // down it rather than stopping dead.
        let wall = map(&["#..", "#..", "#.."]);
        let (moved, contacts) = hitbox(8.0, 0.0).resolve(Velocity::new(-2.0, 1.0), wall);
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
        let (moved, contacts) = hitbox(0.0, 8.0).resolve(Velocity::new(8.0, 1.0), wall);
        assert_eq!(moved, Velocity::new(0.0, 1.0));
        assert!(contacts.right());
        assert!(!contacts.below(), "it landed on a wall");
    }

    #[test]
    fn a_standstill_against_a_wall_reports_nothing() {
        // A side is reported for being moved into, so an entity resting against a wall it is not
        // pushing on has touched nothing this update.
        let room = map(&["###", "#.#", "###"]);
        let (moved, contacts) = hitbox(8.0, 8.0).resolve(Velocity::default(), room);
        assert_eq!(moved, Velocity::default());
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn a_wide_box_cannot_straddle_a_tile() {
        // Three tiles wide, with a single wall tile under its middle: sampling the corners alone
        // would miss it entirely and drop the entity through.
        let spike = map(&["....", "....", ".#.."]);
        let (moved, contacts) = sized(0.0, 8.0, 24, 8).resolve(Velocity::new(0.0, 1.0), spike);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.below());

        // And the same box over clear ground is not stopped by anything.
        let (moved, _) =
            sized(0.0, 8.0, 24, 8).resolve(Velocity::new(0.0, 1.0), map(&["....", "....", "...."]));
        assert_eq!(moved, Velocity::new(0.0, 1.0));
    }

    #[test]
    fn a_box_smaller_than_a_tile_samples_its_own_corners() {
        // A one-pixel entity is one sample, and a small one its own four corners: the sampling
        // never reaches past the box it was given.
        let wall = map(&[".#"]);
        let (moved, contacts) = sized(7.0, 0.0, 1, 1).resolve(Velocity::new(1.0, 0.0), wall);
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());

        // Seven pixels to the left of the same wall, the mote is clear of it.
        let (moved, _) = sized(0.0, 0.0, 1, 1).resolve(Velocity::new(1.0, 0.0), map(&[".#"]));
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
        let body = Body::new(0.0, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        let solid = SpriteFlag::Flag0 | SpriteFlag::Flag1;
        let walls = MapCollider::new(&body, bounds, solid).unwrap();
        assert!(walls.stops_at(SpriteFlag::Flag1.into()));
        assert!(walls.stops_at(SpriteFlag::Flag1 | SpriteFlag::Flag7));
        assert!(!walls.stops_at(SpriteFlag::Flag7.into()));
        // Not, as "contains" would have it, by a tile carrying no flags at all.
        assert!(!walls.stops_at(BitFlags::empty()));
    }
}
