//! The 128x128 indexed-color framebuffer and software drawing primitives.
//!
//! Everything Pixel8 puts on screen — running carts, the console, every
//! editor — is drawn through this one software rasterizer into a buffer of
//! palette indices. The GPU's only job is to scale the result up with
//! nearest-neighbor filtering.

use crate::{
    assets::{MapData, SpriteSheet, SPRITES_PER_ROW, SPRITE_COUNT, SPRITE_SIZE},
    font, palette,
};

/// Virtual screen width in pixels.
pub const WIDTH: i32 = 128;
/// Virtual screen height in pixels.
pub const HEIGHT: i32 = 128;

/// Identity color map: index `i` maps to color `i`.
const IDENTITY_PALETTE: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
/// Default transparency mask: only color 0 is transparent.
const DEFAULT_TRANSPARENT: u16 = 0x0001;

/// The virtual screen: one byte per pixel, each a palette index in `0..16`.
pub struct Framebuffer {
    pixels: Vec<u8>,
    camera_x: i32,
    camera_y: i32,
    clip: (i32, i32, i32, i32),
    draw_pal: [u8; 16],
    display_pal: [u8; 16],
    transparent: u16,
    fill_pattern: u16,
    fill_secondary: u8,
    fill_transparent: bool,
    pen_color: u8,
    cursor_x: i32,
    cursor_y: i32,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; (WIDTH * HEIGHT) as usize],
            camera_x: 0,
            camera_y: 0,
            clip: (0, 0, WIDTH, HEIGHT),
            draw_pal: IDENTITY_PALETTE,
            display_pal: IDENTITY_PALETTE,
            transparent: DEFAULT_TRANSPARENT,
            fill_pattern: 0,
            fill_secondary: 0,
            fill_transparent: false,
            pen_color: 6,
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    /// Raw palette-index pixels, row-major, `WIDTH * HEIGHT` long.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The display palette: at present time, stored index `i` is shown as color
    /// `display_palette()[i]`. Presenters apply this when expanding the indexed
    /// framebuffer to RGB, exactly as `write_rgba` does for GPU upload.
    pub fn display_palette(&self) -> &[u8; 16] {
        &self.display_pal
    }

    /// Expand the indexed framebuffer into an RGBA8 buffer for GPU upload.
    pub fn write_rgba(&self, out: &mut [u8]) {
        // Fold the display palette into a 16-entry RGBA lookup table once, so
        // the per-pixel loop is a plain table read plus a fixed-size copy. This
        // drops the per-pixel `display_pal` + `rgba` work and the range-index
        // bounds check, and autovectorizes cleanly.
        let mut lut = [[0u8; 4]; 16];
        for (i, entry) in lut.iter_mut().enumerate() {
            *entry = palette::rgba(self.display_pal[i]);
        }
        let (chunks, _) = out.as_chunks_mut::<4>();
        for (chunk, &c) in chunks.iter_mut().zip(self.pixels.iter()) {
            *chunk = lut[(c & 0x0f) as usize];
        }
    }

    /// Set the camera offset applied to all subsequent draw operations.
    pub fn camera(&mut self, x: i32, y: i32) {
        self.camera_x = x;
        self.camera_y = y;
    }

    /// Restrict drawing to a screen-space rectangle.
    pub fn clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let x0 = x.clamp(0, WIDTH);
        let y0 = y.clamp(0, HEIGHT);
        let x1 = (x + w).clamp(0, WIDTH);
        let y1 = (y + h).clamp(0, HEIGHT);
        self.clip = (x0, y0, x1, y1);
    }

    /// Remove the clip rectangle.
    pub fn clip_reset(&mut self) {
        self.clip = (0, 0, WIDTH, HEIGHT);
    }

    /// Reset camera and clip to defaults (used between host UI and cart frames).
    pub fn reset_state(&mut self) {
        self.camera_x = 0;
        self.camera_y = 0;
        self.clip_reset();
        self.draw_pal = IDENTITY_PALETTE;
        self.display_pal = IDENTITY_PALETTE;
        self.transparent = DEFAULT_TRANSPARENT;
        self.fill_pattern = 0;
        self.fill_secondary = 0;
        self.fill_transparent = false;
        self.pen_color = 6;
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Make a palette color transparent (or opaque) for sprite draws.
    pub fn set_transparent_color(&mut self, color: u8, transparent: bool) {
        let bit = 1u16 << (color & 0x0f);
        if transparent {
            self.transparent |= bit;
        } else {
            self.transparent &= !bit;
        }
    }

    /// Reset transparency to the default (only color 0 transparent).
    pub fn reset_transparency(&mut self) {
        self.transparent = DEFAULT_TRANSPARENT;
    }

    /// Remap a draw-palette color: later draws of `from` are written as `to`.
    pub fn remap_color(&mut self, from: u8, to: u8) {
        self.draw_pal[(from & 0x0f) as usize] = to & 0x0f;
    }

    /// Remap a display-palette color: `from` is shown as `to` at upload time.
    pub fn remap_display_color(&mut self, from: u8, to: u8) {
        self.display_pal[(from & 0x0f) as usize] = to & 0x0f;
    }

    /// Reset both the draw and display palettes to identity.
    pub fn reset_palette(&mut self) {
        self.draw_pal = IDENTITY_PALETTE;
        self.display_pal = IDENTITY_PALETTE;
    }

    /// Configure the fill pattern for the filled shape primitives. `pattern` is
    /// a 4x4 bitmask (bit 15 = top-left). Pattern-0 pixels take the shape's
    /// color; pattern-1 pixels take `secondary`, or are skipped when
    /// `transparent`. A `pattern` of 0 fills solid.
    pub fn set_fill_pattern(&mut self, pattern: u16, secondary: u8, transparent: bool) {
        self.fill_pattern = pattern;
        self.fill_secondary = secondary & 0x0f;
        self.fill_transparent = transparent;
    }

    /// The color a fill should write at framebuffer pixel `(x, y)`, or `None`
    /// when the transparent pattern skips it. `x`/`y` are post-camera.
    fn fill_color_at(&self, x: i32, y: i32, primary: u8) -> Option<u8> {
        if self.fill_pattern == 0 {
            return Some(primary);
        }
        let idx = ((y & 3) * 4 + (x & 3)) as u16;
        if (self.fill_pattern >> (15 - idx)) & 1 == 0 {
            Some(primary)
        } else if self.fill_transparent {
            None
        } else {
            Some(self.fill_secondary)
        }
    }

    /// Like `raw_pset` but honoring the fill pattern. `x`/`y` are post-camera.
    fn raw_pset_fill(&mut self, x: i32, y: i32, primary: u8) {
        if let Some(c) = self.fill_color_at(x, y, primary) {
            self.raw_pset(x, y, c);
        }
    }

    /// Fill a solid horizontal run on row `y` from `x0..=x1` (inclusive,
    /// POST-camera), clipped to the clip rect. Applies the draw palette. Used by
    /// the solid (non-patterned) fill path: clipping the span once and writing
    /// it as one memset is far cheaper than clipping every pixel.
    fn fill_span(&mut self, x0: i32, x1: i32, y: i32, color: u8) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        if y < cy0 || y >= cy1 {
            return;
        }
        let xa = x0.max(cx0);
        let xb = x1.min(cx1 - 1);
        if xa > xb {
            return;
        }
        let c = self.draw_pal[(color & 0x0f) as usize] & 0x0f;
        let start = (y * WIDTH + xa) as usize;
        let end = (y * WIDTH + xb + 1) as usize;
        self.pixels[start..end].fill(c);
    }

    /// `raw_pset` for a point whose post-camera coordinates need not fit an i32.
    /// Line and circle walks are driven by cart-supplied geometry, which can put a
    /// plot billions of pixels off screen; anything the clip rect rejects is dropped
    /// before the narrowing cast.
    fn plot_far(&mut self, x: i64, y: i64, color: u8) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        if x >= i64::from(cx0) && x < i64::from(cx1) && y >= i64::from(cy0) && y < i64::from(cy1) {
            self.raw_pset(x as i32, y as i32, color);
        }
    }

    /// `fill_span` for a run whose ends need not fit an i32, the span counterpart of
    /// `plot_far`. Returns the clipped run for the patterned path to walk.
    fn clipped_span_far(&self, x0: i64, x1: i64, y: i64) -> Option<(i32, i32, i32)> {
        let (cx0, cy0, cx1, cy1) = self.clip;
        if y < i64::from(cy0) || y >= i64::from(cy1) {
            return None;
        }
        let xa = x0.max(i64::from(cx0));
        let xb = x1.min(i64::from(cx1) - 1);
        if xa > xb {
            return None;
        }
        Some((xa as i32, xb as i32, y as i32))
    }

    /// Intersect an inclusive, PRE-camera row span with the clip rect. An empty
    /// result comes back as `lo > hi`, which every `lo..=hi` loop treats as empty.
    /// Shape primitives take their extents from the cart, so trimming the sweep up
    /// front is what keeps one host call from walking billions of no-op rows.
    fn clipped_rows(&self, ya: i32, yb: i32) -> (i32, i32) {
        let (_, cy0, _, cy1) = self.clip;
        (
            ya.max(cy0.saturating_add(self.camera_y)),
            yb.min((cy1 - 1).saturating_add(self.camera_y)),
        )
    }

    /// Intersect an inclusive, PRE-camera column span with the clip rect, the
    /// column counterpart of `clipped_rows`.
    fn clipped_cols(&self, xa: i32, xb: i32) -> (i32, i32) {
        let (cx0, _, cx1, _) = self.clip;
        (
            xa.max(cx0.saturating_add(self.camera_x)),
            xb.min((cx1 - 1).saturating_add(self.camera_x)),
        )
    }

    /// Fill the whole screen with a color. Does not touch camera/clip.
    pub fn cls(&mut self, color: u8) {
        self.pixels.fill(color & 0x0f);
    }

    #[inline]
    fn raw_pset(&mut self, x: i32, y: i32, color: u8) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        if x >= cx0 && x < cx1 && y >= cy0 && y < cy1 {
            let c = self.draw_pal[(color & 0x0f) as usize] & 0x0f;
            self.pixels[(y * WIDTH + x) as usize] = c;
        }
    }

    /// Set one pixel (camera-relative, like all draw ops).
    pub fn pset(&mut self, x: i32, y: i32, color: u8) {
        self.raw_pset(x - self.camera_x, y - self.camera_y, color);
    }

    /// Read one pixel in screen space. Out-of-bounds reads return 0.
    pub fn pget(&self, x: i32, y: i32) -> u8 {
        if (0..WIDTH).contains(&x) && (0..HEIGHT).contains(&y) {
            self.pixels[(y * WIDTH + x) as usize]
        } else {
            0
        }
    }

    /// Bresenham line between two points, inclusive.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        // Both endpoints come from the cart, so their difference need not fit an i32.
        let (ax, ay) = (
            i64::from(x0) - i64::from(self.camera_x),
            i64::from(y0) - i64::from(self.camera_y),
        );
        let (bx, by) = (
            i64::from(x1) - i64::from(self.camera_x),
            i64::from(y1) - i64::from(self.camera_y),
        );
        let (dx, dy) = ((bx - ax).abs(), (by - ay).abs());
        let (sx, sy) = (if ax < bx { 1 } else { -1 }, if ay < by { 1 } else { -1 });
        // Bresenham advances the major axis once per step and the minor axis
        // `(2 * minor * k + major) / (2 * major)` times over the first `k` of them —
        // the closed form of the error recurrence, so this plots exactly the pixels
        // stepping the recurrence would. Because the major axis moves one pixel per
        // step, solving it against the clip rect turns a walk the cart sized into at
        // most a screen's width of steps, which is what stops one host call from
        // spending billions of iterations off screen.
        let steps = dx.max(dy);
        let x_major = dx >= dy;
        let (cx0, cy0, cx1, cy1) = self.clip;
        let (start, step, lo, hi) = if x_major {
            (ax, sx, i64::from(cx0), i64::from(cx1) - 1)
        } else {
            (ay, sy, i64::from(cy0), i64::from(cy1) - 1)
        };
        let (k_lo, k_hi) = if step > 0 {
            (lo - start, hi - start)
        } else {
            (start - hi, start - lo)
        };
        let (major, minor) = if x_major { (dx, dy) } else { (dy, dx) };
        for k in k_lo.max(0)..=k_hi.min(steps) {
            // `2 * minor * k` needs the wider type: both factors can approach 2^33.
            let m = if major == 0 {
                0
            } else {
                ((2 * i128::from(minor) * i128::from(k) + i128::from(major))
                    / (2 * i128::from(major))) as i64
            };
            let (px, py) = if x_major {
                (ax + sx * k, ay + sy * m)
            } else {
                (ax + sx * m, ay + sy * k)
            };
            self.plot_far(px, py, color);
        }
    }

    /// Rectangle outline with inclusive corners, like PICO-8's `rect`.
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        self.line(xa, ya, xb, ya, color);
        self.line(xa, yb, xb, yb, color);
        self.line(xa, ya, xa, yb, color);
        self.line(xb, ya, xb, yb, color);
    }

    /// Filled rectangle with inclusive corners.
    pub fn rectfill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        // The corners come from the cart, so trim the sweep to the rows (and, for the
        // patterned path, the columns) that the clip rect can accept. Every skipped
        // iteration would have been a no-op, and the fill pattern keys off the pixel
        // coordinate rather than the loop index, so the result is untouched.
        let (ra, rb) = self.clipped_rows(ya, yb);
        if self.fill_pattern == 0 {
            // Solid fill: each row is one clipped memset (xa/xb are pre-camera).
            for y in ra..=rb {
                self.fill_span(
                    xa - self.camera_x,
                    xb - self.camera_x,
                    y - self.camera_y,
                    color,
                );
            }
        } else {
            let (ca, cb) = self.clipped_cols(xa, xb);
            for y in ra..=rb {
                for x in ca..=cb {
                    self.raw_pset_fill(x - self.camera_x, y - self.camera_y, color);
                }
            }
        }
    }

    /// Circle outline (midpoint algorithm).
    pub fn circ(&mut self, cx: i32, cy: i32, r: i32, color: u8) {
        self.circle_impl(cx, cy, r.max(0), color, false);
    }

    /// Filled circle.
    pub fn circfill(&mut self, cx: i32, cy: i32, r: i32, color: u8) {
        self.circle_impl(cx, cy, r.max(0), color, true);
    }

    fn circle_impl(&mut self, cx: i32, cy: i32, r: i32, color: u8, fill: bool) {
        let (cx, cy) = (
            i64::from(cx) - i64::from(self.camera_x),
            i64::from(cy) - i64::from(self.camera_y),
        );
        let r = i64::from(r);
        let (cx0, cy0, cx1, cy1) = self.clip;
        let (cx0, cy0, cx1, cy1) = (
            i64::from(cx0),
            i64::from(cy0),
            i64::from(cx1) - 1,
            i64::from(cy1) - 1,
        );
        // The radius is the cart's to choose, so drop a circle that cannot reach the
        // clip rect before walking an arc proportional to it.
        if cx + r < cx0 || cx - r > cx1 || cy + r < cy0 || cy - r > cy1 {
            return;
        }

        // Even a circle that does reach the clip rect walks `y` from 0 to about
        // `r / sqrt(2)`, which for a cart-sized radius is billions of steps for the
        // single fuel unit the host call costs. Almost all of them plot nothing: the
        // walk touches the screen only where `cy ± y` (the shallow octants) or
        // `cy ± x` (the steep ones, swapped through `cx ± y`) can land inside the
        // clip rect, which is a handful of short runs of `y`. Collect those runs and
        // restart the walk on each, seeded by `arc_x` — the closed form of the same
        // recurrence — so the pixels plotted are exactly the ones a full walk would
        // have plotted.
        // Shallow octants: rows `cy ± y`, so `y` must land within the clip rows.
        let shallow = [(cy0 - cy, cy1 - cy), (cy - cy1, cy - cy0)];
        let steep = if fill {
            // Filled, the steep octants become spans on rows `cy ± x`, so it is `x`
            // that must land within the clip rows; `arc_x` is non-increasing, so each
            // range of rows maps back to one range of `y`.
            [
                arc_rows_to_y(r, cy0 - cy, cy1 - cy),
                arc_rows_to_y(r, cy - cy1, cy - cy0),
            ]
        } else {
            // As an outline the steep octants plot at columns `cx ± y`, so `y` itself
            // must land within the clip columns.
            [(cx0 - cx, cx1 - cx), (cx - cx1, cx - cx0)]
        };
        for (run_lo, run_hi) in merge_runs(shallow, steep, r) {
            let mut y = run_lo;
            let mut x = arc_x(r, y);
            // The walk's error term is a function of its state: keeping the identity
            // here lets a run start anywhere along the arc.
            let mut err = x * x - r * r + y * y + 2 * y - x + 1;
            while x >= y && y <= run_hi {
                if fill && self.fill_pattern == 0 {
                    // Solid fill: each scanline of the disc is one clipped memset.
                    self.circle_span(cx - x, cx + x, cy + y, color);
                    self.circle_span(cx - x, cx + x, cy - y, color);
                    self.circle_span(cx - y, cx + y, cy + x, color);
                    self.circle_span(cx - y, cx + y, cy - x, color);
                } else if fill {
                    self.circle_run(cx - x, cx + x, cy + y, color);
                    self.circle_run(cx - x, cx + x, cy - y, color);
                    self.circle_run(cx - y, cx + y, cy + x, color);
                    self.circle_run(cx - y, cx + y, cy - x, color);
                } else {
                    for (px, py) in [
                        (cx + x, cy + y),
                        (cx - x, cy + y),
                        (cx + x, cy - y),
                        (cx - x, cy - y),
                        (cx + y, cy + x),
                        (cx - y, cy + x),
                        (cx + y, cy - x),
                        (cx - y, cy - x),
                    ] {
                        self.plot_far(px, py, color);
                    }
                }
                y += 1;
                if err < 0 {
                    err += 2 * y + 1;
                } else {
                    x -= 1;
                    err += 2 * (y - x) + 1;
                }
            }
        }
    }

    /// One solid scanline of a circle, in the wider coordinates the walk uses.
    fn circle_span(&mut self, x0: i64, x1: i64, y: i64, color: u8) {
        if let Some((xa, xb, y)) = self.clipped_span_far(x0, x1, y) {
            self.fill_span(xa, xb, y, color);
        }
    }

    /// One patterned scanline of a circle. The pattern keys off the pixel
    /// coordinate, so clipping the run up front leaves the result untouched.
    fn circle_run(&mut self, x0: i64, x1: i64, y: i64, color: u8) {
        let Some((xa, xb, y)) = self.clipped_span_far(x0, x1, y) else {
            return;
        };
        for x in xa..=xb {
            self.raw_pset_fill(x, y, color);
        }
    }

    /// Ellipse outline within the inclusive bounding box `(x0,y0)-(x1,y1)`.
    pub fn oval(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        self.oval_impl(x0, y0, x1, y1, color, false);
    }

    /// Filled ellipse within the inclusive bounding box `(x0,y0)-(x1,y1)`.
    pub fn ovalfill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        self.oval_impl(x0, y0, x1, y1, color, true);
    }

    fn oval_impl(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u8, fill: bool) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        let cx = (xa + xb) as f32 / 2.0;
        let cy = (ya + yb) as f32 / 2.0;
        let a = (xb - xa) as f32 / 2.0;
        let b = (yb - ya) as f32 / 2.0;
        // The bounding box is the cart's, so sweep only the rows and columns the clip
        // rect can accept. The ellipse's center and radii still come from the FULL
        // box, so every row keeps the extent it had before.
        let (ra, rb) = self.clipped_rows(ya, yb);
        if fill {
            for y in ra..=rb {
                let dy = if b > 0.0 { (y as f32 - cy) / b } else { 0.0 };
                let s = 1.0 - dy * dy;
                if s < 0.0 {
                    continue;
                }
                let dx = a * s.sqrt();
                let left = (cx - dx).round() as i32;
                let right = (cx + dx).round() as i32;
                if self.fill_pattern == 0 {
                    // Solid fill: one clipped memset per scanline of the oval.
                    self.fill_span(
                        left - self.camera_x,
                        right - self.camera_x,
                        y - self.camera_y,
                        color,
                    );
                } else {
                    let (ca, cb) = self.clipped_cols(left, right);
                    for x in ca..=cb {
                        self.raw_pset_fill(x - self.camera_x, y - self.camera_y, color);
                    }
                }
            }
        } else {
            // Plot the extremes along each axis so the outline has no gaps.
            for y in ra..=rb {
                let dy = if b > 0.0 { (y as f32 - cy) / b } else { 0.0 };
                let s = 1.0 - dy * dy;
                if s < 0.0 {
                    continue;
                }
                let dx = a * s.sqrt();
                self.pset((cx - dx).round() as i32, y, color);
                self.pset((cx + dx).round() as i32, y, color);
            }
            let (ca, cb) = self.clipped_cols(xa, xb);
            for x in ca..=cb {
                let dx = if a > 0.0 { (x as f32 - cx) / a } else { 0.0 };
                let s = 1.0 - dx * dx;
                if s < 0.0 {
                    continue;
                }
                let dy = b * s.sqrt();
                self.pset(x, (cy - dy).round() as i32, color);
                self.pset(x, (cy + dy).round() as i32, color);
            }
        }
    }

    /// Print text with the built-in font. Returns the x position after the
    /// last character.
    pub fn print(&mut self, text: &str, x: i32, y: i32, color: u8) -> i32 {
        let mut cx = x;
        let mut cy = y;
        for ch in text.chars() {
            if ch == '\n' {
                cx = x;
                cy += font::GLYPH_H;
                continue;
            }
            let rows = font::glyph(ch);
            for (ry, row) in rows.iter().enumerate() {
                for rx in 0..3 {
                    if row & (0b100 >> rx) != 0 {
                        self.pset(cx + rx, cy + ry as i32, color);
                    }
                }
            }
            cx += font::GLYPH_W;
        }
        cx
    }

    /// Set the persistent pen color used by `print_pen`.
    pub fn set_pen_color(&mut self, color: u8) {
        self.pen_color = color & 0x0f;
    }

    /// Set the persistent text cursor used by `print_pen`.
    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    /// Print at the cursor in the pen color, then advance the cursor one line
    /// down. Returns the x position after the last glyph.
    pub fn print_pen(&mut self, text: &str) -> i32 {
        let (x, y) = (self.cursor_x, self.cursor_y);
        let end = self.print(text, x, y, self.pen_color);
        self.cursor_y = y + font::GLYPH_H;
        end
    }

    /// Draw sprite `n` (and the `w x h`-pixel block to its right and below)
    /// from a sheet. Color 0 is transparent, matching the classic default.
    /// `w`/`h` are pixel extents: `w = 4` draws a 4-pixel-wide slice.
    #[allow(clippy::too_many_arguments)]
    pub fn spr(
        &mut self,
        sheet: &SpriteSheet,
        n: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flip_x: bool,
        flip_y: bool,
    ) {
        // `w`/`h` are pixel extents; a partial last cell is clipped mid-sprite.
        let pw = w.max(0);
        let ph = h.max(0);

        // Clip the destination rectangle to the clip rect once, up front, then
        // walk only the visible sub-rectangle. This skips the per-pixel clip
        // test entirely and fast-rejects fully off-screen sprites (a big win
        // for `map`, which calls `spr` once per tile). The destination spans
        // `[dx0, dx0 + pw)` x `[dy0, dy0 + ph)` in post-camera space; the `px`
        // range is where that lands inside `[cx0, cx1)`.
        let (dx0, dy0) = (x - self.camera_x, y - self.camera_y);
        let (cx0, cy0, cx1, cy1) = self.clip;
        let px_lo = (cx0 - dx0).max(0);
        let px_hi = (cx1 - dx0).min(pw);
        let py_lo = (cy0 - dy0).max(0);
        let py_hi = (cy1 - dy0).min(ph);
        if px_lo >= px_hi || py_lo >= py_hi {
            return;
        }

        // Decode the sprite's sheet origin once instead of per pixel. The flip
        // still mirrors about the FULL sprite extent (`pw`/`ph`), and source
        // reads may run past 8 into neighboring sprites for multi-sprite draws,
        // exactly as `sprite_pixel` does.
        let n = (n as usize) % SPRITE_COUNT;
        let base_sx = (n % SPRITES_PER_ROW * SPRITE_SIZE) as i32;
        let base_sy = (n / SPRITES_PER_ROW * SPRITE_SIZE) as i32;
        for py in py_lo..py_hi {
            let sy = if flip_y { ph - 1 - py } else { py };
            for px in px_lo..px_hi {
                let sx = if flip_x { pw - 1 - px } else { px };
                let c = sheet.get(base_sx + sx, base_sy + sy);
                if ((self.transparent >> c) & 1) == 0 {
                    // In bounds by construction: `px`/`py` lie within the clip.
                    let px_dst = dx0 + px;
                    let py_dst = dy0 + py;
                    self.pixels[(py_dst * WIDTH + px_dst) as usize] =
                        self.draw_pal[(c & 0x0f) as usize] & 0x0f;
                }
            }
        }
    }

    /// Draw a sheet rectangle `(sx,sy,sw,sh)` stretched into a screen rectangle
    /// `(dx,dy,dw,dh)` with nearest-neighbor sampling. Honors per-color
    /// transparency and the draw palette.
    #[allow(clippy::too_many_arguments)]
    pub fn sspr(
        &mut self,
        sheet: &SpriteSheet,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        dx: i32,
        dy: i32,
        dw: i32,
        dh: i32,
        flip_x: bool,
        flip_y: bool,
    ) {
        if sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0 {
            return;
        }

        // The destination extent is whatever the cart asked for, and one host call
        // costs it one unit of fuel however big that is — so walk only the part that
        // can survive the clip rect. `px`/`py` keep their original destination-space
        // values and the sampling below still divides by the FULL `dw`/`dh`, so
        // narrowing the range cannot shift which source pixel a column samples.
        let (dx0, dy0) = (
            i64::from(dx) - i64::from(self.camera_x),
            i64::from(dy) - i64::from(self.camera_y),
        );
        let (cx0, cy0, cx1, cy1) = self.clip;
        let px_lo = (i64::from(cx0) - dx0).clamp(0, i64::from(dw)) as i32;
        let px_hi = (i64::from(cx1) - dx0).clamp(0, i64::from(dw)) as i32;
        let py_lo = (i64::from(cy0) - dy0).clamp(0, i64::from(dh)) as i32;
        let py_hi = (i64::from(cy1) - dy0).clamp(0, i64::from(dh)) as i32;
        if px_lo >= px_hi || py_lo >= py_hi {
            return;
        }

        for py in py_lo..py_hi {
            let fy = if flip_y { dh - 1 - py } else { py };
            // `f * s / d` is in `0..s` and so always fits, but the product alone
            // overflows an i32 once the cart asks for a huge destination.
            let src_y = sy.saturating_add((i64::from(fy) * i64::from(sh) / i64::from(dh)) as i32);
            let row = (dy0 + i64::from(py)) as usize * WIDTH as usize;
            for px in px_lo..px_hi {
                let fx = if flip_x { dw - 1 - px } else { px };
                let src_x =
                    sx.saturating_add((i64::from(fx) * i64::from(sw) / i64::from(dw)) as i32);
                let c = sheet.get(src_x, src_y);
                if ((self.transparent >> c) & 1) == 0 {
                    // In bounds by construction: the loop range is the clip rect.
                    let col = (dx0 + i64::from(px)) as usize;
                    self.pixels[row + col] = self.draw_pal[(c & 0x0f) as usize] & 0x0f;
                }
            }
        }
    }

    /// Draw a region of the tile map. `layers` is a flag mask: when nonzero,
    /// only tiles whose flags intersect the mask are drawn. Tile 0 is empty.
    #[allow(clippy::too_many_arguments)]
    pub fn map(
        &mut self,
        map: &MapData,
        sheet: &SpriteSheet,
        cel_x: i32,
        cel_y: i32,
        sx: i32,
        sy: i32,
        cel_w: i32,
        cel_h: i32,
        layers: u8,
    ) {
        // Every cel is a `spr` call, so an unclipped `cel_w`/`cel_h` buys millions of
        // them for the single fuel unit the host call costs. Narrow the cel range to
        // those whose 8x8 destination can overlap the clip rect; `tx`/`ty` keep their
        // original values, so both the map lookup and the placement are unchanged.
        let (cel_w, cel_h) = (i64::from(cel_w.max(0)), i64::from(cel_h.max(0)));
        // The side of one cel in pixels, as against `cel_w`/`cel_h`, which count cels.
        let cel_px = SPRITE_SIZE as i64;
        let (cx0, cy0, cx1, cy1) = self.clip;
        let base_x = i64::from(sx) - i64::from(self.camera_x);
        let base_y = i64::from(sy) - i64::from(self.camera_y);
        // Cel `t` spans `[base + cel_px * t, base + cel_px * t + cel_px)`, so it is
        // visible for `t` from `floor((c0 - base) / cel_px)` to
        // `ceil((c1 - base) / cel_px)`.
        let tx_lo = (i64::from(cx0) - base_x).div_euclid(cel_px).clamp(0, cel_w) as i32;
        let tx_hi = (i64::from(cx1) - base_x + cel_px - 1)
            .div_euclid(cel_px)
            .clamp(0, cel_w) as i32;
        let ty_lo = (i64::from(cy0) - base_y).div_euclid(cel_px).clamp(0, cel_h) as i32;
        let ty_hi = (i64::from(cy1) - base_y + cel_px - 1)
            .div_euclid(cel_px)
            .clamp(0, cel_h) as i32;

        for ty in ty_lo..ty_hi {
            for tx in tx_lo..tx_hi {
                let tile = map.get(cel_x + tx, cel_y + ty);
                if tile == 0 {
                    continue;
                }
                if layers != 0 && sheet.flags(tile as u32) & layers == 0 {
                    continue;
                }
                self.spr(
                    sheet,
                    tile as u32,
                    sx + tx * SPRITE_SIZE as i32,
                    sy + ty * SPRITE_SIZE as i32,
                    SPRITE_SIZE as i32,
                    SPRITE_SIZE as i32,
                    false,
                    false,
                );
            }
        }
    }
}

/// Clamp four candidate `y` runs to `0..=cap`, drop the empty ones and merge the
/// overlaps, so the circle walk visits each `y` at most once. An unused slot comes
/// back as `(0, -1)`, which the walk's `y <= run_hi` guard skips.
fn merge_runs(shallow: [(i64, i64); 2], steep: [(i64, i64); 2], cap: i64) -> [(i64, i64); 4] {
    let mut runs = [(0i64, -1i64); 4];
    let mut n = 0;
    for (lo, hi) in shallow.into_iter().chain(steep) {
        let (lo, hi) = (lo.max(0), hi.min(cap));
        if lo <= hi {
            runs[n] = (lo, hi);
            n += 1;
        }
    }
    runs[..n].sort_unstable();
    let mut merged = [(0i64, -1i64); 4];
    let mut m = 0;
    for &(lo, hi) in &runs[..n] {
        if m > 0 && lo <= merged[m - 1].1 + 1 {
            merged[m - 1].1 = merged[m - 1].1.max(hi);
        } else {
            merged[m] = (lo, hi);
            m += 1;
        }
    }
    merged
}

/// The `y` range over which the circle walk's `x` stays inside `lo..=hi`. `arc_x`
/// is non-increasing in `y`, so one range of `x` maps back to one range of `y`.
fn arc_rows_to_y(r: i64, lo: i64, hi: i64) -> (i64, i64) {
    // `x` is confined to `0..=r` whatever the caller asks for.
    let (lo, hi) = (lo.max(0), hi.min(r));
    if lo > hi {
        return (0, -1);
    }
    // `arc_x(y) <= hi` exactly when `y * y >= r * r - hi * (hi + 1)`, and
    // `arc_x(y) >= lo` exactly when `y * y < r * r - (lo - 1) * lo`.
    let y_lo = ceil_sqrt((r * r - hi * (hi + 1)).max(0));
    let y_hi = if lo == 0 {
        r
    } else {
        ceil_sqrt((r * r - (lo - 1) * lo).max(0)) - 1
    };
    (y_lo, y_hi)
}

/// The `x` the midpoint circle walk holds at row `y`: the smallest `v >= 0` with
/// `v * (v + 1) >= r * r - y * y`. Stepping the walk's recurrence from `y = 0`
/// arrives at exactly this value, so it can seed a restart part way along the arc.
/// Past the octant boundary the two can disagree, but only once `x < y`, where the
/// walk stops under either.
fn arc_x(r: i64, y: i64) -> i64 {
    let m = r * r - y * y;
    if m <= 0 {
        return 0;
    }
    let v = ceil_sqrt(m);
    if v > 0 && (v - 1) * v >= m {
        v - 1
    } else {
        v
    }
}

/// The smallest `s >= 0` with `s * s >= m`.
fn ceil_sqrt(m: i64) -> i64 {
    let s = m.isqrt();
    if s * s < m {
        s + 1
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    #[test]
    fn cls_fills_screen() {
        let mut fb = Framebuffer::new();
        fb.cls(7);
        assert!(fb.pixels().iter().all(|&p| p == 7));
    }

    #[test]
    fn pset_pget_roundtrip() {
        let mut fb = Framebuffer::new();
        fb.pset(10, 20, 8);
        assert_eq!(fb.pget(10, 20), 8);
        assert_eq!(fb.pget(11, 20), 0);
    }

    #[test]
    fn out_of_bounds_is_safe() {
        let mut fb = Framebuffer::new();
        fb.pset(-1, 0, 5);
        fb.pset(0, 99999, 5);
        fb.line(-50, -50, 200, 200, 6);
        fb.circfill(0, 0, 300, 3);
        assert_eq!(fb.pget(-1, 0), 0);
    }

    #[test]
    fn camera_offsets_draws() {
        let mut fb = Framebuffer::new();
        fb.camera(10, 0);
        fb.pset(15, 5, 9);
        assert_eq!(fb.pget(5, 5), 9);
        fb.reset_state();
        fb.pset(15, 5, 9);
        assert_eq!(fb.pget(15, 5), 9);
    }

    #[test]
    fn clip_constrains_drawing() {
        let mut fb = Framebuffer::new();
        fb.clip(0, 0, 4, 4);
        fb.rectfill(0, 0, 127, 127, 7);
        assert_eq!(fb.pget(3, 3), 7);
        assert_eq!(fb.pget(4, 4), 0);
    }

    #[test]
    fn rect_outline_is_hollow() {
        let mut fb = Framebuffer::new();
        fb.rect(0, 0, 4, 4, 7);
        assert_eq!(fb.pget(0, 0), 7);
        assert_eq!(fb.pget(4, 4), 7);
        assert_eq!(fb.pget(2, 2), 0);
    }

    #[test]
    fn print_advances_cursor() {
        let mut fb = Framebuffer::new();
        let end = fb.print("abc", 0, 0, 7);
        assert_eq!(end, 3 * font::GLYPH_W);
    }

    #[test]
    fn partial_pixel_sprite_draws_a_partial_slice() {
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        // Fill sprite 0 (the top-left 8x8 cell) solid.
        for y in 0..8 {
            for x in 0..8 {
                sheet.set(x, y, 7);
            }
        }
        // A 4px width draws only the left four columns.
        fb.spr(&sheet, 0, 0, 0, 4, 8, false, false);
        assert_eq!(fb.pget(3, 4), 7, "left half is drawn");
        assert_eq!(fb.pget(4, 4), 0, "right half is untouched");
        assert_eq!(fb.pget(7, 7), 0, "bottom-right corner is untouched");
    }

    #[test]
    fn spr_clipped_flip_mirrors_about_full_sprite() {
        // Flip must mirror about the FULL 8x8 sprite, then the clip rect cuts
        // the result — not the other way round. Mark the four source corners
        // with distinct colors; double-flip swaps each corner to the opposite
        // one, and a 4x4 top-left clip should keep exactly one of them.
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        sheet.set(0, 0, 8); // top-left  -> dest (7,7)
        sheet.set(7, 0, 9); // top-right -> dest (0,7)
        sheet.set(0, 7, 11); // bottom-left  -> dest (7,0)
        sheet.set(7, 7, 12); // bottom-right -> dest (0,0)
        fb.clip(0, 0, 4, 4);
        fb.spr(&sheet, 0, 0, 0, 8, 8, true, true);
        assert_eq!(
            fb.pget(0, 0),
            12,
            "source bottom-right mirrors to dest (0,0) and survives the clip"
        );
        assert_eq!(fb.pget(7, 7), 0, "dest (7,7) is outside the 4x4 clip");
        assert_eq!(fb.pget(0, 7), 0, "dest (0,7) is outside the 4x4 clip");
        assert_eq!(fb.pget(7, 0), 0, "dest (7,0) is outside the 4x4 clip");
    }

    #[test]
    fn spr_partly_offscreen_under_camera_aligns_source() {
        // With the camera pushing the sprite up-and-left, only its bottom-right
        // part is on screen; the visible pixels must come from the matching
        // source columns/rows (no wrap), starting at dest (0,0).
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        for y in 0..8 {
            for x in 0..8 {
                sheet.set(x, y, 7);
            }
        }
        sheet.set(2, 3, 9); // the source pixel that lands on dest (0,0)
        fb.camera(2, 3);
        fb.spr(&sheet, 0, 0, 0, 8, 8, false, false);
        assert_eq!(
            fb.pget(0, 0),
            9,
            "source (2,3) lands at the top-left corner"
        );
        assert_eq!(fb.pget(1, 0), 7, "source (3,3) is the next column");
        assert_eq!(fb.pget(5, 4), 7, "the rest of the on-screen part is drawn");
        // Nothing wrapped to the far edges of the screen.
        assert_eq!(fb.pget(127, 127), 0, "no wrap to the opposite corner");
    }

    #[test]
    fn spr_partial_pixel_width_meets_clip_edge() {
        // A 4px-wide sprite slice further trimmed by a 2px-wide clip: only the
        // first two destination columns survive.
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        for y in 0..8 {
            for x in 0..8 {
                sheet.set(x, y, 7);
            }
        }
        fb.clip(0, 0, 2, 128);
        fb.spr(&sheet, 0, 0, 0, 4, 8, false, false);
        assert_eq!(fb.pget(1, 3), 7, "inside the clip and the partial width");
        assert_eq!(fb.pget(2, 3), 0, "clipped away at x=2");
        assert_eq!(fb.pget(4, 3), 0, "beyond the 4px width anyway");
    }

    #[test]
    fn transparency_mask_controls_sprite_pixels() {
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        for y in 0..8 {
            for x in 0..8 {
                sheet.set(x, y, 8); // a solid red sprite
            }
        }
        // Default: nonzero colors draw.
        fb.spr(&sheet, 0, 0, 0, 8, 8, false, false);
        assert_eq!(fb.pget(1, 1), 8);
        // Make red transparent: redrawing over green leaves green showing.
        fb.cls(3);
        fb.set_transparent_color(8, true);
        fb.spr(&sheet, 0, 0, 0, 8, 8, false, false);
        assert_eq!(fb.pget(1, 1), 3, "red made transparent");
        // reset_transparency restores the default; red draws again.
        fb.reset_transparency();
        fb.spr(&sheet, 0, 0, 0, 8, 8, false, false);
        assert_eq!(fb.pget(1, 1), 8);
    }

    #[test]
    fn color_zero_can_be_made_opaque() {
        let mut fb = Framebuffer::new();
        let sheet = SpriteSheet::default(); // all color 0
        fb.cls(7);
        fb.set_transparent_color(0, false);
        fb.spr(&sheet, 0, 0, 0, 8, 8, false, false);
        assert_eq!(fb.pget(3, 3), 0, "color 0 now drawn over white");
    }

    #[test]
    fn draw_palette_remaps_writes() {
        let mut fb = Framebuffer::new();
        fb.remap_color(8, 12); // draw red as blue
        fb.pset(5, 5, 8);
        assert_eq!(fb.pget(5, 5), 12);
        fb.reset_palette();
        fb.pset(6, 6, 8);
        assert_eq!(fb.pget(6, 6), 8);
    }

    #[test]
    fn cls_ignores_draw_palette() {
        let mut fb = Framebuffer::new();
        fb.remap_color(0, 8);
        fb.cls(0);
        assert_eq!(fb.pget(10, 10), 0, "cls clears to the literal color");
    }

    #[test]
    fn display_palette_remaps_at_upload() {
        let mut fb = Framebuffer::new();
        fb.pset(0, 0, 8); // red stored
        fb.remap_display_color(8, 12); // show red as blue
        let mut out = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        fb.write_rgba(&mut out);
        assert_eq!(&out[0..4], &palette::rgba(12), "pixel uploaded as blue");
        assert_eq!(fb.pget(0, 0), 8, "stored index is unchanged");
    }

    #[test]
    fn ovalfill_fills_center_not_corner() {
        let mut fb = Framebuffer::new();
        fb.ovalfill(0, 0, 10, 6, 7);
        assert_eq!(fb.pget(5, 3), 7, "center filled");
        assert_eq!(fb.pget(0, 0), 0, "bounding-box corner stays empty");
    }

    #[test]
    fn oval_outline_is_hollow() {
        let mut fb = Framebuffer::new();
        fb.oval(0, 0, 10, 10, 7);
        assert_eq!(fb.pget(5, 0), 7, "top of the outline is set");
        assert_eq!(fb.pget(5, 5), 0, "center is hollow");
    }

    #[test]
    fn two_color_fill_pattern_alternates() {
        let mut fb = Framebuffer::new();
        // bit 15 (top-left) = 1, bit 14 = 0, ...
        fb.set_fill_pattern(0b1010_0101_1010_0101, 12, false);
        fb.rectfill(0, 0, 3, 3, 7);
        assert_eq!(
            fb.pget(0, 0),
            12,
            "pattern-1 pixel uses the secondary color"
        );
        assert_eq!(fb.pget(1, 0), 7, "pattern-0 pixel uses the primary color");
    }

    #[test]
    fn transparent_fill_pattern_skips_pixels() {
        let mut fb = Framebuffer::new();
        fb.cls(3);
        fb.set_fill_pattern(0xffff, 0, true); // every pixel is pattern-1, transparent
        fb.rectfill(0, 0, 3, 3, 7);
        assert_eq!(
            fb.pget(1, 1),
            3,
            "all pattern-1 pixels skipped; background shows"
        );
    }

    #[test]
    fn zero_pattern_fills_solid() {
        let mut fb = Framebuffer::new();
        fb.set_fill_pattern(0, 0, false);
        fb.rectfill(0, 0, 3, 3, 7);
        assert_eq!(fb.pget(2, 2), 7);
    }

    #[test]
    fn sspr_upscales_with_nearest_neighbor() {
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        sheet.set(0, 0, 8); // single red source pixel
        fb.sspr(&sheet, 0, 0, 1, 1, 10, 10, 4, 4, false, false);
        assert_eq!(fb.pget(10, 10), 8);
        assert_eq!(
            fb.pget(13, 13),
            8,
            "the whole 4x4 block is the source pixel"
        );
    }

    #[test]
    fn sspr_respects_transparency() {
        let mut fb = Framebuffer::new();
        let sheet = SpriteSheet::default(); // all color 0
        fb.cls(3);
        fb.sspr(&sheet, 0, 0, 2, 2, 0, 0, 4, 4, false, false);
        assert_eq!(fb.pget(1, 1), 3, "color 0 is transparent by default");
    }

    #[test]
    fn sspr_flips_horizontally() {
        let mut fb = Framebuffer::new();
        let mut sheet = SpriteSheet::default();
        sheet.set(0, 0, 8);
        sheet.set(1, 0, 9);
        fb.sspr(&sheet, 0, 0, 2, 1, 0, 0, 2, 1, true, false);
        assert_eq!(
            fb.pget(1, 0),
            8,
            "flip puts the source-left pixel on the right"
        );
        assert_eq!(fb.pget(0, 0), 9);
    }

    /// One geometry case for the clipping-equivalence tests: source and
    /// destination rectangles plus the camera and clip rect they draw under.
    struct ClipCase {
        name: &'static str,
        src: (i32, i32, i32, i32),
        dst: (i32, i32, i32, i32),
        camera: (i32, i32),
        clip: (i32, i32, i32, i32),
    }

    /// A camera offset and clip rect to draw a case under.
    struct ClipState {
        name: &'static str,
        camera: (i32, i32),
        clip: (i32, i32, i32, i32),
    }

    /// Camera and clip states every span-sweeping primitive is checked against.
    const CLIP_STATES: [ClipState; 4] = [
        ClipState {
            name: "plain",
            camera: (0, 0),
            clip: (0, 0, WIDTH, HEIGHT),
        },
        ClipState {
            name: "camera",
            camera: (23, -17),
            clip: (0, 0, WIDTH, HEIGHT),
        },
        ClipState {
            name: "clip rect",
            camera: (0, 0),
            clip: (12, 20, 33, 41),
        },
        ClipState {
            name: "camera and clip",
            camera: (-9, 6),
            clip: (12, 20, 33, 41),
        },
    ];

    /// A sheet with a varied, non-uniform pattern spanning the first two rows of
    /// sprites — including color 0, so transparency is exercised with the geometry.
    fn patterned_sheet() -> SpriteSheet {
        let mut sheet = SpriteSheet::default();
        for y in 0..24 {
            for x in 0..64 {
                sheet.set(x, y, ((x * 5 + y * 7) % 16) as u8);
            }
        }
        for n in 0..8 {
            sheet.set_flag(n, 0, n % 2 == 0);
        }
        sheet
    }

    /// A map whose cels cycle through the first few sprites, with tile-0 holes so
    /// the empty-cel skip is exercised too.
    fn patterned_map() -> MapData {
        let mut map = MapData::default();
        for y in 0..20 {
            for x in 0..30 {
                map.set(x, y, ((x * 3 + y * 5) % 7) as u8);
            }
        }
        map
    }

    /// A draw palette and transparency mask for a sweep to run under. `spr` and
    /// `sspr` write into the framebuffer directly instead of going through `pset`,
    /// so both have to be checked against a `pset`-based walk with the palette and
    /// the mask off their defaults.
    struct PenState {
        name: &'static str,
        remap: &'static [(u8, u8)],
        transparent: &'static [u8],
    }

    const PEN_STATES: [PenState; 2] = [
        PenState {
            name: "default pen",
            remap: &[],
            transparent: &[],
        },
        PenState {
            name: "remapped pen",
            remap: &[(9, 3), (5, 14), (0, 7)],
            transparent: &[4, 11],
        },
    ];

    /// A framebuffer staged for one case: a nonzero background so stray writes
    /// show up either way, plus the camera, clip rect and pen the case asks for.
    fn staged_fb(camera: (i32, i32), clip: (i32, i32, i32, i32), pen: &PenState) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.cls(1);
        fb.camera(camera.0, camera.1);
        fb.clip(clip.0, clip.1, clip.2, clip.3);
        for &(from, to) in pen.remap {
            fb.remap_color(from, to);
        }
        for &color in pen.transparent {
            fb.set_transparent_color(color, true);
        }
        fb
    }

    /// Assert two framebuffers match, naming the first differing pixel rather than
    /// dumping 16 K palette indices into the failure message.
    fn assert_same_pixels(got: &Framebuffer, want: &Framebuffer, case: &str) {
        let diff = got
            .pixels()
            .iter()
            .zip(want.pixels())
            .position(|(g, w)| g != w);
        if let Some(i) = diff {
            let (x, y) = (i as i32 % WIDTH, i as i32 / WIDTH);
            panic!(
                "{case}: pixel ({x}, {y}) is {}, want {}",
                got.pixels()[i],
                want.pixels()[i]
            );
        }
    }

    /// How long a pathological one-call draw is given to finish. Clipped, each of
    /// the calls below returns in microseconds; unclipped they run for hours, so
    /// without a deadline a regression would stall the whole test run instead of
    /// naming the primitive that broke.
    const SWEEP_DEADLINE: Duration = Duration::from_secs(20);

    /// Run one draw on a worker thread and hand back what it drew, failing the test
    /// if it has not finished within `SWEEP_DEADLINE`.
    fn drawn_within_deadline<F>(what: &str, draw: F) -> Framebuffer
    where
        F: FnOnce(&mut Framebuffer) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut fb = Framebuffer::new();
            draw(&mut fb);
            let _ = tx.send(fb);
        });
        match rx.recv_timeout(SWEEP_DEADLINE) {
            Ok(fb) => fb,
            Err(_) => panic!("{what} did not finish within {SWEEP_DEADLINE:?}"),
        }
    }

    /// `sspr` as it was before the destination range was clipped: walk every
    /// requested pixel and let `pset` reject whatever falls outside.
    fn sspr_unclipped(
        fb: &mut Framebuffer,
        sheet: &SpriteSheet,
        case: &ClipCase,
        flip_x: bool,
        flip_y: bool,
    ) {
        let (sx, sy, sw, sh) = case.src;
        let (dx, dy, dw, dh) = case.dst;
        if sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0 {
            return;
        }
        for py in 0..dh {
            for px in 0..dw {
                let fx = if flip_x { dw - 1 - px } else { px };
                let fy = if flip_y { dh - 1 - py } else { py };
                let c = sheet.get(sx + fx * sw / dw, sy + fy * sh / dh);
                if ((fb.transparent >> c) & 1) == 0 {
                    fb.pset(dx + px, dy + py, c);
                }
            }
        }
    }

    #[test]
    fn sspr_clipping_matches_the_unclipped_walk() {
        let sheet = patterned_sheet();
        let full = (0, 0, WIDTH, HEIGHT);
        let cases = [
            ClipCase {
                name: "fully on screen",
                src: (0, 0, 8, 8),
                dst: (10, 10, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "over the left edge",
                src: (0, 0, 8, 8),
                dst: (-9, 10, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "over the right edge",
                src: (0, 0, 8, 8),
                dst: (121, 10, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "over the top edge",
                src: (0, 0, 8, 8),
                dst: (10, -11, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "over the bottom edge",
                src: (0, 0, 8, 8),
                dst: (10, 119, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "entirely off to the right",
                src: (0, 0, 8, 8),
                dst: (200, 60, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "entirely off past the origin",
                src: (0, 0, 8, 8),
                dst: (-40, -40, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "negative dx and dy straddling the origin",
                src: (0, 0, 8, 8),
                dst: (-5, -3, 24, 24),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "downscale",
                src: (0, 0, 16, 16),
                dst: (60, 60, 3, 3),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "downscale over an edge",
                src: (0, 0, 16, 16),
                dst: (-2, 120, 5, 11),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "upscale with an awkward ratio",
                src: (1, 2, 3, 5),
                dst: (-2, 100, 37, 41),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                // Bigger than the screen in both axes: clamping `dw`/`dh` instead of
                // narrowing the loop would change the sampling denominator here.
                name: "upscale larger than the screen",
                src: (0, 0, 8, 8),
                dst: (-30, -20, 200, 220),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "upscale far larger than the screen, then clipped",
                src: (0, 0, 5, 7),
                dst: (10, 10, 500, 400),
                camera: (0, 0),
                clip: (20, 20, 60, 60),
            },
            ClipCase {
                name: "pushed off screen by the camera",
                src: (0, 0, 8, 8),
                dst: (10, 10, 16, 16),
                camera: (30, 20),
                clip: full,
            },
            ClipCase {
                name: "pulled on screen by a negative camera",
                src: (0, 0, 8, 8),
                dst: (10, 10, 20, 20),
                camera: (-100, -115),
                clip: full,
            },
            ClipCase {
                name: "trimmed by a clip rect",
                src: (0, 0, 8, 8),
                dst: (0, 0, 32, 32),
                camera: (0, 0),
                clip: (4, 4, 20, 20),
            },
            ClipCase {
                name: "camera and clip together",
                src: (2, 3, 12, 9),
                dst: (0, 0, 40, 40),
                camera: (-5, 7),
                clip: (10, 10, 40, 40),
            },
            ClipCase {
                name: "one-pixel destination",
                src: (0, 0, 8, 8),
                dst: (64, 64, 1, 1),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "source origin outside the sheet",
                src: (-4, -4, 8, 8),
                dst: (20, 20, 16, 16),
                camera: (0, 0),
                clip: full,
            },
            ClipCase {
                name: "clip rect that rejects everything",
                src: (0, 0, 8, 8),
                dst: (0, 0, 16, 16),
                camera: (0, 0),
                clip: (100, 100, 4, 4),
            },
        ];
        for case in &cases {
            for pen in &PEN_STATES {
                for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)]
                {
                    let mut got = staged_fb(case.camera, case.clip, pen);
                    let mut want = staged_fb(case.camera, case.clip, pen);
                    let (sx, sy, sw, sh) = case.src;
                    let (dx, dy, dw, dh) = case.dst;
                    got.sspr(&sheet, sx, sy, sw, sh, dx, dy, dw, dh, flip_x, flip_y);
                    sspr_unclipped(&mut want, &sheet, case, flip_x, flip_y);
                    assert_same_pixels(
                        &got,
                        &want,
                        &format!(
                            "{} / {} (flip_x {flip_x}, flip_y {flip_y})",
                            case.name, pen.name
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn sspr_bounded_by_the_screen_not_the_request() {
        // 8x8 -> 4096x4096 is 16.7 M destination pixels bought with one host call, and
        // 8x8 -> a million square is 10^12 of them. Both have to be paid for at the
        // 16 K the screen can actually show, so the deadline is the assertion here:
        // walking the request instead paints the same visible pixels, it just takes
        // until next week. Every on-screen pixel maps back into the source, all of
        // which is color 9.
        for side in [4096, 1_000_000] {
            let fb = drawn_within_deadline(&format!("an 8x8 -> {side}x{side} sspr"), move |fb| {
                let mut sheet = SpriteSheet::default();
                for y in 0..8 {
                    for x in 0..8 {
                        sheet.set(x, y, 9);
                    }
                }
                fb.sspr(&sheet, 0, 0, 8, 8, 0, 0, side, side, false, false);
            });
            assert!(
                fb.pixels().iter().all(|&p| p == 9),
                "the visible part of a {side}-square stretch is drawn"
            );
        }
    }

    #[test]
    fn sspr_survives_a_destination_at_the_i32_ceiling() {
        // Unclipped this is ~4.6e18 inner iterations, and the `fx * sw` product on
        // its own overflows an i32. The destination starts a million pixels off the
        // top-left, so the screen samples the far corner of the source sprite under
        // each flip — all of it color 9.
        let mut sheet = SpriteSheet::default();
        for y in 0..8 {
            for x in 0..8 {
                sheet.set(x, y, 9);
            }
        }
        for (flip_x, flip_y) in [(false, false), (true, false), (false, true), (true, true)] {
            let sheet = sheet.clone();
            let fb = drawn_within_deadline("an sspr at the i32 ceiling", move |fb| {
                fb.sspr(
                    &sheet,
                    0,
                    0,
                    8,
                    8,
                    -1_000_000,
                    -1_000_000,
                    i32::MAX,
                    i32::MAX,
                    flip_x,
                    flip_y,
                );
            });
            assert!(
                fb.pixels().iter().all(|&p| p == 9),
                "flip_x {flip_x}, flip_y {flip_y}"
            );
        }
    }

    /// `spr` written the naive way: sample the sheet and hand each pixel to `pset`,
    /// which is where every other primitive picks up the draw palette.
    #[allow(clippy::too_many_arguments)]
    fn spr_via_pset(
        fb: &mut Framebuffer,
        sheet: &SpriteSheet,
        n: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flip_x: bool,
        flip_y: bool,
    ) {
        let (pw, ph) = (w.max(0), h.max(0));
        let n = (n as usize) % SPRITE_COUNT;
        let base_sx = (n % SPRITES_PER_ROW * SPRITE_SIZE) as i32;
        let base_sy = (n / SPRITES_PER_ROW * SPRITE_SIZE) as i32;
        for py in 0..ph {
            let sy = if flip_y { ph - 1 - py } else { py };
            for px in 0..pw {
                let sx = if flip_x { pw - 1 - px } else { px };
                let c = sheet.get(base_sx + sx, base_sy + sy);
                if ((fb.transparent >> c) & 1) == 0 {
                    fb.pset(x + px, y + py, c);
                }
            }
        }
    }

    #[test]
    fn spr_writes_match_the_pset_path() {
        // `spr` writes into the framebuffer directly rather than through `pset`, so
        // the draw palette and transparency mask it applies on the way have to be
        // checked against the shared path it bypasses — `map` cannot do it, because
        // its reference walk calls `spr` too.
        let sheet = patterned_sheet();
        for pen in &PEN_STATES {
            for state in &CLIP_STATES {
                for (n, at, size) in [
                    (0u32, (10, 10), (8, 8)),
                    (3, (-4, 60), (8, 8)),
                    (9, (124, -2), (16, 16)),
                    (1, (30, 30), (5, 3)),
                ] {
                    for (flip_x, flip_y) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let mut got = staged_fb(state.camera, state.clip, pen);
                        let mut want = staged_fb(state.camera, state.clip, pen);
                        got.spr(&sheet, n, at.0, at.1, size.0, size.1, flip_x, flip_y);
                        spr_via_pset(
                            &mut want, &sheet, n, at.0, at.1, size.0, size.1, flip_x, flip_y,
                        );
                        assert_same_pixels(
                            &got,
                            &want,
                            &format!(
                                "sprite {n} at {at:?} / {} / {} (flip {flip_x} {flip_y})",
                                state.name, pen.name
                            ),
                        );
                    }
                }
            }
        }
    }

    /// One `map` case: the cel rectangle, where it lands on screen, and the
    /// camera/clip state it draws under.
    struct MapCase {
        name: &'static str,
        cel: (i32, i32, i32, i32),
        at: (i32, i32),
        camera: (i32, i32),
        clip: (i32, i32, i32, i32),
    }

    /// `map` as it was before the cel range was clipped.
    fn map_unclipped(
        fb: &mut Framebuffer,
        map: &MapData,
        sheet: &SpriteSheet,
        case: &MapCase,
        layers: u8,
    ) {
        let (cel_x, cel_y, cel_w, cel_h) = case.cel;
        let (sx, sy) = case.at;
        for ty in 0..cel_h {
            for tx in 0..cel_w {
                let tile = map.get(cel_x + tx, cel_y + ty);
                if tile == 0 {
                    continue;
                }
                if layers != 0 && sheet.flags(tile as u32) & layers == 0 {
                    continue;
                }
                fb.spr(
                    sheet,
                    tile as u32,
                    sx + tx * SPRITE_SIZE as i32,
                    sy + ty * SPRITE_SIZE as i32,
                    SPRITE_SIZE as i32,
                    SPRITE_SIZE as i32,
                    false,
                    false,
                );
            }
        }
    }

    #[test]
    fn map_clipping_matches_the_unclipped_walk() {
        let map = patterned_map();
        let sheet = patterned_sheet();
        let full = (0, 0, WIDTH, HEIGHT);
        let cases = [
            MapCase {
                name: "fully on screen",
                cel: (0, 0, 8, 8),
                at: (0, 0),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "wider than the screen",
                cel: (0, 0, 30, 20),
                at: (0, 0),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "over the left edge",
                cel: (0, 0, 20, 16),
                at: (-20, 0),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "over the top edge on a half-cel offset",
                cel: (0, 0, 20, 16),
                at: (-3, -13),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "over the bottom-right corner",
                cel: (0, 0, 20, 16),
                at: (100, 100),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "entirely off screen",
                cel: (0, 0, 20, 16),
                at: (-500, -500),
                camera: (0, 0),
                clip: full,
            },
            MapCase {
                name: "shifted by the camera",
                cel: (0, 0, 20, 16),
                at: (0, 0),
                camera: (37, 21),
                clip: full,
            },
            MapCase {
                name: "pulled back by a negative camera",
                cel: (0, 0, 20, 16),
                at: (0, 0),
                camera: (-19, -5),
                clip: full,
            },
            MapCase {
                name: "trimmed by a clip rect",
                cel: (0, 0, 20, 16),
                at: (0, 0),
                camera: (0, 0),
                clip: (10, 10, 30, 30),
            },
            MapCase {
                name: "cel origin offset into the map",
                cel: (5, 3, 10, 10),
                at: (2, 2),
                camera: (-6, 11),
                clip: (3, 7, 90, 60),
            },
            MapCase {
                name: "empty cel rectangle",
                cel: (0, 0, 0, 0),
                at: (0, 0),
                camera: (0, 0),
                clip: full,
            },
        ];
        for case in &cases {
            for pen in &PEN_STATES {
                for layers in [0u8, 1] {
                    let mut got = staged_fb(case.camera, case.clip, pen);
                    let mut want = staged_fb(case.camera, case.clip, pen);
                    let (cel_x, cel_y, cel_w, cel_h) = case.cel;
                    let (sx, sy) = case.at;
                    got.map(&map, &sheet, cel_x, cel_y, sx, sy, cel_w, cel_h, layers);
                    map_unclipped(&mut want, &map, &sheet, case, layers);
                    assert_same_pixels(
                        &got,
                        &want,
                        &format!("{} / {} (layers {layers})", case.name, pen.name),
                    );
                }
            }
        }
    }

    #[test]
    fn map_bounded_by_the_screen_not_the_request() {
        // 100_000 x 100_000 cels is 10^10 tile lookups for one host call. Only the
        // 16x16 that fit the screen can draw anything, so the result must match
        // asking for exactly those.
        let got = drawn_within_deadline("a 100_000-cel map", |fb| {
            fb.map(
                &patterned_map(),
                &patterned_sheet(),
                0,
                0,
                0,
                0,
                100_000,
                100_000,
                0,
            );
        });
        let mut want = Framebuffer::new();
        want.map(&patterned_map(), &patterned_sheet(), 0, 0, 0, 0, 16, 16, 0);
        assert_same_pixels(&got, &want, "a 100_000-cel request");
    }

    /// `line` as it was before the walk was solved against the clip rect: step the
    /// Bresenham error term from one endpoint to the other, however far that is.
    fn line_unclipped(fb: &mut Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (mut x0, mut y0) = (x0 - fb.camera_x, y0 - fb.camera_y);
        let (x1, y1) = (x1 - fb.camera_x, y1 - fb.camera_y);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            fb.raw_pset(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    #[test]
    fn line_clipping_matches_the_unclipped_walk() {
        // Every ordered pair drawn from this spread: on screen, on each edge, just
        // outside it and far outside. That covers all eight octants, both degenerate
        // axes, the exact diagonal and the single-pixel line, under each camera and
        // clip rect.
        const ENDS: [i32; 7] = [-201, -1, 0, 37, 89, 127, 260];
        for state in CLIP_STATES {
            for x0 in ENDS {
                for y0 in ENDS {
                    for x1 in ENDS {
                        for y1 in ENDS {
                            let mut got = staged_fb(state.camera, state.clip, &PEN_STATES[0]);
                            let mut want = staged_fb(state.camera, state.clip, &PEN_STATES[0]);
                            got.line(x0, y0, x1, y1, 7);
                            line_unclipped(&mut want, x0, y0, x1, y1, 7);
                            assert_same_pixels(
                                &got,
                                &want,
                                &format!("({x0}, {y0}) - ({x1}, {y1}) / {}", state.name),
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn line_bounded_by_the_screen_not_the_endpoints() {
        // A line a billion pixels long is one host call billed one fuel unit, and
        // stepping it is a billion iterations. Solving the major axis against the clip
        // rect costs at most a screen's width instead — the deadline is what pins that
        // down, since an unclipped walk draws the same pixels, just not this decade.
        // The shapes are picked so the visible pixels follow from the geometry alone.
        /// One case: a name, the line's endpoints, and the pixels it must leave.
        type LineCase = (&'static str, (i32, i32, i32, i32), fn(&mut Framebuffer));

        let far = 2_000_000_000;
        let cases: [LineCase; 5] = [
            ("rightwards along row 0", (0, 0, far, 0), |fb| {
                for x in 0..WIDTH {
                    fb.pset(x, 0, 7);
                }
            }),
            ("downwards along column 0", (0, 0, 0, far), |fb| {
                for y in 0..HEIGHT {
                    fb.pset(0, y, 7);
                }
            }),
            ("the main diagonal", (0, 0, far, far), |fb| {
                for i in 0..WIDTH {
                    fb.pset(i, i, 7);
                }
            }),
            (
                "the main diagonal, walked backwards",
                (far, far, 0, 0),
                |fb| {
                    for i in 0..WIDTH {
                        fb.pset(i, i, 7);
                    }
                },
            ),
            // Half slope: Bresenham puts row `(x + 1) / 2` on column `x`.
            ("half slope", (0, 0, far, far / 2), |fb| {
                for x in 0..WIDTH {
                    fb.pset(x, (x + 1) / 2, 7);
                }
            }),
        ];
        for (name, ends, expected) in cases {
            let got = drawn_within_deadline(name, move |fb| {
                fb.line(ends.0, ends.1, ends.2, ends.3, 7);
            });
            let mut want = Framebuffer::new();
            expected(&mut want);
            assert_same_pixels(&got, &want, name);
        }
    }

    /// `rectfill` as it was before the sweep was clipped.
    fn rectfill_unclipped(fb: &mut Framebuffer, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        if fb.fill_pattern == 0 {
            for y in ya..=yb {
                fb.fill_span(xa - fb.camera_x, xb - fb.camera_x, y - fb.camera_y, color);
            }
        } else {
            for y in ya..=yb {
                for x in xa..=xb {
                    fb.raw_pset_fill(x - fb.camera_x, y - fb.camera_y, color);
                }
            }
        }
    }

    #[test]
    fn rectfill_clipping_matches_the_unclipped_walk() {
        let rects = [
            ("fully on screen", (10, 10, 40, 30)),
            ("over the top-left", (-20, -30, 40, 30)),
            ("over the bottom-right", (100, 90, 200, 300)),
            ("entirely off screen", (300, 300, 400, 400)),
            ("inverted corners", (60, 70, 20, 10)),
            ("wider than the screen", (-50, 40, 400, 44)),
            ("a single pixel", (64, 64, 64, 64)),
        ];
        for pattern in [0u16, 0b1010_0101_1010_0101] {
            for (rect_name, (x0, y0, x1, y1)) in rects {
                for state in CLIP_STATES {
                    let (state_name, mut got, mut want) = (
                        state.name,
                        staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                        staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                    );
                    got.set_fill_pattern(pattern, 12, false);
                    want.set_fill_pattern(pattern, 12, false);
                    got.rectfill(x0, y0, x1, y1, 7);
                    rectfill_unclipped(&mut want, x0, y0, x1, y1, 7);
                    assert_same_pixels(
                        &got,
                        &want,
                        &format!("{rect_name} / {state_name} / pattern {pattern:#06x}"),
                    );
                }
            }
        }
    }

    #[test]
    fn rectfill_bounded_by_the_screen_not_the_request() {
        // Corners near the i32 extremes are billions of rows unclipped; the visible
        // result is the same as filling exactly the screen. The extremes are
        // multiples of 4 so the 4x4 fill pattern lands identically either way.
        for pattern in [0u16, 0b1010_0101_1010_0101] {
            let got = drawn_within_deadline("a rectfill spanning the i32 range", move |fb| {
                fb.set_fill_pattern(pattern, 12, false);
                fb.rectfill(
                    i32::MIN / 2,
                    i32::MIN / 2,
                    i32::MAX / 2 + 1,
                    i32::MAX / 2 + 1,
                    7,
                );
            });
            let mut want = Framebuffer::new();
            want.set_fill_pattern(pattern, 12, false);
            want.rectfill(0, 0, WIDTH - 1, HEIGHT - 1, 7);
            assert_same_pixels(&got, &want, &format!("pattern {pattern:#06x}"));
        }
    }

    /// `oval_impl` as it was before the sweep was clipped.
    fn oval_unclipped(
        fb: &mut Framebuffer,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: u8,
        fill: bool,
    ) {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        let cx = (xa + xb) as f32 / 2.0;
        let cy = (ya + yb) as f32 / 2.0;
        let a = (xb - xa) as f32 / 2.0;
        let b = (yb - ya) as f32 / 2.0;
        let half_extent = |v: f32, half: f32| -> Option<f32> {
            let d = if half > 0.0 { v / half } else { 0.0 };
            let s = 1.0 - d * d;
            if s < 0.0 {
                None
            } else {
                Some(s.sqrt())
            }
        };
        if fill {
            for y in ya..=yb {
                let Some(s) = half_extent(y as f32 - cy, b) else {
                    continue;
                };
                let left = (cx - a * s).round() as i32;
                let right = (cx + a * s).round() as i32;
                if fb.fill_pattern == 0 {
                    fb.fill_span(
                        left - fb.camera_x,
                        right - fb.camera_x,
                        y - fb.camera_y,
                        color,
                    );
                } else {
                    for x in left..=right {
                        fb.raw_pset_fill(x - fb.camera_x, y - fb.camera_y, color);
                    }
                }
            }
        } else {
            for y in ya..=yb {
                let Some(s) = half_extent(y as f32 - cy, b) else {
                    continue;
                };
                fb.pset((cx - a * s).round() as i32, y, color);
                fb.pset((cx + a * s).round() as i32, y, color);
            }
            for x in xa..=xb {
                let Some(s) = half_extent(x as f32 - cx, a) else {
                    continue;
                };
                fb.pset(x, (cy - b * s).round() as i32, color);
                fb.pset(x, (cy + b * s).round() as i32, color);
            }
        }
    }

    #[test]
    fn oval_clipping_matches_the_unclipped_walk() {
        let boxes = [
            ("fully on screen", (10, 10, 60, 40)),
            ("over the top-left", (-30, -25, 30, 20)),
            ("over the bottom-right", (90, 80, 190, 200)),
            ("entirely off screen", (300, 300, 380, 360)),
            ("bigger than the screen", (-200, -200, 320, 320)),
            ("a degenerate line", (20, 50, 90, 50)),
        ];
        for pattern in [0u16, 0b1010_0101_1010_0101] {
            for fill in [false, true] {
                for (shape, (x0, y0, x1, y1)) in boxes {
                    for state in CLIP_STATES {
                        let (state_name, mut got, mut want) = (
                            state.name,
                            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                        );
                        got.set_fill_pattern(pattern, 12, false);
                        want.set_fill_pattern(pattern, 12, false);
                        if fill {
                            got.ovalfill(x0, y0, x1, y1, 7);
                        } else {
                            got.oval(x0, y0, x1, y1, 7);
                        }
                        oval_unclipped(&mut want, x0, y0, x1, y1, 7, fill);
                        assert_same_pixels(
                            &got,
                            &want,
                            &format!(
                                "{shape} / {state_name} / fill {fill} / pattern {pattern:#06x}"
                            ),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn oval_bounded_by_the_screen_not_the_request() {
        // A bounding box spanning most of the i32 range is billions of rows
        // unclipped. The screen sits deep inside the ellipse, so the fill covers it
        // completely while the outline runs a billion pixels away on every side.
        let (lo, hi) = (i32::MIN / 2 + 1, i32::MAX / 2);
        let filled = drawn_within_deadline("an ovalfill spanning the i32 range", move |fb| {
            fb.ovalfill(lo, lo, hi, hi, 7);
        });
        assert!(
            filled.pixels().iter().all(|&p| p == 7),
            "the screen is well inside the ellipse"
        );
        let outline = drawn_within_deadline("an oval spanning the i32 range", move |fb| {
            fb.oval(lo, lo, hi, hi, 7);
        });
        assert!(
            outline.pixels().iter().all(|&p| p == 0),
            "the outline never comes near the screen"
        );
    }

    /// `circle_impl` as it was before the arc was rejected and its patterned fill
    /// runs were clipped.
    fn circle_unclipped(fb: &mut Framebuffer, cx: i32, cy: i32, r: i32, color: u8, fill: bool) {
        let (cx, cy) = (cx - fb.camera_x, cy - fb.camera_y);
        let (mut x, mut y, mut err) = (r.max(0), 0, 1 - r.max(0));
        while x >= y {
            if fill && fb.fill_pattern == 0 {
                fb.fill_span(cx - x, cx + x, cy + y, color);
                fb.fill_span(cx - x, cx + x, cy - y, color);
                fb.fill_span(cx - y, cx + y, cy + x, color);
                fb.fill_span(cx - y, cx + y, cy - x, color);
            } else if fill {
                for px in (cx - x)..=(cx + x) {
                    fb.raw_pset_fill(px, cy + y, color);
                    fb.raw_pset_fill(px, cy - y, color);
                }
                for px in (cx - y)..=(cx + y) {
                    fb.raw_pset_fill(px, cy + x, color);
                    fb.raw_pset_fill(px, cy - x, color);
                }
            } else {
                for (px, py) in [
                    (cx + x, cy + y),
                    (cx - x, cy + y),
                    (cx + x, cy - y),
                    (cx - x, cy - y),
                    (cx + y, cy + x),
                    (cx - y, cy + x),
                    (cx + y, cy - x),
                    (cx - y, cy - x),
                ] {
                    fb.raw_pset(px, py, color);
                }
            }
            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }

    #[test]
    fn circle_clipping_matches_the_unclipped_walk() {
        let circles = [
            ("centered", (64, 64, 30)),
            ("over the top-left", (-5, -8, 24)),
            ("over the bottom-right", (130, 120, 40)),
            ("entirely off screen", (400, 400, 50)),
            ("bigger than the screen", (64, 64, 300)),
            ("zero radius", (64, 64, 0)),
            ("negative radius", (64, 64, -9)),
        ];
        for pattern in [0u16, 0b1010_0101_1010_0101] {
            for fill in [false, true] {
                for (shape, (cx, cy, r)) in circles {
                    for state in CLIP_STATES {
                        let (state_name, mut got, mut want) = (
                            state.name,
                            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
                        );
                        got.set_fill_pattern(pattern, 12, false);
                        want.set_fill_pattern(pattern, 12, false);
                        if fill {
                            got.circfill(cx, cy, r, 7);
                        } else {
                            got.circ(cx, cy, r, 7);
                        }
                        circle_unclipped(&mut want, cx, cy, r, 7, fill);
                        assert_same_pixels(
                            &got,
                            &want,
                            &format!(
                                "{shape} / {state_name} / fill {fill} / pattern {pattern:#06x}"
                            ),
                        );
                    }
                }
            }
        }
    }

    /// Draw one circle both ways under a given camera, clip rect and fill pattern,
    /// and assert the results are pixel-identical.
    fn assert_circle_matches(
        cx: i32,
        cy: i32,
        r: i32,
        fill: bool,
        pattern: u16,
        state: &ClipState,
    ) {
        let (mut got, mut want) = (
            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
            staged_fb(state.camera, state.clip, &PEN_STATES[0]),
        );
        got.set_fill_pattern(pattern, 12, false);
        want.set_fill_pattern(pattern, 12, false);
        if fill {
            got.circfill(cx, cy, r, 7);
        } else {
            got.circ(cx, cy, r, 7);
        }
        circle_unclipped(&mut want, cx, cy, r, 7, fill);
        assert_same_pixels(
            &got,
            &want,
            &format!(
                "({cx}, {cy}) r {r} / {} / fill {fill} / pattern {pattern:#06x}",
                state.name
            ),
        );
    }

    #[test]
    fn circle_seeded_walk_matches_the_full_walk_at_every_small_radius() {
        // The clipped walk restarts the arc from `arc_x` rather than stepping it from
        // `y = 0`, so check the two agree at every radius up to a screen's worth, with
        // the centre inside the clip rect, on its corners and outside it.
        for r in 0..=130 {
            for (cx, cy) in [(64, 64), (0, 0), (-30, 70), (140, -12), (127, 127)] {
                for fill in [false, true] {
                    assert_circle_matches(cx, cy, r, fill, 0, &CLIP_STATES[0]);
                }
            }
        }
    }

    #[test]
    fn circle_seeded_walk_matches_the_full_walk_at_a_large_radius() {
        // Radii large enough that the runs the walk visits are a sliver of the arc,
        // but still small enough to step in full for comparison. The centres put each
        // family of octants across the screen in turn: the middle of the arc for the
        // shallow ones, and either pole for the steep ones, which is the case the
        // filled walk has to invert `arc_x` to find.
        for r in [9_001, 120_000] {
            for (cx, cy) in [
                (64, 64),
                (64, 64 + r),
                (64, 64 - r),
                (64 + r, 64),
                (64 - r, 64),
                (40 - r, 90 + r),
            ] {
                for fill in [false, true] {
                    for state in &CLIP_STATES {
                        // Solid only: the reference's patterned fill walks the whole
                        // disc a pixel at a time, quadratic in `r` at this size.
                        assert_circle_matches(cx, cy, r, fill, 0, state);
                    }
                }
            }
        }
    }

    #[test]
    fn circle_bounded_by_the_screen_not_the_radius() {
        // At this radius the full arc is 1.5e9 steps for the one fuel unit the host
        // call costs — three seconds of wall clock the meter never sees. Bounded by
        // the clip rect it is a few hundred, so the deadline is the assertion; the
        // geometry checks come along to show the right few hundred were walked.
        let r = 2_000_000_000;
        let filled = drawn_within_deadline("a circfill swallowing the screen", move |fb| {
            fb.circfill(64, 64, r, 7);
        });
        assert!(
            filled.pixels().iter().all(|&p| p == 7),
            "the screen is well inside the disc"
        );
        let outline = drawn_within_deadline("a circ swallowing the screen", move |fb| {
            fb.circ(64, 64, r, 7);
        });
        assert!(
            outline.pixels().iter().all(|&p| p == 0),
            "the ring runs a billion pixels off every edge"
        );

        // Centre far below the screen, so the top of the circle just touches it: here
        // it is the steep octants that cross the clip rect.
        let capped = drawn_within_deadline("a circfill touching the screen at a pole", move |fb| {
            fb.circfill(64, 64 + r, r, 7);
        });
        assert_eq!(capped.pget(64, 64), 7, "the top of the disc is on screen");
        assert!(
            capped.pixels()[..(64 * WIDTH) as usize]
                .iter()
                .all(|&p| p == 0),
            "nothing above the disc is touched"
        );
        assert!(
            capped.pixels()[(127 * WIDTH) as usize..]
                .iter()
                .all(|&p| p == 7),
            "the bottom row is deep inside the disc"
        );
        let arc = drawn_within_deadline("a circ touching the screen at a pole", move |fb| {
            fb.circ(64, 64 + r, r, 7);
        });
        assert_eq!(arc.pget(64, 64), 7, "the top of the ring is on screen");
        assert!(
            arc.pixels()[..(64 * WIDTH) as usize]
                .iter()
                .all(|&p| p == 0),
            "nothing above the ring is touched"
        );
    }

    #[test]
    fn circle_that_cannot_reach_the_clip_rect_is_dropped() {
        // The radius is the cart's to pick. A circle whose bounding box misses the
        // clip rect must be rejected outright instead of walking an arc
        // proportional to it (and overflowing the coordinates on the way).
        let mut fb = Framebuffer::new();
        fb.circ(2_000_000_000, 0, 1_000_000_000, 7);
        fb.circfill(0, -2_000_000_000, 1_000_000_000, 7);
        assert!(
            fb.pixels().iter().all(|&p| p == 0),
            "nothing on screen was touched"
        );
    }

    #[test]
    fn print_pen_matches_print_at_cursor() {
        let mut a = Framebuffer::new();
        let mut b = Framebuffer::new();
        a.set_pen_color(9);
        a.set_cursor(10, 20);
        let end_a = a.print_pen("hi");
        let end_b = b.print("hi", 10, 20, 9);
        assert_eq!(end_a, end_b);
        assert_eq!(
            a.pixels(),
            b.pixels(),
            "print_pen draws identically to print"
        );
    }

    #[test]
    fn print_pen_advances_cursor_one_line() {
        let mut fb = Framebuffer::new();
        fb.set_cursor(5, 5);
        fb.print_pen("x");
        fb.print_pen("y");
        let mut expect = Framebuffer::new();
        expect.print("x", 5, 5, 6); // default pen color is 6
        expect.print("y", 5, 5 + font::GLYPH_H, 6);
        assert_eq!(fb.pixels(), expect.pixels());
    }
}
