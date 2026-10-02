//! Headless checks on the parts of the run that a screenshot cannot answer: does
//! the camera keep up, does the speed cap actually mean something, and does
//! every run really get a different map.
//!
//! Run with `cargo test`.

use mova::enemy::{Enemy, Kind};
use mova::game::{Arena, Game, ARENA_HX, ARENA_HY};
use mova::input::Input;
use mova::player::{self, Player};
use mova::renderer;
use mova::save::Save;
use mova::score::{EMBER, EMBER_LIFE};

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

/// Crossing the seam has to be a step, not a pan across the whole arena.
///
/// This is the seam's first observable consequence, and it is worth pinning
/// because the failure is silent: the numbers MOVA and the camera are given look
/// like a four-unit step, and they are, but the *difference* between them is 392
/// and only the wrapped form of that difference is small. Ease the raw one and
/// the camera slides the entire width of the arena — right across the view, past
/// everything, and off the far side — whenever MOVA crosses, which on a torus is
/// a thing that happens every few seconds rather than once a run.
///
/// So: settle either side of the seam, then watch how far the camera moves. It
/// must cover the four units between them, not the three hundred and ninety-two
/// the subtraction gives.
#[test]
fn camera_crosses_the_seam_the_short_way() {
    let mut game = started((100, 30));

    // Four units apart in world terms, 392 in coordinate terms.
    let east = ARENA_HX - 2.0;
    let west = -ARENA_HX + 2.0;
    assert!(
        east - west > 380.0,
        "the two ends of the seam are only {} units apart in coordinates, so \
         there is nothing here to catch",
        east - west
    );

    for (from, to) in [(east, west), (west, east)] {
        settled(&mut game, from, 0.0);
        // Measured wrapped. Settling on the far side of a seam leaves the camera
        // at a coordinate 392 away and standing on the same piece of ground, and
        // comparing the raw pair would call that a camera that failed to follow.
        assert!(
            Arena::delta_axis(game.cam_x, from, ARENA_HX).abs() < 1.0,
            "camera never settled on MOVA before the crossing"
        );

        game.player.x = to;
        // One step at the game's own rate, not a settle: the point is how far it
        // goes on the way, and a settle would end in the right place either way.
        let before = game.cam_x;
        game.cam_follow(1.0 / 120.0);
        let moved = (game.cam_x - before).abs();

        assert!(
            moved < 2.0,
            "the camera travelled {moved:.1} units to cover four. It is easing \
             towards the raw coordinate difference instead of the wrapped one, \
             so every seam crossing pans the view the width of the arena"
        );
        // And it is going the right way round, not merely the short way: the
        // short way from east to west is +4, wrapping past the period.
        if from > to {
            assert!(
                game.cam_x > before,
                "the camera went the long way round the seam from {before:.1} to \
                 {:.1}",
                game.cam_x
            );
        } else {
            assert!(
                game.cam_x < before,
                "the camera went the long way round the seam from {before:.1} to \
                 {:.1}",
                game.cam_x
            );
        }
    }
}

/// MOVA at the cap, having just laid a trail down `+x` in open ground, and left
/// to stand still so a test can put something in it.
///
/// Speed is forced rather than steered because the cap is a threshold the wake is
/// gated on and holding it by hand is the honest way to be on one side of it. The
/// plaza is the open ground: the generator clears a disc of three cells about the
/// origin, and MOVA covers about sixty units in the frames below, so he is driven
/// along `+y` instead — the disc is three times deeper in world units than the
/// trail is long, which is the only reason this works without steering.
///
/// Returns nothing; the caller reads `player`, `fx.parts` and picks a spot off an
/// ember rather than guessing at one. An ember is the trail, so measuring against
/// one cannot drift off the line the way a computed coordinate does — the first
/// version of this test did exactly that and failed for reasons that had nothing
/// to do with the mechanic.
fn blazing() -> Game {
    let mut game = started((100, 30));
    let mut keys = Input::default();
    game.player.x = 0.0;
    game.player.y = 0.0;
    for _ in 0..120 {
        game.player.vx = 0.0;
        game.player.vy = game.player.cap() * 1.5;
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
    }
    assert!(
        game.player.blazing(),
        "MOVA is at {} against a cap of {} and is not blazing, so this is \
         testing nothing",
        game.player.speed(),
        game.player.cap()
    );
    assert!(
        game.fx.parts.iter().filter(|p| p.kind == EMBER).count() > 3,
        "MOVA is blazing and has left no trail to be a weapon with"
    );
    game
}

/// The oldest ember, which is the one furthest from MOVA and so the most
/// convincing demonstration that the wake reaches backwards rather than
/// reporting contact with the body itself.
fn tail(game: &Game) -> (f32, f32) {
    let (mut best, mut at) = (f32::MAX, (0.0, 0.0));
    for p in game.fx.parts.iter().filter(|p| p.kind == EMBER) {
        if p.life < best {
            best = p.life;
            at = (p.x, p.y);
        }
    }
    at
}

/// MOVA's trail cuts.
///
/// At the cap the wake is the longest thing on the board and now the strongest,
/// and the claim worth pinning is the shape of it: what it does is a function of
/// *where MOVA has been*, so a line drawn through a crowd works from behind. An
/// enemy put in the tail MOVA laid a moment ago is cut by an ember he left several
/// frames back — which is the mechanic, and the reason it is worth having on a
/// borderless map, where the crowd you have already passed is as available as
/// the one in front of you. The ram only ever reaches the one in front.
#[test]
fn the_wake_cuts_behind_him() {
    let mut game = blazing();
    let (tx, ty) = tail(&game);

    let before = vec![
        // Standing on an ember.
        Enemy::new(tx, ty, Kind::Grunt, 0.0, 0.0, 1.0),
        // Same line, sixty units out: clear of the reach but well within sight,
        // so the second enemy proves the cut is a width and not a radius sweep
        // that happens to include everything.
        Enemy::new(tx + 60.0, ty, Kind::Grunt, 0.0, 0.0, 1.0),
    ];
    let hp: Vec<u8> = before.iter().map(|e| e.hp).collect();
    game.enemies = before;
    game.wake_tick();
    let after: Vec<u8> = game.enemies.iter().map(|e| e.hp).collect();

    assert!(
        after[0] < hp[0],
        "the enemy standing in the tail came through it untouched ({after:?}), so \
         the wake is not a weapon"
    );
    assert_eq!(
        after[1], hp[1],
        "an enemy sixty units past the end of the tail was cut by it"
    );
}

/// Below the cap it must not cut at all.
///
/// The trail is not drawn there and it must not be a weapon there either. If it
/// were, slowing down becomes a trap — the thing you do when a run is going badly
/// would hand you a trail that damages your line of sight's worth of enemies
/// passing through it — and the cap would be the only safe speed rather than the
/// best one.
#[test]
fn the_wake_is_off_below_the_cap() {
    let mut game = blazing();
    let (tx, ty) = tail(&game);
    game.player.vx = 0.0;
    game.player.vy = 0.0;
    assert!(
        !game.player.blazing(),
        "MOVA is still at the cap, so this is testing nothing"
    );

    game.enemies = vec![Enemy::new(tx, ty, Kind::Grunt, 0.0, 0.0, 1.0)];
    let hp = game.enemies[0].hp;
    game.wake_tick();
    assert_eq!(
        game.enemies[0].hp, hp,
        "the trail still cut while MOVA was stopped, so slowing down is a trap \
         rather than a choice"
    );
}

/// A nova is gated on the bounce, and the bounce is gated on speed.
///
/// Two halves, and they are checked separately because they fail separately and
/// for different reasons. The signal has to be raised by the reflection alone or
/// a nova comes off every wall contact; and it has to be ignored below ram speed
/// or the walls are loud instead of useful, since leaning on a post at walking
/// pace would be a weapon.
///
/// Deliberately does not drive MOVA into the furniture to find the wall. A run at
/// the cap into a crowd will also ram something, and a ram throws a ring in the
/// same colour, so the obvious version of this test passes for the wrong reason
/// the moment the wave leaves anything standing. The gate is a function of two
/// numbers and is worth checking as one.
#[test]
fn a_nova_needs_a_bounce_at_speed() {
    /// A ring count taken with MOVA at `speed`, bounced.
    ///
    /// The ring list is emptied first, and that is not tidiness. `Fx::ring` gives
    /// up at twelve live rings and gives up by dropping the new one, which is
    /// right for a screen that only has room for twelve and useless to a test that
    /// counts. A run at the cap through furniture rams things — and every ram
    /// leaves a ring of its own, in the same colour, for the next third of a
    /// second — so the list is somewhere between empty and full depending on the
    /// map, and the map comes off the clock. Counted as a difference, a dropped
    /// nova reads as zero rings thrown, which is how this failed three times
    /// before anyone thought to clear the list.
    fn bounced(speed: f32) -> (usize, f32) {
        let mut game = blazing();
        game.fx.rings.clear();
        game.player.vx = speed;
        game.player.vy = 0.0;
        game.player.bounce_t = 1.0;
        game.nova_tick();
        (game.fx.rings.len(), game.player.speed_ratio())
    }

    let cap = blazing().player.cap();
    let (fast, fast_ratio) = bounced(cap * 1.5);
    assert!(
        fast_ratio >= mova::game::RAM_AT,
        "the fast case is not above ram speed ({fast_ratio}), so there is \
         nothing to catch"
    );
    assert_eq!(
        fast, 1,
        "a bounce above ram speed threw {fast} rings, and a nova is one ring"
    );

    let (slow, slow_ratio) = bounced(cap * 0.3);
    assert!(
        slow_ratio < mova::game::RAM_AT,
        "the slow case is not below ram speed ({slow_ratio}), so there is \
         nothing to catch"
    );
    assert_eq!(
        slow, 0,
        "a nova came off a bounce below ram speed, so drifting into a post is \
         now a weapon and the walls are loud instead of useful"
    );

    // And the signal itself: at ram speed with no bounce, nothing. This is what
    // makes the gate a gate rather than a rule about being fast.
    let mut game = blazing();
    game.fx.rings.clear();
    game.player.vx = game.player.cap() * 1.5;
    game.player.vy = 0.0;
    game.nova_tick();
    assert_eq!(
        game.fx.rings.len(),
        0,
        "a nova came off nothing at all, so the shockwave has stopped being a \
         thing that happens when you hit a wall"
    );
}

/// A nova throws what it catches clear of MOVA, rather than only hurting it.
///
/// The shove is the half of the nova that makes it worth having on a map with no
/// corners: damage on contact is something you aim, whereas the space it opens is
/// ground you were not otherwise going to get to. And it has to survive the wall
/// it was thrown through — an enemy shoved into a block and left there is not
/// shoved anywhere, it is just stored.
#[test]
fn a_nova_throws_enemies_clear() {
    let mut game = blazing();
    game.fx.rings.clear();
    // A brute under MOVA, which the nova should throw out of contact range and
    // take a point off. Placed against MOVA rather than the origin, because
    // `blazing` leaves him most of a hundred units down the plaza and an enemy at
    // the origin is most of a hundred units outside the reach. The health comes
    // off the kind, so the test is about the mechanic and not about what the wave
    // left lying about.
    let (px, py) = (game.player.x, game.player.y);
    let hp = Enemy::new(px, py, Kind::Brute, 0.0, 0.0, 1.0).hp;
    game.enemies = vec![Enemy::new(px, py, Kind::Brute, 0.0, 0.0, 1.0)];

    game.player.bounce_t = 1.0;
    game.nova_tick();
    let e = &game.enemies[0];

    assert_eq!(
        e.hp,
        hp - 1,
        "a nova landed on something inside its reach and did no damage, so the \
         reach is wrong"
    );
    assert!(
        Arena::delta_axis(game.player.x, e.x, ARENA_HX).hypot(Arena::delta_axis(
            game.player.y,
            e.y,
            ARENA_HY
        )) > 8.0,
        "the nova hit it and left it where it was, so the shove is not doing \
         anything"
    );
    assert!(
        !game.solids.inside(e.x, e.y),
        "the enemy was thrown into a block and left there"
    );
}

/// One bounce is one nova.
///
/// The signal is raised for a step and read once, so the question is whether two
/// reflections arriving together produce two shockwaves. They should not: there
/// was one bounce, and a wall MOVA is already sliding along raises a reflection
/// every frame — which without this would be a nova per frame for as long as the
/// graze lasted.
#[test]
fn a_nova_is_consumed_by_its_bounce() {
    let mut game = blazing();
    game.fx.rings.clear();
    game.enemies = vec![Enemy::new(0.0, 0.0, Kind::Brute, 0.0, 0.0, 1.0)];
    game.player.bounce_t = 1.0;

    let rings_before = game.fx.rings.len();
    game.nova_tick();
    let after_first = game.fx.rings.len();
    assert!(after_first > rings_before, "the first bounce threw nothing");

    game.nova_tick();
    assert_eq!(
        game.fx.rings.len(),
        after_first,
        "a second tick off the same bounce threw a second nova, so sliding \
         along a wall is a machine gun"
    );
}

/// The wake reaches across the seam.
///
/// The trail is drawn wrapped — it is part of the picture, and the picture wraps
/// — so an ember left against one edge of the map and an enemy standing just past
/// the other are drawn touching. Measured raw they are a fraction of three hundred
/// and ninety-four apart, and the wake silently stops working on exactly the part
/// of the arena that looks like a seam rather than a wall, which is to say the
/// part the player is most likely to be reading for a shot of the rebound.
///
/// The ember is injected rather than laid down by driving MOVA there, because the
/// claim is about where the *measurement* happens and a trail MOVA had to be
/// walked into the seam to produce would prove it a hundred units at a time.
#[test]
fn the_wake_reaches_across_the_seam() {
    let mut game = blazing();
    // A unit inside each edge, so the pair is two units apart the short way round
    // the seam — inside a grunt's wake reach — and three hundred and ninety-four
    // the long way, which is nowhere near it. A mutation that drops the wrap
    // cannot tell this apart from an enemy in another district entirely.
    game.fx
        .particle(ARENA_HX - 1.0, 0.0, 0.0, 0.0, EMBER_LIFE, EMBER);
    let hp = Enemy::new(0.0, 0.0, Kind::Grunt, 0.0, 0.0, 1.0).hp;
    game.enemies = vec![Enemy::new(-ARENA_HX + 1.0, 0.0, Kind::Grunt, 0.0, 0.0, 1.0)];
    game.wake_tick();

    assert_eq!(
        game.enemies[0].hp,
        hp - 1,
        "the wake stopped at the seam: an ember and an enemy drawn touching \
         neither cut nor missed"
    );
}

/// One enemy is cut once by a long tail, not once per ember it is lying in.
///
/// Without the cooldown the trail is a beam: a brute sitting anywhere along
/// thirty units of tail loses its whole health in a single frame, which reads as a
/// laser and makes the tail's width the only thing about it that matters. A brute
/// specifically, because a grunt has two hit points and would die either way and
/// prove nothing.
#[test]
fn the_wake_cuts_an_enemy_once() {
    let mut game = blazing();
    let (tx, ty) = tail(&game);
    let hp = Enemy::new(tx, ty, Kind::Brute, 0.0, 0.0, 1.0).hp;
    assert!(
        hp > 1,
        "a brute has one hit point, so there is nothing to spare"
    );

    game.enemies = vec![Enemy::new(tx, ty, Kind::Brute, 0.0, 0.0, 1.0)];
    game.wake_tick();
    assert_eq!(
        game.enemies[0].hp,
        hp - 1,
        "one frame of trail took {} off a brute, so the wake is a beam",
        hp - (hp - 1)
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

/// A handful of embers is a flame. A cloud is the same embers emitted per step
/// instead of per gap, and that is seventy of them — which is why they are spaced
/// by ground covered.
///
/// The bound is *derived* rather than a fixed number, because in v3 the tail's
/// length is a function of speed and no single figure survives it. Embers are
/// thrown in pairs every [`Player::EMBER_GAP`] of ground and live for
/// `EMBER_LIFE`, so a body holding the cap always has the same number alive:
/// `2 * cap * life / gap`. At v2's 26-unit cap that was fourteen; v3 raised the
/// cap to 44, the surge to 63.8 and the dash to 190, and the honest steady state
/// at each is 20, 29 and 87. A hard-coded bound had to be either flaky or blind,
/// and both of those failures are silent — so it is computed from the same three
/// constants the game throws from.
///
/// Which is what makes it catch the regression it exists for. Emitting per step
/// gives seventy whatever MOVA is doing, so at the cap — where the derived bound
/// is about twenty-four — it is nearly three times over, and the surged case is
/// checked too rather than assumed.
#[test]
fn the_flame_is_a_tail_and_not_a_cloud() {
    // Run once at the plain cap and once surging. The surge is the highest speed
    // MOVA can hold indefinitely, and it is the case where a fixed bound would
    // have to be loosened far enough to miss the failure.
    for surge in [false, true] {
        let mut game = started((100, 30));
        let mut keys = Input::default();
        game.player.y = LANE;
        if surge {
            game.player.surge = Player::SURGE_TIME;
        }
        // The highest cap MOVA has held over the last ember lifetime. Embers
        // alive now were thrown inside that window, so the tail can only be as
        // dense as the fastest cap in it.
        //
        // The instantaneous cap is the wrong number and reads low: a ram lifts the
        // cap to 44 * 1.28 for half a second, MOVA burns at that density, and the
        // lift expires with a third of a second of embers still to live. Measuring
        // against the cap at that moment under-counts the tail by the whole boost,
        // which is exactly the run where the tail is biggest.
        let window = (mova::score::EMBER_LIFE * 120.0).ceil() as usize;
        let mut caps: Vec<f32> = vec![0.0; window];
        for step in 0..600 {
            game.enemies.clear();
            // Held at the cap rather than at a held direction. v3 walls bounce, so
            // holding `right` into one pins MOVA against it and he oscillates
            // through the cap rather than sitting on it — which left the burn
            // intermittent and the test measuring its own map. Forcing the
            // velocity is what puts MOVA in the state whose tail length is the
            // claim, every step, on every map.
            game.player.vx = Player::new().cap() * 1.5;
            game.player.vy = 0.0;
            game.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
            caps[step % window] = game.player.cap();

            let live = game.fx.parts.iter().filter(|p| p.kind == 3).count();
            // Read the life off a live ember, so this follows the constant rather
            // than restating it.
            let life = game
                .fx
                .parts
                .iter()
                .find(|p| p.kind == 3)
                .map_or(mova::score::EMBER_LIFE, |p| p.max_life);
            let peak_cap = caps.iter().copied().fold(0.0f32, f32::max);
            let expected = 2.0 * peak_cap * life / Player::EMBER_GAP;
            // Four of slack. Throws and expiries are both discrete, so the live
            // count wanders a pair either side of the steady state, and a wall
            // brushing MOVA can hold him under the cap for a step or two.
            assert!(
                live as f32 <= expected + 4.0,
                "{} embers alive at once against a recent peak cap of {peak_cap:.1} \
                 (surging: {surge}), where the spacing allows about {expected:.0}: \
                 that is a cloud, not a flame",
                live,
            );
        }
    }
}

/// A lane MOVA is held on. Not a clearance guarantee any more — v3's walls are too
/// wide for any fixed row-relative strip to be open on every seed — just somewhere
/// for him to start, so the run does not open inside a block.
const LANE: f32 = 6.0;

/// Drives MOVA at the cap for `secs` and reports how many embers he threw per
/// world unit of ground he covered.
///
/// Coverage is the sum of the wrapped displacement between successive positions,
/// which is the same quantity the flame is spaced by. v2 divided by net
/// displacement from start to finish, and that only equals the distance run when
/// the line is clear and straight — which on a fresh random map it never is. A
/// pillar brushing MOVA reverses part of his travel, so net displacement
/// under-reads it and the density came out several times too high, by an amount
/// depending on which map the run drew. v3 also bounced MOVA off walls, which
/// spends much of a step in the perpendicular axis; measuring only x, as an
/// earlier version of this did, under-counted for the same reason.
fn run_at_the_cap(rate: f32, secs: f32) -> (f32, usize) {
    let mut game = started((100, 30));
    let mut keys = Input::default();
    game.player.y = LANE;
    let mut prev = (game.player.x, game.player.y);
    let mut travel = 0.0f32;
    let mut thrown = 0usize;
    let dt = 1.0 / rate;

    for _ in 0..(secs * rate) as u32 {
        game.enemies.clear();
        // Above the cap, so the drag inside the step cannot leave MOVA a hair
        // under it: the cap clamp then lands the speed exactly on the cap at any
        // step size, which is the state the tail is a tail in. Set every step so a
        // bounce cannot bleed him below it, and the run is MOVA holding a
        // direction rather than MOVA reacting to the map.
        //
        // Read the cap fresh each step rather than latching it. An earlier version
        // latched it, and a Surge pickup in the first two seconds raised the cap
        // above anything the latch could reach: MOVA fell short of it, stopped
        // being blazing, and threw no embers while still covering ground. That
        // read as a third too few embers on the slow rates and passed at 120.
        game.player.vx = Player::new().cap() * 1.5;
        game.player.vy = 0.0;
        game.update(dt, &mut keys);
        keys.clear_edges();

        let now = (game.player.x, game.player.y);
        // Wrapped, so crossing the seam mid-run is a step rather than a jump
        // across the whole arena.
        travel += (Arena::delta_axis(prev.0, now.0, ARENA_HX).powi(2)
            + Arena::delta_axis(prev.1, now.1, ARENA_HY).powi(2))
        .sqrt();
        prev = now;

        // An ember born on this step has had one step of life taken off it; an
        // older one has had at least two. A whole step of margin between them,
        // and nothing here depends on the count drifting.
        thrown += game
            .fx
            .parts
            .iter()
            .filter(|p| p.kind == 3 && p.life > p.max_life - 1.5 * dt)
            .count();
    }
    assert!(
        travel > 1.0,
        "MOVA covered {travel} units in {secs}s at {rate} steps a second"
    );
    (thrown as f32 / travel, thrown)
}

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
/// reason. Counting the embers *born* on each step is immune to that.
///
/// Three seconds, so the two embers thrown before the first step of travel is
/// covered is under a twentieth of the tally.
#[test]
fn the_flame_does_not_depend_on_the_step_rate() {
    // Two embers per gap of ground. The per-frame version is a factor of the
    // step size out — five at 30 steps a second, where a step is most of a gap —
    // so a seventh either way leaves room for the tally to wobble and still
    // fails on anything that is actually spacing by frames.
    let want = 2.0 / Player::EMBER_GAP;
    for rate in [120.0f32, 60.0, 30.0, 15.0] {
        let (got, thrown) = run_at_the_cap(rate, 3.0);
        assert!(thrown > 0, "no embers at {rate} steps a second");
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
