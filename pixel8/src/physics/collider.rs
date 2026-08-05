//! Stopping at what is solid: the box the world moves, and the resolution between them.

use super::{Bounds, Contact, Contacts, Velocity};
use crate::{BitFlags, Body, SpriteFlag};

/// An entity's own rectangle, placed exactly, and the questions it asks about what is in its way.
///
/// What [`World::step`](super::World::step) builds for each entity out of what the entity says
/// about itself — the [`bounds`](super::Kinetic::bounds) it covers and the
/// [`solid`](super::Kinetic::solid) flags that stop it — and throws away again once that entity is
/// resolved. A cart never sees one: it describes the shape, and this is the shape doing the work.
///
/// Nothing is held here but the box. What is in the way is asked for as the resolution goes: the
/// map through a `tiles` closure, and the rest of the cast through a [`Cast`] walk. Both answer
/// the same question in the same vocabulary — flags shared with `solid` make a wall, and every
/// flag met is reported whether it was a wall or not — so the resolution never has to know which of
/// them it is talking to, and never has to know who anybody is.
pub(super) struct Collider {
    /// The top-left corner, in the exact sub-pixel coordinates the resolution keeps to.
    position: (f32, f32),
    /// How big the box is, in pixels.
    size: (u16, u16),
    /// The sprite flags that mean *wall*. Possibly none: an entity that names nothing is stopped
    /// by nothing, and it is still told whatever it heeds.
    solid: BitFlags<SpriteFlag>,
    /// Everything the entity has any use for meeting: what stops it and what it
    /// [heeds](super::Kinetic::heeds), in one set. Anything outside it is dropped before it is
    /// worked out — a neighbour before its edges are, a tile's flags before they are collected —
    /// which is what a narrow one buys. `solid` is always in it, so nothing here can cost the
    /// entity a wall.
    mask: BitFlags<SpriteFlag>,
    /// The flags the entity itself carries, which is what its arrival means to whoever it arrives
    /// on: a neighbour listening for any of these is [told](Cast::note) of the meeting this
    /// entity's own movement makes. Empty for an entity wearing nothing, whose comings and goings
    /// are nobody's news.
    worn: BitFlags<SpriteFlag>,
    /// Whether the map is worth asking about at all. See
    /// [`World::mapless`](super::World::mapless).
    reads_map: bool,
}

/// One thing in the way that is not a tile: the rectangle it covers *right now*, the sprite
/// flags it carries, and the flags it wants to hear about.
///
/// The whole of what one cast member is to another. The third element is the neighbour's own
/// listening — everything it heeds or calls solid, or nothing at all for a prop — and is what a
/// meeting the *entity's* movement makes is judged against before the neighbour is
/// [told](Cast::note) of it. Nothing in any of it says which entity it was — a collision is judged
/// on pixels and flags, and who is who is [`World`](super::World)'s to know, which is how an
/// entity comes to be skipped against itself without any of this having heard of identity.
pub(super) type Neighbour = (Bounds, BitFlags<SpriteFlag>, BitFlags<SpriteFlag>);

/// The rest of the cast, as the resolution asks about it.
///
/// A slot at a time rather than a list, because there is no list: the world holds the cast as two
/// slices either side of the entity being stepped, and answers for each neighbour as it is
/// reached. So nothing is gathered, nothing is allocated, and every rectangle is the one that
/// neighbour covers *now* rather than one remembered from somewhere else.
pub(super) trait Cast {
    /// Every flag any neighbour carries, or none at all where there is nobody to meet.
    ///
    /// The one question that can be answered about the whole cast without walking it, and what
    /// lets the expensive halves of a step be skipped outright: an entity none of these flags is
    /// solid to has nothing to be pushed out of, and one facing no flags at all has nothing to
    /// meet. It may say more than the walk would — the entity's own slot is in it, and a
    /// neighbour it never reaches — so it can only ever cost a walk that was not needed, never
    /// skip one that was.
    fn carried(&self) -> BitFlags<SpriteFlag>;

    /// How many slots there are to ask about — none at all when nothing in the cast is wearing
    /// anything, since then there is nobody in any of them to meet.
    fn len(&self) -> usize;

    /// What the neighbour in `index` is worth, or `None` where there is nobody in it: the entity
    /// being stepped, or one wearing nothing anybody could run into.
    ///
    /// An accessor the caller drives rather than a visitor handed a closure, and measured rather
    /// than assumed. A closure is fine while it stays small; the moment it captures what a
    /// resolution actually needs — the rectangles of the strip, the endpoint and the ground the
    /// step began on, the flags, the answer so far — it is built on the shadow stack once a walk
    /// and left out of line, so every neighbour costs a call and the loop body is opaque to the
    /// loop. An index costs the caller a bounds check and inlines whole. A generic rather than a
    /// `dyn` for the reason it always was — an indirect call per neighbour on top — and there is
    /// one cast in the console, so there is nothing for monomorphizing this to duplicate.
    fn at(&self, index: usize) -> Option<Neighbour>;

    /// The neighbour in `index` was met by the entity being stepped, and is listening for
    /// something it wears.
    ///
    /// A meeting has two parties and one sweep: only the mover's step sees it, and a neighbour
    /// stepped earlier — or standing still — would otherwise learn of an arrival a frame late, or
    /// never, where the arriver dies of the meeting and leaves the cast. This is the resolution
    /// saying so at the moment it knows, for the world to deliver once the whole cast has moved.
    /// Called only where the neighbour's own listening — the third of its
    /// [three answers](Neighbour) — says the news is wanted; the default drops it, for the walks
    /// written down in tests where nobody is waiting.
    fn note(&self, _index: usize) {}
}

impl Collider {
    /// The box `bounds` is stopped at, or `None` when there is no box at all.
    ///
    /// The one way of having nothing to resolve: a rectangle that is [empty](Bounds::is_empty)
    /// covers no pixel anything could be under. Everything else gets a collider, whatever it
    /// calls solid — an entity that names no flag is stopped by nothing and still comes back
    /// knowing what it walked through.
    #[inline(always)]
    pub(super) fn new(
        body: &Body,
        bounds: Bounds,
        solid: BitFlags<SpriteFlag>,
        heeds: BitFlags<SpriteFlag>,
        worn: BitFlags<SpriteFlag>,
        reads_map: bool,
    ) -> Option<Self> {
        if bounds.is_empty() {
            return None;
        }

        // The rectangle is whole pixels measured from where the body draws, and the resolution
        // below keeps to the exact sub-pixel position: it adds this update's movement *before*
        // truncating to a pixel, so a body a fraction short of a tile still enters it. Carrying
        // the rectangle across as an offset from the drawn corner keeps both — the fraction, and
        // wherever the entity chose to put its rectangle.
        let (x, y) = body.pos();
        let (draw_x, draw_y) = body.draw_pos();
        // The rectangle almost always sits exactly where the body draws — `Bounds::of` puts it
        // there, and an entity that insets a hurtbox is the exception. Where it does, the offset is
        // nothing and the exact position is the body's own, which saves widening two whole-pixel
        // offsets into floats and adding them to it.
        let position = (
            x + (bounds.x() - draw_x) as f32,
            y + (bounds.y() - draw_y) as f32,
        );

        Some(Self {
            position,
            size: (bounds.width(), bounds.height()),
            solid,
            mask: solid | heeds,
            worn,
            reads_map,
        })
    }

    /// Pushes the box out of any solid neighbour it is already inside, before anything is asked to
    /// move: how far it had to go, and the sides it was pushed from.
    ///
    /// A tile cannot arrive on top of an entity; another entity can. A lift stepped a pixel up
    /// into the rider standing flush on it, a crate shoved into somebody, two things spawned on
    /// the same pixels: left alone that overlap would be permanent, since nothing already reached
    /// can stop the entity. So the separating happens first, and the resolution below starts from
    /// a box that is out.
    ///
    /// Out the shallower way — the pixel or two it moved in by, never the length of the thing —
    /// and out the side the box is already nearer, the two rectangles' middles compared rather
    /// than their edges, so a box a whisker past the middle still leaves the way it came. A tie
    /// goes to the vertical, which is the sliver a platform sliding under a rider makes.
    ///
    /// The side reported is the one the entity was pushed *from*, so it reads exactly as being
    /// stopped there does: the rider carried up by the lift is pushed up and reports
    /// [`below`](Contacts::below) — still standing on it, which is all the cart wanted to know.
    ///
    /// A pass takes the neighbours overlapping the box where it found it, in the order the world
    /// keeps the cast, and works through them against a box that keeps moving: something shoved out
    /// of one solid and into another it was already touching is pushed again by that one. A shove
    /// can also carry the box into a solid that only *shared an edge* with it when the pass began,
    /// and which was therefore not on that pass's list at all — so a pass that moved the box is
    /// followed by another, taken from where the box now is, up to [`PASSES`] of them and no
    /// further. A pass that moved nothing ends it early, and everything any pass pushed against is
    /// in the one answer. Neighbours sharing no flag never push and are not reported here — being
    /// in the water is [`resolve`](Self::resolve)'s to tell.
    ///
    /// Only worth calling where [`could_be_inside_something`](Self::could_be_inside_something)
    /// says so.
    #[inline(always)]
    pub(super) fn expel(&mut self, cast: &impl Cast) -> ((f32, f32), Contacts) {
        let solid = self.solid;
        let members = cast.len();
        let mut placed = self.placed(self.position.0, self.position.1);
        let (mut moved_x, mut moved_y) = (0, 0);
        let mut sides = BitFlags::empty();
        let mut touched = BitFlags::empty();

        for _ in 0..PASSES {
            // The box as this pass found it, kept still while the box is pushed around inside it:
            // one pass is one look at who is overlapping, so the shoving cannot chase itself round
            // a cast that never moved.
            let began = placed;
            let began_edges = edges(began);

            for index in 0..members {
                let Some((bounds, flags, _)) = cast.at(index) else {
                    continue;
                };
                let neighbour = edges(bounds);
                if !solid.intersects(flags)
                    || !overlap(began_edges, neighbour)
                    || !overlap(edges(placed), neighbour)
                {
                    continue;
                }

                let (dx, dy, side) = shove(placed, bounds);
                placed = nudged(placed, dx, dy);
                moved_x += dx;
                moved_y += dy;
                sides = sides | side;
                touched = touched | (flags & self.mask);
            }

            // A pass exists only because the one before it moved the box.
            if placed == began {
                break;
            }
        }

        let push = (moved_x as f32, moved_y as f32);
        self.position = (self.position.0 + push.0, self.position.1 + push.1);

        (push, Contacts { sides, touched })
    }

    /// Whether anything in `cast` is a wall this entity could be standing inside.
    ///
    /// The one question about a whole cast that is answered without walking it, and what
    /// [`expel`](Self::expel) is worth calling on: an entity that calls nothing solid can stand in
    /// anything, and one whose walls nobody out there is wearing has nothing to be pushed out of.
    /// The flags may say more than a walk would — the entity's own are in them, and a neighbour it
    /// never reaches — so this can only ever allow a separating that turns out to be unnecessary,
    /// never skip one that was.
    #[inline(always)]
    pub(super) fn could_be_inside_something(&self, cast: &impl Cast) -> bool {
        self.solid.intersects(cast.carried())
    }

    /// One update of `velocity` with whatever ran into a wall taken out of it — and everything the
    /// step met on the way.
    ///
    /// `tiles` answers, for a pair of *tile* coordinates, which sprite flags that tile carries
    /// (none, off the map). `cast` walks the rest of the scene's moving matter, each of them a
    /// rectangle and the flags it carries. Both are handed in rather than reached for, so the
    /// resolution can be exercised against a map and a cast written down in a test.
    ///
    /// One axis at a time, `x` first and then `y` from where `x` ended up, which is what lets an
    /// entity slide along a wall it is pressed into instead of stopping dead in the corner. A
    /// blocked axis is zeroed outright: this update's movement along it is dropped *and* so is the
    /// speed behind it, since a fall that lands has been spent and a walk into a wall is over.
    /// Anything driven by input writes its sideways speed afresh every update anyway, so the only
    /// entity that notices is one carrying its own momentum — which is the one that should.
    ///
    /// The two halves of the answer are asked two different questions, because they are two
    /// different questions. *Stopping* is the endpoint's: the box is placed where each axis is
    /// trying to go, and what is there stops it or does not. *Meeting* is the whole step's: each
    /// axis collects its flags over the strip the box swept along it — from where the step began to
    /// where it was trying to end, tiles and neighbours alike — so a narrow thing crossed between
    /// one pixel and the next is reported like a wide one, and the ground the entity began on is in
    /// the answer even when it has walked off it by the end.
    ///
    /// That leaves one asymmetry, and it is the honest one: something thinner than an update's
    /// movement can be stepped clean over without stopping the entity, and it is still reported as
    /// met. What keeps a fall from doing it to a floor is a terminal velocity — nothing moving
    /// slower than a wall is thick can pass one — which is what
    /// [`Gravity`](super::Gravity)'s cap is for.
    #[inline(always)]
    pub(super) fn resolve(
        &self,
        velocity: Velocity,
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy,
        cast: &impl Cast,
    ) -> (Velocity, Contacts) {
        let (x, y) = self.position;
        // The four pixels this step can put a corner of the box on, floored once each. A float
        // floored to a pixel is a handful of instructions on a wasm target, and placing the box
        // where each axis begins and ends used to redo the same two of them three times over.
        let (from_x, from_y) = (floor(x), floor(y));
        let to_x = floor(x + velocity.dx);
        // The pixels the box covers before it moves, worked out once: a solid neighbour it is
        // *still* inside once the pushing out is done cannot stop it, or a thing wedged between two
        // of them would never move again — see `crossed`.
        let already = self.at(from_x, from_y);
        let mut moved = velocity;
        let mut sides = BitFlags::empty();

        // Sideways first, over everything between where the box was and where it is trying for —
        // a strip that begins at `already`, so what the step started inside is always in the
        // answer.
        //
        // A step going nowhere sideways sweeps nothing here either: its strip is the box it began
        // as, and a step that is falling sweeps its column *from* that very box — so the fall's
        // strip contains this one whole, and everything it would report comes back in the fall's
        // answer anyway. Asking twice would only buy the same rectangle twice over. A step going
        // nowhere at all has no fall to stand in for it, and is still owed the ground it is
        // standing on.
        // An axis going nowhere cannot be stopped, so the wall question is only put where there is
        // movement to lose.
        let (mut touched, stopped) = if moved.dx == 0.0 && moved.dy != 0.0 {
            (BitFlags::empty(), false)
        } else {
            let attempted = self.at(to_x, from_y);
            self.crossed(
                span(already, attempted),
                attempted,
                already,
                moved.dx != 0.0,
                tiles,
                cast,
            )
        };
        if stopped {
            if moved.dx > 0.0 {
                sides = sides | Contact::Right;
            } else {
                sides = sides | Contact::Left;
            }
            moved.dx = 0.0;
        }
        // From the `x` that survived, so a diagonal move that is blocked sideways still falls —
        // and so the strip the fall is collected over is the column the entity really fell down.
        // A step not falling at all sweeps nothing new here: its column is the box where `x`
        // ended up, which the sideways strip already covered.
        if moved.dy != 0.0 {
            // Where `x` really ended up: where it was trying for, or where it began whenever it
            // went nowhere — either because it was never going anywhere or because a wall took it.
            let end_x = if moved.dx == 0.0 { from_x } else { to_x };
            let from = if moved.dx == 0.0 {
                already
            } else {
                self.at(end_x, from_y)
            };
            let attempted = self.at(end_x, floor(y + moved.dy));
            let (met, stopped) =
                self.crossed(span(from, attempted), attempted, already, true, tiles, cast);
            touched = touched | met;
            if stopped {
                if moved.dy > 0.0 {
                    sides = sides | Contact::Below;
                } else {
                    sides = sides | Contact::Above;
                }
                moved.dy = 0.0;
            }
        }

        (moved, Contacts { sides, touched })
    }

    /// One axis, asked once: every flag met over the strip it `swept`, and whether anything stops
    /// the box where the axis was trying to `reach`.
    ///
    /// Two questions of one walk, because they are two questions about the same cast and walking it
    /// twice would buy nothing. They are asked over different ground, though, and that is the whole
    /// design: *meeting* is the strip's — everywhere the box went, so a thing crossed between one
    /// pixel and the next is named — and *stopping* is the endpoint's alone, which is where a wall
    /// has to be to be one. Meeting is collected whatever the entity calls solid: one that names no
    /// flag is told what it walked through exactly as a wall-stopper is, which is a sensor for
    /// nothing. Stopping is asked only where there is movement to lose, which is what `stopping`
    /// says.
    ///
    /// The `already` exemption is the wedge safety. A push that could not fully separate — out of
    /// one solid and straight into another — leaves the entity overlapping something, and a thing
    /// that has already been reached must not also be a wall, or the entity would be pinned there
    /// for good. It reports, and the entity is still free to move.
    #[inline(always)]
    fn crossed(
        &self,
        swept: Bounds,
        reach: Bounds,
        already: Bounds,
        stopping: bool,
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag>,
        cast: &impl Cast,
    ) -> (BitFlags<SpriteFlag>, bool) {
        // Nothing stops what calls nothing solid, and no walk below could say otherwise.
        let wall_hunting = stopping && !self.solid.is_empty();
        let (mut flags, mut stopped) = self.tiles_under(swept, reach, wall_hunting, tiles);

        // The three rectangles the walk below asks about, unpacked into their four edges apiece
        // before anybody is asked anything. Every one of those edges used to be worked out inside
        // the comparison that wanted it — a saturating `i16` addition and the narrowing and
        // widening around it, four of them an overlap — and every neighbour asked for all three.
        // Taken out here they are ordinary `i32` locals the loop compares, which is the whole of
        // what an overlap is.
        let swept = edges(swept);
        let reach = edges(reach);
        let already = edges(already);
        let solid = self.solid;
        let mask = self.mask;
        let worn = self.worn;
        for index in 0..cast.len() {
            let Some((bounds, carried, wants)) = cast.at(index) else {
                continue;
            };
            // A meeting can matter to either of its parties: to this entity, where the neighbour
            // carries something it is stopped by or asked to hear about, and to the neighbour,
            // where it is listening for something this entity wears. One that is neither is
            // refused here, before a single edge of it is worked out — and `solid` is inside the
            // mask, so a wall can never be refused by this.
            let mine = mask.intersects(carried);
            let theirs = wants.intersects(worn);
            if !mine && !theirs {
                continue;
            }
            let bounds = edges(bounds);
            if overlap(swept, bounds) {
                if mine {
                    flags = flags | (carried & mask);
                }
                // The other party's half of the meeting, said where the sweep knows it: the
                // neighbour was met, and it is listening. The world delivers once the cast has
                // moved.
                if theirs {
                    cast.note(index);
                }
            }
            if wall_hunting
                && !stopped
                && solid.intersects(carried)
                && overlap(reach, bounds)
                && !overlap(already, bounds)
            {
                stopped = true;
            }
        }

        (flags, stopped)
    }

    /// Every flag carried by the tiles under the strip the box `swept` — and, where it is hunting a
    /// wall, whether any tile under the endpoint it tried to `reach` is one.
    ///
    /// Both off the one walk of the map. The endpoint is always inside the strip — the strip is the
    /// box at either end of the axis and everything between — so the tiles the wall question is
    /// asked over are a rectangle of the tiles the flags are collected over, and asking separately
    /// bought the same rows a second time, host call and all.
    ///
    /// Flags are collected whatever the entity calls solid, so a step always comes back knowing
    /// which tiles it was on — the water it is swimming in, the hazard it walked over — and never
    /// only the ones that stopped it.
    ///
    /// The pixel a box is on is floored rather than truncated, so it is the one the box would draw
    /// at on both sides of the origin, and the tile is the one that pixel falls in.
    #[inline(always)]
    fn tiles_under(
        &self,
        swept: Bounds,
        reach: Bounds,
        wall_hunting: bool,
        tiles: impl Fn(i16, i16) -> BitFlags<SpriteFlag>,
    ) -> (BitFlags<SpriteFlag>, bool) {
        let mut flags = BitFlags::empty();
        let mut stopped = false;
        // A scene whose map is scenery asks it nothing: no tile carries anything anybody could be
        // stopped by or told about, so the sweep below would collect an empty answer a host call
        // at a time. See [`World::mapless`](super::World::mapless).
        if !self.reads_map {
            return (flags, stopped);
        }
        // Nor is a box that neither stops at nor hears about anything worth a single tile: every
        // answer would be masked to nothing, and `solid` is inside the mask, so nothing could
        // have stopped it either.
        if self.mask.is_empty() {
            return (flags, stopped);
        }
        // The sweep is cut to the map before a tile of it is asked about. Everything past the
        // map's edges answers empty anyway, so nothing changes but the bill: a rectangle the size
        // of the coordinate space — a perfectly safe thing for a cart to write — costs the map's
        // eight thousand tiles, not the sixteen million the space could name.
        let (Some((left, right)), Some((top, bottom))) = (
            fenced(crossed(swept.x(), swept.width()), crate::MAP_WIDTH_TILES),
            fenced(crossed(swept.y(), swept.height()), crate::MAP_HEIGHT_TILES),
        ) else {
            return (flags, stopped);
        };
        // Worked out only where the wall question is going to be asked. A strip with nothing to
        // hunt in it is a plain sweep of the map, and the endpoint's own tiles are nobody's
        // business.
        let ((wall_left, wall_right), (wall_top, wall_bottom)) = if wall_hunting {
            (
                crossed(reach.x(), reach.width()),
                crossed(reach.y(), reach.height()),
            )
        } else {
            ((0, 0), (0, 0))
        };

        // Counted out by hand rather than over a `top..=bottom`, which carries a flag for whether
        // it is finished and tests it twice a tile. The near edge of a span is never past its far
        // one, so the first tile is always asked about, and ending on the `==` is what keeps a span
        // reaching the last tile of the coordinate space from stepping off the end of it.
        let mut ty = top;
        loop {
            let row = wall_hunting && ty >= wall_top && ty <= wall_bottom;
            let mut tx = left;
            loop {
                // Narrowed with nothing to check: the fence above already cut both spans to the
                // map, whose tiles all fit an `i16` with room to spare.
                let carried = tiles(tx as i16, ty as i16);
                flags = flags | (carried & self.mask);
                if row && tx >= wall_left && tx <= wall_right && self.stops_at(carried) {
                    stopped = true;
                }
                if tx == right {
                    break;
                }
                tx += 1;
            }
            if ty == bottom {
                break;
            }
            ty += 1;
        }

        (flags, stopped)
    }

    /// Whether something carrying `flags` — a tile or a neighbour — is a wall to this entity.
    ///
    /// Any flag in common is enough, which is what lets one map carry a cart's walls, its water
    /// and its ladders on separate flags and each entity stop at the ones that concern it. The
    /// rest of the cast answers to the very same rule, so a crate is a wall to whatever the walls
    /// are walls to and water is water to everyone.
    #[inline(always)]
    pub(super) fn stops_at(&self, flags: BitFlags<SpriteFlag>) -> bool {
        self.solid.intersects(flags)
    }

    /// The box, placed with its top-left corner at the pixel the exact position (`x`, `y`) falls
    /// on — floored, like everything here, so both sides of the origin agree.
    #[inline(always)]
    fn placed(&self, x: f32, y: f32) -> Bounds {
        self.at(floor(x), floor(y))
    }

    /// The box, placed with its top-left corner on a pixel already floored.
    #[inline(always)]
    fn at(&self, x: i16, y: i16) -> Bounds {
        let (width, height) = self.size;

        Bounds::new(x, y, width, height)
    }
}

/// The pixel an exact position falls on — floored, like everything here, so both sides of the
/// origin agree.
#[inline(always)]
fn floor(value: f32) -> i16 {
    crate::motion::floor_i16(value)
}

/// A rectangle's four edges, in the `i32` every comparison of them is done in.
///
/// The near ones as they are and the far ones as [`Bounds::right`] and [`Bounds::bottom`] give
/// them, saturating and all. Worked out once per rectangle and then compared as often as the walk
/// needs, rather than worked out again inside each comparison — which is where the arithmetic of a
/// crowded scene used to go.
#[inline(always)]
fn edges(bounds: Bounds) -> (i32, i32, i32, i32) {
    (
        bounds.x() as i32,
        bounds.y() as i32,
        far(bounds.x(), bounds.width()),
        far(bounds.y(), bounds.height()),
    )
}

/// Whether two rectangles have a pixel in common: [`Bounds::overlaps`], off edges already taken.
///
/// A rectangle whose far edge is not past its near one has no pixels to share — either it is
/// [empty](Bounds::is_empty), or it sits at the very end of the coordinate space with a far edge
/// that had nowhere to go, and nothing can reach past that to meet it. Everything else is the four
/// comparisons an overlap has always been.
#[inline(always)]
fn overlap(one: (i32, i32, i32, i32), other: (i32, i32, i32, i32)) -> bool {
    let (left, top, right, bottom) = one;
    let (oleft, otop, oright, obottom) = other;

    left < right
        && top < bottom
        && oleft < oright
        && otop < obottom
        && left < oright
        && right > oleft
        && top < obottom
        && bottom > otop
}

/// The way out of `other` for a box that is inside it: how far along each axis, and the side the
/// box was pushed *from*.
///
/// Whole pixels in `i32`, where neither the overlap nor the doubled middles can run off the end of
/// the coordinate space the rectangles are measured in.
#[inline(always)]
fn shove(placed: Bounds, other: Bounds) -> (i32, i32, Contact) {
    let (x, y) = (placed.x() as i32, placed.y() as i32);
    let (width, height) = (placed.width() as i32, placed.height() as i32);
    let (ox, oy) = (other.x() as i32, other.y() as i32);
    let (owidth, oheight) = (other.width() as i32, other.height() as i32);
    // How deep in it is each way. Both are at least one pixel: the two rectangles overlap.
    let across = (x + width).min(ox + owidth) - x.max(ox);
    let down = (y + height).min(oy + oheight) - y.max(oy);

    // The shallower way out, the vertical taking a tie — the sliver a platform makes is the case
    // worth deciding in the rider's favour. Then the side of it the box is already nearer, which
    // is the middles compared: doubled, so a rectangle of odd width has a middle to compare.
    if across < down {
        if x * 2 + width <= ox * 2 + owidth {
            (-across, 0, Contact::Right)
        } else {
            (across, 0, Contact::Left)
        }
    } else if y * 2 + height <= oy * 2 + oheight {
        (0, -down, Contact::Below)
    } else {
        (0, down, Contact::Above)
    }
}

/// `placed` moved by whole pixels, held inside the coordinate space its corner is measured in: a
/// push at the very end of the world stops there rather than wrapping round to the other one.
#[inline(always)]
fn nudged(placed: Bounds, dx: i32, dy: i32) -> Bounds {
    let held = |value: i32| value.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

    Bounds::new(
        held(placed.x() as i32 + dx),
        held(placed.y() as i32 + dy),
        placed.width(),
        placed.height(),
    )
}

/// The strip a box swept moving `from` one placement `to` another: the two of them and every pixel
/// between.
///
/// The same box at two positions, so the rectangle that holds both is exactly the ground it
/// crossed — which is what an axis collects its flags over. It always contains `from`, so a step
/// asked this way is always told what it began on.
///
/// Measured in `i32`, where a side of it always fits: the far edges saturate rather than wrap, so
/// the widest a strip can be is the whole coordinate space — which is exactly what a `u16` counts.
#[inline(always)]
fn span(from: Bounds, to: Bounds) -> Bounds {
    let x = from.x().min(to.x());
    let y = from.y().min(to.y());
    // The far edges, taken in `i32` and squeezed to what a side can be exactly as
    // [`Bounds::right`] squeezes them — one comparison apiece, where going through the `i16`
    // saturating addition and back is that plus the narrowing and the widening around it.
    let right = far(from.x(), from.width()).max(far(to.x(), to.width()));
    let bottom = far(from.y(), from.height()).max(far(to.y(), to.height()));

    Bounds::new(x, y, (right - x as i32) as u16, (bottom - y as i32) as u16)
}

/// The first pixel past a side that starts at `near` and is `size` long: [`Bounds::right`] and
/// [`Bounds::bottom`], in the `i32` the arithmetic around them is done in.
///
/// Saturating as they are, so a rectangle at the end of the coordinate space is squeezed flat
/// rather than wrapped round to the start of it.
#[inline(always)]
pub(super) fn far(near: i16, size: u16) -> i32 {
    let edge = near as i32 + size as i32;
    if edge > i16::MAX as i32 {
        i16::MAX as i32
    } else {
        edge
    }
}

/// A tile span cut to the map's own run of `tiles`: the part inside `0..tiles`, or `None` when
/// the whole of it lies off the map.
///
/// What keeps a sweep's cost proportional to the map rather than to the coordinate space — see
/// [`Collider::tiles_under`] — and safe to apply because off-map tiles carry nothing: the cut
/// changes what is visited, never what is answered.
fn fenced((near, far): (i32, i32), tiles: u16) -> Option<(i32, i32)> {
    let near = near.max(0);
    let far = far.min(tiles as i32 - 1);

    (near <= far).then_some((near, far))
}

/// The tiles one side of a `size`-pixel box starting at `start` crosses: the first and the last,
/// and everything between is every tile it is on. `size` is never zero — an empty box has no
/// collider at all.
///
/// A side is asked about as a span of tiles rather than as a handful of pixels sampled a tile
/// apart, which is the same set of tiles and a great deal less arithmetic: two shifts a side
/// instead of a division per sample, no iterator to build per row, and a tile asked about once
/// where the near and the far sample used to land in the same one. A sprite-sized 8-pixel side is
/// one tile or two, so the four corners are all an ordinary entity ever costs — the same four the
/// hand-rolled version of this in every platformer checks — and a wider box crosses the tiles
/// under its middle as well, which is exactly what stops it straddling one.
///
/// In `i32`, because the far pixel is `start + size - 1` of a box that may begin at the bottom of
/// `i16` and reach the top: worked out in the narrow type it either wraps or, clamped, walks its
/// far edge to the *left* of its near one and loses the positive half of everything it covers.
/// The far pixel itself is held at `i16::MAX`, which is where the box's own far edge saturates.
#[inline(always)]
fn crossed(start: i16, size: u16) -> (i32, i32) {
    let last = (start as i32 + size as i32 - 1).min(i16::MAX as i32);

    (start as i32 >> TILE_BITS, last >> TILE_BITS)
}

/// A map tile is eight pixels square, as everything else in the console is.
const TILE: i16 = 8;

/// The same eight, said as the shift that divides by it: an arithmetic shift right *is* the floor
/// division a pixel's tile is worked out by, on both sides of the origin, and it is one
/// instruction where the division is a handful. Taken off [`TILE`] itself, so the two cannot drift
/// apart.
const TILE_BITS: u32 = TILE.trailing_zeros();

/// How many times over [`expel`](Collider::expel) is willing to look again.
///
/// Every pass past the first exists only because the one before it moved the box, and there is a
/// scene where that never stops: two solids with no room between them, each shoving the box back
/// into the other, for ever. It has no right answer to find — nowhere in it is out — so the cap
/// ends the shoving where the box stands, after a handful of looks that a scene with room to
/// separate never needs. Nothing is left wedged by stopping there: whatever the box is still inside
/// cannot block it either — see [`crossed`](Collider::crossed) — so it is free to walk out.
const PASSES: usize = 4;

#[cfg(test)]
mod tests {
    use core::iter;

    use super::*;

    /// A tile map written down: `#` is a wall, `~` is water, anything else is air. Row 0 is the
    /// top, and a coordinate off the edges carries nothing, exactly as
    /// [`Context::map_tile`](crate::Context::map_tile) reports one off the map.
    fn map(rows: &'static [&'static str]) -> impl Fn(i16, i16) -> BitFlags<SpriteFlag> + Copy {
        move |tx: i16, ty: i16| {
            if tx < 0 || ty < 0 {
                return BitFlags::empty();
            }
            match rows
                .get(ty as usize)
                .and_then(|row| row.as_bytes().get(tx as usize))
            {
                Some(b'#') => WALL.into(),
                Some(b'~') => WATER.into(),
                _ => BitFlags::empty(),
            }
        }
    }

    /// No walls and no water anywhere: the map every box amid a cast of its own is resolved
    /// against.
    fn air(_: i16, _: i16) -> BitFlags<SpriteFlag> {
        BitFlags::empty()
    }

    /// A cast written down: the rest of the scene's moving matter, handed over in the order it is
    /// listed, exactly as the world hands over its own.
    fn cast(list: &[Neighbour]) -> Written<'_> {
        Written(list)
    }

    /// An empty scene: an entity stepped on its own, which every cart with one moving thing in it
    /// is.
    fn alone() -> Written<'static> {
        Written(&[])
    }

    /// The written-down cast itself.
    struct Written<'a>(&'a [Neighbour]);

    impl Cast for Written<'_> {
        fn carried(&self) -> BitFlags<SpriteFlag> {
            self.0
                .iter()
                .fold(BitFlags::empty(), |all, &(_, f, _)| all | f)
        }

        fn len(&self) -> usize {
            self.0.len()
        }

        fn at(&self, index: usize) -> Option<Neighbour> {
            self.0.get(index).copied()
        }
    }

    /// One sprite's worth of box at a position, stopping at the wall flag — the ordinary entity.
    fn hitbox(x: f32, y: f32) -> Collider {
        sized(x, y, 8, 8)
    }

    /// One of a size of its own, for the boxes that are not a sprite.
    fn sized(x: f32, y: f32, width: u16, height: u16) -> Collider {
        let body = Body::new(x, y);
        let bounds = Bounds::of(&body, width, height);

        Collider::new(
            &body,
            bounds,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap()
    }

    /// One that is told about only what it names, for the tests that pin what heeding buys.
    fn heeding(
        x: f32,
        y: f32,
        solid: BitFlags<SpriteFlag>,
        heeds: BitFlags<SpriteFlag>,
    ) -> Collider {
        let body = Body::new(x, y);
        let bounds = Bounds::of(&body, 8, 8);

        Collider::new(&body, bounds, solid, heeds, BitFlags::empty(), true).unwrap()
    }

    /// A neighbour covering a rectangle of the screen and carrying whatever the tiles carry where
    /// they are walls: the lifts, the crates and the closed doors of a level.
    fn wall(x: i16, y: i16, width: u16, height: u16) -> Neighbour {
        (
            Bounds::new(x, y, width, height),
            WALL.into(),
            BitFlags::empty(),
        )
    }

    /// One carrying a flag of its own, for the things that are met and never stopped at.
    fn carrying(x: i16, y: i16, flags: BitFlags<SpriteFlag>) -> Neighbour {
        (Bounds::new(x, y, 8, 8), flags, BitFlags::empty())
    }

    #[test]
    fn a_rectangle_with_no_pixels_in_it_has_no_collider() {
        // It covers nothing, so there is nothing for a tile to be under — the same answer
        // `Bounds::overlaps` gives it, rather than the single stray pixel a zero-length side
        // would otherwise be sampled at.
        let body = Body::new(0.0, 0.0);
        let solid = WALL.into();
        for empty in [Bounds::new(0, 0, 0, 8), Bounds::new(0, 0, 8, 0)] {
            assert!(
                Collider::new(
                    &body,
                    empty,
                    solid,
                    BitFlags::all(),
                    BitFlags::empty(),
                    true
                )
                .is_none(),
                "{empty:?}"
            );
        }

        // And a box that names nothing solid still gets one: it is stopped by nothing, and it is
        // asked what it walked through all the same.
        let bounds = Bounds::of(&body, 8, 8);
        assert!(Collider::new(
            &body,
            bounds,
            BitFlags::empty(),
            BitFlags::all(),
            BitFlags::empty(),
            true
        )
        .is_some());
    }

    #[test]
    fn what_a_box_heeds_is_what_it_is_told_about() {
        // Two neighbours over the box, one carrying water and one carrying spikes, and a box that
        // asked about the water alone. It swims, and the spikes are not its business.
        let pond = [carrying(0, 0, WATER.into()), carrying(0, 0, SPIKES.into())];
        let (_, contacts) = heeding(0.0, 0.0, BitFlags::empty(), WATER.into()).resolve(
            Velocity::new(1.0, 0.0),
            air,
            &cast(&pond),
        );
        assert!(contacts.touches(WATER));
        assert!(!contacts.touches(SPIKES));

        // And the same box told about neither hears nothing at all, though it swam through both.
        let (_, contacts) = heeding(0.0, 0.0, BitFlags::empty(), BitFlags::empty()).resolve(
            Velocity::new(1.0, 0.0),
            air,
            &cast(&pond),
        );
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn a_box_is_told_about_the_tiles_it_heeds_and_no_others() {
        // The map answers to the same word as the cast: a box swimming across water and spikes
        // and heeding only the water is told only about the water.
        let pool = map(&["~~", "~~"]);
        let (_, contacts) = heeding(0.0, 0.0, BitFlags::empty(), SPIKES.into()).resolve(
            Velocity::new(1.0, 0.0),
            pool,
            &alone(),
        );
        assert_eq!(
            contacts,
            Contacts::empty(),
            "the water was not its business"
        );

        let (_, contacts) = heeding(0.0, 0.0, BitFlags::empty(), WATER.into()).resolve(
            Velocity::new(1.0, 0.0),
            pool,
            &alone(),
        );
        assert!(contacts.touches(WATER));
    }

    #[test]
    fn a_box_the_size_of_the_coordinate_space_costs_the_map_s_worth_of_tiles() {
        // A rectangle covering everything a coordinate can name — a perfectly safe thing for a
        // cart to write — sweeps 4096x4096 tiles' worth of space. The map is 128x64, and the
        // sweep must be billed for the map: every tile past its edges answers empty, so visiting
        // them buys nothing and a cast of such rectangles would turn one metered call into
        // millions of lookups.
        use core::cell::Cell;

        let asked = Cell::new(0u32);
        let counted = |_: i16, _: i16| {
            asked.set(asked.get() + 1);
            BitFlags::empty()
        };
        let body = Body::new(0.0, 0.0);
        let everywhere = Bounds::new(i16::MIN, i16::MIN, u16::MAX, u16::MAX);
        let collider = Collider::new(
            &body,
            everywhere,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        collider.resolve(Velocity::new(1.0, 0.0), counted, &alone());
        assert_eq!(
            asked.get(),
            128 * 64,
            "a box over everything must be asked about the whole map — no less (a span cut wrong
             loses real coverage) and no more (the coordinate space is a thousand times bigger)"
        );

        // And a box standing wholly off the map asks about none at all.
        asked.set(0);
        let body = Body::new(-300.0, -300.0);
        let outside = Bounds::of(&body, 8, 8);
        let collider = Collider::new(
            &body,
            outside,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        collider.resolve(Velocity::new(1.0, 0.0), counted, &alone());
        assert_eq!(asked.get(), 0);
    }

    #[test]
    fn a_fall_stops_on_the_floor_and_reports_it() {
        // Air with a floor along the bottom; the entity sits one pixel above it, falling fast.
        let floor = map(&["....", "....", "####"]);
        let (moved, contacts) = hitbox(0.0, 7.0).resolve(Velocity::new(0.0, 4.0), floor, &alone());
        assert_eq!(moved, Velocity::default(), "it fell through the floor");
        assert!(contacts.below());
        assert!(!contacts.above() && !contacts.left() && !contacts.right());
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn a_fall_that_clears_the_floor_is_left_alone() {
        let floor = map(&["....", "....", "####"]);
        let velocity = Velocity::new(0.5, 1.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, floor, &alone());
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
        let collider = Collider::new(
            &body,
            bounds,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        assert_eq!(
            bounds.x(),
            0,
            "the drawn pixel is the floor of the exact one"
        );
        let (moved, contacts) = collider.resolve(Velocity::new(0.5, 0.0), wall, &alone());
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());
    }

    #[test]
    fn a_box_too_long_to_measure_still_samples_inside_itself() {
        // A side longer than an `i16` can count. Left alone, its far edge wraps round to the
        // left of its near one and the box is stopped by nothing at all; held inside the
        // coordinate space, it is still a box with a wall to its right.
        for size in [32768u16, 32769, 40000, u16::MAX] {
            let (near, far) = crossed(0, size);
            assert_eq!(near, 0, "{size} lost its near edge");
            assert!(far >= near, "{size} crossed backwards: {near}..{far}");
        }

        let wall = map(&[".#"]);
        let (moved, contacts) =
            sized(0.0, 0.0, 40000, 8).resolve(Velocity::new(1.0, 0.0), wall, &alone());
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
        let collider = Collider::new(
            &body,
            Bounds::of(&body, 9, 8),
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        );
        let (moved, contacts) = collider
            .unwrap()
            .resolve(Velocity::default(), wall, &alone());
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
        let collider = Collider::new(
            &body,
            inset,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(2.0, 0.0), wall, &alone());
        assert_eq!(
            moved,
            Velocity::new(2.0, 0.0),
            "the inset box was stopped early"
        );
        assert_eq!(contacts, Contacts::empty());

        // Two pixels further and the box itself reaches the wall, so it is stopped there.
        let collider = Collider::new(
            &body,
            inset,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(4.0, 0.0), map(&[".#"]), &alone());
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());

        // And the sprite-sized box at the same body, which reaches the wall two pixels sooner.
        let whole = Bounds::of(&body, 8, 8);
        let collider = Collider::new(
            &body,
            whole,
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(2.0, 0.0), map(&[".#"]), &alone());
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
            let (moved, contacts) =
                hitbox(8.0, 8.0).resolve(velocity, map(&["###", "#.#", "###"]), &alone());
            assert_eq!(moved, Velocity::default(), "it left the room");
            assert_eq!(
                contacts.sides,
                expected.into(),
                "{velocity:?} touched the wrong side"
            );
            assert!(contacts.touches(WALL));
        }

        // Into a corner: both sides at once, and the entity stays put.
        let (moved, contacts) = hitbox(8.0, 8.0).resolve(Velocity::new(-2.0, 2.0), room, &alone());
        assert_eq!(moved, Velocity::default());
        assert!(contacts.left() && contacts.below());
    }

    #[test]
    fn a_blocked_axis_leaves_the_other_one_moving() {
        // A wall on the left of the hollow only: pressed into it while falling, the entity slides
        // down it rather than stopping dead.
        let wall = map(&["#..", "#..", "#.."]);
        let (moved, contacts) = hitbox(8.0, 0.0).resolve(Velocity::new(-2.0, 1.0), wall, &alone());
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
        let (moved, contacts) = hitbox(0.0, 8.0).resolve(Velocity::new(8.0, 1.0), wall, &alone());
        assert_eq!(moved, Velocity::new(0.0, 1.0));
        assert!(contacts.right());
        assert!(!contacts.below(), "it landed on a wall");
    }

    #[test]
    fn a_standstill_against_a_wall_reports_nothing() {
        // A side is reported for being moved into, so an entity resting against a wall it is not
        // pushing on has touched nothing this update — and it is not inside the wall either, so
        // there is no flag to report.
        let room = map(&["###", "#.#", "###"]);
        let (moved, contacts) = hitbox(8.0, 8.0).resolve(Velocity::default(), room, &alone());
        assert_eq!(moved, Velocity::default());
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn a_wide_box_cannot_straddle_a_tile() {
        // Three tiles wide, with a single wall tile under its middle: sampling the corners alone
        // would miss it entirely and drop the entity through.
        let spike = map(&["....", "....", ".#.."]);
        let (moved, contacts) =
            sized(0.0, 8.0, 24, 8).resolve(Velocity::new(0.0, 1.0), spike, &alone());
        assert_eq!(moved, Velocity::default());
        assert!(contacts.below());

        // And the same box over clear ground is not stopped by anything.
        let (moved, _) = sized(0.0, 8.0, 24, 8).resolve(
            Velocity::new(0.0, 1.0),
            map(&["....", "....", "...."]),
            &alone(),
        );
        assert_eq!(moved, Velocity::new(0.0, 1.0));
    }

    #[test]
    fn a_box_smaller_than_a_tile_samples_its_own_corners() {
        // A one-pixel entity is one sample, and a small one its own four corners: the sampling
        // never reaches past the box it was given.
        let wall = map(&[".#"]);
        let (moved, contacts) =
            sized(7.0, 0.0, 1, 1).resolve(Velocity::new(1.0, 0.0), wall, &alone());
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());

        // Seven pixels to the left of the same wall, the mote is clear of it.
        let (moved, _) =
            sized(0.0, 0.0, 1, 1).resolve(Velocity::new(1.0, 0.0), map(&[".#"]), &alone());
        assert_eq!(moved, Velocity::new(1.0, 0.0));
    }

    #[test]
    fn a_side_crosses_every_tile_it_covers_and_no_others() {
        // The tiles a side is asked about are exactly the tiles its pixels fall in: both ends,
        // everything between, and nothing outside the box.
        for start in [-100i16, -9, -8, -1, 0, 1, 7, 100] {
            for size in [1u16, 2, 7, 8, 9, 16, 17, 24, 40] {
                let (near, far) = crossed(start, size);
                let last = start as i32 + size as i32 - 1;
                assert_eq!(
                    near,
                    (start as i32).div_euclid(8),
                    "{start}+{size} missed its near tile"
                );
                assert_eq!(
                    far,
                    last.div_euclid(8),
                    "{start}+{size} missed its far tile"
                );
                // Every pixel of the side is in a tile the span holds, and every tile the span
                // holds has a pixel of the side in it.
                for pixel in start as i32..=last {
                    let tile = pixel.div_euclid(8);
                    assert!((near..=far).contains(&tile), "{pixel} was left out");
                }
                for tile in near..=far {
                    assert!(
                        (start as i32..=last).any(|pixel| pixel.div_euclid(8) == tile),
                        "{start}+{size} asked about tile {tile}, which it is not on"
                    );
                }
            }
        }
    }

    #[test]
    fn a_side_crosses_the_tiles_a_pixel_walk_of_it_lands_in() {
        // The span replaced a walk that sampled the side every eight pixels and at its far end.
        // Whatever such a walk lands in, the span holds — and holds nothing besides — so no box
        // anywhere is stopped by a tile it was not stopped by before, or let through one it was.
        // The walk is taken over the pixels the side really covers, in arithmetic wide enough to
        // hold them: a side can begin at the bottom of the space and reach the top, where its far
        // edge saturates exactly as a `Bounds`'s does.
        for start in [i16::MIN, -300, -8, -1, 0, 1, 120, i16::MAX - 40] {
            for size in [1u16, 2, 8, 9, 16, 24, 40, 128, 32768, u16::MAX] {
                let (near, far) = crossed(start, size);
                let last = (start as i32 + size as i32 - 1).min(i16::MAX as i32);
                let sampled: Vec<i32> = (start as i32..=last)
                    .step_by(TILE as usize)
                    .chain(iter::once(last))
                    .map(|pixel| pixel.div_euclid(TILE as i32))
                    .collect();
                assert_eq!(
                    (near, far),
                    (
                        *sampled.iter().min().unwrap(),
                        *sampled.iter().max().unwrap()
                    ),
                    "{start}+{size}"
                );
                // Contiguous, so the span is the sampled set rather than merely its hull.
                for tile in near..=far {
                    assert!(sampled.contains(&tile), "{start}+{size} grew tile {tile}");
                }
            }
        }
    }

    #[test]
    fn a_tile_stops_an_entity_by_any_flag_they_share() {
        let body = Body::new(0.0, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        let solid = SpriteFlag::Flag0 | SpriteFlag::Flag1;
        let walls = Collider::new(
            &body,
            bounds,
            solid,
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        assert!(walls.stops_at(SpriteFlag::Flag1.into()));
        assert!(walls.stops_at(SpriteFlag::Flag1 | SpriteFlag::Flag7));
        assert!(!walls.stops_at(SpriteFlag::Flag7.into()));
        // Not, as "contains" would have it, by a tile carrying no flags at all.
        assert!(!walls.stops_at(BitFlags::empty()));
    }

    #[test]
    fn a_tile_that_is_no_wall_is_passed_through_and_still_reported() {
        // Water, to an entity that calls only walls solid: it swims straight through, and the
        // step comes back knowing it is in there. One call, and the cart knows to draw bubbles.
        let pool = map(&["~~~~", "~~~~"]);
        let velocity = Velocity::new(1.0, 1.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, pool, &alone());
        assert_eq!(moved, velocity);
        assert_eq!(contacts.sides, BitFlags::empty());
        assert!(contacts.touches(WATER));
        assert!(!contacts.touches(WALL));
    }

    #[test]
    fn an_entity_stopped_by_nothing_is_still_told_what_it_walked_through() {
        // A sensor: no flag is a wall to it, so nothing anywhere stops it — and the pool of tiles
        // it swam through and the neighbour it walked past both come back all the same.
        let pool = map(&["~~~~", "~~~~"]);
        let body = Body::new(0.0, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        let collider = Collider::new(
            &body,
            bounds,
            BitFlags::empty(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let hazard = [carrying(4, 0, SPIKES.into())];
        let velocity = Velocity::new(1.0, 1.0);
        let (moved, contacts) = collider.resolve(velocity, pool, &cast(&hazard));
        assert_eq!(moved, velocity, "something stopped a sensor");
        assert_eq!(contacts.sides, BitFlags::empty());
        assert!(contacts.touches(WATER) && contacts.touches(SPIKES));

        // And a solid neighbour is no more of a wall to it than the water was.
        let collider = Collider::new(
            &body,
            bounds,
            BitFlags::empty(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let walls = [wall(4, 0, 8, 8)];
        let (moved, contacts) = collider.resolve(velocity, air, &cast(&walls));
        assert_eq!(moved, velocity, "a sensor was walled in");
        assert_eq!(contacts.sides, BitFlags::empty());
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn a_sensor_is_never_pushed_out_of_anything() {
        // The same entity standing inside a solid neighbour: nothing is a wall to it, so nothing
        // shoves it anywhere — and the resolution still reports what it is inside.
        let body = Body::new(0.0, 0.0);
        let bounds = Bounds::of(&body, 8, 8);
        let mut collider = Collider::new(
            &body,
            bounds,
            BitFlags::empty(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let walls = [wall(4, 0, 8, 8)];
        let (push, pushed) = collider.expel(&cast(&walls));
        assert_eq!(push, (0.0, 0.0));
        assert_eq!(pushed, Contacts::empty());
        let (_, contacts) = collider.resolve(Velocity::new(1.0, 0.0), air, &cast(&walls));
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn a_neighbour_stops_a_fall_like_a_floor_tile() {
        // Another entity a pixel under the box, and no map at all.
        let floor = [wall(0, 16, 8, 8)];
        let (moved, contacts) =
            hitbox(0.0, 7.0).resolve(Velocity::new(0.0, 4.0), air, &cast(&floor));
        assert_eq!(moved, Velocity::default(), "it fell through the neighbour");
        assert!(contacts.below());
        assert!(!contacts.above() && !contacts.left() && !contacts.right());
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn each_side_is_reported_off_a_neighbour_as_off_a_tile() {
        // The hollow-in-a-box room of the tile tests, built out of cast members instead.
        let room = [
            wall(0, 0, 24, 8),
            wall(0, 16, 24, 8),
            wall(0, 8, 8, 8),
            wall(16, 8, 8, 8),
        ];
        for (velocity, expected) in [
            (Velocity::new(0.0, 2.0), Contact::Below),
            (Velocity::new(0.0, -2.0), Contact::Above),
            (Velocity::new(-2.0, 0.0), Contact::Left),
            (Velocity::new(2.0, 0.0), Contact::Right),
        ] {
            let (moved, contacts) = hitbox(8.0, 8.0).resolve(velocity, air, &cast(&room));
            assert_eq!(moved, Velocity::default(), "it left the room");
            assert_eq!(
                contacts.sides,
                expected.into(),
                "{velocity:?} touched the wrong side"
            );
            assert!(contacts.touches(WALL));
        }
    }

    #[test]
    fn a_blocked_axis_slides_along_a_neighbour() {
        // Pressed into another's side while falling: the entity slides down it, exactly as it
        // would down a wall of tiles.
        let side = [wall(0, 0, 8, 24)];
        let (moved, contacts) =
            hitbox(8.0, 0.0).resolve(Velocity::new(-2.0, 1.0), air, &cast(&side));
        assert_eq!(moved, Velocity::new(0.0, 1.0));
        assert!(contacts.left() && !contacts.below());
    }

    #[test]
    fn a_standstill_against_a_neighbour_reports_nothing() {
        // Resting exactly on top of it — a shared edge, not an overlap — and not moving into it:
        // nothing was stopped and nothing was met, so nothing is reported.
        let floor = [wall(0, 8, 8, 8)];
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(Velocity::default(), air, &cast(&floor));
        assert_eq!(moved, Velocity::default());
        assert_eq!(contacts, Contacts::empty());
    }

    #[test]
    fn the_sub_pixel_position_is_kept_against_a_neighbour_too() {
        // A fraction short of the neighbour, moving half a pixel: the movement is added before the
        // truncation, so the entity is stopped this update, exactly as at a tile.
        let side = [wall(8, 0, 8, 8)];
        let body = Body::new(0.5, 0.0);
        let collider = Collider::new(
            &body,
            Bounds::of(&body, 8, 8),
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(0.5, 0.0), air, &cast(&side));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());
    }

    #[test]
    fn a_neighbour_sharing_no_flag_is_swum_through_and_still_reported() {
        // A pond the level drags around. It shares no flag with this entity, so it is no wall to
        // it and never shoves it anywhere — and the step still says the entity is in the water.
        let pond = [(Bounds::new(0, 0, 24, 24), WATER.into(), BitFlags::empty())];
        let mut collider = hitbox(8.0, 8.0);
        let (push, pushed) = collider.expel(&cast(&pond));
        assert_eq!(push, (0.0, 0.0), "the water shoved the swimmer out");
        assert_eq!(pushed, Contacts::empty());

        let velocity = Velocity::new(2.0, 0.0);
        let (moved, contacts) = collider.resolve(velocity, air, &cast(&pond));
        assert_eq!(moved, velocity);
        assert_eq!(contacts.sides, BitFlags::empty());
        assert!(contacts.touches(WATER));
    }

    #[test]
    fn a_neighbour_standing_on_the_entity_pushes_it_back_out() {
        // The lift that stepped a pixel up into the rider standing flush on it: a sliver of
        // overlap down and a whole box of it across, so out the rider goes the way it came — up,
        // which is what being stood on reads as.
        let lift = [wall(0, 31, 24, 8)];
        let mut collider = hitbox(0.0, 24.0);
        let (push, pushed) = collider.expel(&cast(&lift));
        assert_eq!(push, (0.0, -1.0));
        assert!(pushed.below() && !pushed.above());
        assert!(pushed.touches(WALL));

        // And out sideways where that is the shallower way: a crate shoved a pixel into the
        // entity's right leaves it out the left, which is the side it was already nearer.
        let crated = [wall(7, 24, 8, 8)];
        let mut collider = hitbox(0.0, 24.0);
        let (push, pushed) = collider.expel(&cast(&crated));
        assert_eq!(push, (-1.0, 0.0));
        assert!(pushed.right() && !pushed.left());
    }

    #[test]
    fn an_entity_the_push_could_not_free_is_still_free_to_move() {
        // Two solids with a two-pixel gap between them, and an eight-pixel entity across both:
        // shoved out of the left one and straight into the right one, pass after pass, it ends
        // the push inside something whatever the passes do. What it is already inside cannot also
        // be a wall to it, or it would be wedged there for ever — so it reports, and the entity
        // walks out of the trap.
        let jaws = [wall(0, 0, 12, 24), wall(14, 0, 12, 24)];
        let mut collider = hitbox(8.0, 8.0);
        let (push, pushed) = collider.expel(&cast(&jaws));
        assert_ne!(push, (0.0, 0.0), "there was room to fit after all");
        assert!(pushed.left() && pushed.right());

        // The passes end with it clear of the left jaw and six pixels into the right one, so the
        // right is the exempt one and the way out is rightwards.
        let velocity = Velocity::new(2.0, 0.0);
        let (moved, contacts) = collider.resolve(velocity, air, &cast(&jaws));
        assert_eq!(moved, velocity, "it was wedged in for good");
        assert!(contacts.touches(WALL));

        // And the jaw it is no longer touching is an ordinary wall again: being freed of one is
        // not being freed of walls.
        let (moved, contacts) = collider.resolve(Velocity::new(-2.0, 0.0), air, &cast(&jaws));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.left());
    }

    #[test]
    fn a_shove_into_something_it_only_shared_an_edge_with_is_answered_for() {
        // Seven pixels of daylight between two solids and an eight-pixel entity in it, overlapping
        // the right-hand one by a pixel. The first shove pushes the entity left — and left is
        // where the other solid is, one that only *shared an edge* with the box when the pass
        // began and so was on no list taken there. Asked again from where the box now is, the next
        // pass finds it and pushes back.
        let adjacent = [wall(7, 0, 8, 8), wall(-8, 0, 8, 8)];
        let mut collider = hitbox(0.0, 0.0);
        let (push, pushed) = collider.expel(&cast(&adjacent));

        // Eight pixels of entity and seven of gap: nowhere in this scene is out, so the passes
        // trade the box back and forth a pixel at a time and the cap ends them where it stands —
        // which is where it started, a pixel inside the right-hand solid and clear of the left. So
        // the two pushes cancel, and both are reported.
        assert_eq!(push, (0.0, 0.0), "there was room to fit after all");
        assert!(pushed.left() && pushed.right());
        assert!(pushed.touches(WALL));

        // Which is the whole point of looking again: the solid the box is *not* inside is an
        // ordinary wall to it, so it cannot walk off through the one the first shove put it in.
        let (moved, contacts) = collider.resolve(Velocity::new(-2.0, 0.0), air, &cast(&adjacent));
        assert_eq!(
            moved,
            Velocity::default(),
            "it walked out through the left-hand solid"
        );
        assert!(contacts.left());

        // And the one it *is* inside cannot pin it there: out to the right is still out.
        let velocity = Velocity::new(2.0, 0.0);
        let (moved, _) = collider.resolve(velocity, air, &cast(&adjacent));
        assert_eq!(velocity, moved, "it was wedged in for good");
    }

    #[test]
    fn the_shoving_ends_even_where_no_amount_of_it_would_free_the_box() {
        // The scene with no answer in it, on both axes at once: four solids around an eight-pixel
        // box with seven pixels of gap each way, so every shove out of one puts the box a pixel
        // inside the next. Each pass moves it, so each pass earns another — and the cap is what
        // ends them. Reaching this assertion at all is the test: an uncapped loop never would.
        let cage = [
            wall(-8, -8, 8, 24),
            wall(7, -8, 8, 24),
            wall(-8, -8, 24, 8),
            wall(-8, 7, 24, 8),
        ];
        let mut collider = hitbox(0.0, 0.0);
        let (push, pushed) = collider.expel(&cast(&cage));
        assert!(
            push.0.abs() <= 1.0 && push.1.abs() <= 1.0,
            "the shoving ran on: {push:?}"
        );
        assert_eq!(
            pushed.sides(),
            Contact::Left | Contact::Right | Contact::Above | Contact::Below
        );

        // And the box the cap left where it stands is a box that still works: whatever it is
        // inside cannot block it, so it walks out of the corner it was traded into.
        let velocity = Velocity::new(2.0, 2.0);
        let (moved, contacts) = collider.resolve(velocity, air, &cast(&cage));
        assert_eq!(moved, velocity, "the cage kept it after all");
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn the_map_and_the_cast_stop_the_same_step() {
        // A wall of tiles to the left and another entity underneath: a diagonal into the corner is
        // stopped by one on each axis, in the one resolution, and both are reported.
        let tiles = map(&["#..", "#..", "#.."]);
        let body = Body::new(8.0, 0.0);
        let floor = [wall(0, 8, 24, 8)];
        let collider = Collider::new(
            &body,
            Bounds::of(&body, 8, 8),
            WALL.into(),
            BitFlags::all(),
            BitFlags::empty(),
            true,
        )
        .unwrap();
        let (moved, contacts) = collider.resolve(Velocity::new(-2.0, 1.0), tiles, &cast(&floor));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.left() && contacts.below());
        assert!(contacts.touches(WALL));
    }

    #[test]
    fn only_the_neighbours_over_the_box_reach_the_answer() {
        // The whole cast is walked, and geometry does the culling: one nowhere near the box shares
        // no pixel with it, and so is in neither half of the answer, whatever it carries.
        let elsewhere = [wall(100, 100, 8, 8), carrying(4, 0, WATER.into())];
        let (moved, contacts) =
            hitbox(0.0, 0.0).resolve(Velocity::new(1.0, 0.0), air, &cast(&elsewhere));
        assert_eq!(moved, Velocity::new(1.0, 0.0));
        assert!(contacts.touches(WATER) && !contacts.touches(WALL));
    }

    #[test]
    fn a_narrow_neighbour_crossed_between_the_endpoints_is_still_reported() {
        // A two-pixel strip of water at 9..10, and twelve pixels of movement over it. The box
        // covers neither end of the strip — 0..7 before the step, 12..19 after — so a pair of
        // endpoint snapshots passes clean over it and reports nothing at all. The flags are taken
        // over the ground the step covered instead, and the swim comes back.
        let stream = [(Bounds::new(9, 0, 2, 8), WATER.into(), BitFlags::empty())];
        let velocity = Velocity::new(12.0, 0.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, air, &cast(&stream));
        assert_eq!(moved, velocity, "the water stopped it");
        assert!(contacts.touches(WATER));

        // The same strip, met falling rather than walking: the vertical sweep is the column the
        // entity really went down.
        let strip = [(Bounds::new(0, 9, 8, 2), WATER.into(), BitFlags::empty())];
        let velocity = Velocity::new(0.0, 12.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, air, &cast(&strip));
        assert_eq!(moved, velocity);
        assert!(contacts.touches(WATER));
    }

    #[test]
    fn a_tile_column_stepped_clean_over_is_still_reported() {
        // The same case on the map, where the narrowest thing there is is a tile: a column of
        // water at 8..15, an eight-pixel box, and sixteen pixels of movement — enough that neither
        // the box it starts as (0..7) nor the box it ends as (16..23) is sampled anywhere inside
        // that column. The strip between them is.
        let stream = map(&[".~.."]);
        let velocity = Velocity::new(16.0, 0.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, stream, &alone());
        assert_eq!(moved, velocity, "the water stopped it");
        assert!(contacts.touches(WATER));
    }

    #[test]
    fn the_ground_a_step_began_on_is_reported_even_when_it_leaves() {
        // Standing in the pond and walking out of it in one update. Both endpoints the stopping is
        // resolved at are clear of the water — the entity is out by the time the update ends — and
        // the sweep begins where the step began, so the cart is still told the hero was in there
        // this frame. Nothing pushed it out on the way: water is no wall to anybody.
        let pond = [carrying(0, 0, WATER.into())];
        let velocity = Velocity::new(12.0, 0.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, air, &cast(&pond));
        assert_eq!(moved, velocity);
        assert!(contacts.touches(WATER));

        // And the same pond as a tile, walked off in one update.
        let pool = map(&["~..."]);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, pool, &alone());
        assert_eq!(moved, velocity);
        assert!(contacts.touches(WATER));
    }

    #[test]
    fn a_wall_thin_enough_to_be_stepped_over_is_reported_and_not_stopped_at() {
        // The asymmetry, pinned. A two-pixel paling at 9..10 and twelve pixels of movement past
        // it: stopping is resolved where the box was trying to go, and it was trying to go
        // somewhere the paling is not, so nothing stops it. Meeting is the whole step's, so it is
        // told exactly what it went through — and a cart that must not be gone through keeps a
        // terminal velocity, so that nothing moves further in an update than a wall is thick.
        let paling = [(Bounds::new(9, 0, 2, 8), WALL.into(), BitFlags::empty())];
        let velocity = Velocity::new(12.0, 0.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, air, &cast(&paling));
        assert_eq!(moved, velocity);
        assert_eq!(
            contacts.sides(),
            BitFlags::empty(),
            "it was stopped after all"
        );
        assert!(contacts.touches(WALL));

        // Two pixels of movement at the same paling, which is the ordinary speed the ordinary
        // answer comes back for: the box reaches it, and it is a wall.
        let velocity = Velocity::new(2.0, 0.0);
        let (moved, contacts) = hitbox(0.0, 0.0).resolve(velocity, air, &cast(&paling));
        assert_eq!(moved, Velocity::default());
        assert!(contacts.right());
    }

    /// What a cart flags its walls and floors with, on the map and on everything else.
    const WALL: SpriteFlag = SpriteFlag::Flag0;

    /// And its water, which stops nothing and is worth knowing about all the same.
    const WATER: SpriteFlag = SpriteFlag::Flag1;

    /// And the spikes, for the sensor that is only ever told about things.
    const SPIKES: SpriteFlag = SpriteFlag::Flag2;
}
