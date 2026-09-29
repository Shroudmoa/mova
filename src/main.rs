//! MOVA — a very small survival arcade game.

use std::time::{Duration, Instant};

use mova::game::{Game, Phase};
use mova::input::{self, Action, Repeat};
use mova::{renderer, save};

/// Simulation runs at a fixed step so the game feels identical everywhere.
const STEP: f32 = 1.0 / 120.0;
/// Frame budget. The loop sleeps the remainder instead of spinning.
const FRAME: Duration = Duration::from_micros(16_666);
/// Longest delta we will ever simulate in one go, so a stall cannot teleport
/// anything through a wall.
const MAX_DT: f32 = 0.25;

/// The menu's third screen. Plain bool rather than an enum: it has exactly two
/// places it can be entered from and nowhere else to go back to.
struct Overlay {
    goals: bool,
    /// Swallow any keypress so the one that opened the panel does not also
    /// start a run the instant it closes.
    swallow: bool,
    /// Run frozen. Kept here rather than on `Game` so pausing does not have to
    /// be understood by the simulation, which has no business knowing.
    paused: bool,
}

/// Feeds the terminal's current drawable size into the simulation, so spawns
/// land just past the edge of the screen instead of inside it.
fn set_view(area: ratatui::layout::Rect, game: &mut Game) {
    let (hx, hy) = renderer::view_half(area);
    game.set_view(hx, hy);
}

fn main() -> std::io::Result<()> {
    let mut term = ratatui::init();
    term.hide_cursor()?;
    let mut game = Game::new(save::load());
    let mut overlay = Overlay {
        goals: false,
        swallow: false,
        paused: false,
    };
    let mut repeat = Repeat::default();

    let mut last = Instant::now();
    let mut acc = 0.0f32;
    let result = loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(MAX_DT);
        last = now;
        acc += dt;

        let (mut keys, action) = input::poll(dt, &mut repeat);
        if overlay.goals {
            // Any key backs out; quitting from here still quits.
            if let Action::Quit = action {
                break Ok(());
            }
            if !overlay.swallow {
                overlay.goals = false;
            }
            overlay.swallow = false;
            acc = 0.0;
            term.draw(|f| {
                let area = f.area();
                set_view(area, &mut game);
                if overlay.goals {
                    renderer::goals(f.buffer_mut(), area, &game);
                } else {
                    renderer::draw(f, &game, overlay.paused);
                }
            })?;
            sleep_frame(last);
            continue;
        }

        match action {
            Action::Quit => break Ok(()),
            Action::Confirm => game.confirm(),
            Action::Goals => {
                if game.phase != Phase::Playing {
                    overlay.goals = true;
                    overlay.swallow = true;
                }
            }
            Action::Pause => {
                // Only meaningful mid-run; on the menu it would do nothing.
                if game.phase == Phase::Playing {
                    overlay.paused = !overlay.paused;
                    acc = 0.0;
                }
            }
            Action::Radar => game.radar = !game.radar,
            Action::None => {}
        }

        // A paused run keeps rendering and keeps time moving in the HUD, but
        // nothing is simulated, so no timer drains while you read the screen.
        if !overlay.paused {
            while acc >= STEP {
                game.update(STEP, &mut keys);
                keys.clear_edges();
                acc -= STEP;
            }
        } else {
            keys.clear_edges();
            game.tick_idle(STEP);
        }

        term.draw(|f| {
            let area = f.area();
            set_view(area, &mut game);
            renderer::draw(f, &game, overlay.paused);
        })?;

        sleep_frame(last);
    };

    ratatui::restore();
    result
}

fn sleep_frame(last: Instant) {
    let spent = last.elapsed();
    if spent < FRAME {
        std::thread::sleep(FRAME - spent);
    }
}
