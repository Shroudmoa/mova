//! Headless checks on the first v3 slice: meta-progression, the new powers and
//! the new enemy.
//!
//! Run with `cargo test`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use mova::enemy::{Enemy, Kind};
use mova::game::{Game, Phase};
use mova::input::Input;
use mova::meta;
use mova::pickup::{Kind as Pick, Pickup};
use mova::player::Player;
use mova::renderer;
use mova::save::Save;

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

fn dump(buf: &Buffer) -> String {
    lines(buf).join("\n")
}

/// A started run with the view sized to a plausible terminal.
fn started(view: (u16, u16)) -> Game {
    let mut game = Game::new(Save::default());
    let area = Rect::new(0, 0, view.0, view.1);
    let (hx, hy) = renderer::view_half(area);
    game.set_view(hx, hy);
    game.start();
    game
}

fn blazing(game: &mut Game) {
    let cap = game.player.cap();
    game.player.vx = cap * 1.5;
    game.player.vy = 0.0;
}

// ---- meta-progression ------------------------------------------------------

/// A finished run pays out into the meta layer, and the shop can only ever
/// spend what a run left behind. The payout itself is the meta layer's business:
/// score is the base economy, a clean run pays a small performance bonus.
#[test]
fn a_run_pays_out_cores_and_the_shop_sells_permanently() {
    let game = Game::new(Save::default());
    let payout = meta::run_payout(10_000, 2, true);
    assert_eq!(game.save.cores, 0);
    assert_eq!(payout, 10_000 / 25 + 25 + 2 * 10);

    // The shop can only ever buy what the player has.
    let mut save = game.save;
    save.cores = 0;
    assert!(!meta::owns(&save, meta::ITEM_CHAIN));
    assert!(!meta::buy(&mut save, meta::ITEM_CHAIN));
    save.cores = 200;
    assert!(meta::buy(&mut save, meta::ITEM_CHAIN));
    assert!(meta::owns(&save, meta::ITEM_CHAIN));
    assert_eq!(save.cores, 200 - meta::cost(meta::ITEM_CHAIN));

    // An item once owned cannot be bought twice.
    assert!(!meta::buy(&mut save, meta::ITEM_CHAIN));
}

/// An unlocked power is on the rack; a locked one is not, and the two base
/// powers are the player's from the start.
#[test]
fn the_rack_reads_its_unlocks() {
    let mut game = Game::new(Save::default());
    assert!(game.power.ready(mova::power::VIRUS));
    assert!(game.power.ready(mova::power::OVERCHARGE));

    game.save.unlocks |= 1u32 << meta::ITEM_CHAIN;
    game.power = mova::power::Power::new(game.save.unlocks);
    assert!(game.power.ready(mova::power::CHAIN));

    // A fresh save has no shop bits, and yet the base two are the player's.
    let fresh = Game::new(Save::default());
    assert!(fresh.power.ready(mova::power::VIRUS));
    assert!(!fresh.power.ready(mova::power::MAGNET));
}

/// Drive upgrades bought in the shop are permanent overrides on the player,
/// and a player who bought none reads exactly like the base tuning.
#[test]
fn drive_upgrades_are_permanent_overrides() {
    let base = Player::new();
    assert_eq!(base.cap(), mova::player::MAX_SPEED);
    assert_eq!(base.surge_time(), Player::SURGE_TIME);
    assert_eq!(base.overcharge_time(), Player::OVERCHARGE_TIME);

    let mut save = Save::default();
    save.unlocks |= 1u32 << meta::ITEM_OVERCHARGE;
    let boosted = Player::from_save(&save);
    assert_eq!(
        boosted.overcharge_time(),
        Player::OVERCHARGE_TIME + 3.0,
        "the overcharge capacitor did not widen the burn tier"
    );

    save.unlocks |= 1u32 << meta::ITEM_MOMENTUM;
    let heavy = Player::from_save(&save);
    assert!(
        heavy.cap() > boosted.cap(),
        "the momentum core did not lift the ceiling"
    );
}

// ---- the chain power ------------------------------------------------------

/// A kill at speed is a lightning rod: the bolt leaves the victim and takes the
/// next thing in line, which is what makes a fast kill a line rather than a
/// punctuation mark.
#[test]
fn a_kill_chains_to_the_next_enemy_in_range() {
    let mut game = started((100, 30));
    blazing(&mut game);
    let (px, py) = (game.player.x, game.player.y);
    game.enemies = vec![
        Enemy::new(px + 1.5, py, Kind::Grunt, 4.0, 0.0, 1.0),
        Enemy::new(px + 5.0, py, Kind::Grunt, 4.0, 0.0, 1.0),
    ];

    game.update(1.0 / 120.0, &mut Input::default());

    assert!(
        game.enemies.is_empty(),
        "the kill chained and took the second grunt: {} left standing",
        game.enemies.len()
    );
}

// ---- the magnet power -----------------------------------------------------

/// A magnet hold pulls a pickup into grab range, which is the whole difference
/// between hoarding a core and having a reason to roam.
#[test]
fn a_magnet_hold_pulls_a_pickup_home() {
    let mut game = started((100, 30));
    game.player.x = 0.0;
    game.player.y = 0.0;
    game.player.magnet();
    // Just inside the magnet's pull radius, outside what a hand would need: the
    // point of the power is that MOVA does not have to go get it.
    game.pickups = vec![Pickup::new(Pick::Core, 6.0, 0.0, 0.0)];

    let mut keys = Input::default();
    for _ in 0..20 {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
    }

    assert_eq!(
        game.stats.cores, 1,
        "the magnet never pulled a pickup into grab range"
    );
}

// ---- the harrier ---------------------------------------------------------

/// The harrier commits when it closes in, rather than firing off bursts on a
/// free-running clock: the charge is proximity-driven, and while it holds the
/// burst is a straight line instead of a weave.
#[test]
fn the_harrier_commits_when_it_closes_in() {
    let mut game = started((100, 30));
    game.enemies.clear();
    game.enemies.push(Enemy::new(
        game.player.x + 12.0,
        game.player.y,
        Kind::Harrier,
        5.0,
        0.0,
        1.0,
    ));
    let mut keys = Input::default();

    let mut charging = false;
    for _ in 0..240 {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
        if game.enemies[0].rush > 0.0
            && game.arena.distance(
                game.enemies[0].x,
                game.enemies[0].y,
                game.player.x,
                game.player.y,
            ) < 9.0
        {
            charging = true;
            break;
        }
    }
    assert!(charging, "the harrier never committed to a burst");
}

// ---- the shop screen -----------------------------------------------------

/// The shop renders its catalog, and it is a screen and not a simulation state:
/// nothing moves while it is up.
#[test]
fn the_shop_renders_the_catalog() {
    let (w, h) = (100u16, 30u16);
    let mut game = Game::new(Save::default());
    let area = Rect::new(0, 0, w, h);
    let (hx, hy) = renderer::view_half(area);
    game.set_view(hx, hy);
    game.phase = Phase::Shop;

    let buf = frame(w, h, &game);
    let ls = lines(&buf);
    assert!(
        ls.iter().any(|l| l.contains("S H O P")),
        "the shop screen is missing its title:\n{}",
        dump(&buf)
    );
    assert!(
        ls.iter().any(|l| l.contains("CHAIN CABLE")),
        "the shop screen is missing the chain unlock offer:\n{}",
        dump(&buf)
    );
    assert!(
        ls.iter().any(|l| l.contains("BUY")),
        "the shop screen is missing its controls:\n{}",
        dump(&buf)
    );
}

// ---- the warden ----------------------------------------------------------

/// The late-wave siege is an elite by trade: you do not shove it, you burn it
/// down, and its threat is that holding is the plan rather than the accident.
#[test]
fn the_warden_is_a_ram_proof_elite() {
    assert!(Kind::Warden.heavy(), "the warden should not be shovable");
    assert_eq!(Kind::Warden.hp(), 14, "the warden should be a siege, not a skirmish");
    assert_eq!(Kind::Warden.mult(), 4.5, "the warden should pay off its cost");
}

/// An enraged warden stops pretending to be slow. The enrage is read off the
/// hull — a threshold on hp — rather than stored as state, which is what lets a
/// later tier of elite behaviour arrive without new spawn logic.
#[test]
fn the_warden_enrages_when_damaged() {
    let mut game = started((100, 30));
    game.enemies.clear();
    game.enemies.push(Enemy::new(0.0, 0.0, Kind::Warden, 2.0, 0.0, 1.0));
    game.enemies[0].hp = 14;

    let mut keys = Input::default();
    game.enemies[0].hp = 14;
    game.enemies[0].x = 20.0;
    game.enemies[0].y = 0.0;

    let calm = {
        let x0 = game.enemies[0].x;
        let y0 = game.enemies[0].y;
        for _ in 0..6 {
            game.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
        }
        (game.enemies[0].x - x0).abs().max((game.enemies[0].y - y0).abs())
    };
    assert!(
        calm > 0.02,
        "the warden should actually be walking toward MOVA"
    );

    let fired = {
        game.enemies[0].hp = 6;
        let x0 = game.enemies[0].x;
        let y0 = game.enemies[0].y;
        for _ in 0..6 {
            game.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
        }
        (game.enemies[0].x - x0).abs().max((game.enemies[0].y - y0).abs())
    };
    assert!(
        fired > calm * 1.5,
        "the warden should enrage once its hull is burning, not walk on calm"
    );
}

// ---- the harbinger -------------------------------------------------------

/// The mid-run boss wears the same vocabulary as everything before it: it
/// closes slowly, then commits to a charge with a longer fuse and a harder
/// commitment than the harrier. The difference is scale, not a new rulebook.
#[test]
fn the_harbinger_commits_when_it_closes_in() {
    let mut game = started((100, 30));
    game.enemies.clear();
    game.enemies.push(Enemy::new(
        game.player.x + 12.0,
        game.player.y,
        Kind::Harbinger,
        5.0,
        0.0,
        1.0,
    ));
    let mut keys = Input::default();
    let mut charging = false;
    for _ in 0..240 {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
        if game.enemies[0].rush > 0.0
            && game.arena.distance(
                game.enemies[0].x,
                game.enemies[0].y,
                game.player.x,
                game.player.y,
            ) < 14.0
        {
            charging = true;
            break;
        }
    }
    assert!(charging, "the harbinger never committed to a charge");
}

/// The boss is on the road, not in a random pool: wave seven opens with a
/// harbinger, and the wave arithmetic decides when.
#[test]
fn a_boss_wave_opens_with_a_harbinger() {
    let mut game = started((100, 30));
    // Enough hull to survive the road to the first boss wave without the test
    // turning into a survival run of its own.
    game.player.max_hp = 250;
    game.player.hp = 250;

    let mut keys = Input::default();
    for _ in 0..(120 * 90) {
        game.update(1.0 / 120.0, &mut keys);
        keys.clear_edges();
        if game.wave >= 7 {
            break;
        }
    }

    assert_eq!(game.wave, 7, "the run never reached the first boss wave");
    assert!(
        game.enemies.iter().any(|e| e.kind == Kind::Harbinger),
        "wave seven did not open with the harbinger"
    );
}
