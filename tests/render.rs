//! Headless checks on the frames themselves.
//!
//! These exist because several layout bugs got through code review, and code
//! review is not a rendering engine: two centred strings sharing a row, a goal
//! column sized by an eyeballed constant, a corner panel sized by arithmetic
//! nobody re-ran. Each one is now pinned by a test.
//!
//! Run with `cargo test`.

use mova::game::{Game, Phase};
use mova::input::Input;
use mova::renderer;
use mova::save::Save;
use mova::solid::SolidGrid;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// The radar's self marker. Nothing else in the renderer emits this glyph,
/// which is what lets these tests find the disc without a second opinion about
/// which pixels belong to it.
const SELF: char = '◆';

/// MOVA's own body glyph. Counted rather than eyeballed, because the bug being
/// pinned here was MOVA being drawn as two or three of these.
const BODY: char = '●';

/// Renders one frame and hands back the buffer.
fn frame(w: u16, h: u16, game: &Game) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    renderer::render(&mut buf, area, game);
    buf
}

/// The frame as characters, so a test can address a cell by column the way a
/// reader counts it. Char rather than byte, because the radar marker and the
/// pillars are both multi-byte.
fn cells(buf: &Buffer) -> Vec<Vec<char>> {
    let area = buf.area;
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect()
        })
        .collect()
}

/// The same frame as text, which is what "does this line say X" wants.
fn lines(buf: &Buffer) -> Vec<String> {
    cells(buf).iter().map(|r| r.iter().collect()).collect()
}

/// Joins the frame into one blob for a failure message.
fn dump(buf: &Buffer) -> String {
    lines(buf).join("\n")
}

/// A started run, simulated far enough in that enemies and pickups are out.
fn playing(w: u16, h: u16) -> Game {
    let area = Rect::new(0, 0, w, h);
    let mut game = Game::new(Save::default());
    let (hx, hy) = renderer::view_half(area);
    game.set_view(hx, hy);
    game.start();
    let mut keys = Input::default();
    for i in 0..600 {
        keys.right = (i / 40) % 2 == 0;
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
    }
    game
}

/// The radar has to sit in the corner, centred on MOVA, without eating the
/// frame. An off-by-one that puts the disc one column too far right is exactly
/// the kind of thing that only shows up when you count columns.
///
/// The placement is asked of the renderer rather than re-derived here: a test
/// that hard-codes the radius just fails every time the radius is retuned,
/// which is the opposite of a regression guard.
#[test]
fn radar_is_centred_and_leaves_the_frame_alone() {
    let (w, h) = (100u16, 30u16);
    let g = cells(&frame(w, h, &playing(w, h)));

    // `Block::bordered` eats a row and a column each side.
    let inner = Rect::new(1, 1, w - 2, h - 2);
    let (cx, cy) = renderer::radar_center(inner).expect("terminal is wide enough for the radar");
    let (cx, cy) = (cx as usize, cy as usize);
    assert_eq!(g[cy][cx], SELF, "radar centre is not on MOVA");

    // Exactly one disc, not two drawn on top of each other.
    let marks = g.iter().flatten().filter(|c| **c == SELF).count();
    assert_eq!(marks, 1, "expected one radar, found {marks}");

    // The frame belongs to the frame. Nothing the radar draws may land on it.
    let (wm, hm) = (w as usize, h as usize);
    assert_eq!(g[0][0], '┌', "top-left corner");
    assert_eq!(g[0][wm - 1], '┐', "top-right corner");
    assert_eq!(g[hm - 1][0], '└', "bottom-left corner");
    assert_eq!(g[hm - 1][wm - 1], '┘', "bottom-right corner");
    for (y, row) in g.iter().enumerate().take(hm - 1).skip(1) {
        assert_eq!(row[0], '│', "left border broken on row {y}");
        assert_eq!(row[wm - 1], '│', "right border broken on row {y}");
    }
}

/// MOVA is one glyph at every speed.
///
/// The body used to swell with the speed ratio, and a glyph wide enough to
/// straddle a cell boundary comes apart into two or three as it moves — which
/// reads as a second MOVA dragging along behind the first. The split depended
/// on where in the cell MOVA happened to be standing, so it flickered on and
/// off with every step. This sweeps the position and the speed together,
/// because neither axis alone produces a duplicate.
#[test]
fn the_player_is_always_exactly_one_cell() {
    let (w, h) = (100u16, 30u16);
    for step in 0..60 {
        for (ratio, dashing) in [
            (0.0f32, false),
            (0.3, false),
            (0.55, false),
            (0.8, false),
            (1.0, false),
            (1.0, true),
        ] {
            let mut game = Game::new(Save::default());
            let (hx, hy) = renderer::view_half(Rect::new(0, 0, w, h));
            game.set_view(hx, hy);
            game.start();
            // Stride the position by a fraction of a cell, which is the axis
            // that decides whether a multi-cell mark collapses or splits.
            game.player.x = -30.0 + step as f32 * 0.37;
            game.player.y = -8.0 + step as f32 * 0.11;
            game.player.vx = game.player.cap() * ratio;
            if dashing {
                game.player.dash_t = 1.0;
            }
            let buf = frame(w, h, &game);
            let n = cells(&buf).iter().flatten().filter(|c| **c == BODY).count();
            assert_eq!(
                n,
                1,
                "{n} copies of MOVA at ratio {ratio} dashing {dashing}, \
                 position {step}\n{}",
                dump(&buf)
            );
        }
    }
}

/// The radar is a navigation aid, not a fixture. Below the width where it can
/// sit clear of the centred sector banner it is dropped entirely rather than
/// drawn over the top of it.
#[test]
fn radar_is_dropped_on_a_narrow_terminal() {
    let (w, h) = (40u16, 24u16);
    let buf = frame(w, h, &playing(w, h));
    assert!(
        !lines(&buf).iter().any(|l| l.contains(SELF)),
        "radar drawn on a {w}-column terminal, where it would cover the sector banner:\n{}",
        dump(&buf)
    );
}

/// And it is a toggle, not a permanent fixture.
#[test]
fn radar_can_be_switched_off() {
    let (w, h) = (100u16, 30u16);
    let mut game = playing(w, h);
    assert!(
        lines(&frame(w, h, &game)).iter().any(|l| l.contains(SELF)),
        "radar missing on a terminal wide enough for it"
    );
    game.radar = false;
    assert!(
        !lines(&frame(w, h, &game)).iter().any(|l| l.contains(SELF)),
        "radar still drawn after being switched off"
    );
}

/// The radar has to be an exact projection of the map, not a resampling of it.
///
/// The pillar lattice is twelve units and every block sits on a lattice
/// position, so walking exactly twelve units puts you somewhere the pillar field
/// is identical, just moved. A radar that projects geometry reproduces itself
/// under that translation. One that samples the map a cell at a time does not:
/// the four-unit lanes are the same width as its cell, so they drop in and out
/// of the disc as you walk, and this is where that shows up.
#[test]
fn radar_is_a_projection_and_not_a_resampling() {
    let (w, h) = (110u16, 34u16);
    let mut game = playing(w, h);
    // Pinned, because the map is generated fresh each run and this test needs
    // pillars around a fixed spot rather than whatever the last run rolled.
    game.solids = SolidGrid::new(0x4D4F_5641);

    // Well inside the arena, so the disc is ringed by map on every side rather
    // than clipped by the arena wall, and in a part of the pinned map that has
    // both pillars and lanes right across the disc. An all-floor spot would
    // satisfy the translation check for free.
    game.player.x = -36.0;
    game.player.y = -24.0;
    let (cx, cy) = renderer::radar_center(Rect::new(1, 1, w - 2, h - 2))
        .expect("terminal is wide enough for the radar");

    // The disc's block pattern, as rows of booleans, `None` outside the disc.
    let disc = |g: &Game| -> Vec<Vec<Option<bool>>> {
        let buf = frame(w, h, g);
        let cells = cells(&buf);
        (0..(2 * RADAR_DISC_HALF + 1))
            .map(|j| {
                (0..(2 * RADAR_DISC_HALF + 1))
                    .map(|i| {
                        let dx = i - RADAR_DISC_HALF;
                        let dy = j - RADAR_DISC_HALF;
                        let inside = dx * dx + dy * dy <= RADAR_DISC_HALF * RADAR_DISC_HALF;
                        inside.then(|| {
                            let ch = cells[(cy + dy) as usize][(cx + dx) as usize];
                            ch == '█'
                        })
                    })
                    .collect()
            })
            .collect()
    };

    let before = disc(&game);
    game.player.x += 12.0;
    let after = disc(&game);

    // Twelve world units is three radar cells at four units each, and the world
    // slides left across the disc as MOVA walks right.
    const SHIFT: i32 = 3;
    let mut compared = 0;
    let mut blocks = 0;
    let mut lanes = 0;
    for j in 0..(2 * RADAR_DISC_HALF + 1) {
        for i in 0..(2 * RADAR_DISC_HALF + 1) {
            let (dx, dy) = (i - RADAR_DISC_HALF, j - RADAR_DISC_HALF);
            let d2 = dx * dx + dy * dy;
            // Skip the rim: a cell there has different map behind it on the
            // other side of the translation, so it is clipped rather than
            // compared. `None` marks outside the disc.
            if d2 > (RADAR_DISC_HALF - 4) * (RADAR_DISC_HALF - 4) {
                continue;
            }
            // The radar draws MOVA's own marker over the centre, so the centre
            // is not a reading of the map at all. That rules out the centre
            // itself and the one cell whose partner is the centre after the
            // shift. Leaving either in makes the test fail against a correct
            // radar whenever a pillar happens to sit under MOVA.
            if (dx, dy) == (0, 0) || (dx, dy) == (SHIFT, 0) {
                continue;
            }
            let here = before[j as usize][i as usize];
            let there = after[j as usize]
                .get((i - SHIFT) as usize)
                .copied()
                .flatten();
            assert_eq!(
                here, there,
                "pillar pattern differs at cell {i},{j} after a lattice shift"
            );
            blocks += usize::from(here == Some(true));
            lanes += usize::from(here == Some(false));
            compared += 1;
        }
    }
    assert!(
        compared > 40,
        "test proved nothing: only {compared} cells compared"
    );
    // Both kinds have to be there. A run of floor is trivially self-similar, so
    // the shift above would pass with the projection replaced by anything, and a
    // disc that is nothing but pillar is too coarse to tell a projection from a
    // nearest-neighbour sample either.
    assert!(
        blocks > 12 && lanes > 12,
        "test proved nothing: {blocks} pillar cells and {lanes} lane cells"
    );
}

/// Half the radar's width in cells, as a test constant. `radar_center` gives the
/// centre; this gives the disc's extent, which no caller should have to know.
const RADAR_DISC_HALF: i32 = 8;

/// `center` blanks the cells it is about to write, so two centred strings on one
/// row means the second silently erases the first. The menu used to lose its
/// whole lifetime-stats line this way, and nothing in the code said so.
#[test]
fn menu_does_not_overdraw_its_own_stats() {
    let save = Save {
        best_wave: 7,
        best_combo: 12,
        total_rams: 34,
        ..Save::default()
    };
    let buf = frame(90, 26, &Game::new(save));
    let lines = lines(&buf);

    let stats = lines
        .iter()
        .position(|l| l.contains("WAVE 07") && l.contains("COMBO x12") && l.contains("34 RAMS"))
        .unwrap_or_else(|| panic!("lifetime stats are not on the menu:\n{}", dump(&buf)));
    let tag = lines
        .iter()
        .position(|l| l.contains("MOVE FAST"))
        .unwrap_or_else(|| panic!("menu tagline missing:\n{}", dump(&buf)));
    assert_ne!(
        stats, tag,
        "tagline and stats share a row, so one erases the other"
    );
}

/// The goal ledger is two columns laid out from the longest label. When the v2
/// goals arrived with longer names than v1's, a fixed half-width column ran the
/// right-hand one straight off the frame.
#[test]
fn every_goal_label_fits_inside_the_frame() {
    let area = Rect::new(0, 0, 96, 26);
    let mut buf = Buffer::empty(area);
    renderer::goals(&mut buf, area, &Game::new(Save::default()));
    let lines = lines(&buf);

    for (i, g) in mova::goal::GOALS.iter().enumerate() {
        assert!(
            lines.iter().any(|l| l.contains(g.label)),
            "goal {i} ({}) does not fit in the ledger:\n{}",
            g.label,
            dump(&buf)
        );
    }
}

/// The game-over screen stacks a run summary, a roam summary and a goal strip.
/// Same failure mode as the menu: a row written twice means one of them is gone.
#[test]
fn game_over_screen_shows_run_and_roam_progress() {
    let (w, h) = (100u16, 30u16);
    let mut game = Game::new(Save::default());
    game.save.total_score = 130_000;
    game.save.goals = 0b0000_0000_0011_0111;
    game.new_goals = 0b0000_0000_0011_0000;
    game.stats.kills = 41;
    game.stats.rams = 9;
    game.stats.cores = 22;
    game.stats.sectors = 3;
    game.phase = Phase::Over;
    let buf = frame(w, h, &game);
    let lines = lines(&buf);

    for needle in ["RUN ENDED", "41 KILLS", "3 SECTORS", "22 CORES"] {
        assert!(
            lines.iter().any(|l| l.contains(needle)),
            "game-over screen is missing {needle:?}:\n{}",
            dump(&buf)
        );
    }
    // A freshly unlocked goal is the whole point of the strip.
    let first = mova::goal::GOALS
        .iter()
        .find(|g| game.new_goals & g.bit != 0)
        .map(|g| g.label)
        .expect("test fixture unlocked nothing");
    assert!(
        lines.iter().any(|l| l.contains(first)),
        "newly unlocked goal {first:?} not shown:\n{}",
        dump(&buf)
    );
}
