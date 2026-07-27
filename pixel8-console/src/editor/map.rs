//! The map editor: a scrollable tile viewport with six PICO-8-style tools
//! (draw, paste, select, pan, fill, circle), a clipboard, and a full-page
//! sprite sheet picker along the bottom.

use super::history::History;
use crate::{
    shell::{Key, Mods},
    ui::{self, draw_icon8, Icon8, Mouse, ICON_PENCIL},
};
use pixel8_runtime::{
    assets::{Assets, MapData, MAP_H, MAP_W, SPRITES_PER_ROW, SPRITE_COUNT},
    clipboard::{self, ClipboardPayload, Pasted},
    fb::Framebuffer,
    palette::col,
};

// --- Layout (see the design spec) -------------------------------------------
// Eight map tiles + a 4-row sheet leave 4 px of slack; it is absorbed into the
// toolbar (same dark-grey fill) so the sheet sits flush against the status bar
// and the separator sits flush against the sheet — no stray band anywhere.
const TOOLBAR_Y: i32 = 8; // tool chrome fills y 8..=21; icons drawn at y 9.
const VIEW_Y: i32 = 22;
const VIEW_TILES_X: i32 = 16;
const VIEW_TILES_Y: i32 = 8;
const SEP_Y: i32 = 86;
const SHEET_Y: i32 = 88; // the sprite strip, flush with the status bar.
const SIZE_BTNS: (i32, i32) = (57, TOOLBAR_Y); // four "1/2/4/8" block-size buttons
const BRUSH_X: i32 = 92; // 8×8 brush preview
const PAGE_X: i32 = 104; // four 5×5 page dots
const PAGE_Y: i32 = 11;

/// Selectable brush sizes, in sprites per side — the same 1x1/2x2/4x4/8x8 blocks
/// the sprite editor draws, so a block drawn there goes down as one stamp here.
/// Only the Draw tool stamps the whole block; the area tools (fill, circle) paint
/// single cells with its top-left sprite.
const SIZES: [i32; 4] = [1, 2, 4, 8];

/// Rows of sprites on the sheet (16), the limit a brush block must stay within.
const SHEET_ROWS: i32 = (SPRITE_COUNT / SPRITES_PER_ROW) as i32;

fn tool_x(i: usize) -> i32 {
    2 + i as i32 * 9
}

// --- Tool icons (Draw reuses the shared pencil; rest sampled from PICO-8) ----
const ICON_STAMP: Icon8 = [0x38, 0x38, 0x38, 0x38, 0xFE, 0x82, 0xFE, 0x00]; // paste
const ICON_SELECT: Icon8 = [0xAA, 0x00, 0x82, 0x00, 0x82, 0x00, 0xAA, 0x00];
const ICON_HAND: Icon8 = [0x28, 0x2A, 0x2A, 0x3E, 0xBE, 0x7E, 0x1C, 0x00]; // pan
const ICON_FILL: Icon8 = [0x08, 0x04, 0x02, 0x7F, 0xBE, 0x9C, 0x88, 0x00]; // bucket
const ICON_CIRCLE: Icon8 = [0x38, 0x44, 0x82, 0x82, 0x82, 0x44, 0x38, 0x00];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Draw,
    Paste,
    Select,
    Pan,
    Fill,
    Circle,
}

// Icons stored by value (`Icon8` is `[u8; 8]`, `Copy`), matching the sprite
// editor's `tools` array, so iterating yields `icon: &Icon8` for `draw_icon8`.
const TOOLS: [(Tool, Icon8); 6] = [
    (Tool::Draw, ICON_PENCIL),
    (Tool::Paste, ICON_STAMP),
    (Tool::Select, ICON_SELECT),
    (Tool::Pan, ICON_HAND),
    (Tool::Fill, ICON_FILL),
    (Tool::Circle, ICON_CIRCLE),
];

/// A rectangular cell selection, in map space (it scrolls with the camera).
#[derive(Debug, Clone, Copy)]
struct Selection {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// A copied block of tiles, row-major.
#[derive(Debug, Clone)]
struct Clipboard {
    w: i32,
    h: i32,
    tiles: Vec<u8>,
}

/// The in-progress mouse drag, if any.
#[derive(Debug, Clone)]
enum Drag {
    None,
    Selecting {
        ax: i32,
        ay: i32,
    },
    Moving {
        tiles: Vec<u8>,
        w: i32,
        h: i32,
        gx: i32,
        gy: i32,
    },
    Panning {
        amx: i32,
        amy: i32,
        acx: i32,
        acy: i32,
    },
    /// Fill-rect and circle previews.
    Shape {
        ax: i32,
        ay: i32,
    },
}

pub struct MapEditor {
    tool: Tool,
    fullscreen: bool,
    /// The brush's top-left sprite. With [`size`](Self::size) above 1 it is the
    /// corner of a block, not the whole brush.
    brush: u32,
    /// Brush size in sprites per side (one of [`SIZES`]).
    size: i32,
    cam_x: i32,
    cam_y: i32,
    page: u32,
    frame: u32,
    /// Last-known cursor position in screen pixels (for hover box / previews).
    mx: i32,
    my: i32,
    sel: Option<Selection>,
    clip: Option<Clipboard>,
    drag: Drag,
    /// Undo/redo of the tile map (last 10 edits).
    history: History<MapData>,
    status: ui::StatusMsg,
}

impl MapEditor {
    pub fn new() -> Self {
        Self {
            tool: Tool::Draw,
            fullscreen: false,
            brush: 1,
            size: 1,
            cam_x: 0,
            cam_y: 0,
            page: 0,
            frame: 0,
            mx: -16,
            my: -16,
            sel: None,
            clip: None,
            drag: Drag::None,
            history: History::new(),
            status: ui::StatusMsg::default(),
        }
    }

    /// Switch the active tool, abandoning any in-progress drag.
    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.drag = Drag::None;
    }

    /// The sheet cell (column, row) of the brush's top-left sprite.
    fn brush_cell(&self) -> (i32, i32) {
        let n = self.brush as i32;
        (n % SPRITES_PER_ROW as i32, n / SPRITES_PER_ROW as i32)
    }

    /// Keep the whole block on the sheet: one picked near the right or bottom
    /// edge slides back until it fits, as the sprite editor's window does.
    fn clamp_brush(&mut self) {
        let (col, row) = self.brush_cell();
        let col = col.min(SPRITES_PER_ROW as i32 - self.size);
        let row = row.min(SHEET_ROWS - self.size);
        self.brush = (row * SPRITES_PER_ROW as i32 + col) as u32;
    }

    /// The strip along the bottom, sized and positioned for the brush block.
    fn strip(&self) -> ui::Strip {
        ui::Strip::new(self.size, self.brush_cell().1, self.page)
    }

    /// The sprite at offset `(dx, dy)` within the brush block.
    fn brush_sprite(&self, dx: i32, dy: i32) -> u8 {
        (self.brush as i32 + dy * SPRITES_PER_ROW as i32 + dx) as u8
    }

    /// Lay the whole block down with its top-left at map cell `(cx, cy)`.
    fn stamp_brush(&self, assets: &mut Assets, cx: i32, cy: i32) {
        for dy in 0..self.size {
            for dx in 0..self.size {
                assets.map.set(cx + dx, cy + dy, self.brush_sprite(dx, dy));
            }
        }
    }

    /// Whether the fullscreen (bare-map) view is active.
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Top y of the map viewport for the active view.
    fn view_y(&self) -> i32 {
        if self.fullscreen {
            8
        } else {
            VIEW_Y
        }
    }

    /// Visible tile rows for the active view.
    fn view_tiles_y(&self) -> i32 {
        if self.fullscreen {
            14
        } else {
            VIEW_TILES_Y
        }
    }

    /// Bottom y (inclusive) of the map viewport for the active view.
    fn view_bottom(&self) -> i32 {
        self.view_y() + self.view_tiles_y() * 8 - 1
    }

    /// Switch the view, re-clamping the camera so the (taller) fullscreen view
    /// cannot scroll past the map's edges.
    fn set_fullscreen(&mut self, on: bool) {
        self.fullscreen = on;
        self.cam_x = self.cam_x.min(MAP_W as i32 - VIEW_TILES_X).max(0);
        self.cam_y = self.cam_y.min(MAP_H as i32 - self.view_tiles_y()).max(0);
    }

    /// Abandon any in-progress drag. The shell calls this on an editor switch,
    /// so a drag left mid-gesture cannot commit a stale selection or move when
    /// the map editor regains focus with the button already released.
    pub fn cancel_drag(&mut self) {
        self.drag = Drag::None;
    }

    pub fn key(&mut self, key: Key, mods: Mods, assets: &mut Assets) {
        if mods.ctrl {
            if let Key::Char(c) = key {
                match c.to_ascii_lowercase() {
                    'z' if mods.shift => {
                        self.history.redo(&mut assets.map);
                        return;
                    }
                    'z' => {
                        self.history.undo(&mut assets.map);
                        return;
                    }
                    'y' => {
                        self.history.redo(&mut assets.map);
                        return;
                    }
                    _ => {}
                }
            }
        }
        // A key edit (cut / delete) is a self-contained gesture: snapshot before
        // and commit after, so it lands as a single undo step. The commit
        // compares, so non-editing keys (tool switches, camera moves) record
        // nothing.
        self.history.begin(&assets.map);
        match key {
            Key::Left => self.cam_x = (self.cam_x - 1).max(0),
            Key::Right => self.cam_x = (self.cam_x + 1).min(MAP_W as i32 - VIEW_TILES_X),
            Key::Up => self.cam_y = (self.cam_y - 1).max(0),
            Key::Down => self.cam_y = (self.cam_y + 1).min(MAP_H as i32 - self.view_tiles_y()),
            Key::PageUp => self.page = (self.page + 3) % 4,
            Key::PageDown => self.page = (self.page + 1) % 4,
            Key::Delete | Key::Backspace => self.delete_selection(assets),
            Key::Char('d') => self.set_tool(Tool::Draw),
            Key::Char('t') => self.set_tool(Tool::Paste),
            Key::Char('s') => self.set_tool(Tool::Select),
            Key::Char('h') => self.set_tool(Tool::Pan),
            Key::Char('f') => self.set_tool(Tool::Fill),
            Key::Char('c') => self.set_tool(Tool::Circle),
            Key::Tab => self.set_fullscreen(!self.fullscreen),
            _ => {}
        }
        self.history.commit(&assets.map);
    }

    pub fn tick(&mut self, mouse: &Mouse, assets: &mut Assets) {
        self.frame = self.frame.wrapping_add(1);
        self.status.tick();
        self.mx = mouse.x;
        self.my = mouse.y;

        // Bracket each mouse gesture for undo. Some tools (fill, circle, move)
        // only mutate the map on the release frame, so the snapshot is taken
        // while the button is held and committed once it is released — at the
        // end of tick, after that release-frame mutation has run.
        let down = mouse.left || mouse.right;
        if down {
            self.history.begin(&assets.map);
        }

        // Toolbar / sheet / page-dot clicks take priority over map drags.
        if mouse.left_pressed && self.handle_chrome_click(mouse) {
            return;
        }

        // Per-tool map interaction.
        match self.tool {
            Tool::Draw => {
                if mouse.left {
                    if let Some((cx, cy)) = self.hovered_cell() {
                        self.stamp_brush(assets, cx, cy);
                    }
                }
            }
            Tool::Paste => {
                if mouse.left_pressed {
                    if let Some((cx, cy)) = self.hovered_cell() {
                        self.paste_at(assets, cx, cy);
                    }
                }
            }
            Tool::Select => {
                if mouse.left_pressed && mouse.y >= self.view_y() && mouse.y <= self.view_bottom() {
                    let (cx, cy) = self.clamped_cell();
                    if self.point_in_selection(cx, cy) {
                        let s = self.sel.unwrap();
                        let mut tiles = Vec::with_capacity((s.w * s.h) as usize);
                        for y in 0..s.h {
                            for x in 0..s.w {
                                tiles.push(assets.map.get(s.x + x, s.y + y));
                            }
                        }
                        self.drag = Drag::Moving {
                            tiles,
                            w: s.w,
                            h: s.h,
                            gx: cx,
                            gy: cy,
                        };
                    } else {
                        self.drag = Drag::Selecting { ax: cx, ay: cy };
                    }
                }
                if !mouse.left {
                    match std::mem::replace(&mut self.drag, Drag::None) {
                        Drag::Selecting { ax, ay } => {
                            let (cx, cy) = self.clamped_cell();
                            let (x, y, w, h) = normalize_rect(ax, ay, cx, cy);
                            self.sel = Some(Selection { x, y, w, h });
                        }
                        Drag::Moving {
                            tiles,
                            w,
                            h,
                            gx,
                            gy,
                        } => {
                            let s = self.sel.unwrap();
                            let (cx, cy) = self.clamped_cell();
                            let (nx, ny) = (s.x + (cx - gx), s.y + (cy - gy));
                            for y in 0..h {
                                for x in 0..w {
                                    assets.map.set(s.x + x, s.y + y, 0);
                                }
                            }
                            for y in 0..h {
                                for x in 0..w {
                                    let (dx, dy) = (nx + x, ny + y);
                                    if (0..MAP_W as i32).contains(&dx)
                                        && (0..MAP_H as i32).contains(&dy)
                                    {
                                        assets.map.set(dx, dy, tiles[(y * w + x) as usize]);
                                    }
                                }
                            }
                            self.sel = Some(Selection { x: nx, y: ny, w, h });
                        }
                        // A non-Select drag (e.g. a stale Shape/Pan) is put back untouched.
                        other => self.drag = other,
                    }
                }
            }
            Tool::Fill => {
                if mouse.left_pressed && mouse.y >= self.view_y() && mouse.y <= self.view_bottom() {
                    let (cx, cy) = self.clamped_cell();
                    self.drag = Drag::Shape { ax: cx, ay: cy };
                }
                if !mouse.left {
                    if let Drag::Shape { ax, ay } = std::mem::replace(&mut self.drag, Drag::None) {
                        let (cx, cy) = self.clamped_cell();
                        if (cx, cy) == (ax, ay) {
                            self.flood_fill(assets, ax, ay);
                        } else {
                            let (x, y, w, h) = normalize_rect(ax, ay, cx, cy);
                            for yy in y..y + h {
                                for xx in x..x + w {
                                    assets.map.set(xx, yy, self.brush as u8);
                                }
                            }
                        }
                    }
                }
            }
            Tool::Circle => {
                if mouse.left_pressed && mouse.y >= self.view_y() && mouse.y <= self.view_bottom() {
                    let (cx, cy) = self.clamped_cell();
                    self.drag = Drag::Shape { ax: cx, ay: cy };
                }
                if !mouse.left {
                    if let Drag::Shape { ax, ay } = std::mem::replace(&mut self.drag, Drag::None) {
                        let (cx, cy) = self.clamped_cell();
                        let (x, y, w, h) = normalize_rect(ax, ay, cx, cy);
                        self.stamp_ellipse(assets, x, y, w, h);
                    }
                }
            }
            Tool::Pan => {
                if mouse.left_pressed {
                    self.drag = Drag::Panning {
                        amx: mouse.x,
                        amy: mouse.y,
                        acx: self.cam_x,
                        acy: self.cam_y,
                    };
                }
                if let Drag::Panning { amx, amy, acx, acy } = self.drag {
                    if mouse.left {
                        self.cam_x =
                            (acx + (amx - mouse.x) / 8).clamp(0, MAP_W as i32 - VIEW_TILES_X);
                        self.cam_y = (acy + (amy - mouse.y) / 8)
                            .clamp(0, MAP_H as i32 - self.view_tiles_y());
                    } else {
                        self.drag = Drag::None;
                    }
                }
            }
        }

        // Close the gesture once the button is released, after any release-frame
        // mutation above. The commit compares, so pans and empty marquees (which
        // leave the map untouched) record nothing.
        if !down {
            self.history.commit(&assets.map);
        }
    }

    /// Handle a click on the toolbar, page dots, or sprite sheet. Returns true
    /// if the click was consumed.
    fn handle_chrome_click(&mut self, m: &Mouse) -> bool {
        // View-toggle buttons (top-left), available in both views.
        if m.over(4, 0, 12, 7) {
            self.set_fullscreen(false);
            return true;
        } else if m.over(13, 0, 22, 7) {
            self.set_fullscreen(true);
            return true;
        }
        // Fullscreen hides the toolbar, page dots and sheet, so nothing else in
        // the chrome is clickable there.
        if self.fullscreen {
            return false;
        }
        // Tool icons.
        for (i, (tool, _)) in TOOLS.iter().enumerate() {
            let x = tool_x(i);
            if m.over(x, TOOLBAR_Y + 1, x + 7, TOOLBAR_Y + 8) {
                self.set_tool(*tool);
                return true;
            }
        }
        // Block-size buttons.
        for (i, &n) in SIZES.iter().enumerate() {
            let x = SIZE_BTNS.0 + i as i32 * 8;
            if m.over(x, SIZE_BTNS.1, x + 6, SIZE_BTNS.1 + 7) {
                self.size = n;
                self.clamp_brush(); // a bigger block may not fit at the old corner.
                return true;
            }
        }
        // Page dots. A block big enough that the strip follows it rather than
        // the page would leave them inert, so they move the brush instead.
        for p in 0..4 {
            let x = PAGE_X + p * 6;
            if m.over(x, PAGE_Y, x + 4, PAGE_Y + 4) {
                self.page = p as u32;
                if ui::Strip::follows_block(self.size) {
                    self.brush = p as u32 * 64;
                    self.clamp_brush();
                }
                return true;
            }
        }
        // Sprite sheet: pick the brush. The whole band belongs to the strip,
        // including the margins half-size cells leave at its sides, so a click
        // there is never taken for a map gesture.
        if m.over(0, SHEET_Y, 127, SHEET_Y + ui::STRIP_H - 1) {
            if let Some((cx, cy)) = self.strip().cell_at(SHEET_Y, m.x, m.y) {
                self.brush = (cy * SPRITES_PER_ROW as i32 + cx) as u32;
                self.clamp_brush();
            }
            return true;
        }
        false
    }

    pub fn draw(&self, fb: &mut Framebuffer, assets: &Assets) {
        if !self.fullscreen {
            self.draw_toolbar(fb, assets);
        }
        self.draw_view(fb, assets);
        self.draw_selection(fb);
        self.draw_shape_preview(fb);
        if !self.fullscreen {
            fb.rectfill(0, SEP_Y, 127, SEP_Y + 1, col::LIGHT_GREY);
            self.draw_sheet(fb, assets);
        }
        self.draw_status(fb, assets);
    }

    fn draw_toolbar(&self, fb: &mut Framebuffer, assets: &Assets) {
        fb.rectfill(0, TOOLBAR_Y, 127, VIEW_Y - 1, col::DARK_GREY);
        for (i, (tool, icon)) in TOOLS.iter().enumerate() {
            let x = tool_x(i);
            let color = if *tool == self.tool {
                fb.rectfill(x - 1, TOOLBAR_Y, x + 8, TOOLBAR_Y + 8, col::BLACK);
                col::WHITE
            } else {
                col::LAVENDER
            };
            draw_icon8(fb, icon, x, TOOLBAR_Y + 1, color);
        }
        // Block-size buttons ("1/2/4/8", one per entry in SIZES), laid out like
        // the sprite editor's: a 3x5 digit centred in a 9x9 highlight box.
        for (i, &n) in SIZES.iter().enumerate() {
            let x = SIZE_BTNS.0 + i as i32 * 8;
            let color = if n == self.size {
                fb.rectfill(x - 1, SIZE_BTNS.1, x + 7, SIZE_BTNS.1 + 8, col::BLACK);
                col::WHITE
            } else {
                col::LAVENDER
            };
            fb.print(&n.to_string(), x + 2, SIZE_BTNS.1 + 2, color);
        }
        // Brush preview: the whole block squeezed into the 8x8 box, so a bigger
        // brush shows what it stamps rather than just its corner sprite.
        let (ox, oy) = sprite_origin(self.brush);
        for py in 0..8 {
            for px in 0..8 {
                fb.pset(
                    BRUSH_X + px,
                    TOOLBAR_Y + 1 + py,
                    assets.sprites.get(ox + px * self.size, oy + py * self.size),
                );
            }
        }
        fb.rect(
            BRUSH_X - 1,
            TOOLBAR_Y,
            BRUSH_X + 8,
            TOOLBAR_Y + 9,
            col::BLACK,
        );
        // Page dots.
        for p in 0..4u32 {
            let x = PAGE_X + p as i32 * 6;
            let c = if p == self.page {
                col::WHITE
            } else {
                col::LAVENDER
            };
            fb.rectfill(x, PAGE_Y, x + 4, PAGE_Y + 4, c);
        }
    }

    fn draw_view(&self, fb: &mut Framebuffer, assets: &Assets) {
        fb.rectfill(0, self.view_y(), 127, self.view_bottom(), col::BLACK);
        fb.map(
            &assets.map,
            &assets.sprites,
            self.cam_x,
            self.cam_y,
            0,
            self.view_y(),
            VIEW_TILES_X,
            self.view_tiles_y(),
            0,
        );
        if let Some((cx, cy)) = self.hovered_cell() {
            let sx = (cx - self.cam_x) * 8;
            let sy = self.view_y() + (cy - self.cam_y) * 8;
            // The box outlines what a click would stamp, so a big brush hovered
            // low down overhangs the viewport; clip it there, as the selection
            // marquee is clipped.
            let (view_y, tiles_y) = (self.view_y(), self.view_tiles_y());
            fb.clip(0, view_y, 128, tiles_y * 8);
            let side = self.size * 8;
            fb.rect(sx, sy, sx + side - 1, sy + side - 1, col::WHITE);
            fb.clip_reset();
        }
    }

    fn draw_selection(&self, fb: &mut Framebuffer) {
        let rect = self
            .selection_in_progress()
            .or_else(|| self.sel.map(|s| (s.x, s.y, s.w, s.h)));
        let Some((x, y, w, h)) = rect else { return };
        let (view_y, view_bottom) = (self.view_y(), self.view_bottom());
        let sx = (x - self.cam_x) * 8;
        let sy = view_y + (y - self.cam_y) * 8;
        let (x0, y0, x1, y1) = (sx, sy, sx + w * 8 - 1, sy + h * 8 - 1);
        let mut i = 0i32;
        let ant = |fb: &mut Framebuffer, px: i32, py: i32, i: &mut i32| {
            if (0..=127).contains(&px) && (view_y..=view_bottom).contains(&py) {
                let on = ((*i + (self.frame / 4) as i32) / 2) % 2 == 0;
                fb.pset(px, py, if on { col::WHITE } else { col::BLACK });
            }
            *i += 1;
        };
        for px in x0..=x1 {
            ant(fb, px, y0, &mut i);
        }
        for py in (y0 + 1)..=y1 {
            ant(fb, x1, py, &mut i);
        }
        for px in (x0..x1).rev() {
            ant(fb, px, y1, &mut i);
        }
        for py in ((y0 + 1)..y1).rev() {
            ant(fb, x0, py, &mut i);
        }
    }

    fn draw_sheet(&self, fb: &mut Framebuffer, assets: &Assets) {
        let strip = self.strip();
        strip.draw(fb, &assets.sprites, SHEET_Y);
        let (col, row) = self.brush_cell();
        strip.draw_block(fb, SHEET_Y, col, row, self.size);
    }

    fn draw_status(&self, fb: &mut Framebuffer, assets: &Assets) {
        let brush = self.brush_label();
        let text = if let Some((_, _, w, h)) = self.selection_in_progress() {
            format!("Sel {}x{}        {} pg{}", w, h, brush, self.page)
        } else if let Some(tool) = self.tool_under_cursor() {
            tool_label(tool).to_string()
        } else {
            match self.hovered_cell() {
                Some((cx, cy)) => format!(
                    "x{:03} y{:03} t{:03} {} pg{}",
                    cx,
                    cy,
                    assets.map.get(cx, cy),
                    brush,
                    self.page
                ),
                None => format!("{} pg{}", brush, self.page),
            }
        };
        self.status.show(fb, &text);
    }

    /// The brush's status-bar label: its sprite number, and the block size too
    /// once the brush covers more than one tile.
    fn brush_label(&self) -> String {
        if self.size == 1 {
            format!("b{:03}", self.brush)
        } else {
            format!("b{:03}x{}", self.brush, self.size)
        }
    }

    /// The tool whose toolbar icon is under the cursor, if any.
    fn tool_under_cursor(&self) -> Option<Tool> {
        // The toolbar is hidden in fullscreen, so no tool can be under the cursor.
        if self.fullscreen {
            return None;
        }
        TOOLS.iter().enumerate().find_map(|(i, (tool, _))| {
            let x = tool_x(i);
            let over =
                self.mx >= x && self.mx <= x + 7 && self.my > TOOLBAR_Y && self.my <= TOOLBAR_Y + 8;
            over.then_some(*tool)
        })
    }

    /// The cursor's map cell, clamped to the visible view (for drags).
    fn clamped_cell(&self) -> (i32, i32) {
        let mx = self.mx.clamp(0, 127);
        let my = self.my.clamp(self.view_y(), self.view_bottom());
        (self.cam_x + mx / 8, self.cam_y + (my - self.view_y()) / 8)
    }

    /// The rectangle (x, y, w, h) of an in-progress selection drag, if any.
    fn selection_in_progress(&self) -> Option<(i32, i32, i32, i32)> {
        if let Drag::Selecting { ax, ay } = self.drag {
            let (cx, cy) = self.clamped_cell();
            Some(normalize_rect(ax, ay, cx, cy))
        } else {
            None
        }
    }

    /// Copy (or cut) the current selection into the paste buffer and return it as a
    /// native clipboard blob (for the shell to put on the system clipboard).
    pub fn copy_selection(&mut self, assets: &mut Assets, cut: bool) -> Option<String> {
        let s = self.sel?;
        let mut tiles = Vec::with_capacity((s.w * s.h) as usize);
        for y in 0..s.h {
            for x in 0..s.w {
                tiles.push(assets.map.get(s.x + x, s.y + y));
            }
        }
        self.clip = Some(Clipboard {
            w: s.w,
            h: s.h,
            tiles: tiles.clone(),
        });
        if cut {
            // Cut clears the source cells but keeps the selection rectangle.
            for y in 0..s.h {
                for x in 0..s.w {
                    assets.map.set(s.x + x, s.y + y, 0);
                }
            }
        }
        let verb = if cut { "cut" } else { "copied" };
        self.status.set(format!("{verb} {}x{} tiles", s.w, s.h));
        Some(clipboard::encode(&ClipboardPayload::Map {
            w: s.w as u8,
            h: s.h as u8,
            tiles,
        }))
    }

    /// Clear the current selection to tile 0 without touching the clipboard. The
    /// selection itself is kept, so it can be re-filled, moved, or pasted over.
    fn delete_selection(&self, assets: &mut Assets) {
        let Some(s) = self.sel else { return };
        for y in 0..s.h {
            for x in 0..s.w {
                assets.map.set(s.x + x, s.y + y, 0);
            }
        }
    }

    /// Stamp the clipboard at map cell (cx, cy).
    fn paste_at(&mut self, assets: &mut Assets, cx: i32, cy: i32) {
        let Some(clip) = self.clip.clone() else {
            return;
        };
        for y in 0..clip.h {
            for x in 0..clip.w {
                let (dx, dy) = (cx + x, cy + y);
                if (0..MAP_W as i32).contains(&dx) && (0..MAP_H as i32).contains(&dy) {
                    assets
                        .map
                        .set(dx, dy, clip.tiles[(y * clip.w + x) as usize]);
                }
            }
        }
        self.status
            .set(format!("pasted {}x{} tiles", clip.w, clip.h));
    }

    /// Set a transient bottom-bar message (used for clipboard errors).
    pub fn set_status(&mut self, msg: String) {
        self.status.set(msg);
    }

    /// Load a decoded clipboard blob into the paste buffer and arm the Paste tool.
    /// Only `Map` blobs apply here; other kinds set a hint.
    pub fn paste(&mut self, pasted: &Pasted) {
        match pasted {
            Pasted::Map { w, h, tiles } => self.begin_paste(*w, *h, tiles.clone()),
            Pasted::Sprites { .. } => self.status.set("sprites - use sprite editor".into()),
            Pasted::Sfx(_) => self.status.set("sfx - use sfx editor".into()),
        }
    }

    /// Fill the paste buffer with an external tile block and switch to the Paste
    /// tool so the user can stamp it.
    fn begin_paste(&mut self, w: usize, h: usize, tiles: Vec<u8>) {
        self.clip = Some(Clipboard {
            w: w as i32,
            h: h as i32,
            tiles,
        });
        self.set_tool(Tool::Paste);
    }

    /// Whether a tile block is loaded in the paste buffer (used by shell tests,
    /// which cannot reach the private `clip` field from another module).
    #[cfg(test)]
    pub(crate) fn has_paste_buffer(&self) -> bool {
        self.clip.is_some()
    }

    /// Whether map cell (cx, cy) is inside the current selection.
    fn point_in_selection(&self, cx: i32, cy: i32) -> bool {
        match self.sel {
            Some(s) => cx >= s.x && cx < s.x + s.w && cy >= s.y && cy < s.y + s.h,
            None => false,
        }
    }

    /// Flood-fill from (sx, sy) replacing all contiguous same-tile cells with the brush.
    fn flood_fill(&self, assets: &mut Assets, sx: i32, sy: i32) {
        let target = assets.map.get(sx, sy);
        let brush = self.brush as u8;
        if target == brush {
            return;
        }
        let mut stack = vec![(sx, sy)];
        while let Some((x, y)) = stack.pop() {
            if !(0..MAP_W as i32).contains(&x) || !(0..MAP_H as i32).contains(&y) {
                continue;
            }
            if assets.map.get(x, y) != target {
                continue;
            }
            assets.map.set(x, y, brush);
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
    }

    /// Stamp the ellipse outline inscribed in the `w × h` bounding box at `(x, y)`.
    fn stamp_ellipse(&self, assets: &mut Assets, x: i32, y: i32, w: i32, h: i32) {
        let brush = self.brush as u8;
        // Centre and radii reach the cell centres on the box edges, so the
        // outline touches the bounding box (radius (n-1)/2 for an n-cell span).
        let (cx, cy) = (
            x as f32 + (w - 1) as f32 / 2.0,
            y as f32 + (h - 1) as f32 / 2.0,
        );
        let (rx, ry) = ((w - 1) as f32 / 2.0, (h - 1) as f32 / 2.0);
        // Walk the perimeter by angle; overlapping samples just re-write a cell.
        let steps = ((w + h) * 4).max(16);
        for s in 0..steps {
            let t = std::f32::consts::TAU * s as f32 / steps as f32;
            let px = (cx + rx * t.cos()).round() as i32;
            let py = (cy + ry * t.sin()).round() as i32;
            if (0..MAP_W as i32).contains(&px) && (0..MAP_H as i32).contains(&py) {
                assets.map.set(px, py, brush);
            }
        }
    }

    /// Draw a rectangle outline preview while dragging the Fill (or Circle) tool.
    fn draw_shape_preview(&self, fb: &mut Framebuffer) {
        let Drag::Shape { ax, ay } = self.drag else {
            return;
        };
        let (cx, cy) = self.clamped_cell();
        let (x, y, w, h) = normalize_rect(ax, ay, cx, cy);
        let sx = (x - self.cam_x) * 8;
        let sy = self.view_y() + (y - self.cam_y) * 8;
        fb.rect(sx, sy, sx + w * 8 - 1, sy + h * 8 - 1, col::WHITE);
    }

    /// The map cell under the cursor, if the cursor is over the view.
    fn hovered_cell(&self) -> Option<(i32, i32)> {
        if self.mx < 0 || self.mx > 127 || self.my < self.view_y() || self.my > self.view_bottom() {
            return None;
        }
        Some((
            self.cam_x + self.mx / 8,
            self.cam_y + (self.my - self.view_y()) / 8,
        ))
    }
}

fn sprite_origin(n: u32) -> (i32, i32) {
    (
        (n as i32 % SPRITES_PER_ROW as i32) * 8,
        (n as i32 / SPRITES_PER_ROW as i32) * 8,
    )
}

/// The tool's display name and keyboard shortcut, shown in the status bar.
fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Draw => "Draw (d)",
        Tool::Paste => "Paste (t)",
        Tool::Select => "Select (s)",
        Tool::Pan => "Pan (h)",
        Tool::Fill => "Fill (f)",
        Tool::Circle => "Circle (c)",
    }
}

/// Two corner cells into a top-left origin and inclusive (w, h).
fn normalize_rect(ax: i32, ay: i32, bx: i32, by: i32) -> (i32, i32, i32, i32) {
    let x = ax.min(bx);
    let y = ay.min(by);
    (x, y, (ax - bx).abs() + 1, (ay - by).abs() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixel8_runtime::assets::Assets;

    fn press(x: i32, y: i32) -> Mouse {
        Mouse {
            x,
            y,
            left: true,
            left_pressed: true,
            ..Default::default()
        }
    }

    #[test]
    fn inactive_tool_icon_is_lavender() {
        // Tool 0 (Draw) is active; tool 1 (Paste/stamp) is inactive. The first
        // row of ICON_STAMP is 0x38 = 0b00111000, which lights rx=2,3,4. Assert
        // that the pixel at (tool_x(1)+2, TOOLBAR_Y+1) — a lit icon pixel — is
        // lavender on the dark-grey toolbar.
        let ed = MapEditor::new();
        let a = Assets::default();
        let mut fb = Framebuffer::new();
        ed.draw(&mut fb, &a);
        assert_eq!(fb.pget(tool_x(1) + 2, TOOLBAR_Y + 1), col::LAVENDER);
    }

    #[test]
    fn clicking_a_tool_slot_selects_it() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        // Slot 2 is Select (0=Draw,1=Paste,2=Select).
        ed.tick(&press(tool_x(2) + 1, 10), &mut a);
        assert_eq!(ed.tool, Tool::Select);
    }

    #[test]
    fn clicking_a_page_dot_sets_the_page() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tick(&press(PAGE_X + 2 * 6 + 1, PAGE_Y + 1), &mut a);
        assert_eq!(ed.page, 2);
    }

    #[test]
    fn clicking_the_sheet_sets_the_brush() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.page = 1; // page 1 starts at sprite 64.
                     // Second row, third column of the strip -> 64 + 1*16 + 2 = 82.
        ed.tick(&press(2 * 8 + 1, SHEET_Y + 8 + 1), &mut a);
        assert_eq!(ed.brush, 82);
    }

    #[test]
    fn clicking_a_size_button_sets_the_brush_size() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        assert_eq!(ed.size, 1);
        // Button index 3 -> size 8, at x = SIZE_BTNS.0 + 3*8.
        ed.tick(&press(SIZE_BTNS.0 + 3 * 8 + 1, SIZE_BTNS.1 + 1), &mut a);
        assert_eq!(ed.size, 8);
    }

    #[test]
    fn draw_tool_stamps_the_whole_block() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.brush = 7;
        ed.size = 2;
        // Hover cell (1, 0): the block's top-left, with 8 to its right and the
        // next sheet row (23, 24) below.
        ed.tick(&press(9, VIEW_Y + 1), &mut a);
        assert_eq!(
            [
                a.map.get(1, 0),
                a.map.get(2, 0),
                a.map.get(1, 1),
                a.map.get(2, 1)
            ],
            [7, 8, 23, 24]
        );
    }

    #[test]
    fn picking_a_big_brush_keeps_the_block_on_the_sheet() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.size = 8;
        ed.brush = 0;
        // At 8x the strip is half-size and centred, so this is column 12 of row
        // 3 — a corner an 8x8 block runs off, sliding back to column 8.
        ed.tick(&press(32 + 12 * 4 + 1, SHEET_Y + 3 * 4 + 1), &mut a);
        assert_eq!(ed.brush, 3 * 16 + 8);
    }

    #[test]
    fn a_big_block_is_boxed_over_the_strips_full_height() {
        let mut ed = MapEditor::new();
        let a = Assets::default();
        let mut fb = Framebuffer::new();
        ed.size = 8;
        ed.brush = 0;
        ed.draw(&mut fb, &a);
        // Half-size cells, centred: 8 cells is 32 px each way, and the box
        // reaches the last row of the band rather than stopping half way.
        assert_eq!(fb.pget(32, SHEET_Y), col::WHITE);
        assert_eq!(fb.pget(63, SHEET_Y + ui::STRIP_H - 1), col::WHITE);
        assert_ne!(
            fb.pget(64, SHEET_Y + ui::STRIP_H - 1),
            col::WHITE,
            "the block is 8 cells wide"
        );
    }

    #[test]
    fn growing_the_brush_pulls_it_back_onto_the_sheet() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        // The sheet's bottom-right cell, then button index 1 -> size 2, which
        // needs room for one more cell each way.
        ed.brush = 255;
        ed.tick(&press(SIZE_BTNS.0 + 8 + 1, SIZE_BTNS.1 + 1), &mut a);
        assert_eq!((ed.size, ed.brush), (2, 14 * 16 + 14));
    }

    #[test]
    fn draw_tool_paints_the_hovered_cell() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.brush = 7;
        // Hover cell (1, 0): x in [8,15], y in [VIEW_Y, VIEW_Y+7].
        ed.tick(&press(9, VIEW_Y + 1), &mut a);
        assert_eq!(a.map.get(1, 0), 7);
    }

    #[test]
    fn draw_respects_the_camera_offset() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.brush = 5;
        ed.cam_x = 10;
        ed.cam_y = 4;
        let held = Mouse {
            x: 0,
            y: VIEW_Y,
            left: true,
            ..Default::default()
        };
        ed.tick(&held, &mut a);
        assert_eq!(a.map.get(10, 4), 5);
    }

    #[test]
    fn hover_box_outlines_the_cell_under_the_cursor() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        let hover = Mouse {
            x: 9,
            y: VIEW_Y + 1,
            ..Default::default()
        };
        ed.tick(&hover, &mut a);
        let mut fb = Framebuffer::new();
        ed.draw(&mut fb, &a);
        // Cell (1,0) screen rect top-left corner at (8, VIEW_Y).
        assert_eq!(fb.pget(8, VIEW_Y), col::WHITE);
    }

    // A held drag frame (button down, no fresh press edge).
    fn held(x: i32, y: i32) -> Mouse {
        Mouse {
            x,
            y,
            left: true,
            ..Default::default()
        }
    }
    // A release frame: the real input keeps the cursor position and only drops
    // `left` (the shell updates x/y on CursorMoved, not MouseInput), so release at
    // the drag-end coordinates — NOT off-screen.
    fn rel(x: i32, y: i32) -> Mouse {
        Mouse {
            x,
            y,
            ..Default::default()
        }
    }

    #[test]
    fn select_drag_builds_a_normalized_selection() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Select;
        // Press at cell (3,1), drag up-left to cell (1,0), release there.
        ed.tick(&press(3 * 8 + 1, VIEW_Y + 9), &mut a);
        ed.tick(&held(9, VIEW_Y + 1), &mut a);
        ed.tick(&rel(9, VIEW_Y + 1), &mut a);
        let s = ed.sel.expect("selection set");
        assert_eq!((s.x, s.y, s.w, s.h), (1, 0, 3, 2));
    }

    #[test]
    fn selecting_shows_size_in_status_bar() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Select;
        ed.tick(&press(8 + 1, VIEW_Y + 1), &mut a); // cell (1,0).
        ed.tick(&held(3 * 8 + 1, VIEW_Y + 8 + 1), &mut a); // drag to (3,1).
        assert!(matches!(ed.drag, Drag::Selecting { .. }));
        assert_eq!(ed.selection_in_progress(), Some((1, 0, 3, 2)));
    }

    #[test]
    fn switching_tool_abandons_an_in_progress_select_drag() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Select;
        ed.tick(&press(8 + 1, VIEW_Y + 1), &mut a); // start a marquee at cell (1,0).
        assert!(matches!(ed.drag, Drag::Selecting { .. }));
        ed.key(Key::Char('d'), Mods::default(), &mut a); // switch tools mid-drag.
        assert!(matches!(ed.drag, Drag::None), "tool switch clears the drag");
        // Back in Select with the button up: must not commit a phantom selection.
        ed.key(Key::Char('s'), Mods::default(), &mut a);
        ed.tick(&rel(40, VIEW_Y + 40), &mut a);
        assert!(ed.sel.is_none(), "no phantom selection committed");
    }

    fn make_selection(ed: &mut MapEditor, a: &mut Assets, x: i32, y: i32, w: i32, h: i32) {
        ed.tool = Tool::Select;
        ed.tick(&press(x * 8 + 1, VIEW_Y + y * 8 + 1), a);
        ed.tick(&held((x + w - 1) * 8 + 1, VIEW_Y + (y + h - 1) * 8 + 1), a);
        ed.tick(&rel((x + w - 1) * 8 + 1, VIEW_Y + (y + h - 1) * 8 + 1), a);
    }

    #[test]
    fn copy_then_paste_reproduces_the_block() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(2, 1, 11);
        a.map.set(3, 1, 12);
        make_selection(&mut ed, &mut a, 2, 1, 2, 1);
        ed.copy_selection(&mut a, false);
        let clip = ed.clip.clone().expect("clipboard filled");
        assert_eq!((clip.w, clip.h, clip.tiles), (2, 1, vec![11, 12]));
        // Paste at cell (5, 4).
        ed.tool = Tool::Paste;
        ed.tick(&press(5 * 8 + 1, VIEW_Y + 4 * 8 + 1), &mut a);
        assert_eq!(a.map.get(5, 4), 11);
        assert_eq!(a.map.get(6, 4), 12);
    }

    #[test]
    fn cut_clears_the_source() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(2, 1, 11);
        make_selection(&mut ed, &mut a, 2, 1, 1, 1);
        ed.copy_selection(&mut a, true);
        assert_eq!(a.map.get(2, 1), 0);
        assert_eq!(ed.clip.as_ref().unwrap().tiles, vec![11]);
    }

    #[test]
    fn delete_clears_without_touching_the_clipboard() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(2, 1, 11);
        make_selection(&mut ed, &mut a, 2, 1, 1, 1);
        ed.key(Key::Delete, Mods::default(), &mut a);
        assert_eq!(a.map.get(2, 1), 0);
        assert!(ed.clip.is_none());
    }

    #[test]
    fn dragging_inside_a_selection_moves_the_block() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(2, 1, 11);
        make_selection(&mut ed, &mut a, 2, 1, 1, 1);
        // Press inside the selection (cell 2,1), drag to cell (5,4), release there.
        ed.tick(&press(2 * 8 + 1, VIEW_Y + 9), &mut a);
        ed.tick(&held(5 * 8 + 1, VIEW_Y + 4 * 8 + 1), &mut a);
        ed.tick(&rel(5 * 8 + 1, VIEW_Y + 4 * 8 + 1), &mut a);
        assert_eq!(a.map.get(2, 1), 0, "source cleared");
        assert_eq!(a.map.get(5, 4), 11, "block moved");
        let s = ed.sel.expect("selection follows the move");
        assert_eq!((s.x, s.y, s.w, s.h), (5, 4, 1, 1));
    }

    #[test]
    fn fill_click_flood_fills_the_contiguous_region() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Fill;
        ed.brush = 9;
        // Default map is all 0; a single click should flood the whole visible-and-beyond region.
        ed.tick(&press(8 + 1, VIEW_Y + 1), &mut a); // cell (1,0), tile 0
        ed.tick(&rel(8 + 1, VIEW_Y + 1), &mut a); // release on the same cell -> flood
        assert_eq!(a.map.get(1, 0), 9);
        assert_eq!(a.map.get(0, 0), 9, "neighbour flooded");
        assert_eq!(a.map.get(40, 30), 9, "far same-tile cell flooded");
    }

    #[test]
    fn fill_drag_fills_a_rectangle() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(50, 50, 3); // make the map non-uniform so flood wouldn't cover the rect.
        ed.tool = Tool::Fill;
        ed.brush = 4;
        ed.tick(&press(9, VIEW_Y + 9), &mut a); // (1,1)
        ed.tick(&held(2 * 8 + 1, VIEW_Y + 2 * 8 + 1), &mut a); // drag to (2,2)
        ed.tick(&rel(2 * 8 + 1, VIEW_Y + 2 * 8 + 1), &mut a); // release on (2,2) -> rect
        for y in 1..=2 {
            for x in 1..=2 {
                assert_eq!(a.map.get(x, y), 4, "cell ({x},{y}) filled");
            }
        }
        assert_eq!(a.map.get(0, 0), 0, "outside rect untouched");
    }

    #[test]
    fn pan_drag_scrolls_the_camera() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Pan;
        ed.cam_x = 20;
        ed.cam_y = 20;
        // Press at x=80, drag left to x=40 (delta -40 px = -5 cells) -> cam_x += 5.
        ed.tick(&press(80, VIEW_Y + 40), &mut a);
        ed.tick(&held(40, VIEW_Y + 16), &mut a); // dx -40 -> +5 cells; dy -24 -> +3 cells
        assert_eq!(ed.cam_x, 25);
        assert_eq!(ed.cam_y, 23);
    }

    #[test]
    fn circle_drag_stamps_an_ellipse_outline() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Circle;
        ed.brush = 6;
        // Bounding box cells (2,2)..(6,6): a 5x5 box centred at (4,4).
        ed.tick(&press(2 * 8 + 1, VIEW_Y + 2 * 8 + 1), &mut a);
        ed.tick(&held(6 * 8 + 1, VIEW_Y + 6 * 8 + 1), &mut a);
        ed.tick(&rel(6 * 8 + 1, VIEW_Y + 6 * 8 + 1), &mut a);
        // Outline touches the box-edge midpoints...
        assert_eq!(a.map.get(4, 2), 6, "top midpoint on outline");
        assert_eq!(a.map.get(2, 4), 6, "left midpoint on outline");
        assert_eq!(a.map.get(6, 4), 6, "right midpoint on outline");
        assert_eq!(a.map.get(4, 6), 6, "bottom midpoint on outline");
        // ...and the centre stays empty (outline only).
        assert_eq!(a.map.get(4, 4), 0, "centre not filled");
    }

    #[test]
    fn cancel_drag_prevents_a_phantom_selection_on_reentry() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tool = Tool::Select;
        ed.tick(&press(8 + 1, VIEW_Y + 1), &mut a); // start a marquee.
        assert!(matches!(ed.drag, Drag::Selecting { .. }));
        ed.cancel_drag(); // the shell calls this when the editor loses focus.
        assert!(matches!(ed.drag, Drag::None));
        // Button up on re-entry must not commit a stale selection.
        ed.tick(&rel(40, VIEW_Y + 40), &mut a);
        assert!(ed.sel.is_none(), "no phantom selection after cancel_drag");
    }

    #[test]
    fn paste_skips_cells_past_the_map_edge() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.clip = Some(Clipboard {
            w: 2,
            h: 1,
            tiles: vec![9, 8],
        });
        // The left cell lands on the last column; the second cell is off-map.
        ed.paste_at(&mut a, MAP_W as i32 - 1, 0);
        assert_eq!(a.map.get(MAP_W as i32 - 1, 0), 9, "in-bounds cell pasted");
        assert_eq!(a.map.get(0, 0), 0, "off-map cell skipped, not wrapped");
    }

    #[test]
    fn hovering_a_tool_reports_its_label() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        // Hover the Select icon (slot 2) without clicking.
        let hover = Mouse {
            x: tool_x(2) + 1,
            y: TOOLBAR_Y + 2,
            ..Default::default()
        };
        ed.tick(&hover, &mut a);
        assert_eq!(ed.tool, Tool::Draw, "hovering does not select the tool");
        assert_eq!(ed.tool_under_cursor(), Some(Tool::Select));
        assert_eq!(tool_label(Tool::Select), "Select (s)");
    }

    #[test]
    fn sheet_sits_flush_against_the_status_bar() {
        // No stray band between the sheet and the status bar (which starts at
        // HEIGHT - 8); the sheet's last row is the row directly above it.
        assert_eq!(SHEET_Y + ui::STRIP_H, pixel8_runtime::fb::HEIGHT - 8);
    }

    #[test]
    fn fullscreen_expands_the_viewport() {
        let mut ed = MapEditor::new();
        assert_eq!(ed.view_y(), VIEW_Y);
        assert_eq!(ed.view_tiles_y(), VIEW_TILES_Y);
        ed.set_fullscreen(true);
        assert!(ed.is_fullscreen());
        assert_eq!(ed.view_y(), 8);
        assert_eq!(ed.view_tiles_y(), 14);
        assert_eq!(ed.view_bottom(), 119);
    }

    #[test]
    fn toggling_fullscreen_clamps_the_camera() {
        let mut ed = MapEditor::new();
        ed.cam_y = MAP_H as i32 - VIEW_TILES_Y; // 56, the normal-view max.
        ed.set_fullscreen(true);
        assert_eq!(ed.cam_y, MAP_H as i32 - 14, "clamped to the taller view");
    }

    #[test]
    fn clicking_view_buttons_toggles_fullscreen() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.tick(&press(15, 2), &mut a); // fullscreen button
        assert!(ed.is_fullscreen());
        ed.tick(&press(6, 2), &mut a); // normal button
        assert!(!ed.is_fullscreen());
    }

    #[test]
    fn tab_toggles_fullscreen() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.key(Key::Tab, Mods::default(), &mut a);
        assert!(ed.is_fullscreen());
        ed.key(Key::Tab, Mods::default(), &mut a);
        assert!(!ed.is_fullscreen());
    }

    #[test]
    fn copy_sets_status() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.sel = Some(Selection {
            x: 0,
            y: 0,
            w: 3,
            h: 2,
        });
        ed.copy_selection(&mut a, false);
        assert_eq!(ed.status.current(), Some("copied 3x2 tiles"));
        ed.copy_selection(&mut a, true);
        assert_eq!(ed.status.current(), Some("cut 3x2 tiles"));
    }

    #[test]
    fn paste_sets_status() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.clip = Some(Clipboard {
            w: 2,
            h: 1,
            tiles: vec![4, 5],
        });
        ed.paste_at(&mut a, 0, 0);
        assert_eq!(ed.status.current(), Some("pasted 2x1 tiles"));
    }

    fn ctrl(shift: bool) -> Mods {
        Mods {
            ctrl: true,
            shift,
            ..Default::default()
        }
    }

    #[test]
    fn undo_and_redo_a_draw_stroke() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.brush = 7;
        // Paint cell (1,0), then release to close the stroke.
        ed.tick(&press(9, VIEW_Y + 1), &mut a);
        ed.tick(&rel(9, VIEW_Y + 1), &mut a);
        assert_eq!(a.map.get(1, 0), 7);
        ed.key(Key::Char('z'), ctrl(false), &mut a);
        assert_eq!(a.map.get(1, 0), 0, "undo clears the tile");
        ed.key(Key::Char('y'), ctrl(false), &mut a); // Ctrl+Y redo
        assert_eq!(a.map.get(1, 0), 7, "redo restores the tile");
    }

    #[test]
    fn undo_reverts_a_fill_committed_on_release() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(50, 50, 3); // make the map non-uniform.
        ed.tool = Tool::Fill;
        ed.brush = 4;
        // A drag fills a rectangle on the release frame.
        ed.tick(&press(9, VIEW_Y + 9), &mut a); // (1,1)
        ed.tick(&held(2 * 8 + 1, VIEW_Y + 2 * 8 + 1), &mut a); // (2,2)
        ed.tick(&rel(2 * 8 + 1, VIEW_Y + 2 * 8 + 1), &mut a);
        assert_eq!(a.map.get(1, 1), 4);
        ed.key(Key::Char('z'), ctrl(false), &mut a);
        assert_eq!(
            a.map.get(1, 1),
            0,
            "release-frame fill is undone in one step"
        );
        assert_eq!(a.map.get(2, 2), 0);
    }

    #[test]
    fn undo_reverts_a_delete_key() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        a.map.set(2, 1, 11);
        make_selection(&mut ed, &mut a, 2, 1, 1, 1);
        ed.key(Key::Delete, Mods::default(), &mut a);
        assert_eq!(a.map.get(2, 1), 0);
        ed.key(Key::Char('z'), ctrl(false), &mut a);
        assert_eq!(a.map.get(2, 1), 11, "delete is one undo step");
    }

    #[test]
    fn fullscreen_draw_paints_through_the_taller_viewport() {
        let mut ed = MapEditor::new();
        let mut a = Assets::default();
        ed.set_fullscreen(true);
        ed.brush = 7;
        // In fullscreen the view top is y8; screen (1, 9) is map cell (0, 0).
        ed.tick(&press(1, 9), &mut a);
        assert_eq!(a.map.get(0, 0), 7);
    }

    #[test]
    fn status_messages_fit_the_bar() {
        use pixel8_runtime::{fb::WIDTH, font::text_width};
        let budget = WIDTH - 2;
        // Widest map clipboard message: full-map dimensions.
        assert!(text_width(&format!("copied {}x{} tiles", MAP_W, MAP_H)) <= budget);
        assert!(text_width(&format!("pasted {}x{} tiles", MAP_W, MAP_H)) <= budget);
    }

    #[test]
    fn copy_returns_native_map_blob() {
        use pixel8_runtime::clipboard::{parse, Pasted};
        let mut a = Assets::default();
        let mut ed = MapEditor::new();
        a.map.set(2, 1, 9);
        make_selection(&mut ed, &mut a, 2, 1, 2, 1);
        let blob = ed.copy_selection(&mut a, false).expect("blob");
        let Pasted::Map { w, h, tiles } = parse(&blob).unwrap() else {
            panic!("not map")
        };
        assert_eq!((w, h), (2, 1));
        assert_eq!(tiles[0], 9);
    }

    #[test]
    fn paste_arms_the_tool_from_a_map_blob() {
        use pixel8_runtime::clipboard::Pasted;
        let mut ed = MapEditor::new();
        ed.paste(&Pasted::Map {
            w: 2,
            h: 1,
            tiles: vec![4, 5],
        });
        let clip = ed.clip.clone().expect("paste buffer filled");
        assert_eq!((clip.w, clip.h), (2, 1));
        assert_eq!(clip.tiles, vec![4, 5]);
        assert!(matches!(ed.tool, Tool::Paste));
    }
}
