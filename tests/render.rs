//! Headless checks on the frames themselves.
//!
//! These exist because several layout bugs got through code review, and code
//! review is not a rendering engine: two centred strings sharing a row, a goal
//! column sized by an eyeballed constant, a corner panel sized by arithmetic
//! nobody re-ran. Each one is now pinned by a test.
//!
//! Run with `cargo test`.

use mova::enemy::{Enemy, Kind};
use mova::game::{Arena, Game, Phase, ARENA_HX, ARENA_HY};
use mova::input::Input;
use mova::renderer;
use mova::save::Save;
use mova::solid::{SolidGrid, CELL};

use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::layout::Rect;

/// The radar's self marker. Nothing else in the renderer emits this glyph,
/// which is what lets these tests find the disc without a second opinion about
/// which pixels belong to it.
const SELF: char = '◆';

/// MOVA's own body glyph. Counted rather than eyeballed, because the bug being
/// pinned here was MOVA being drawn as two or three of these.
const BODY: char = '●';

/// A furniture block. The renderer emits this for every cell of a block's
/// footprint, which is what makes it countable: a block on screen is a filled
/// rectangle of these, and nothing else draws one.
const BLOCK: char = '█';
/// The three enemy kinds' glyphs. Nothing else in the renderer emits them, which
/// is what lets the palette test name an enemy rather than guess at one.
/// A burning ember off a blazing MOVA. Also the goal marker glyph, which is why
/// this test reaches it through the particle list rather than off the frame.
const FLAME: char = '▲';

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

/// Where a world point lands on screen.
///
/// Derived from `view_half`, which is the renderer's own statement of how much
/// world fits across the frame. Not restated here: a test that re-derives the
/// scale factor re-derives it wrongly the moment someone retunes the renderer,
/// which is the opposite of a regression guard. The offset — one cell in from the
/// border, then half the play field — is the frame box the renderer draws into.
///
/// The offset from the camera is taken with `Arena::delta_axis`, because the
/// arena is a torus and that is the only distance on it that means anything. In
/// the middle of the map the two agree, which is why a raw subtraction can look
/// right for most of a run.
fn project(w: u16, h: u16, cam: (f32, f32), at: (f32, f32)) -> (f32, f32) {
    let (hx, hy) = renderer::view_half(Rect::new(0, 0, w, h));
    let (kx, ky) = (f32::from(w - 2) / (2.0 * hx), f32::from(h - 2) / (2.0 * hy));
    (
        1.0 + f32::from(w - 2) / 2.0 + Arena::delta_axis(cam.0, at.0, ARENA_HX) * kx,
        1.0 + f32::from(h - 2) / 2.0 + Arena::delta_axis(cam.1, at.1, ARENA_HY) * ky,
    )
}

/// A block covers the rectangle it occupies, and nothing but that rectangle.
///
/// This is the property every other thing on screen is measured against. The
/// lane widths, whether two posts leave a gap worth threading, whether the map
/// reads as sparse or as a thicket — all of it is this, read off a drawn block.
/// A block drawn as stripes or as a smudge makes the whole map illegible and no
/// amount of good generation rescues it.
///
/// On a lone post, and located by projecting it rather than by hunting the frame
/// for something that looks like one. A fused wall's drawn outline is wherever
/// its neighbour happened to stop and `erode` notches it, so "a run of blocks is
/// a rectangle" is false and is not what is claimed; a post standing in open
/// floor is the one piece of furniture whose rectangle has boundaries on all
/// four sides. Nor is the frame scanned for candidates: the radar draws one `█`
/// per pillar cell and three of those in an L are not a rectangle, and the speed
/// gauge is a rectangle that is not a block.
#[test]
fn a_block_draws_as_exactly_its_own_rectangle() {
    for (w, h) in [(100u16, 30u16), (140, 44), (80, 24)] {
        let mut game = playing(w, h);
        game.solids = SolidGrid::new(0x4D4F_5641);

        // A post with clear floor all the way round, so it draws on its own.
        // Compared against the blocks rather than probed through `hit`, because a
        // wall is seven and a half units to a side: its neighbour cell's *centre*
        // is four units clear of it, so every probe at a centre comes back empty
        // and the post reads as isolated while a wall is touching it.
        let blocks = &game.solids.solids;
        let post = blocks
            .iter()
            .enumerate()
            .find(|(n, s)| {
                !s.wall()
                    && blocks.iter().enumerate().all(|(m, o)| {
                        m == *n
                            || (s.x - o.x).abs() >= s.hw + o.hw + 3.0
                            || (s.y - o.y).abs() >= s.hh + o.hh + 3.0
                    })
            })
            .map(|(_, s)| *s)
            .expect("the map has a lone post");

        // Standing MOVA off to one side rather than on it. His glyph is drawn
        // over the furniture, so putting him on the block punches a hole in the
        // very rectangle being measured. The camera is set rather than settled:
        // it eases, and an eased camera would leave a residue between where the
        // test thinks MOVA is and where the renderer puts him.
        game.player.x = post.x + 26.0;
        game.player.y = post.y;
        game.cam_x = game.player.x;
        game.cam_y = game.player.y;

        let buf = frame(w, h, &game);
        let g = cells(&buf);
        let (px, py) = project(w, h, (game.cam_x, game.cam_y), (post.x, post.y));
        let (sx, sy) = (px.round() as usize, py.round() as usize);
        assert!(
            g[sy][sx] == BLOCK,
            "at {w}x{h} the post did not draw at ({sx},{sy})\n{}",
            dump(&buf)
        );

        // Flood the run of blocks around it. A post is a filled rectangle and
        // the floor round it is floor, so the run stops at its edges.
        let (h_, w_) = (g.len(), g[0].len());
        let mut seen = std::collections::BTreeSet::new();
        let mut stack = vec![(sx, sy)];
        while let Some((x, y)) = stack.pop() {
            if x >= w_ || y >= h_ || g[y][x] != BLOCK || !seen.insert((x, y)) {
                continue;
            }
            stack.extend([
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ]);
        }
        let [lo, to, hi, bo] = [
            seen.iter().map(|c| c.0).min().unwrap(),
            seen.iter().map(|c| c.1).min().unwrap(),
            seen.iter().map(|c| c.0).max().unwrap(),
            seen.iter().map(|c| c.1).max().unwrap(),
        ];
        let (cw, ch) = (hi - lo + 1, bo - to + 1);
        assert_eq!(
            seen.len(),
            cw * ch,
            "at {w}x{h} the post drew {} cells inside a {cw}x{ch} box: not a filled \\
             rectangle\\n{}",
            seen.len(),
            dump(&buf)
        );

        // And it is the size the block says it is. `draw_solid` fills from the rounded
        // near corner to the rounded far corner *inclusive*, so a block draws as
        // its span plus the cell its two ends sit in — a hair generous on each
        // side, and the reason two blocks in a row meet with no seam of
        // background between them. One cell of rounding either side of that is
        // the whole budget. Past it, the drawn size has come adrift from the
        // collision geometry, which is what this is here to notice: the lane
        // widths the map is judged by are the drawn sizes.
        let (kx, ky) = {
            let (hx, hy) = renderer::view_half(Rect::new(0, 0, w, h));
            (f32::from(w - 2) / (2.0 * hx), f32::from(h - 2) / (2.0 * hy))
        };
        let (want_w, want_h) = (2.0 * post.hw * kx + 1.0, 2.0 * post.hh * ky + 1.0);
        assert!(
            (cw as f32 - want_w).abs() <= 1.0 && (ch as f32 - want_h).abs() <= 1.0,
            "at {w}x{h} the post drew {cw}x{ch} cells but is {want_w:.1}x{want_h:.1} \\
             on screen\\n{}",
            dump(&buf)
        );
    }
}

/// The seam does not empty the frame.
///
/// The arena is a torus, so a pillar a few units to MOVA's left is *stored* most
/// of a period to the right of him. Subtracted raw, that reads as a period away:
/// culled, and the play field goes bare — on the side the map is coming back
/// round on, which is where MOVA is running. Every coordinate the simulation
/// holds is folded into the fundamental rectangle, and the projection has to fold
/// before it measures or the renderer is describing a map that does not exist.
///
/// The camera is walked right round the torus and every block whose centre lands
/// on screen is asked whether it was drawn. This held in the middle of the map
/// and nowhere else, which is what made it read as "leaving the centre makes
/// everything disappear": on one fixed map at 100x30 the same frame held 927 of
/// the 1513 block cells it should have at the seam and every one of them 1545 up
/// to the east of it.
#[test]
fn crossing_the_seam_does_not_empty_the_frame() {
    let (w, h) = (100u16, 30u16);
    let area = Rect::new(0, 0, w, h);
    let inner = Rect::new(1, 1, w - 2, h - 2);

    // The corners and both seam lines, because the seam is at +hx on each axis
    // and the bug is invisible everywhere else. Asymmetric signs on purpose: the
    // fold is not symmetric-looking to a raw subtraction, so one corner on its own
    // only catches half of it.
    let spots = [
        (0.0, 0.0),
        (ARENA_HX * 0.5, 0.0),
        (ARENA_HX - 8.0, 0.0),
        (-ARENA_HX + 8.0, 0.0),
        (0.0, ARENA_HY - 8.0),
        (0.0, -ARENA_HY + 8.0),
        (ARENA_HX - 8.0, ARENA_HY - 8.0),
        (-ARENA_HX + 8.0, -ARENA_HY + 8.0),
    ];

    for seed in [0x5EED_1234u32, 0x4D4F_5641] {
        let mut game = Game::new(Save::default());
        let (hx, hy) = renderer::view_half(area);
        game.set_view(hx, hy);
        game.start();
        game.solids = SolidGrid::new(seed);

        // Furniture only, so nothing else on the frame can stand in for a block
        // and no banner can be mistaken for one overwriting it.
        game.enemies.clear();
        game.pickups.clear();
        game.bullets.clear();
        game.fx.clear();
        game.radar = false;
        game.banner_t = 0.0;
        game.sector_t = 0.0;
        game.hint_t = 0.0;

        for (px, py) in spots {
            // Both set rather than settled: the camera eases, and an eased camera
            // would leave the test measuring a view MOVA is not standing in.
            game.player.x = px;
            game.player.y = py;
            game.cam_x = px;
            game.cam_y = py;

            let buf = frame(w, h, &game);
            let g = cells(&buf);
            for s in &game.solids.solids {
                let (fx, fy) = project(w, h, (px, py), (s.x, s.y));
                let (sx, sy) = (fx.round() as i32, fy.round() as i32);
                if sx < i32::from(inner.x)
                    || sx >= i32::from(inner.right())
                    || sy < i32::from(inner.y)
                    || sy >= i32::from(inner.bottom())
                {
                    continue;
                }
                assert_eq!(
                    g[sy as usize][sx as usize],
                    BLOCK,
                    "seed {seed:#x}, camera at ({px:.1},{py:.1}): the block at \
                     ({:.1},{:.1}) projects to ({sx},{sy}) and was not drawn\n{}",
                    s.x,
                    s.y,
                    dump(&buf)
                );
            }
        }
    }
}

/// MOVA is not the colour of the things that want him dead.
///
/// This is a design rule, not a preference, and it is the one that v3 broke
/// first: he was drawn in the same red as the enemies, which at a hundred and
/// ninety units a second left the player answering "is that me or is that the
/// thing chasing me" by looking closely at a moving glyph. His trail was the
/// same red too, and the trail is longer than the enemy.
///
/// Checked on the frame, on every state MOVA can be in, against every enemy
/// kind on screen. Not a constant compared to a constant: a test that reads
/// `renderer::COOL` and asserts it is blue proves nothing about the frame, and
/// would still pass if `draw_player` went back to red.
///
/// Warm and cold are read as hue families rather than as exact triples, because
/// the renderer mixes along ramps — MOVA at speed and MOVA at rest are different
/// blues, and an equality check would only pass for one of them.
#[test]
fn mova_is_cold_and_everything_that_hurts_him_is_warm() {
    /// Blue-dominant: MOVA and his own effects.
    fn is_cold(c: ratatui::style::Color) -> bool {
        match c {
            ratatui::style::Color::Rgb(r, g, b) => b > r && b > g,
            _ => false,
        }
    }
    /// Red-dominant: enemies, damage, the past of a hit.
    fn is_warm(c: ratatui::style::Color) -> bool {
        match c {
            ratatui::style::Color::Rgb(r, g, b) => r > b && r > g,
            _ => false,
        }
    }

    let (w, h) = (120u16, 38u16);

    // A run with MOVA blazing in open ground, a tail laid down behind him, and
    // enemies on the board.
    //
    // `playing` alone will not do. It rolls a fresh map every run, and MOVA can
    // come out of it pressed flat against a wall — at the cap, the whole map
    // behind him, and not one unit of ground under him. `burn` measures ground
    // covered, so pinned he throws no embers at all while blazing brightly, which
    // is exactly correct and makes this test prove nothing.
    //
    // So he goes back to the plaza, which the generator clears three cells in
    // every direction, and is driven along +x for just long enough to lay a tail
    // inside it: at the cap, forty steps is fifteen units, eleven embers at the
    // one-and-a-bit-unit gap, and the plaza runs to about nineteen.
    let blazing = |steps: u32| -> Game {
        let mut g = playing(w, h);
        g.player.x = 0.0;
        g.player.y = 0.0;
        g.cam_x = 0.0;
        g.cam_y = 0.0;
        let mut keys = Input::default();
        for _ in 0..steps {
            // Above the cap, so drag inside the step cannot leave him a hair under
            // it and the clamp then lands him exactly on it. Every step, so a
            // bounce cannot bleed him below — the same trick, and for the same
            // reason, as `run_at_the_cap` in the play tests.
            g.player.vx = g.player.cap() * 1.5;
            g.player.vy = 0.0;
            g.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
        }
        g
    };

    // The flag says whether this case is required to have laid down a wake. Only
    // the two built by `blazing` are: a plain `playing` ends wherever it ends, and
    // a MOVA who happens not to be at his cap has no trail to be cold.
    // Three enemies, one of each kind, standing where they will be seen.
    //
    // Staged rather than hoped for. `playing` rolls a fresh map and a fresh
    // spawn every run, so on some runs nothing is inside the view at all, and a
    // palette test that quietly passes when there is no enemy on screen is a
    // test that proves nothing on those runs. It is also the only way all three
    // kinds get checked every time: a wave that has not thrown a brute yet would
    // otherwise skip that branch of the renderer run after run, and it is the
    // branch that draws the biggest mark on screen.
    //
    // Placed after the last update, so nothing simulates them. The point is the
    // colour the renderer gives each glyph, and an enemy that wandered into a
    // block or into MOVA before the frame was drawn would be a worse test of
    // that. The close one is five units out rather than on top of him, because
    // MOVA draws last and would paint over an enemy sharing his cell.
    let stage = |mut g: Game| {
        g.enemies = [
            // Point blank. The renderer turns an enemy warm-hot inside seven
            // units, which is a second and much louder colour than the depth
            // ramp, and it is the branch that matters most — it is what you see
            // in the last frame before something hits you. Left unexercised, a
            // change to it here would pass this test on every run.
            (Kind::Grunt, 5.0, 2.0),
            (Kind::Runner, -14.0, 10.0),
            (Kind::Brute, 6.0, 20.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(n, (kind, dx, dy))| {
            Enemy::new(
                g.player.x + dx,
                g.player.y + dy,
                kind,
                20.0,
                n as f32 * 2.1,
                if n % 2 == 0 { 1.0 } else { -1.0 },
            )
        })
        .collect();
        g
    };

    for (label, want_trail, game) in [
        ("at rest", false, stage(playing(w, h))),
        ("at cap", true, stage(blazing(40))),
        (
            "dashing",
            true,
            stage({
                let mut g = blazing(40);
                g.player.dash_t = 0.1;
                g.player.dash_dx = 1.0;
                g.player.dash_dy = 0.0;
                g
            }),
        ),
    ] {
        let buf = frame(w, h, &game);

        // MOVA, wherever he landed. `BODY` is emitted by nothing else.
        let cells = cells(&buf);
        let body: Vec<(usize, usize)> = (0..h as usize)
            .flat_map(|y| (0..w as usize).map(move |x| (x, y)))
            .filter(|(x, y)| cells[*y][*x] == BODY)
            .collect();
        assert_eq!(
            body.len(),
            1,
            "{label}: expected one MOVA, found {} — cannot read a colour from a \
             glyph that is not there",
            body.len()
        );
        let (mx, my) = body[0];
        let mova = buf[Position::new(mx as u16, my as u16)]
            .style()
            .fg
            .unwrap_or(ratatui::style::Color::Reset);
        assert!(
            is_cold(mova),
            "{label}: MOVA is {mova:?}, which is not blue. He is the only cold \\
             thing on screen that moves; if he has gone warm he is one more \\
             red shape among the red ones."
        );

        // Enemies and embers are found by asking the game where they are and
        // projecting them, not by scanning the frame for their glyphs.
        //
        // Scanning was the obvious approach and it is wrong twice over. It misses
        // things — a cold enemy simply stops being found, so recolouring one blue
        // passes a test whose whole subject is that enemy. And it finds things
        // that are not what it is looking for: the HUD reads "COMBO x5" along
        // the bottom bar, so every frame on earth has two `O` on it in MOVA's
        // own colour.
        //
        // A cell is only judged when it really is the entity's own glyph, because
        // anything can be overdrawn between the entity and now — which is not a
        // failure, it is the frame being busy. What matters is that at least one
        // of each was actually visible, or the case proved nothing.
        let mut warm_seen = 0;
        for e in &game.enemies {
            // Off screen is not a colour failure, it is an enemy out of frame.
            // The renderer culls these before drawing; this culls them the same
            // way, rather than indexing a cell that is not there.
            let (sx, sy) = project(w, h, (game.cam_x, game.cam_y), (e.x, e.y));
            let (x, y) = (sx.round(), sy.round());
            if x < 1.0 || y < 1.0 || x > f32::from(w - 2) || y > f32::from(h - 2) {
                continue;
            }
            let cell = &buf[Position::new(x as u16, y as u16)];
            if cell.symbol() != e.kind.glyph().to_string().as_str() {
                continue;
            }
            warm_seen += 1;
            let col = cell.style().fg.unwrap_or(ratatui::style::Color::Reset);
            assert!(
                is_warm(col),
                "{label}: enemy {} is {col:?}, which is not warm. MOVA is the \
                 only cold thing on screen that moves; a cold enemy is a second \
                 one, and the two become the same shape.\n{}",
                e.kind.glyph(),
                dump(&buf)
            );
            warm_seen += 1;
        }
        assert!(
            warm_seen > 0,
            "{label}: no enemy was actually visible, so there was no enemy colour \
             to read"
        );

        // His trail. This is the one that was red, and it is the worst of the
        // three: the trail is longer than the enemy, so a warm ember puts a red
        // streak *behind* MOVA running at a red shape, which reads as two
        // enemies flanking him rather than as him and his wake.
        let mut embers = 0;
        for p in game.fx.parts.iter().filter(|p| p.kind == 3) {
            let (sx, sy) = project(w, h, (game.cam_x, game.cam_y), (p.x, p.y));
            let (x, y) = (sx.round(), sy.round());
            if x < 1.0 || y < 1.0 || x > f32::from(w - 2) || y > f32::from(h - 2) {
                continue;
            }
            let cell = &buf[Position::new(x as u16, y as u16)];
            if cell.symbol() != FLAME.to_string().as_str() {
                continue;
            }
            embers += 1;
            let col = cell.style().fg.unwrap_or(ratatui::style::Color::Reset);
            assert!(
                is_cold(col),
                "{label}: ember is {col:?}, which is warm. His wake is the longest \
                 thing he draws; in the enemy's colour it reads as a second \
                 pursuer.\n{}",
                dump(&buf)
            );
        }
        if want_trail {
            assert!(
                embers > 0,
                "{label}: no ember was actually visible at {}, so there is no trail \
                 to check — he is not blazing, or the flame has stopped being \
                 drawn",
                game.player.speed()
            );
        }
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

    let (cx, cy) = renderer::radar_center(Rect::new(1, 1, w - 2, h - 2))
        .expect("terminal is wide enough for the radar");

    // Where to stand, measured rather than chosen by eye — and measured by
    // sweeping the map, not by hard-coding a spot that was good last time.
    //
    // The position used to be a literal, and it has now been invalidated twice by
    // changes that had nothing to do with the radar: once when the map became
    // stamped, and again when the rooms grew lanes and the spawn cleared its
    // sightlines. Both times the test failed as "proved nothing" — the honest
    // outcome, and no use to anyone. A literal that a change to the map generator
    // can invalidate is a second thing to remember, and it guards nothing about
    // the radar while it guards a great deal about the generator.
    //
    // So it sweeps for the most balanced vantage on the pinned map: the spot with
    // the most of whichever of pillar and lane it has *less* of. That is the right
    // figure of merit because the comparison below needs both kinds and is only
    // as strong as the scarcer one, and because it fails loudly if the map ever
    // stops having anywhere mixed to stand in.
    let count = |g: &Game| -> (usize, usize) {
        let cells = cells(&frame(w, h, g));
        let mut blocks = 0;
        let mut lanes = 0;
        for j in 0..(2 * RADAR_DISC_HALF + 1) {
            for i in 0..(2 * RADAR_DISC_HALF + 1) {
                let (dx, dy) = (i - RADAR_DISC_HALF, j - RADAR_DISC_HALF);
                // The rim is excluded from the count as well as the comparison:
                // a cell out there reads different map after the shift.
                if dx * dx + dy * dy > (RADAR_DISC_HALF - 4) * (RADAR_DISC_HALF - 4) {
                    continue;
                }
                if cells[(cy + dy) as usize][(cx + dx) as usize] == '█' {
                    blocks += 1;
                } else {
                    lanes += 1;
                }
            }
        }
        (blocks, lanes)
    };

    // Every eighth unit, which is finer than the disc could resolve anyway: it is
    // thirty-two units across, so a finer sweep would only re-measure the same
    // handful of map cells from a hundred positions.
    let mut best = ((0usize, 0usize), (0.0f32, 0.0f32));
    let mut y = -ARENA_HY;
    while y <= ARENA_HY {
        let mut x = -ARENA_HX;
        while x <= ARENA_HX {
            game.player.x = x;
            game.player.y = y;
            let got = count(&game);
            if got.0.min(got.1) > best.0 .0.min(best.0 .1) {
                best = (got, (x, y));
            }
            x += 8.0;
        }
        y += 8.0;
    }
    assert!(
        best.0 .0.min(best.0 .1) > 12,
        "test proved nothing: the most balanced vantage on the pinned map has \
         {} pillar cells and {} lane cells under the disc, so there is nowhere \
         mixed to stand and the radar cannot be checked against a shift",
        best.0 .0,
        best.0 .1
    );
    game.player.x = best.1 .0;
    game.player.y = best.1 .1;

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

/// One radar cell in world units. The radar's resolution, which the width check
/// below is stated in. A test's own copy, like [`RADAR_DISC_HALF`], because the
/// renderer has no reason to publish it.
const RADAR_CELL: f32 = 4.0;

/// And the half of the above that the shift check does not cover.
///
/// The translation test on its own turns out to be weaker than it looks, and it
/// took a mutation to show it. Resampling the map — asking "is this radar cell
/// inside a block?" for each cell in turn — is *also* equivariant under walking a
/// whole lattice cell, because the lattice and the radar grid both divide twelve
/// by four exactly. A radar built that way reproduces itself under the shift just
/// as well as a real projection does, so the shift check passes it, and the
/// comment above it was claiming more than the assertion could see.
///
/// What separates them is width. A block seven and a bit units either side of its
/// centre covers four radar cells when projected, because the projection spans
/// from `round(-1.9)` to `round(1.9)`; sampled at cell centres it covers three,
/// because the outermost two centres fall outside the block. A full cell is the
/// difference between a radar you can judge distance on and one that quietly
/// rounds every wall in the game.
///
/// The block has to be alone, or its neighbours run into the run being measured
/// and the count means nothing.
#[test]
fn radar_projects_block_widths_instead_of_sampling_them() {
    let (w, h) = (110u16, 34u16);
    let mut game = playing(w, h);
    game.solids = SolidGrid::new(0x4D4F_5641);
    let grid = game.solids.clone();
    let (cx, cy) = renderer::radar_center(Rect::new(1, 1, w - 2, h - 2))
        .expect("terminal is wide enough for the radar");

    // Two cells of clear ground all round, so nothing else can widen the run.
    // Two rather than one because a block is fifteen units across and its
    // neighbour's begins only four units past the first one's edge.
    let isolated = grid
        .solids
        .iter()
        .find(|s| {
            let (ci, cj) = ((s.x / CELL).round() as i32, (s.y / CELL).round() as i32);
            let clear = |i: i32, j: i32| grid.cell_kind(i, j) == mova::solid::EMPTY;
            [1, 2].iter().all(|&k| {
                clear(ci - k, cj) && clear(ci + k, cj) && clear(ci, cj - k) && clear(ci, cj + k)
            })
        })
        .copied()
        .expect("the pinned map has a lone block somewhere on it");

    // A whole lattice cell west of it, so the block lands three radar cells right
    // of the middle — clear of MOVA's own marker, which is drawn last and would
    // hide a cell the run depends on.
    game.player.x = isolated.x - CELL;
    game.player.y = isolated.y;
    let cells = cells(&frame(w, h, &game));

    // The block's own row. Offset zero in disc terms *is* the middle, so this is
    // the disc's centre row rather than its bottom one.
    let at = |dx: i32| cells[cy as usize][(cx + dx) as usize] == '█';
    let want = (2.0 * isolated.hw / RADAR_CELL).ceil() as i32;
    let centre = (CELL / RADAR_CELL).round() as i32;

    assert!(at(centre), "the lone block is not on the radar at all");
    let mut lo = centre;
    let mut hi = centre;
    while at(lo - 1) {
        lo -= 1;
    }
    while at(hi + 1) {
        hi += 1;
    }
    assert_eq!(
        hi - lo + 1,
        want,
        "a block {:.1} units wide covers {} radar cells on the radar, not the {} \
         its width calls for — it is being sampled rather than projected",
        2.0 * isolated.hw,
        hi - lo + 1,
        want
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
