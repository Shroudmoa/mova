//! The map has names.
//!
//! Six sectors, so wherever MOVA is, the readout can say it is somewhere else
//! from where it was. This is what turns "big empty rectangle" into "place".

pub const COLS: i32 = 3;
pub const ROWS: i32 = 2;
pub const COUNT: usize = (COLS * ROWS) as usize;

/// Every bit set: used by the cartographer goal.
pub const ALL: u32 = (1 << COUNT) - 1;

/// Indexed by `at`, left to right then bottom to top.
pub const NAMES: [&str; COUNT] = [
    "LANTERN YARD",
    "THE SPINE",
    "DEEP WELL",
    "ASH MILE",
    "QUIET GATE",
    "HOLLOW MILE",
];

/// Sector names are laid out so the spawn plaza sits in the middle column of
/// the lower row, and the furthest corner from it is the last one charted.
pub const SPAWN_SECTOR: usize = 1;

/// Which sector a world position falls in.
#[inline]
pub fn at(x: f32, y: f32, hx: f32, hy: f32) -> usize {
    let col = (((x + hx) / (2.0 * hx)) * COLS as f32).floor();
    let row = (((y + hy) / (2.0 * hy)) * ROWS as f32).floor();
    let col = (col as i32).clamp(0, COLS - 1);
    let row = (row as i32).clamp(0, ROWS - 1);
    (row * COLS + col) as usize
}
