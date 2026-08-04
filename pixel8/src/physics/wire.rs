//! The cast on the wire: how a step crosses the ABI, written down once for both sides of it.
//!
//! [`World::step`](super::World::step) hands the whole cast to the console in one `step_cast`
//! import — everything the engine needs to know about an entity in one fixed-size [`Record`], a
//! cast of them in one buffer — and the console runs the very engine this module's neighbours in
//! `world.rs` and `collider.rs` are, natively, over its own map and sprite sheet. What comes back
//! in the same buffer is everything the step decided: where each body ended up, what survived of
//! its velocity, and what it met. The cart pays for the writing down and the reading back;
//! the collisions themselves never spend a drop of cart fuel.
//!
//! Both halves of the crossing live in this one file so they cannot drift: the SDK fills and
//! reads [`Record`]s in cart memory, and the console — which depends on this very crate — decodes
//! them with [`Record::read`], steps a cast of [`Recast`]s, and writes the answers back with
//! [`Record::write`]. The layout is `#[repr(C)]`, little-endian like wasm itself, and pinned by
//! the tests at the bottom.
//!
//! Nothing here is a cart's business: the whole module is hidden, and the one thing a cart calls
//! is still [`World::step`](super::World::step).

use super::{Bounds, Contacts, Kinetic, Velocity};
use crate::{BitFlags, Body, SpriteFlag, SpriteId};

/// How many cast members fit over the wire in one step: the ceiling on a
/// [`World`](super::World)'s own `CAST` parameter.
///
/// Sixty-four records is under 3 KiB of cart memory, and sixty-four moving, colliding things is
/// well past what fits on a 128x128 screen. A cast past its world's ceiling is a cart bug, and
/// the step refuses it loudly — see the [ceiling](super::World#the-cast-ceiling).
pub const CAP: usize = 64;

/// The record's `meta` bit for a [prop](Kinetic::prop): met, never moved.
pub const PROP: u8 = 1;

/// The record's `meta` bit for an entity that named [confines](Kinetic::confines).
pub const CONFINED: u8 = 1 << 1;

/// The `sprite` field's value for an entity that wears nothing.
pub const UNWORN: u16 = u16::MAX;

/// One cast member on the wire: what the engine needs of it going in, and what the step decided
/// coming back, in the same forty-four bytes.
///
/// Going in, everything is a plain copy of what the entity [describes](Kinetic) — with `solid`
/// already settled between the entity's own rule and the world's, so the engine never has to ask
/// whose word it was. Coming back, `x`/`y`/`rx`/`ry` are the body's whole state after the step,
/// `dx`/`dy` the velocity that survived it, and `sides`/`touched` the [`Contacts`]; the rest
/// comes back untouched.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Record {
    /// The body's exact position — in, and out.
    pub x: f32,
    pub y: f32,
    /// The velocity — in, and what survived, out.
    pub dx: f32,
    pub dy: f32,
    /// The body's coherent drawn pixel — in, and out.
    pub rx: i16,
    pub ry: i16,
    /// The rectangle the entity covers, as [`Kinetic::bounds`] gave it.
    pub bx: i16,
    pub by: i16,
    pub bw: u16,
    pub bh: u16,
    /// The limits it may not leave — meaningful only under [`CONFINED`].
    pub cx: i16,
    pub cy: i16,
    pub cw: u16,
    pub ch: u16,
    /// The cell it wears, or [`UNWORN`].
    pub sprite: u16,
    /// What stops it — the entity's own rule or the world's, already settled.
    pub solid: u8,
    /// What it cares to be told about.
    pub heeds: u8,
    /// [`PROP`] and [`CONFINED`].
    pub meta: u8,
    /// Out: the sides of its [`Contacts`].
    pub sides: u8,
    /// Out: the flags of everything it met.
    pub touched: u8,
    /// Padding, so the size is spelled out rather than implied.
    pub pad: u8,
}

/// The record size the layout above must come to — the wire stride, pinned by a test.
pub const RECORD: usize = 44;

/// A record of nothing, for the buffer to start as.
pub const EMPTY: Record = Record {
    x: 0.0,
    y: 0.0,
    dx: 0.0,
    dy: 0.0,
    rx: 0,
    ry: 0,
    bx: 0,
    by: 0,
    bw: 0,
    bh: 0,
    cx: 0,
    cy: 0,
    cw: 0,
    ch: 0,
    sprite: UNWORN,
    solid: 0,
    heeds: 0,
    meta: 0,
    sides: 0,
    touched: 0,
    pad: 0,
};

impl Record {
    /// Everything the engine will need of `entity`, written down — with `solid` already settled
    /// between the entity's own rule and the world's.
    pub fn of(entity: &dyn Kinetic, world_solid: BitFlags<SpriteFlag>, velocity: Velocity) -> Self {
        let (x, y, rx, ry) = entity.body().wire();
        let bounds = entity.bounds();
        let (confined, limits) = match entity.confines() {
            Some(limits) => (CONFINED, limits),
            None => (0, Bounds::new(0, 0, 0, 0)),
        };

        Self {
            x,
            y,
            dx: velocity.dx,
            dy: velocity.dy,
            rx,
            ry,
            bx: bounds.x(),
            by: bounds.y(),
            bw: bounds.width(),
            bh: bounds.height(),
            cx: limits.x(),
            cy: limits.y(),
            cw: limits.width(),
            ch: limits.height(),
            sprite: entity.sprite().map_or(UNWORN, |sprite| sprite.0 as u16),
            solid: entity.solid().unwrap_or(world_solid).bits(),
            heeds: entity.heeds().bits(),
            meta: confined | if entity.prop() { PROP } else { 0 },
            sides: 0,
            touched: 0,
            pad: 0,
        }
    }

    /// The record read out of raw wire bytes — the console's side of the crossing.
    ///
    /// Field by field and little-endian, so the answer is the cart's layout whatever the host is,
    /// and nothing is assumed about the alignment of a pointer a cart handed over.
    pub fn read(bytes: &[u8; RECORD]) -> Self {
        #[inline]
        fn f32_at(bytes: &[u8], at: usize) -> f32 {
            f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        }
        #[inline]
        fn i16_at(bytes: &[u8], at: usize) -> i16 {
            i16::from_le_bytes([bytes[at], bytes[at + 1]])
        }

        Self {
            x: f32_at(bytes, 0),
            y: f32_at(bytes, 4),
            dx: f32_at(bytes, 8),
            dy: f32_at(bytes, 12),
            rx: i16_at(bytes, 16),
            ry: i16_at(bytes, 18),
            bx: i16_at(bytes, 20),
            by: i16_at(bytes, 22),
            bw: i16_at(bytes, 24) as u16,
            bh: i16_at(bytes, 26) as u16,
            cx: i16_at(bytes, 28),
            cy: i16_at(bytes, 30),
            cw: i16_at(bytes, 32) as u16,
            ch: i16_at(bytes, 34) as u16,
            sprite: i16_at(bytes, 36) as u16,
            solid: bytes[38],
            heeds: bytes[39],
            meta: bytes[40],
            sides: bytes[41],
            touched: bytes[42],
            pad: 0,
        }
    }

    /// The step's answers, written back into the wire bytes the record was read from.
    ///
    /// Only what the step decides goes back — body, velocity, contacts — so everything the cart
    /// wrote stays exactly as the cart wrote it.
    pub fn write(&self, bytes: &mut [u8; RECORD]) {
        bytes[0..4].copy_from_slice(&self.x.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.y.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.dx.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.dy.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.rx.to_le_bytes());
        bytes[18..20].copy_from_slice(&self.ry.to_le_bytes());
        bytes[41] = self.sides;
        bytes[42] = self.touched;
    }
}

/// A record recast as a cast member, on the console's side of the wire: the engine steps these
/// exactly as it steps a cart's own entities, because they are [`Kinetic`] like everything else.
///
/// The one modelling choice is the rectangle. An entity's [`bounds`](Kinetic::bounds) travel as
/// the rectangle they were at the top of the step, and the engine needs them to *follow the
/// body* as it moves — which is what they do in every cart, [`Bounds::of`] and inset hurtboxes
/// alike: a rectangle at a fixed offset from the drawn pixel. So the offset is taken once, at
/// decode, and the rectangle is wherever the body now draws plus that.
pub struct Recast {
    body: Body,
    velocity: Velocity,
    contacts: Contacts,
    /// The rectangle, as an offset from the drawn pixel and a size. The offset is two `i16`
    /// coordinates apart and so needs the wider type: a body drawn at one end of the coordinate
    /// space wearing a rectangle at the other is a strange entity, but it is a *safe* one, and it
    /// must not wrap into a different geometry here.
    off_x: i32,
    off_y: i32,
    width: u16,
    height: u16,
    sprite: Option<SpriteId>,
    solid: BitFlags<SpriteFlag>,
    heeds: BitFlags<SpriteFlag>,
    confines: Option<Bounds>,
    prop: bool,
}

impl Recast {
    /// The record, recast for the engine.
    ///
    /// Flag bits are taken as they came: the SDK on the other side wrote them out of real
    /// [`BitFlags`], so an unknown bit here is an ABI mismatch and the mismatch message is the
    /// honest answer.
    pub fn of(record: &Record) -> Self {
        let mismatch = "step_cast record carried an unknown flag bit (pixel8 host/SDK mismatch)";

        Self {
            body: Body::from_wire((record.x, record.y, record.rx, record.ry)),
            velocity: Velocity::new(record.dx, record.dy),
            contacts: Contacts::empty(),
            off_x: record.bx as i32 - record.rx as i32,
            off_y: record.by as i32 - record.ry as i32,
            width: record.bw,
            height: record.bh,
            sprite: match record.sprite {
                UNWORN => None,
                id => Some(SpriteId(id as u8)),
            },
            solid: BitFlags::from_bits(record.solid).expect(mismatch),
            heeds: BitFlags::from_bits(record.heeds).expect(mismatch),
            confines: (record.meta & CONFINED != 0)
                .then(|| Bounds::new(record.cx, record.cy, record.cw, record.ch)),
            prop: record.meta & PROP != 0,
        }
    }

    /// What the step decided, written into the record this was recast from.
    pub fn report(&self, record: &mut Record) {
        (record.x, record.y, record.rx, record.ry) = self.body.wire();
        record.dx = self.velocity.dx;
        record.dy = self.velocity.dy;
        (record.sides, record.touched) = self.contacts.wire();
    }
}

impl Kinetic for Recast {
    fn body(&self) -> &Body {
        &self.body
    }

    fn body_mut(&mut self) -> &mut Body {
        &mut self.body
    }

    fn velocity_mut(&mut self) -> &mut Velocity {
        &mut self.velocity
    }

    fn contacts(&self) -> &Contacts {
        &self.contacts
    }

    fn contacts_mut(&mut self) -> &mut Contacts {
        &mut self.contacts
    }

    fn bounds(&self) -> Bounds {
        let (x, y) = self.body.draw_pos();

        // Saturating at the ends of the space, exactly where the rectangle's own edges saturate:
        // a corner past them was never a pixel anything could stand on.
        Bounds::new(
            (x as i32 + self.off_x).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            (y as i32 + self.off_y).clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            self.width,
            self.height,
        )
    }

    fn solid(&self) -> Option<BitFlags<SpriteFlag>> {
        // Already settled between the entity's rule and the world's before the crossing, so it is
        // its own rule here whoever's it was.
        Some(self.solid)
    }

    fn heeds(&self) -> BitFlags<SpriteFlag> {
        self.heeds
    }

    fn sprite(&self) -> Option<SpriteId> {
        self.sprite
    }

    fn confines(&self) -> Option<Bounds> {
        self.confines
    }

    fn prop(&self) -> bool {
        self.prop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn the_layout_is_the_one_written_down() {
        // `read`/`write` address bytes by hand; the struct the SDK fills is `repr(C)`. This is
        // the seam between them, so every offset is pinned — a reordered field fails here, not in
        // a cart.
        assert_eq!(size_of::<Record>(), RECORD);
        assert_eq!(offset_of!(Record, x), 0);
        assert_eq!(offset_of!(Record, y), 4);
        assert_eq!(offset_of!(Record, dx), 8);
        assert_eq!(offset_of!(Record, dy), 12);
        assert_eq!(offset_of!(Record, rx), 16);
        assert_eq!(offset_of!(Record, ry), 18);
        assert_eq!(offset_of!(Record, bx), 20);
        assert_eq!(offset_of!(Record, by), 22);
        assert_eq!(offset_of!(Record, bw), 24);
        assert_eq!(offset_of!(Record, bh), 26);
        assert_eq!(offset_of!(Record, cx), 28);
        assert_eq!(offset_of!(Record, cy), 30);
        assert_eq!(offset_of!(Record, cw), 32);
        assert_eq!(offset_of!(Record, ch), 34);
        assert_eq!(offset_of!(Record, sprite), 36);
        assert_eq!(offset_of!(Record, solid), 38);
        assert_eq!(offset_of!(Record, heeds), 39);
        assert_eq!(offset_of!(Record, meta), 40);
        assert_eq!(offset_of!(Record, sides), 41);
        assert_eq!(offset_of!(Record, touched), 42);
    }

    #[test]
    fn a_record_crosses_the_wire_and_comes_back_itself() {
        let record = Record {
            x: 12.75,
            y: -3.5,
            dx: 1.25,
            dy: -0.5,
            rx: 12,
            ry: -4,
            bx: 13,
            by: -3,
            bw: 6,
            bh: 7,
            cx: -8,
            cy: 0,
            cw: 144,
            ch: 128,
            sprite: 9,
            solid: 0b0000_0101,
            heeds: 0b0000_0010,
            meta: CONFINED,
            sides: 0,
            touched: 0,
            pad: 0,
        };

        // Out through the struct's own bytes, in through the hand-addressed reader: the two
        // descriptions of the layout, agreeing on every field.
        let bytes: [u8; RECORD] = unsafe { core::mem::transmute(record) };
        let across = Record::read(&bytes);
        assert_eq!(across.x, record.x);
        assert_eq!(across.y, record.y);
        assert_eq!((across.dx, across.dy), (record.dx, record.dy));
        assert_eq!((across.rx, across.ry), (record.rx, record.ry));
        assert_eq!((across.bx, across.by, across.bw, across.bh), (13, -3, 6, 7));
        assert_eq!(
            (across.cx, across.cy, across.cw, across.ch),
            (-8, 0, 144, 128)
        );
        assert_eq!(across.sprite, 9);
        assert_eq!(
            (across.solid, across.heeds, across.meta),
            (0b101, 0b10, CONFINED)
        );

        // And the answers written back land exactly where the reader looks for them.
        let mut bytes = [0u8; RECORD];
        let mut answered = across;
        answered.sides = 0b1010;
        answered.touched = 0b1;
        answered.write(&mut bytes);
        let back = Record::read(&bytes);
        assert_eq!((back.x, back.y), (record.x, record.y));
        assert_eq!((back.sides, back.touched), (0b1010, 0b1));
    }

    #[test]
    fn a_recast_at_the_ends_of_the_coordinate_space_does_not_wrap() {
        // A body drawn at one extreme wearing a rectangle at the other: the offset between them
        // is wider than an `i16`, and a wrap here would be a different collision geometry — or a
        // panic in a checked build — from input every field of which is safe on its own.
        let mut record = EMPTY;
        (record.x, record.y, record.rx, record.ry) = (-32768.0, 0.0, i16::MIN, 0);
        (record.bx, record.by, record.bw, record.bh) = (i16::MAX - 8, 0, 8, 8);
        let recast = Recast::of(&record);
        assert_eq!(recast.bounds(), Bounds::new(i16::MAX - 8, 0, 8, 8));
    }

    #[test]
    fn a_recast_stands_where_its_record_says_and_answers_as_it_answered() {
        let mut record = EMPTY;
        (record.x, record.y, record.rx, record.ry) = (20.5, 30.25, 20, 30);
        (record.bx, record.by, record.bw, record.bh) = (22, 30, 4, 8);
        record.sprite = 7;
        record.solid = 0b1;
        record.heeds = 0b11;
        let mut recast = Recast::of(&record);

        // The rectangle is the record's, and it follows the body: two pixels in from the drawn
        // corner, wherever that now is.
        assert_eq!(recast.bounds(), Bounds::new(22, 30, 4, 8));
        recast.body_mut().move_by(2.0, 0.0);
        assert_eq!(recast.bounds(), Bounds::new(24, 30, 4, 8));

        assert_eq!(recast.solid(), Some(BitFlags::from_bits(0b1).unwrap()));
        assert!(!recast.prop());
        assert_eq!(recast.confines(), None);

        recast.report(&mut record);
        assert_eq!((record.x, record.y), (22.5, 30.25));
        assert_eq!((record.rx, record.ry), (22, 30));
    }
}
