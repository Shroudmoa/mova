//! Everything that touches the buffer lives here.
//!
//! The world is flat 2D. The camera simply squashes the Y axis and shrinks
//! whatever drifts toward the top of the screen, which is enough to read as
//! looking down into a shallow arena without a single matrix.

use std::fmt::Write as _;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Widget};
use ratatui::Frame;

use crate::enemy::Enemy;
use crate::game::{Arena, Game, Phase, ARENA_HX, ARENA_HY};
use crate::goal;
use crate::pickup::Pickup;
use crate::player::{Player, DASH_CD};
use crate::region;
use crate::save;
use crate::solid::{Solid, SolidGrid, CELL, EMPTY};

// ---- palette --------------------------------------------------------------
// Two families, and the split between them is the single most load-bearing thing
// in this file.
//
// MOVA used to be red, the same red as everything that wants him dead. At dash
// speed the frame is mostly streak — his embers, the enemy, the blood of the
// last one you hit, all smeared along the direction of travel — and "is that me
// or is that the thing hunting me" became a question answered by looking closely
// at a moving glyph. He is blue now, and everything that can hurt him stays warm.
// Nothing else on screen is cold and moving, so at a glance, in peripheral
// vision, at two hundred units a second, he is findable.
//
// Furniture is a cold violet: present, structural, never alive. Text is grey.

const BG: Color = Color::Rgb(6, 9, 20);
const GRID_FAR: Color = Color::Rgb(19, 26, 52);
const GRID_NEAR: Color = Color::Rgb(37, 50, 94);
/// Every fifth intersection: the landmarks that make a big arena navigable.
const GRID_MARK: Color = Color::Rgb(64, 88, 152);
/// Outer window chrome: the quietest line on screen.
const CHROME: Color = Color::Rgb(34, 43, 78);
/// The coldest accent, and the bottom of every MOVA ramp.
const DEEP: Color = Color::Rgb(20, 64, 136);
/// MOVA's own colour.
const COOL: Color = Color::Rgb(72, 158, 255);
/// MOVA at his hottest: blazing, dashing, mid-ember. The near-white end of cold.
const GLARE: Color = Color::Rgb(178, 236, 255);

/// The warm family. Every one of these is a threat, or the memory of one: the
/// enemies, the spark off MOVA's ribs when he is hit, the HP gauge, the flash
/// that fills the frame when he dies. Keeping them out of the blues is what lets
/// a colour on screen answer a question without the player reading any glyph.
const DIM_RED: Color = Color::Rgb(116, 26, 44);
const RED: Color = Color::Rgb(214, 48, 64);
const HOT: Color = Color::Rgb(255, 106, 106);

const PALE: Color = Color::Rgb(206, 214, 240);
const FAINT: Color = Color::Rgb(74, 98, 158);
/// Pillar faces. Brighter than the floor grid, much dimmer than anything alive,
/// so the map reads as structure rather than as another threat.
const BLOCK_FAR: Color = Color::Rgb(29, 27, 62);
const BLOCK_NEAR: Color = Color::Rgb(50, 47, 96);
/// The lit top edge of a block, which is what sells the height.
const BLOCK_TOP: Color = Color::Rgb(90, 86, 154);

const PLAYER: char = '●';
/// A burning ember. Solid and distinct from every other particle so a trail
/// reads as fire rather than as more of the same dots.
const FLAME: char = '▲';
const BULLET: char = '·';
const DUST: char = '·';
const GRID: char = '·';
const HP_FULL: char = '▪';
const HP_EMPTY: char = '▪';
const SPD_FULL: char = '█';
const SPD_EMPTY: char = '░';

// ---- camera ---------------------------------------------------------------

/// Y squash. Below 1.0 the arena reads as receding rather than top-down.
const DEPTH: f32 = 0.60;
/// Cells per world unit at full size; shrinks only to fit tiny terminals.
const SCALE: f32 = 1.0;
/// Grid spacing in world units, and how many steps make a marked intersection.
const GRID_STEP: f32 = 4.0;
const GRID_EVERY: i32 = 5;

struct Cam {
    /// Screen-space centre of the view: the column and row the camera's own
    /// world point lands on.
    cx: f32,
    cy: f32,
    kx: f32,
    ky: f32,
    /// World position at the centre of the view.
    wx: f32,
    wy: f32,
    half_h: f32,
}

impl Cam {
    /// Offset of a world point from the camera, in world units, measured the
    /// short way round the seam.
    ///
    /// This is the whole reason the torus renders at all. Every coordinate the
    /// simulation holds is folded into the fundamental rectangle, so a pillar a
    /// unit to MOVA's left is stored at `x = 191` while the camera is at `-192`,
    /// and subtracting the two naively says the pillar is 383 units away and
    /// throws it off the screen. The map is still there — it has come back round
    /// to meet you — but on screen it is gone, and it is gone on the side MOVA is
    /// running towards, which reads as the world falling away as he crosses.
    /// Fold first, then subtract.
    #[inline]
    fn off(&self, x: f32, y: f32) -> (f32, f32) {
        (
            Arena::delta_axis(self.wx, x, ARENA_HX),
            Arena::delta_axis(self.wy, y, ARENA_HY),
        )
    }

    /// Screen position of an offset from the camera, in world units.
    #[inline]
    fn place(&self, ox: f32, oy: f32) -> (f32, f32) {
        (self.cx + ox * self.kx, self.cy + oy * self.ky)
    }
}

/// How many world units fit across `outer` and up `outer`, accounting for the
/// window border. This is the single source of truth for both the camera and
/// the spawn ring, so the two can never disagree.
pub fn view_half(outer: Rect) -> (f32, f32) {
    let w = f32::from(outer.width.saturating_sub(2)).max(8.0);
    let h = f32::from(outer.height.saturating_sub(2)).max(6.0);
    let k = scale(w, h);
    (w / (2.0 * k), h / (2.0 * k * DEPTH))
}

/// Chosen so the visible patch of the world is roughly the same size on any
/// terminal: big windows see more detail, small ones see less, both play the
/// same game.
#[inline]
fn scale(w: f32, h: f32) -> f32 {
    ((h / 34.0).min(w / 60.0) * SCALE).clamp(0.30, SCALE)
}

fn cam(area: Rect, wx: f32, wy: f32) -> Cam {
    let w = f32::from(area.width);
    let h = f32::from(area.height);
    let k = scale(w, h);
    Cam {
        // The screen centre, not the camera's own projection of the world
        // origin. Everything downstream wants one or the other and mixing them
        // shifts the view by twice the camera's offset — which is a bug that
        // only shows up away from the middle of the map, because that is the
        // only place the two numbers differ.
        cx: f32::from(area.x) + w / 2.0,
        cy: f32::from(area.y) + h / 2.0,
        kx: k,
        ky: k * DEPTH,
        wx,
        wy,
        half_h: h / 2.0,
    }
}

/// A world point to its screen position.
///
/// Seam-aware, and every caller goes through it: on a torus there is no such
/// thing as "off screen", only "on the far side", and a projection that cannot
/// tell the two apart empties the frame every time MOVA walks past the seam.
#[inline]
fn proj(c: &Cam, x: f32, y: f32) -> (f32, f32) {
    let (ox, oy) = c.off(x, y);
    c.place(ox, oy)
}

/// 0 at the near edge of the view, 1 at the far edge.
#[inline]
fn depth(c: &Cam, sy: f32) -> f32 {
    ((c.cy - sy) / c.half_h).clamp(0.0, 1.0)
}

#[inline]
fn mix(a: Color, b: Color, t: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg_, bb)) = (a, b) else {
        return a;
    };
    let t = t.clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Color::Rgb(
        (ar as f32 + (br as f32 - ar as f32) * t) as u8,
        (ag as f32 + (bg_ as f32 - ag as f32) * t) as u8,
        (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
    )
}

#[inline]
fn dim(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
        _ => c,
    }
}

// ---- buffer primitives ----------------------------------------------------

/// Decimal digit count, so the HUD can measure a readout before building it.
fn digits(v: u32) -> i32 {
    let mut n = 1;
    let mut v = v;
    while v >= 10 {
        v /= 10;
        n += 1;
    }
    n
}

#[inline]
fn putc(buf: &mut Buffer, area: Rect, x: i32, y: i32, ch: char, st: Style) {
    if x < area.x as i32
        || y < area.y as i32
        || x >= area.right() as i32
        || y >= area.bottom() as i32
    {
        return;
    }
    if let Some(c) = buf.cell_mut((x as u16, y as u16)) {
        c.set_char(ch);
        c.set_style(st);
    }
}

#[inline]
fn putf(buf: &mut Buffer, area: Rect, x: f32, y: f32, ch: char, st: Style) {
    putc(buf, area, x.round() as i32, y.round() as i32, ch, st);
}

fn line(buf: &mut Buffer, area: Rect, x: i32, y: i32, s: &str, st: Style) {
    for (i, ch) in s.chars().enumerate() {
        let cx = x + i as i32;
        if cx >= area.right() as i32 {
            break;
        }
        putc(buf, area, cx, y, ch, st);
    }
}

/// A round-ish blob, sampled on a 3x3 sub-grid so it can read as genuinely
/// bigger or smaller than a single cell.
fn dot(buf: &mut Buffer, area: Rect, sx: f32, sy: f32, r: f32, ch: char, st: Style) {
    if r <= 0.34 {
        putf(buf, area, sx, sy, ch, st);
        return;
    }
    let mut any = false;
    for j in -1..=1i32 {
        for i in -1..=1i32 {
            let (ox, oy) = (i as f32 / 3.0, j as f32 / 3.0);
            if ox * ox + oy * oy <= r * r {
                putf(buf, area, sx + ox, sy + oy, ch, st);
                any = true;
            }
        }
    }
    if !any {
        putf(buf, area, sx, sy, ch, st);
    }
}

// ---- entry point ----------------------------------------------------------

/// Terminal-facing entry point. Does nothing but hand over the buffer.
pub fn draw(f: &mut Frame, game: &Game, paused: bool) {
    let area = f.area();
    render(f.buffer_mut(), area, game);
    if paused {
        paused_panel(f.buffer_mut(), area, game);
    }
}

/// Draws one complete frame into `buf`, clipped to `area`.
pub fn render(buf: &mut Buffer, area: Rect, game: &Game) {
    buf.set_style(area, Style::new().bg(BG));

    match game.phase {
        Phase::Menu => menu(buf, area, game),
        Phase::Playing => play(buf, area, game),
        Phase::Over => over(buf, area, game),
    }
}

/// The pause dim. Darkens the frame underneath so the readout is legible
/// without hiding the run you are about to come back to.
fn paused_panel(buf: &mut Buffer, area: Rect, game: &Game) {
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    for y in inner.top()..inner.bottom() {
        for x in inner.left()..inner.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                let Color::Rgb(r, g, b) = c.fg else {
                    continue;
                };
                // Halve everything already drawn; the pause text goes on top.
                c.set_fg(Color::Rgb(r / 3, g / 3, b / 3));
            }
        }
    }

    let pulse = 0.7 + 0.3 * (game.time * 3.0).sin();
    let y = area.y as i32 + area.height as i32 / 2;
    center(
        buf,
        area,
        y - 1,
        "PAUSED",
        Style::new().fg(mix(DEEP, COOL, pulse)).bg(BG),
    );

    let s = format!(
        "WAVE {:02}    SCORE {:06}    {:>4}s LEFT",
        game.wave,
        game.score.points,
        // Shown as elapsed, which is what the player can still influence.
        (game.run_t.max(0.0)) as u32
    );
    center(buf, area, y + 1, &s, Style::new().fg(PALE).bg(BG));
    center(
        buf,
        area,
        y + 2,
        region::NAMES[game.sector],
        Style::new().fg(COOL).bg(BG),
    );
    center(
        buf,
        area,
        y + 4,
        "[ P ] RESUME     [ Q ] QUIT",
        Style::new().fg(FAINT).bg(BG),
    );
}

fn frame_box(buf: &mut Buffer, area: Rect) -> Rect {
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(CHROME).bg(BG))
        .style(Style::new().bg(BG));
    let inner = block.inner(area);
    block.render(area, buf);
    inner
}

// ---- menu -----------------------------------------------------------------

fn menu(buf: &mut Buffer, area: Rect, game: &Game) {
    let inner = frame_box(buf, area);
    // The menu looks at the actual map, centred on the spawn plaza, so the
    // pillars behind the title are the pillars you are about to run into.
    let c = cam(inner, 0.0, 0.0);
    grid(buf, inner, &c, false);
    for s in &game.solids.solids {
        draw_solid(buf, inner, &c, &game.solids, s, false);
    }

    // Slow pulse so the title never looks like a static screenshot.
    let pulse = 0.72 + 0.28 * (game.time * 1.5).sin();
    let rows = 13i32;
    let y = area.y as i32 + (area.height as i32 - rows) / 2;

    centered_colored(buf, area, y, "M O V A", &[DEEP, COOL, GLARE, PALE], pulse);
    center(
        buf,
        area,
        y + 2,
        "SURVIVE / ROAM / OUTRUN",
        Style::new().fg(FAINT).bg(BG),
    );

    let mut s = String::with_capacity(24);
    s.clear();
    let _ = write!(s, "HIGH  {:06}", game.save.high);
    center(buf, area, y + 5, &s, Style::new().fg(PALE).bg(BG));

    // Rank, plus how far into the current band the lifetime score has climbed.
    s.clear();
    let _ = write!(s, "RANK  {}", save::rank_name(game.save.total_score));
    center(buf, area, y + 6, &s, Style::new().fg(COOL).bg(BG));
    rank_bar(buf, area, y + 7, game.save.total_score);

    // Lifetime progress. This shares no row with the tagline below it: `center`
    // blanks the cells it is about to write, so two centred lines on one row
    // means the second quietly erases the first.
    s.clear();
    let _ = write!(
        s,
        "WAVE {:02}   COMBO x{:02}   {} RAMS",
        game.save.best_wave, game.save.best_combo, game.save.total_rams
    );
    center(buf, area, y + 8, &s, Style::new().fg(FAINT).bg(BG));

    center(
        buf,
        area,
        y + 9,
        "MOVE FAST. THE MAP IS YOURS TO CROSS.",
        Style::new().fg(Color::Rgb(120, 70, 150)).bg(BG),
    );
    center(
        buf,
        area,
        y + 11,
        "[ ENTER ] RUN   [ P ] PAUSE   [ M ] RADAR   [ G ] GOALS   [ Q ] QUIT",
        Style::new().fg(PALE).bg(BG),
    );
}

/// A short bar showing progress toward the next rank.
fn rank_bar(buf: &mut Buffer, area: Rect, y: i32, total: u32) {
    let w = 24i32;
    let x = area.x as i32 + (area.width as i32 - w) / 2;
    let full = save::rank_index(total) >= save::RANK_MAX;
    let t = save::rank_progress(total).unwrap_or(1.0);
    for i in 0..w {
        let on = t * w as f32 >= (i + 1) as f32;
        let col = if on {
            if full {
                GLARE
            } else {
                COOL
            }
        } else {
            Color::Rgb(44, 22, 60)
        };
        putc(
            buf,
            area,
            x + i,
            y,
            if on { SPD_FULL } else { SPD_EMPTY },
            Style::new().fg(col).bg(BG),
        );
    }
}

/// The goal ledger, reached from the menu with `G`.
pub fn goals(buf: &mut Buffer, area: Rect, game: &Game) {
    let inner = frame_box(buf, area);
    let c = cam(inner, 0.0, 0.0);
    grid(buf, inner, &c, false);
    for s in &game.solids.solids {
        draw_solid(buf, inner, &c, &game.solids, s, false);
    }

    let half = goal::COUNT.div_ceil(2) as i32;
    let rows = 8i32 + half;
    let y = area.y as i32 + (area.height as i32 - rows) / 2;

    centered_colored(buf, area, y, "G O A L S", &[DEEP, COOL, GLARE], 0.2);
    center(
        buf,
        area,
        y + 2,
        "EARNED ONCE, KEPT FOREVER",
        Style::new().fg(FAINT).bg(BG),
    );

    // Two columns, both measured from the longest label rather than assumed.
    // The v2 goals have long names, and a fixed half-width column silently runs
    // the right-hand one off the frame.
    let label_w = goal::GOALS
        .iter()
        .map(|g| g.label.chars().count() as i32)
        .max()
        .unwrap_or(12)
        + 3;
    let gap_w = 5;
    let total = label_w * 2 + gap_w;
    let left = area.x as i32 + ((area.width as i32 - total) / 2).max(1);

    for i in 0..goal::COUNT {
        let g = &goal::GOALS[i];
        let got = game.save.goals & g.bit != 0;
        let (col, row) = if i < half as usize {
            (0, i as i32)
        } else {
            (1, i as i32 - half)
        };
        let rx = left + col * (label_w + gap_w);
        let ry = y + 4 + row;
        // Blank the whole cell span first. The list sits on top of the map
        // backdrop, and without this the pillars show through the gaps between
        // the marker and the label.
        gap(buf, area, rx - 1, ry, label_w + 1);
        putc(
            buf,
            area,
            rx,
            ry,
            '▪',
            Style::new()
                .fg(if got { COOL } else { Color::Rgb(48, 26, 52) })
                .bg(BG),
        );
        line(
            buf,
            area,
            rx + 2,
            ry,
            g.label,
            Style::new().fg(if got { PALE } else { FAINT }).bg(BG),
        );
    }

    // Chart progress. The map remembers which sectors you have stood in, so
    // this is the one persistent measure of how much of it you have seen.
    let charted = game.save.sectors.count_ones() as usize;
    let mut s = String::with_capacity(40);
    let _ = write!(
        s,
        "{} / {} DONE      CHARTED {}/{} SECTORS",
        goal::count_done(&game.save),
        goal::COUNT,
        charted,
        region::COUNT
    );
    center(buf, area, y + rows - 2, &s, Style::new().fg(COOL).bg(BG));
    center(
        buf,
        area,
        y + rows - 1,
        "[ ANY KEY ] BACK",
        Style::new().fg(FAINT).bg(BG),
    );
}

/// "M O V A" with a per-letter ramp from deep purple to hot red.
fn centered_colored(buf: &mut Buffer, area: Rect, y: i32, s: &str, ramp: &[Color], pulse: f32) {
    let chars: Vec<char> = s.chars().collect();
    let x = area.x as i32 + (area.width as i32 - chars.len() as i32) / 2;
    gap(buf, area, x - 1, y, chars.len() as i32 + 2);
    for (i, ch) in chars.iter().enumerate() {
        let base = ramp[i.min(ramp.len() - 1)];
        putc(
            buf,
            area,
            x + i as i32,
            y,
            *ch,
            Style::new().fg(mix(base, GLARE, pulse * 0.35)).bg(BG),
        );
    }
}

fn center(buf: &mut Buffer, area: Rect, y: i32, s: &str, st: Style) {
    let n = s.chars().count() as i32;
    let x = area.x as i32 + (area.width as i32 - n) / 2;
    // Clear the grid dots that would otherwise crowd the text.
    gap(buf, area, x - 1, y, n + 2);
    line(buf, area, x, y, s, st);
}

// ---- game over ------------------------------------------------------------

fn over(buf: &mut Buffer, area: Rect, game: &Game) {
    let inner = frame_box(buf, area);
    let c = cam(inner, 0.0, 0.0);
    grid(buf, inner, &c, false);
    for s in &game.solids.solids {
        draw_solid(buf, inner, &c, &game.solids, s, false);
    }

    let st = &game.stats;
    // The goal strip grows by one line only when the run actually earned one.
    let gained = (0..goal::COUNT)
        .filter(|&i| game.new_goals & goal::GOALS[i].bit != 0)
        .map(|i| goal::GOALS[i].label)
        .collect::<Vec<_>>();
    let rows = 14i32 + if gained.is_empty() { 0 } else { 2 };
    let y = area.y as i32 + (area.height as i32 - rows) / 2;

    // Cooling down. The name is still his name; the run is over.
    centered_colored(buf, area, y, "M O V A", &[DEEP, COOL, DIM_RED, RED], 0.2);
    center(buf, area, y + 2, "RUN ENDED", Style::new().fg(FAINT).bg(BG));

    let mut s = String::with_capacity(32);
    s.clear();
    let _ = write!(s, "SCORE  {}", game.score.points);
    let score_col = if game.new_high { GLARE } else { PALE };
    center(buf, area, y + 4, &s, Style::new().fg(score_col).bg(BG));

    s.clear();
    let _ = write!(s, "{}", st.summary());
    center(buf, area, y + 5, &s, Style::new().fg(FAINT).bg(BG));

    // What the free roam added to the run. A line of its own, because these
    // are the numbers a roaming run should actually be judged on.
    s.clear();
    let _ = write!(
        s,
        "{} CORES   {} SECTORS   {} BRUTES",
        st.cores, st.sectors, st.brutes
    );
    center(
        buf,
        area,
        y + 6,
        &s,
        Style::new().fg(Color::Rgb(132, 88, 168)).bg(BG),
    );

    s.clear();
    let _ = write!(
        s,
        "WAVE {:02}    COMBO x{:02}    PEAK SPD {:03}",
        st.wave,
        st.best_combo,
        (st.peak_speed * 100.0) as u32
    );
    center(buf, area, y + 8, &s, Style::new().fg(COOL).bg(BG));

    // Anything this run unlocked, called out before the standing records.
    let mut row = 10i32;
    if !gained.is_empty() {
        center(
            buf,
            area,
            y + row,
            "UNLOCKED",
            Style::new().fg(GLARE).bg(BG),
        );
        for (n, label) in gained.iter().enumerate() {
            let mut t = String::with_capacity(20);
            let _ = write!(t, "▪ {label}");
            center(
                buf,
                area,
                y + row + 1 + n as i32,
                &t,
                Style::new().fg(PALE).bg(BG),
            );
        }
        row += 1 + gained.len() as i32 + 1;
    }

    s.clear();
    if game.new_high {
        let _ = write!(s, "NEW HIGH  {:06}", game.save.high);
        center(buf, area, y + row, &s, Style::new().fg(GLARE).bg(BG));
    } else {
        let _ = write!(s, "HIGH  {:06}", game.save.high);
        center(buf, area, y + row, &s, Style::new().fg(FAINT).bg(BG));
    }

    s.clear();
    let _ = write!(
        s,
        "RANK  {}   {}/{} GOALS   {} RUNS",
        save::rank_name(game.save.total_score),
        goal::count_done(&game.save),
        goal::COUNT,
        game.save.runs
    );
    center(buf, area, y + row + 1, &s, Style::new().fg(COOL).bg(BG));

    center(
        buf,
        area,
        y + rows - 1,
        "[ ENTER ] RETRY     [ G ] GOALS     [ Q ] QUIT",
        Style::new().fg(PALE).bg(BG),
    );
}

// ---- playing --------------------------------------------------------------

fn play(buf: &mut Buffer, area: Rect, game: &Game) {
    let inner = frame_box(buf, area);
    let c = cam(inner, game.cam_x, game.cam_y);

    hud(buf, area, inner, game);

    if inner.width < 8 || inner.height < 4 {
        return;
    }

    grid(buf, inner, &c, true);

    // Pillars before anything alive, so bodies and bullets always read on top
    // of the map rather than behind it.
    for s in &game.solids.solids {
        draw_solid(buf, inner, &c, &game.solids, s, true);
    }

    for p in &game.fx.parts {
        let (sx, sy) = proj(&c, p.x, p.y);
        let a = (p.life / p.max_life).clamp(0.0, 1.0);
        let (ch, col) = match p.kind {
            0 => (DUST, mix(RED, DIM_RED, 1.0 - a)),
            1 => (DUST, mix(COOL, DEEP, 1.0 - a)),
            2 => (DUST, mix(HOT, DIM_RED, 1.0 - a)),
            // Embers cool, so the ramp runs the other way from the debris above:
            // born white, dying to MOVA's own deep blue. A trail that dims as it
            // ages is a spark; one that cools reads as fire, because fire is the
            // thing that cools. `mix` clamps, so the head past `a = 0.6` is plain
            // glare and the first of the life is spent in it.
            //
            // This is the single most important colour decision in the game. It
            // was red, which made the trail the same colour as the thing chasing
            // MOVA — and the trail is longer than the enemy.
            _ => (
                FLAME,
                mix(mix(DEEP, GLARE, a), PALE, (a - 0.6).max(0.0) / 0.4),
            ),
        };
        putf(buf, inner, sx, sy, ch, Style::new().fg(col).bg(BG));
    }

    for b in &game.bullets {
        let (sx, sy) = proj(&c, b.x, b.y);
        let a = (b.life / 1.5).clamp(0.0, 1.0);
        putf(
            buf,
            inner,
            sx,
            sy,
            BULLET,
            Style::new().fg(mix(COOL, DEEP, 1.0 - a)).bg(BG),
        );
    }

    for r in &game.fx.rings {
        draw_ring(buf, inner, &c, r);
    }

    for p in &game.pickups {
        draw_pickup(buf, inner, &c, p);
    }

    for e in &game.enemies {
        draw_enemy(buf, inner, &c, e, game);
    }

    draw_player(buf, inner, &c, &game.player);

    for p in &game.fx.pops {
        draw_pop(buf, inner, &c, p);
    }

    for n in &game.fx.notes {
        draw_note(buf, inner, &c, n);
    }

    // With a scrolling camera the screen edges matter, so the nearest threat
    // off view gets one quiet tick rather than a whole radar.
    threat_tick(buf, inner, &c, game);

    if game.radar {
        radar(buf, inner, game);
    }

    let mid = inner.y as i32 + inner.height as i32 / 3;

    // The sector banner sits above the wave banner: which wave you are on is
    // the lesser fact while it is on screen.
    if game.sector_t > 0.0 {
        let a = (game.sector_t / 1.6).clamp(0.0, 1.0);
        let s = if game.sector_new {
            format!("{} DISCOVERED", region::NAMES[game.sector])
        } else {
            region::NAMES[game.sector].to_string()
        };
        center(
            buf,
            inner,
            mid,
            &s,
            Style::new().fg(mix(DEEP, GLARE, a)).bg(BG),
        );
    } else if game.banner_t > 0.0 {
        let a = (game.banner_t / 1.8).clamp(0.0, 1.0);
        let mut s = String::with_capacity(8);
        s.clear();
        let _ = write!(s, "WAVE {:02}", game.wave);
        center(
            buf,
            inner,
            mid,
            &s,
            Style::new().fg(mix(DEEP, COOL, a)).bg(BG),
        );
    }

    // The one time MOVA learns what speed is for: first that it breaks contact
    // at the ram line, then that the cap itself is past touching.
    if game.hint_t > 0.0 {
        let (msg, life) = if game.hint_t > 2.2 {
            ("NOTHING LANDS AT FULL SPEED", 2.4)
        } else {
            ("SPEED BREAKS CONTACT", 2.2)
        };
        let a = (game.hint_t / life).clamp(0.0, 1.0);
        let y = inner.bottom() as i32 - 3;
        center(
            buf,
            inner,
            y,
            msg,
            Style::new().fg(mix(DEEP, COOL, a)).bg(BG),
        );
    }

    // A sustained Surge readout, because a 7-second window nobody notices is
    // seven seconds of an unclaimed bonus.
    if game.player.surging() {
        let a = (game.player.surge / Player::SURGE_TIME).clamp(0.0, 1.0);
        let left = game.player.surge.ceil() as u32;
        let mut s = String::with_capacity(12);
        let _ = write!(s, "SURGE {}s", left);
        let y = inner.bottom() as i32 - 5;
        center(
            buf,
            inner,
            y,
            &s,
            Style::new().fg(mix(GLARE, COOL, 1.0 - a)).bg(BG),
        );
    }

    // Brief full-screen tint when MOVA takes a hit.
    if game.player.flash > 0.0 {
        let a = (game.player.flash / 0.22).clamp(0.0, 1.0);
        buf.set_style(area, Style::new().bg(mix(BG, DIM_RED, a * 0.55)));
    }
}

/// One arrow on the inside of the frame, pointing at the closest enemy that is
/// not on screen. Cheap, and enough to know which way to turn.
fn threat_tick(buf: &mut Buffer, area: Rect, c: &Cam, game: &Game) {
    let (px, py) = (game.player.x, game.player.y);
    let mut best = f32::MAX;
    let mut best_dir = None;
    for e in &game.enemies {
        // The short way round the seam, so the tick points at the thing that is
        // actually nearest rather than at whichever enemy happens to share an
        // edge of the fundamental rectangle with MOVA.
        let dx = Arena::delta_axis(px, e.x, ARENA_HX);
        let dy = Arena::delta_axis(py, e.y, ARENA_HY);
        let d = dx * dx + dy * dy;
        if d < best {
            best = d;
            best_dir = Some((dx, dy));
        }
    }
    let Some((dx, dy)) = best_dir else { return };
    // `dx, dy` are already offsets from MOVA and MOVA is what the camera is on,
    // so these are offsets from the camera too.
    let (sx, sy) = c.place(dx, dy);
    if sx >= area.x as f32
        && sx < area.right() as f32
        && sy >= area.y as f32
        && sy < area.bottom() as f32
    {
        return;
    }

    // Project the direction onto the frame, one cell in from the border.
    let l = area.x as i32 + 1;
    let r = area.right() as i32 - 2;
    let t = area.y as i32 + 1;
    let b = area.bottom() as i32 - 2;
    if r < l || b < t {
        return;
    }
    let (dxs, dys) = (sx - c.cx, sy - c.cy);
    let len = (dxs * dxs + dys * dys).sqrt().max(0.001);
    let (ux, uy) = (dxs / len, dys / len);

    // Far enough out that the clamp always lands on an edge.
    let far = f32::from(area.width) + f32::from(area.height);
    let x = (c.cx + ux * far).clamp(l as f32, r as f32) as i32;
    let y = (c.cy + uy * far).clamp(t as f32, b as f32) as i32;
    let ch = if ux.abs() >= uy.abs() {
        if ux > 0.0 {
            '>'
        } else {
            '<'
        }
    } else if uy > 0.0 {
        'v'
    } else {
        '^'
    };
    putc(buf, area, x, y, ch, Style::new().fg(DIM_RED).bg(BG));
}

// ---- radar ----------------------------------------------------------------

/// Radar radius in cells, so the disc is 17x17. About as much corner as a
/// normal terminal can spare before it crowds the play field.
const RADAR_R: i32 = 8;
/// World units per radar cell.
///
/// The scale is set by the finest thing the map has: the four-unit lane between
/// two neighbouring pillars. At four units to the cell a block spans exactly
/// two cells and a lane exactly one, so both survive the reduction. Coarser
/// than this and the lanes fall under the resolution of the grid.
const RADAR_CELL: f32 = 4.0;
/// World units from MOVA to the outer ring: about four blocks, which is far
/// enough to see the corner you are about to round and nothing further.
const RADAR_RANGE: f32 = RADAR_R as f32 * RADAR_CELL;
/// How far out an enemy is still worth putting on the rim. Beyond this the ring
/// fills with a solid band of blips, which is a wall of noise, not a bearing.
const RADAR_BEARING: f32 = RADAR_RANGE * 2.5;
/// Open floor inside the disc. Quieter than a pillar face, so the blips read.
const RADAR_FLOOR: Color = Color::Rgb(38, 22, 56);
/// The centred sector banner is the only other thing that can reach this
/// corner, and it runs to 23 columns ("LANTERN YARD DISCOVERED"). Below this
/// width the two would start to overlap, so the radar is dropped rather than
/// drawn on top of the name. Two radii for the disc, plus the 25 columns the
/// banner needs to the left of it, plus a margin.
const RADAR_MIN_W: i32 = 2 * RADAR_R + 41;
/// Just tall enough for the disc plus a row of margin.
const RADAR_MIN_H: i32 = 2 * RADAR_R + 6;

/// Screen centre of the radar disc within a play area, or `None` when the
/// terminal is too small to hold it.
///
/// Exposed so the layout can be checked from a test without re-deriving the
/// constants, which is how a test ends up disagreeing with the renderer.
pub fn radar_center(area: Rect) -> Option<(i32, i32)> {
    if i32::from(area.width) < RADAR_MIN_W || i32::from(area.height) < RADAR_MIN_H {
        return None;
    }
    let r = RADAR_R;
    Some((area.right() as i32 - r - 1, area.y as i32 + r + 1))
}

/// A local radar in the top corner of the play field.
///
/// The scrolling camera only ever shows a fraction of the arena, and the
/// pillars that matter are usually the ones just off screen. This answers the
/// questions the threat tick cannot: what is around the corner, which way, and
/// for the loot as well as the threat.
///
/// It is a second camera, not a sampled grid: a block is projected and filled as
/// the rectangle it actually is. Testing one point per cell instead looks
/// equivalent and is not — the cell would have to be a lattice cell to be
/// stable, and a lattice cell is three times too coarse to show a lane, so the
/// lanes appear and vanish as MOVA walks past them.
fn radar(buf: &mut Buffer, area: Rect, game: &Game) {
    let Some((cx, cy)) = radar_center(area) else {
        return;
    };
    let r = RADAR_R;
    let r2 = r * r;
    let (px, py) = (game.player.x, game.player.y);
    // Radar cells per world unit, and the inverse for culling.
    let k = r as f32 / RADAR_RANGE;
    let st = Style::new().fg(BLOCK_NEAR).bg(BG);
    // Every offset below is taken the short way round the seam. The arena wraps,
    // so a pillar a unit to MOVA's left can be a large positive coordinate, and
    // measuring it raw would drop it out of range and leave the radar with a
    // hole in it that opens as MOVA crosses and never closes.
    let (hx, hy) = (ARENA_HX, ARENA_HY);
    let off = |x: f32, y: f32| (Arena::delta_axis(px, x, hx), Arena::delta_axis(py, y, hy));

    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                putc(
                    buf,
                    area,
                    cx + dx,
                    cy + dy,
                    '·',
                    Style::new().fg(RADAR_FLOOR).bg(BG),
                );
            }
        }
    }

    // Pillars. The corners are rounded rather than floored so the cell span
    // follows the block's true width, so the lane beside a block is as wide as
    // it is, at every position rather than at the lucky ones.
    for s in &game.solids.solids {
        let (ox, oy) = off(s.x, s.y);
        if ox.abs() > RADAR_RANGE + s.hw || oy.abs() > RADAR_RANGE + s.hh {
            continue;
        }
        let x0 = ((ox - s.hw) * k).round() as i32;
        let x1 = ((ox + s.hw) * k).round() as i32;
        let y0 = ((oy - s.hh) * k).round() as i32;
        let y1 = ((oy + s.hh) * k).round() as i32;
        for j in y0..y1 {
            for i in x0..x1 {
                if i * i + j * j <= r2 {
                    putc(buf, area, cx + i, cy + j, '█', st);
                }
            }
        }
    }

    // Loot only inside the range. There are twenty of them across the whole
    // arena, and pinning every distant one to the rim would leave a permanent
    // ring of blips that says nothing.
    for p in &game.pickups {
        let (ox, oy) = off(p.x, p.y);
        let (dx, dy) = ((ox * k).round() as i32, (oy * k).round() as i32);
        if ox.hypot(oy) > RADAR_RANGE || dx * dx + dy * dy > r2 {
            continue;
        }
        let col = match p.kind {
            crate::pickup::Kind::Core => COOL,
            crate::pickup::Kind::Surge => GLARE,
            crate::pickup::Kind::Heal => PALE,
        };
        putc(
            buf,
            area,
            cx + dx,
            cy + dy,
            p.kind.glyph(),
            Style::new().fg(col).bg(BG),
        );
    }

    // Enemies are the other way round: pulled onto the rim, because knowing
    // which way something is coming from is worth more than a blip you cannot
    // see anyway. Only out to the bearing range, so the ring stays a set of
    // directions rather than a solid band.
    for e in &game.enemies {
        let (ex, ey) = off(e.x, e.y);
        let Some((dx, dy)) = radar_blip(ex, ey, k, r) else {
            continue;
        };
        // Same rule the body on screen uses, so the radar and the arena never
        // disagree about what is urgent.
        let near = ex * ex + ey * ey < 49.0;
        let col = if e.hit > 0.0 || near { HOT } else { RED };
        putc(
            buf,
            area,
            cx + dx,
            cy + dy,
            '▪',
            Style::new().fg(col).bg(BG),
        );
    }

    putc(buf, area, cx, cy, '◆', Style::new().fg(PALE).bg(BG));
}

/// Radar cell for a contact, or `None` if it is too far out to be worth a
/// blip. `ox, oy` are already measured the short way round the seam. Anything
/// past the ring is pulled back onto it, so the rim reads as a bearing rather
/// than as a wall.
fn radar_blip(ox: f32, oy: f32, k: f32, r: i32) -> Option<(i32, i32)> {
    let (dx, dy) = (ox * k, oy * k);
    let d = (dx * dx + dy * dy).sqrt();
    if d > RADAR_BEARING * k {
        return None;
    }
    if d <= r as f32 {
        return Some((dx.round() as i32, dy.round() as i32));
    }
    // One cell short of the rim: rounding afterwards would otherwise push the
    // blip straight back out of the mask.
    let s = (r - 1) as f32 / d;
    let (dx, dy) = ((dx * s).round() as i32, (dy * s).round() as i32);
    (dx * dx + dy * dy <= r * r).then_some((dx, dy))
}

/// A pillar as a solid block with a lit top edge. Drawing the top row brighter
/// is what makes it read as something with height rather than a hole in the
/// floor, and it costs one extra pass over the same cells.
fn draw_solid(buf: &mut Buffer, area: Rect, c: &Cam, g: &SolidGrid, s: &Solid, lit: bool) {
    // The block's centre is folded to the camera's side of the seam once, and the
    // corners are then measured out from it in that already-folded frame. Folding
    // each corner on its own looks equivalent and is not: a block straddling the
    // seam has one corner inside the period and the other a period out, so the
    // two fold to opposite ends of the rectangle and the block is drawn as a
    // 400-cell band across the whole screen.
    //
    // `y - hh` is the far edge, which projects to the smaller screen row. The
    // corners must be taken in that order or the vertical span comes out
    // inverted and every block collapses to a single line.
    let (ox, oy) = c.off(s.x, s.y);
    let (x0, y0) = c.place(ox - s.hw, oy - s.hh);
    let (x1, y1) = c.place(ox + s.hw, oy + s.hh);
    let lx = x0.round() as i32;
    let rx = x1.round() as i32;
    let ty = y0.round() as i32;
    let by = y1.round() as i32;
    // Cull whole blocks before touching the buffer. At the arena's scale most
    // of them are off screen on any given frame.
    if rx < area.x as i32
        || lx >= area.right() as i32
        || by < area.y as i32
        || ty >= area.bottom() as i32
    {
        return;
    }

    // Depth sets the tone, so the far side of the map recedes like the floor
    // does rather than reading as a flat wall of blocks. On the menu screens
    // the whole field is pushed back a further step so it stays backdrop.
    let t = depth(c, (ty as f32 + by as f32) * 0.5) * if lit { 1.0 } else { 0.45 };
    let face = mix(BLOCK_FAR, BLOCK_NEAR, t);
    // The far edge catches the light. One brighter row along the top is what
    // sells the height, and it costs a single extra pass over the same cells.
    let top = mix(face, BLOCK_TOP, if lit { 0.85 } else { 0.4 });

    // Whether the far edge is an edge at all. A solid is larger than the lattice
    // cell it is indexed by, and walls fuse with their neighbours on all eight
    // sides, so a run of them is one mass — and a lit line drawn along the top of
    // every block in it slices that mass into separate slabs, each one pretending
    // to be a free-standing wall with floor behind it. v2 drew the line
    // unconditionally and a map full of it read as loose bricks, not architecture.
    //
    // So it is drawn only where there is floor behind. The cell is recovered from
    // the block's own centre rather than carried on the block: `buckets` is keyed
    // by cell, so a block's index into `solids` is not its cell.
    let (ci, cj) = ((s.x / CELL).round() as i32, (s.y / CELL).round() as i32);
    let open_behind = g.cell_kind(ci, cj - 1) == EMPTY;

    for y in ty..=by {
        for x in lx..=rx {
            putc(buf, area, x, y, '█', Style::new().fg(face).bg(face));
        }
    }
    if open_behind {
        for x in lx..=rx {
            putc(buf, area, x, ty, '█', Style::new().fg(top).bg(face));
        }
    }
}

/// A pickup, with a soft halo so it stays findable across the room and a
/// vertical bob that makes it read as hovering rather than as debris.
fn draw_pickup(buf: &mut Buffer, area: Rect, c: &Cam, p: &Pickup) {
    let (sx, sy) = proj(c, p.x, p.y);
    if sx < area.x as f32
        || sx >= area.right() as f32
        || sy < area.y as f32
        || sy >= area.bottom() as f32
    {
        return;
    }
    let col = match p.kind {
        crate::pickup::Kind::Core => COOL,
        crate::pickup::Kind::Surge => GLARE,
        crate::pickup::Kind::Heal => PALE,
    };
    // Fades up over half a second so a refill is visible as it happens.
    let a = p.settle();

    // Halo first, then the glyph over the top of it.
    putf(
        buf,
        area,
        sx,
        sy - 0.55,
        '·',
        Style::new().fg(mix(BG, col, 0.42 * a)).bg(BG),
    );
    putf(
        buf,
        area,
        sx,
        sy,
        p.kind.glyph(),
        Style::new().fg(mix(BG, col, a)).bg(BG),
    );
}

/// An expanding floor ring. Cheap: it samples a circle and writes cells, so
/// there is no geometry to buffer.
fn draw_ring(buf: &mut Buffer, area: Rect, c: &Cam, r: &crate::score::Ring) {
    let (rad, fade) = r.extent();
    let steps = (rad * 2.4).ceil().max(8.0) as i32;
    let col = mix(BG, if r.mine { GLARE } else { HOT }, fade * 0.5);
    let st = Style::new().fg(col).bg(BG);
    for i in 0..steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        // Sampled around the ring's own centre, which `proj` folds to the camera
        // first: measuring each sample point off the stored centre instead would
        // let a ring opened across the seam wrap its points one by one and come
        // apart into two arcs on opposite edges of the screen.
        let (sx, sy) = proj(c, r.x + a.cos() * rad, r.y + a.sin() * rad);
        putf(buf, area, sx, sy, '·', st);
    }
}

fn draw_enemy(buf: &mut Buffer, area: Rect, c: &Cam, e: &Enemy, game: &Game) {
    let (sx, sy) = proj(c, e.x, e.y);
    if sx < area.x as f32
        || sx >= area.right() as f32
        || sy < area.y as f32
        || sy >= area.bottom() as f32
    {
        return;
    }
    let t = depth(c, sy);
    // Measured the short way round, for the same reason the projection is: an
    // enemy pressed up against MOVA across the seam is touching him, and taking
    // the long way reads it as a whole period away and keeps it the wrong colour.
    let d = game.arena.distance(e.x, e.y, game.player.x, game.player.y);
    // Depth sets the base tone; closing in, or taking a hit, turns it hot.
    let col = if e.hit > 0.0 || d < 7.0 {
        HOT
    } else {
        mix(RED, DIM_RED, t)
    };
    let st = Style::new().fg(col).bg(BG);

    // Brutes are physically bigger in the simulation, so they have to look it.
    // Three cells is the smallest mark that still separates them at a glance.
    if e.kind.heavy() {
        dot(buf, area, sx, sy - 0.30, 0.62, e.kind.glyph(), st);
        dot(buf, area, sx, sy, 0.62, e.kind.glyph(), st);
        dot(buf, area, sx, sy + 0.30, 0.62, e.kind.glyph(), st);
    } else {
        putf(buf, area, sx, sy, e.kind.glyph(), st);
    }
}

fn draw_player(buf: &mut Buffer, area: Rect, c: &Cam, p: &Player) {
    // Blink while invulnerable so the state is unmissable.
    if p.invuln > 0.0 && (p.invuln * 22.0).fract() < 0.5 {
        return;
    }
    let (sx, sy) = proj(c, p.x, p.y);
    // Exactly one cell, always. MOVA used to swell with speed, but a glyph
    // wide enough to straddle a cell boundary comes apart into two or three as
    // it moves, which reads as a second MOVA dragging along behind. Speed is
    // sold by the colour, the embers and the wake instead.
    // His own blue, brightening with speed, white once he is gone. Dashing reads
    // as a white streak because at a hundred and ninety units a second he *is*
    // one, and a dimmer glyph at that speed is a glyph you cannot follow.
    let col = if p.dashing() {
        PALE
    } else if p.blazing() {
        GLARE
    } else {
        mix(DEEP, GLARE, 0.25 + p.speed_ratio() * 0.75)
    };
    putf(buf, area, sx, sy, PLAYER, Style::new().fg(col).bg(BG));
}

fn draw_pop(buf: &mut Buffer, area: Rect, c: &Cam, p: &crate::score::Pop) {
    use crate::score::PopKind;
    let (sx, sy) = proj(c, p.x, p.y);
    let a = (p.life / 0.85).clamp(0.0, 1.0);

    let mut s = String::with_capacity(16);
    s.clear();
    let base = match p.kind {
        PopKind::Ram => {
            s.push_str("RAM+");
            GLARE
        }
        PopKind::Fast => {
            s.push_str("FAST+");
            GLARE
        }
        PopKind::Near => {
            s.push_str("NEAR+");
            COOL
        }
        PopKind::Kill => {
            s.push('+');
            RED
        }
        // Pickups and discoveries. These do not say how fast you were moving,
        // so the prefix would be a lie; just the payout.
        PopKind::Core => {
            s.push_str("CORE+");
            COOL
        }
        PopKind::Surge => {
            s.push_str("SURGE+");
            GLARE
        }
        PopKind::Heal => {
            s.push_str("FIX+");
            PALE
        }
        PopKind::Found => {
            s.push_str("NEW+");
            GLARE
        }
    };
    let _ = write!(s, "{}", p.value);

    let y = (sy - 0.5 - (1.0 - a) * 1.2).round() as i32;
    let n = s.chars().count() as i32;
    let x = sx.round() as i32 - n / 2;
    line(
        buf,
        area,
        x,
        y,
        &s,
        Style::new().fg(mix(base, DIM_RED, (1.0 - a) * 0.8)).bg(BG),
    );
}

/// A word popup. These sit a little higher than score popups and hold for
/// longer, because they carry a name worth reading rather than a number.
fn draw_note(buf: &mut Buffer, area: Rect, c: &Cam, n: &crate::score::Note) {
    use crate::score::PopKind;
    let (sx, sy) = proj(c, n.x, n.y);
    let a = (n.life / 1.5).clamp(0.0, 1.0);
    let col = match n.kind {
        PopKind::Heal => PALE,
        PopKind::Found => GLARE,
        _ => COOL,
    };
    let w = n.text.chars().count() as i32;
    let x = sx.round() as i32 - w / 2;
    let y = (sy - 1.0 - (1.0 - a) * 1.6).round() as i32;
    gap(buf, area, x - 1, y, w + 2);
    line(
        buf,
        area,
        x,
        y,
        n.text,
        Style::new().fg(mix(col, DIM_RED, (1.0 - a) * 0.7)).bg(BG),
    );
}

// ---- environment ----------------------------------------------------------

/// Floor dots on a world-space lattice, every fifth one brighter so a large
/// arena still reads as having structure. `lit` dims the whole field for the
/// menu backdrops.
fn grid(buf: &mut Buffer, area: Rect, c: &Cam, lit: bool) {
    let (w, h) = (f32::from(area.width), f32::from(area.height));
    let hx = w / (2.0 * c.kx) + GRID_STEP;
    let hy = h / (2.0 * c.ky) + GRID_STEP;
    // The lattice is anchored to the world, so the range follows the camera.
    let i0 = ((c.wx - hx) / GRID_STEP).floor() as i32;
    let i1 = ((c.wx + hx) / GRID_STEP).ceil() as i32;
    let j0 = ((c.wy - hy) / GRID_STEP).floor() as i32;
    let j1 = ((c.wy + hy) / GRID_STEP).ceil() as i32;

    for j in j0..=j1 {
        for i in i0..=i1 {
            let (wx, wy) = (i as f32 * GRID_STEP, j as f32 * GRID_STEP);
            let (sx, sy) = proj(c, wx, wy);
            let mark = i.rem_euclid(GRID_EVERY) == 0 && j.rem_euclid(GRID_EVERY) == 0;
            let col = if mark {
                mix(GRID_MARK, GRID_NEAR, depth(c, sy) * 0.7)
            } else {
                mix(GRID_NEAR, GRID_FAR, depth(c, sy))
            };
            // On a torus there is no outside, so the floor is lit everywhere.
            // The check that used to be here asked whether the point fell inside
            // the arena rectangle, and every point does.
            let col = if lit { col } else { dim(col) };
            putf(buf, area, sx, sy, GRID, Style::new().fg(col).bg(BG));
        }
    }
}

// ---- hud ------------------------------------------------------------------

/// Blanks a run of cells so HUD text can sit on top of a border line
/// without the line showing through the gaps.
fn gap(buf: &mut Buffer, area: Rect, x: i32, y: i32, n: i32) {
    for i in 0..n {
        putc(buf, area, x + i, y, ' ', Style::new().bg(BG));
    }
}

fn hud(buf: &mut Buffer, area: Rect, inner: Rect, game: &Game) {
    let top = area.y as i32;
    let bot = area.bottom() as i32 - 1;
    let left = area.x as i32 + 2;
    let right = area.right() as i32 - 2;

    // --- top row
    line(buf, area, left, top, "MOVA", Style::new().fg(COOL).bg(BG));
    gap(buf, area, left + 4, top, 1);
    for i in 0..game.player.max_hp {
        let on = i < game.player.hp;
        let (ch, col) = if on {
            (HP_FULL, RED)
        } else {
            (HP_EMPTY, Color::Rgb(46, 20, 42))
        };
        putc(
            buf,
            area,
            left + 5 + i as i32,
            top,
            ch,
            Style::new().fg(col).bg(BG),
        );
    }

    // Where the centred score below will start, and so the first column it
    // blanks. Everything left of that gap on the top row is measured against
    // it, because these strings change length as the run goes on and a fixed
    // width gate quietly erases the tail of a long one.
    let mut s = String::with_capacity(20);
    s.clear();
    let _ = write!(s, "SCORE {:06}", game.score.points);
    let room = area.x as i32 + (area.width as i32 - s.chars().count() as i32) / 2 - 2;

    // Two pieces of persistent context share the space left of the score. The
    // sector name wins when only one of them fits: knowing where you are is
    // worth more on screen than a rank you already know by heart.
    let (rank, name) = (
        save::rank_name(game.save.total_score),
        region::NAMES[game.sector],
    );
    let x = left + 5 + game.player.max_hp as i32 + 1;
    let (rank_w, name_w) = (rank.chars().count() as i32, name.chars().count() as i32);

    if x + name_w <= room {
        gap(buf, area, x - 1, top, name_w + 1);
        let fresh = game.save.sectors & (1 << game.sector) != 0;
        line(
            buf,
            area,
            x,
            top,
            name,
            Style::new()
                .fg(if fresh { COOL } else { Color::Rgb(38, 54, 104) })
                .bg(BG),
        );
    } else if x + 2 + rank_w <= room {
        let rx = x + 2;
        gap(buf, area, rx - 1, top, rank_w + 1);
        line(buf, area, rx, top, rank, Style::new().fg(FAINT).bg(BG));
    }

    center(buf, area, top, &s, Style::new().fg(PALE).bg(BG));

    s.clear();
    let _ = write!(s, "WAVE {:02}", game.wave);
    line(
        buf,
        area,
        right - s.chars().count() as i32,
        top,
        &s,
        Style::new().fg(RED).bg(BG),
    );

    let sc = &game.score;
    // Width is known without building the string: "COMBO x" plus the digits.
    let combo_w = 7 + digits(sc.combo);
    let combo_x = area.x as i32 + (area.width as i32 - combo_w) / 2;

    // --- bottom row
    let p = &game.player;
    let ratio = p.speed_ratio();
    // Above the ram threshold the whole speed readout goes hot; at the cap it
    // burns out to white. That is the gauge reporting the two states that
    // change the rules rather than adding icons for them.
    let armed = p.armed();
    let blazing = p.blazing();
    s.clear();
    let _ = write!(s, "SPD {:03}", (ratio * 100.0) as u32);
    let spd_len = s.chars().count() as i32;
    let spd_col = if blazing || armed { GLARE } else { PALE };
    line(buf, area, left, bot, &s, Style::new().fg(spd_col).bg(BG));

    // The bar is the first thing to go when the window is too narrow to hold
    // it without colliding with the combo readout.
    let bx = left + spd_len + 1;
    gap(buf, area, left + spd_len, bot, 1);
    if bx + 9 < combo_x {
        for i in 0..8 {
            let on = ratio * 8.0 >= (i + 1) as f32;
            let col = if on {
                if blazing {
                    PALE
                } else {
                    mix(DEEP, GLARE, ratio)
                }
            } else {
                Color::Rgb(44, 22, 60)
            };
            putc(
                buf,
                area,
                bx + i,
                bot,
                if on { SPD_FULL } else { SPD_EMPTY },
                Style::new().fg(col).bg(BG),
            );
        }
        // The cell where contact turns lethal is drawn as a tick, so the
        // threshold is a place on the gauge and not a number to memorise.
        let rx = bx + (crate::game::RAM_AT * 8.0) as i32;
        putc(
            buf,
            area,
            rx,
            bot - 1,
            if armed { '┃' } else { '┆' },
            Style::new().fg(if armed { GLARE } else { DEEP }).bg(BG),
        );
        gap(buf, area, bx + 8, bot, 1);
    }

    let combo_col = if sc.combo == 0 {
        FAINT
    } else if sc.charge() > 0.4 {
        GLARE
    } else {
        DIM_RED
    };
    s.clear();
    let _ = write!(s, "COMBO x{}", sc.combo);
    line(
        buf,
        area,
        combo_x,
        bot,
        &s,
        Style::new().fg(combo_col).bg(BG),
    );

    let p = &game.player;
    s.clear();
    let dash = if p.dashing() {
        s.push_str("DASH");
        GLARE
    } else if p.dash_ready() {
        s.push_str("DASH READY");
        COOL
    } else {
        let _ = write!(s, "DASH {:.1}", p.dash_cd / DASH_CD);
        DIM_RED
    };
    line(
        buf,
        area,
        right - s.chars().count() as i32,
        bot,
        &s,
        Style::new().fg(dash).bg(BG),
    );

    // Surge meter, tucked in above the dash readout. Only drawn while it is
    // live so it costs nothing on a normal run.
    if p.surging() {
        let left = p.surge.ceil() as i32;
        let y = bot - 2;
        if area.width >= 60 {
            let mut s = String::with_capacity(16);
            let _ = write!(s, "SURGE {}s", left);
            let w = s.chars().count() as i32;
            let lx = right - w;
            gap(buf, area, lx - 1, y, w + 1);
            line(
                buf,
                area,
                lx,
                y,
                &s,
                Style::new()
                    .fg(mix(GLARE, COOL, 1.0 - p.surge / Player::SURGE_TIME))
                    .bg(BG),
            );
        }
    }

    let _ = inner;
}
