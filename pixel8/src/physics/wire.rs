//! The cast on the wire: how a step and a draw cross the ABI, written down once for both sides of
//! it.
//!
//! A [`World`](super::World) keeps its whole cast in these records — one fixed-size [`Record`] a
//! seat, in the world itself — and [`step`](super::World::step) hands that very array to the
//! console in one `step_cast` import. The console runs the engine this module's neighbours in
//! `world.rs` and `collider.rs` are, natively, over its own map and sprite sheet, and answers into
//! the same bytes: where each body ended up, what survived of its velocity, and what it met.
//! Nothing is marshalled in either direction — the cart's state *is* the buffer — and the
//! collisions themselves never spend a drop of cart fuel.
//!
//! The same bytes carry what a draw needs too — the cell a member wears, which way round, how
//! many cells its block spans, and whether it is shown at all — and a `draw_cast` import hands
//! the very same array across the wire again, unaltered, for the console to walk with [`looks`]
//! and draw natively in one call: the drawing twin of the step above, just as free of cart fuel.
//!
//! Both halves of the crossing live in this one file so they cannot drift: the SDK fills and reads
//! [`Record`]s in cart memory, and the console — which depends on this very crate — decodes them
//! with [`Record::read`], steps a cast of [`Recast`]s, and writes the answers back with
//! [`Record::write`]. The layout is `#[repr(C)]`, little-endian like wasm itself, and pinned by
//! the tests at the bottom.
//!
//! Nothing here is a cart's business: the whole module is hidden, and the one thing a cart calls
//! is still [`World::step`](super::World::step).

use super::{Bounds, Contacts, Kinetic, Velocity};
use crate::{BitFlags, Body, SpriteFlag, SpriteId};

/// How many cast members fit over the wire in one step: the ceiling on a
/// [`World`](super::World)'s `N`.
///
/// Sixty-four records is under 3 KiB, and sixty-four moving, colliding things is well past what
/// fits on a 128x128 screen. A world with more seats than this is refused at compile time, in
/// [`World::new`](super::World::new)'s own `const` check.
pub const CAP: usize = 64;

/// The record's `meta` bit for a [prop](Kinetic::prop): met, never moved. Read by the step, never
/// the draw.
pub const PROP: u8 = 1;

/// The record's `meta` bit for an entity that named [confines](Kinetic::confines). Read by the
/// step, never the draw.
pub const CONFINED: u8 = 1 << 1;

/// The record's `meta` bit for a member drawn mirrored left to right. Read by the draw, never the
/// step.
pub const FLIP_X: u8 = 1 << 2;

/// The record's `meta` bit for a member drawn mirrored top to bottom. Read by the draw, never the
/// step.
pub const FLIP_Y: u8 = 1 << 3;

/// The record's `meta` bit for a member met like anybody else, but never drawn. Read by the draw,
/// never the step.
pub const HIDDEN: u8 = 1 << 4;

/// The `sprite` field's value for an entity that wears nothing.
pub const UNWORN: u16 = u16::MAX;

/// One cast member on the wire — and, since the world keeps its cast in these, one seat of a
/// [`World`](super::World): what the engine needs of a member going in, and what the step decided
/// coming back, in the same forty-four bytes.
///
/// Going in, everything is what the cart said as it [enlisted](super::World::enlist) the member —
/// with `solid` already settled between the member's own rule and the world's, so the engine never
/// has to ask whose word it was. Coming back, `x`/`y`/`rx`/`ry` are the body's whole state after
/// the step, `dx`/`dy` the velocity that survived it, and `sides`/`touched` the [`Contacts`]; the
/// rest comes back untouched, which is why the world can keep its state here between steps.
///
/// The same bytes are also what a draw reads: the cell a member wears, how many cells its block
/// spans and which way round, and whether it is shown at all — [`look`](Record::look) is that
/// reading, worked out once and read the same way by both sides of the wire.
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
    /// [`PROP`] and [`CONFINED`], read by the step; [`FLIP_X`], [`FLIP_Y`] and [`HIDDEN`], read by
    /// the draw. The step never writes `meta` at all, so whatever a cart set here — the look bits
    /// included — survives every step untouched.
    pub meta: u8,
    /// Out: the sides of its [`Contacts`].
    pub sides: u8,
    /// Out: the flags of everything it met.
    pub touched: u8,
    /// The block of the sheet the member is drawn from, beyond the one cell it wears: the low
    /// four bits are the cells across less one, the high four bits the cells down less one; zero
    /// — what every record starts as — is the single cell. Read by the draw alone.
    pub span: u8,
}

/// The record size the layout above must come to — the wire stride, pinned by a test.
pub const RECORD: usize = 44;

/// An empty seat, as it travels: a prop covering no pixels, wearing nothing and listening for
/// nothing.
///
/// The world's seats are always all there — `N` of them, filled or not — and the crossing carries
/// everything up to the last one taken. A vacant one goes along as this, and the engine is inert
/// against it without knowing anything about vacancy: a prop is never moved and never given
/// contacts, no force reaches it, and a rectangle of no size overlaps nothing, so it is dropped
/// from the snapshot for wearing nothing and listening for nothing. Which is what lets a world
/// with gaps in its cast cross an *unchanged* wire — a cart built today still steps on a console
/// built before any of this.
pub const VACANT: Record = Record {
    meta: PROP,
    ..EMPTY
};

/// A record of nothing, for a buffer to start as.
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
    span: 0,
};

impl Record {
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
            span: bytes[43],
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

    /// What this record shows, or `None` for one that shows nothing: an empty seat, a member that
    /// wears nothing, and one that is hidden.
    pub fn look(&self) -> Option<Look> {
        if self.sprite == UNWORN || self.meta & HIDDEN != 0 {
            return None;
        }
        Some(Look {
            sprite: SpriteId(self.sprite as u8),
            x: self.rx,
            y: self.ry,
            width: (u16::from(self.span & 0x0f) + 1) * CELL,
            height: (u16::from(self.span >> 4) + 1) * CELL,
            flip_x: self.meta & FLIP_X != 0,
            flip_y: self.meta & FLIP_Y != 0,
        })
    }
}

/// Everything a cast shows, in the order it is drawn — front to back, so a later record lands on
/// top of an earlier one — narrowed, for a non-empty `layers`, to the records whose worn cell
/// carries one of those flags: the very filter [`Graphics::map`](crate::Graphics::map) puts its
/// tiles through, put to the cast. `carried` is what the sheet says a cell carries.
///
/// It is the one answer for both sides of the wire: the console walks it over the records a
/// `draw_cast` handed across, and the SDK's native build over the world's own seats.
pub fn looks<'a, F>(
    records: &'a [Record],
    layers: BitFlags<SpriteFlag>,
    carried: F,
) -> impl Iterator<Item = Look> + 'a
where
    F: Fn(SpriteId) -> BitFlags<SpriteFlag> + 'a,
{
    records
        .iter()
        .filter_map(Record::look)
        .filter(move |look| layers.is_empty() || carried(look.sprite).intersects(layers))
}

/// What one record looks like on screen — the block of the sheet it is drawn from, where it goes
/// and which way round — worked out once for both sides of the wire, and drawn by the very blit a
/// cart's own [`Graphics::sprite_ext`](crate::Graphics::sprite_ext) reaches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Look {
    /// The cell at the block's top left: the one the member wears.
    pub sprite: SpriteId,
    /// Where the block's top left is drawn: the member's coherent pixel.
    pub x: i16,
    pub y: i16,
    /// The block's size in pixels: whole cells, one unless the record spans more.
    pub width: u16,
    pub height: u16,
    /// Mirrored left to right / top to bottom.
    pub flip_x: bool,
    pub flip_y: bool,
}

/// A record recast as a cast member, for the engine to step: on the console's side of the wire,
/// and on the world's own native path, where the very same engine is run over the world's records
/// in place of the crossing.
///
/// The one modelling choice is the rectangle. A member's rectangle travels as the one it covered
/// at the top of the step, and the engine needs it to *follow the body* as it moves — which is
/// what a rectangle does: it keeps a fixed offset from the drawn pixel, zero for the usual one
/// over the sprite and whatever an inset hurtbox was given. So the offset is taken once, at
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

/// The side of one cell of the sheet, in pixels.
const CELL: u16 = 8;

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
        assert_eq!(offset_of!(Record, span), 43);
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
            meta: CONFINED | FLIP_Y,
            sides: 0,
            touched: 0,
            span: 0x21,
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
            (0b101, 0b10, CONFINED | FLIP_Y)
        );
        assert_eq!(across.span, 0x21);

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
    fn a_step_s_answers_leave_the_look_alone() {
        let record = Record {
            meta: CONFINED | FLIP_X,
            span: 0x12,
            sprite: 3,
            ..EMPTY
        };
        // SAFETY: `Record` is `#[repr(C)]` and exactly `RECORD` bytes with no padding — every byte
        // is a field, which `the_layout_is_the_one_written_down` pins — so its bytes are a valid
        // `[u8; RECORD]`. (`write` would not do: it writes the step's answers and nothing else.)
        let bytes: [u8; RECORD] = unsafe { core::mem::transmute(record) };

        // The step's own route: read what a raw client wrote, change what a step decides, and
        // write the answer back into those same bytes — exactly what `step_the_cast` does.
        let mut answered = Record::read(&bytes);
        (answered.x, answered.y) = (5.0, 6.0);
        (answered.rx, answered.ry) = (5, 6);
        (answered.sides, answered.touched) = (0b1, 0b10);
        let mut bytes = bytes;
        answered.write(&mut bytes);

        let back = Record::read(&bytes);
        assert_eq!(back.meta, CONFINED | FLIP_X, "write must not touch meta");
        assert_eq!(back.span, 0x12, "write must not touch span");
        assert_eq!((back.x, back.y), (5.0, 6.0), "the answered position landed");
        assert_eq!(
            (back.rx, back.ry),
            (5, 6),
            "the answered drawn pixel landed"
        );
        assert_eq!(
            (back.sides, back.touched),
            (0b1, 0b10),
            "the answered contacts landed"
        );
    }

    #[test]
    fn nothing_shown_has_no_look() {
        assert_eq!(EMPTY.look(), None, "an empty record wears nothing");
        assert_eq!(VACANT.look(), None, "a vacant seat wears nothing");
        let hidden = Record {
            sprite: 3,
            meta: HIDDEN,
            ..EMPTY
        };
        assert_eq!(hidden.look(), None, "a hidden record is never drawn");
    }

    #[test]
    fn a_plain_record_looks_like_one_cell_at_the_drawn_pixel() {
        // rx/ry differ from both the float position and the rectangle corner, so only the drawn
        // pixel driving the look proves which of the three the look actually reads.
        let record = Record {
            x: 12.75,
            y: 30.5,
            rx: -5,
            ry: 20,
            bx: 13,
            by: 25,
            sprite: 9,
            ..EMPTY
        };
        assert_eq!(
            record.look(),
            Some(Look {
                sprite: SpriteId(9),
                x: -5,
                y: 20,
                width: 8,
                height: 8,
                flip_x: false,
                flip_y: false,
            })
        );
    }

    #[test]
    fn span_and_flip_bits_size_and_mirror_the_look() {
        let base = Record { sprite: 5, ..EMPTY };

        let two_rows = Record { span: 0x10, ..base }.look().unwrap();
        assert_eq!(
            (two_rows.width, two_rows.height),
            (8, 16),
            "high nibble is cells down"
        );

        let two_cols = Record { span: 0x01, ..base }.look().unwrap();
        assert_eq!(
            (two_cols.width, two_cols.height),
            (16, 8),
            "low nibble is cells across"
        );

        let whole_sheet = Record { span: 0xff, ..base }.look().unwrap();
        assert_eq!(
            (whole_sheet.width, whole_sheet.height),
            (128, 128),
            "0xff spans the sheet"
        );

        let flipped = Record {
            meta: FLIP_X | FLIP_Y,
            ..base
        }
        .look()
        .unwrap();
        assert!(flipped.flip_x && flipped.flip_y);

        let still_plain = Record {
            meta: PROP | CONFINED,
            ..base
        }
        .look()
        .unwrap();
        assert!(
            !still_plain.flip_x && !still_plain.flip_y,
            "PROP/CONFINED are not look bits"
        );
    }

    #[test]
    fn looks_keeps_order_and_skips_what_shows_nothing() {
        let a = Record { sprite: 1, ..EMPTY };
        let hidden_b = Record {
            sprite: 2,
            meta: HIDDEN,
            ..EMPTY
        };
        let c = Record { sprite: 3, ..EMPTY };
        let records = [a, VACANT, hidden_b, c];

        let shown: Vec<SpriteId> = looks(&records, BitFlags::empty(), |_| BitFlags::empty())
            .map(|look| look.sprite)
            .collect();
        assert_eq!(shown, [SpriteId(1), SpriteId(3)]);
    }

    #[test]
    fn layers_filter_by_what_the_worn_cell_carries() {
        let plain = Record { sprite: 1, ..EMPTY }; // carries nothing
        let flagged = Record { sprite: 2, ..EMPTY }; // carries Flag0
        let records = [plain, flagged];
        let carried = |sprite: SpriteId| {
            if sprite == SpriteId(2) {
                BitFlags::from(SpriteFlag::Flag0)
            } else {
                BitFlags::empty()
            }
        };

        let all: Vec<_> = looks(&records, BitFlags::empty(), carried)
            .map(|look| look.sprite)
            .collect();
        assert_eq!(
            all,
            [SpriteId(1), SpriteId(2)],
            "empty layers draws everything"
        );

        let picked: Vec<_> = looks(&records, BitFlags::from(SpriteFlag::Flag0), carried)
            .map(|look| look.sprite)
            .collect();
        assert_eq!(
            picked,
            [SpriteId(2)],
            "only the cell carrying the flag is picked"
        );

        let none: Vec<_> = looks(&records, BitFlags::from(SpriteFlag::Flag1), carried)
            .map(|look| look.sprite)
            .collect();
        assert!(
            none.is_empty(),
            "a cell carrying no flags is never picked by a non-empty layer"
        );
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

    #[test]
    fn the_step_reads_nothing_of_the_look() {
        let record = Record {
            meta: FLIP_X | FLIP_Y | HIDDEN,
            span: 0xff,
            ..EMPTY
        };
        let recast = Recast::of(&record);
        assert!(!recast.prop(), "FLIP_X/FLIP_Y/HIDDEN are not PROP");
        assert_eq!(
            recast.confines(),
            None,
            "FLIP_X/FLIP_Y/HIDDEN are not CONFINED"
        );
    }
}
