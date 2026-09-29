//! Headless checks on the parts of the run that a screenshot cannot answer: does
//! the camera keep up, does the speed cap actually mean something, and does
//! every run really get a different map.
//!
//! Run with `cargo test`.

use mova::game::{Game, ARENA_HX, ARENA_HY};
use mova::input::Input;
use mova::player::{self, Player};
use mova::renderer;
use mova::save::Save;

use ratatui::layout::Rect;

/// A started run with the view sized to a plausible terminal.
fn started(view: (u16, u16)) -> Game {
    let mut game = Game::new(Save::default());
    let (hx, hy) = renderer::view_half(Rect::new(0, 0, view.0, view.1));
    game.set_view(hx, hy);
    game.start();
    game
}

/// Teleports MOVA and settles the camera on the new spot. The position does not
/// have to be walkable: the camera follows a point, and a spot the collision
/// resolve would shove MOVA out of is not a thing the camera has an opinion
/// about. Letting the run simulate instead would mean a pillar getting between
/// the test and the answer.
fn settled(game: &mut Game, x: f32, y: f32) {
    game.player.x = x;
    game.player.y = y;
    for _ in 0..600 {
        game.cam_follow(1.0 / 120.0);
    }
}

/// The camera has to actually move. It used to be clamped to zero on any
/// terminal smaller than the arena, which is every terminal, so it sat on the
/// spawn plaza for the whole run and MOVA walked off the side of the screen.
#[test]
fn camera_tracks_the_player() {
    let mut game = started((100, 30));
    for (x, y) in [
        (0.0, 0.0),
        (60.0, 0.0),
        (-60.0, 0.0),
        (0.0, 40.0),
        (0.0, -40.0),
    ] {
        settled(&mut game, x, y);
        assert!(
            (game.cam_x - x).abs() < 1.0 && (game.cam_y - y).abs() < 1.0,
            "camera at ({}, {}) while MOVA is at ({x}, {y})",
            game.cam_x,
            game.cam_y
        );
    }
}

/// And it has to get out of the way: the far corner of the arena is only
/// interesting if the view can reach it.
#[test]
fn camera_reaches_the_far_corner() {
    let mut game = started((100, 30));
    settled(&mut game, -ARENA_HX + 4.0, -ARENA_HY + 4.0);
    assert!(
        game.cam_x < -50.0 && game.cam_y < -30.0,
        "camera never left the middle: ({}, {})",
        game.cam_x,
        game.cam_y
    );
}

/// The clamp only exists to keep the void off screen on a window big enough to
/// see past the wall, and it must not creep back into the normal case.
#[test]
fn camera_stops_at_the_arena_wall() {
    // A terminal wide enough that the view overhangs the arena on both axes.
    let mut game = Game::new(Save::default());
    game.set_view(ARENA_HX + 30.0, ARENA_HY + 30.0);
    game.start();
    settled(&mut game, ARENA_HX - 2.0, ARENA_HY - 2.0);
    assert!(
        game.cam_x.abs() <= ARENA_HX + 0.01 && game.cam_y.abs() <= ARENA_HY + 0.01,
        "camera drifted past the wall: ({}, {})",
        game.cam_x,
        game.cam_y
    );
}

/// A player travelling at the cap cannot be touched. This is the whole point of
/// chasing the cap, so it is checked against `damage` directly rather than
/// against a frame.
#[test]
fn full_speed_makes_mova_untouchable() {
    let mut p = Player::new();
    p.vx = p.cap();
    assert!(p.blazing(), "at the cap but not reported as blazing");
    assert!(!p.damage(-2.0, 0.0), "a hit landed at the cap");
    assert_eq!(p.hp, p.max_hp);
    // And the shove that normally follows a hit is not applied either, or MOVA
    // would be slowed by something that never happened.
    assert!((p.vx - p.cap()).abs() < 0.01, "speed changed anyway");
}

/// Just under the cap is still MOVA. The threshold cannot be the cap itself
/// approached from a distance, or a hair of float error would make a fast run
/// take hits without the player ever being able to tell.
#[test]
fn just_under_full_speed_still_takes_damage() {
    let mut p = Player::new();
    p.vx = p.cap() * 0.9;
    assert!(!p.blazing());
    assert!(p.damage(-2.0, 0.0), "a hit did not land at 90% of the cap");
    assert_eq!(p.hp, p.max_hp - 1);
}

/// A dash is faster than the cap, so it is full speed by definition.
#[test]
fn a_dash_is_full_speed() {
    let mut p = Player::new();
    p.dash_t = 0.1;
    p.dash_dx = 1.0;
    p.dash_dy = 0.0;
    p.update(
        1.0 / 120.0,
        &Input::default(),
        &mova::game::Arena::new(),
        &mova::solid::SolidGrid::new(1),
        &mut mova::score::Fx::new(),
    );
    assert!(p.blazing(), "a dash did not count as full speed");
}

/// The burn is the visible half of the same state, so it has to be there or the
/// invulnerability reads as a bug.
#[test]
fn blazing_throws_embers() {
    let mut game = started((100, 30));
    let mut keys = Input {
        right: true,
        ..Default::default()
    };
    let mut saw_embers = false;
    for _ in 0..600 {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
        if game.player.blazing() && game.fx.parts.iter().any(|p| p.kind == 3) {
            saw_embers = true;
            break;
        }
    }
    assert!(
        saw_embers,
        "reached the cap without throwing a single ember"
    );
}

/// A handful of embers is a flame. Seventy of them is a cloud, and seventy is
/// exactly what throwing one pair per step produces — which is why the embers
/// are spaced by distance covered rather than emitted every frame.
///
/// The bound is loose on purpose: the exact count moves with the map, since a
/// pillar brushing MOVA slows it and slower means a denser tail. Two hundred
/// runs of a held direction peak at fourteen, and the failure it is here to catch
/// runs to about seventy.
#[test]
fn the_flame_is_a_tail_and_not_a_cloud() {
    let mut game = started((100, 30));
    let mut keys = Input {
        right: true,
        ..Default::default()
    };
    for _ in 0..600 {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
        let live = game.fx.parts.iter().filter(|p| p.kind == 3).count();
        assert!(
            live <= 24,
            "{live} embers alive at once: that is a cloud, not a flame"
        );
    }
}

/// A whole lane between two pillar rows, which is clear of the furniture on any
/// seed: blocks sit on a twelve-unit lattice with four-unit half-heights, so
/// nothing ever occupies the strip four to eight units off a row line.
const LANE: f32 = 6.0;

/// The flame is spaced by ground covered, not by steps taken. That is the
/// property worth pinning: a step is an implementation detail of the loop, and
/// tying the tail to it would make the flame a function of the frame clock
/// rather than of how fast MOVA is actually going. The loop is fixed at a
/// hundred and twenty steps a second today, so nothing on screen would show the
/// difference — which is exactly why it is worth a test.
///
/// Measured as embers thrown per world unit covered, not as a count on screen.
/// The count is phase-locked to the step: a throw and an expiry land on the same
/// step, so a frame-by-frame tally reads a different number at each rate for no
/// reason. Counting the embers *born* on each step is immune to that, and
/// dividing by the ground covered is immune to the map — a pillar brushing MOVA
/// slows it, and slower would otherwise mean more embers per unit of ground.
///
/// Three seconds, so the two embers thrown before the first step of travel is
/// covered is under a twentieth of the tally.
#[test]
fn the_flame_does_not_depend_on_the_step_rate() {
    const SECS: f32 = 3.0;
    let density_at = |rate: f32| {
        let mut game = started((100, 30));
        let mut keys = Input::default();
        let cap = game.player.cap();
        // MOVA held at the cap and steered down a clear lane. The position is
        // carried forward from wherever the simulation actually put it, so the
        // run stays a straight line whatever the drag and the step size do
        // between frames.
        let mut x = 0.0f32;
        let mut thrown = 0usize;
        let dt = 1.0 / rate;
        for _ in 0..(SECS * rate) as u32 {
            game.enemies.clear();
            game.player.x = x;
            game.player.y = LANE;
            // Above the cap, so the drag inside the step cannot leave MOVA a
            // hair under it: the cap clamp then lands the speed exactly on the
            // cap at any step size, which is the state the tail is a tail in.
            game.player.vx = cap * 1.5;
            game.player.vy = 0.0;
            game.update(dt, &mut keys);
            keys.clear_edges();
            x = game.player.x;
            // An ember born on this step has had one step of life taken off it;
            // an older one has had at least two. A whole step of margin between
            // them, and nothing here depends on the count drifting.
            thrown += game
                .fx
                .parts
                .iter()
                .filter(|p| p.kind == 3 && p.life > p.max_life - 1.5 * dt)
                .count();
        }
        assert!(
            thrown > 0,
            "no embers at {rate} steps a second, travelling {} units a second",
            game.player.speed()
        );
        thrown as f32 / x
    };
    // Two embers per gap of ground. The per-frame version is a factor of the
    // step size out — five at 30 steps a second, where a step is most of a gap —
    // so a seventh either way leaves room for the tally to wobble and still
    // fails on anything that is actually spacing by frames.
    let want = 2.0 / Player::EMBER_GAP;
    for rate in [120.0f32, 60.0, 30.0, 15.0] {
        let got = density_at(rate);
        assert!(
            (got - want).abs() < 0.15,
            "at {rate} steps a second MOVA throws {got:.3} embers per unit of \
             travel, and the gap says {want:.3}"
        );
    }
}

/// A slow run must not be burning, or the flames say nothing.
#[test]
fn a_still_player_is_not_burning() {
    let mut game = started((100, 30));
    game.fx.clear();
    for _ in 0..60 {
        game.update(1.0 / 120.0, &mut Input::default());
    }
    assert!(!game.player.blazing());
    assert!(
        !game.fx.parts.iter().any(|p| p.kind == 3),
        "embers while standing still"
    );
}

/// The seed has to move. This is the narrower claim than
/// [`each_run_generates_a_different_map`] and it fails differently: if the seed
/// stopped advancing but the generator stayed varied, the maps could still pass
/// while every run was drawing from the same pool.
#[test]
fn the_seed_advances_every_draw() {
    let mut game = Game::new(Save::default());
    let mut seeds = std::collections::BTreeSet::new();
    for _ in 0..8 {
        seeds.insert(game.map_seed());
    }
    assert_eq!(seeds.len(), 8, "the generator kept drawing the same seed");
}

/// Every run gets its own map. Two identical arenas in a row would make the
/// whole sector-and-pillar loop feel like it was not working.
#[test]
fn each_run_generates_a_different_map() {
    let mut game = Game::new(Save::default());
    let mut seen = std::collections::BTreeSet::new();
    for run in 0..8 {
        game.start();
        let sig: Vec<(i32, i32)> = game
            .solids
            .solids
            .iter()
            .map(|s| (s.x as i32, s.y as i32))
            .collect();
        assert!(!sig.is_empty(), "run {run} generated an empty map");
        assert!(
            seen.insert(sig),
            "run {run} repeated an earlier map: the seed is not being advanced"
        );
    }
}

/// A different map is only worth having if it is still playable, so the spawn
/// plaza has to be clear on every single run rather than on the one a test
/// happened to check.
#[test]
fn every_run_spawns_in_the_open() {
    let mut game = Game::new(Save::default());
    for _ in 0..8 {
        game.start();
        assert!(
            !game.solids.hit(0.0, 0.0, player::RADIUS),
            "a run generated a map with a pillar on the spawn plaza"
        );
        for p in &game.pickups {
            assert!(
                !game.solids.inside(p.x, p.y),
                "a pickup spawned inside a block at ({}, {})",
                p.x,
                p.y
            );
        }
    }
}
