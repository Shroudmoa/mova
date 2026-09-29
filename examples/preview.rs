//! Renders frames to plain text so the layout can be checked without a TTY.
//!
//! ```text
//! cargo run --example preview -- [cols rows] [steps] [seed...]
//! ```

use mova::game::{Game, Phase};
use mova::input::Input;
use mova::renderer;
use mova::save::Save;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let w: u16 = args.first().and_then(|s| s.parse().ok()).unwrap_or(100);
    let h: u16 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let steps: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    let mut game = Game::new(Save::default());

    let (hx, hy) = renderer::view_half(area);
    game.set_view(hx, hy);

    if steps > 0 {
        game.start();
        let mut keys = Input::default();
        for i in 0..steps {
            // A slow figure-eight, so the camera, the pillars and the radar all
            // get exercised instead of the player standing in the plaza.
            keys.left = (i / 40) % 4 == 0 || (i / 40) % 4 == 3;
            keys.right = (i / 40) % 4 == 1 || (i / 40) % 4 == 2;
            keys.up = (i / 60) % 2 == 0;
            keys.down = (i / 60) % 2 == 1;
            keys.dash = i % 90 == 0;
            game.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
        }
    }

    renderer::render(&mut buf, area, &game);

    let mode = args.get(3).map(String::as_str).unwrap_or("auto");
    let phase = match game.phase {
        Phase::Menu => "menu",
        Phase::Playing => "playing",
        Phase::Over => "over",
    };
    println!(
        "phase={phase} sector={} wave={} enemies={} pickups={} solids={}",
        game.sector,
        game.wave,
        game.enemies.len(),
        game.pickups.len(),
        game.solids.solids.len()
    );

    match mode {
        // The goals and game-over screens are reached by a second draw onto the
        // same buffer, which is not how the game ever does it: ratatui resets
        // the buffer between frames, and `Buffer::set_style` — which is all
        // `render` does to clear — leaves the old symbols in place. Reset here
        // so the second draw lands on a blank field and the output can be read
        // as what the player actually sees.
        "goals" => {
            buf.reset();
            renderer::goals(&mut buf, area, &game);
        }
        // Grant some progress so the game-over screen shows its unlocked strip.
        "over" => {
            game.save.total_score = 130_000;
            game.save.sectors = 0b0011_0101;
            game.save.goals = 0b0000_0000_0011_0111;
            game.new_goals = 0b0000_0000_0011_0000;
            game.save.high = 42_000;
            game.new_high = true;
            game.score.points = 88_123;
            game.phase = Phase::Over;
            buf.reset();
            renderer::render(&mut buf, area, &game);
        }
        _ => {}
    }

    for y in area.y..area.bottom() {
        let mut line = String::with_capacity(area.width as usize);
        for x in area.x..area.right() {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("{}", line.trim_end());
    }
}
