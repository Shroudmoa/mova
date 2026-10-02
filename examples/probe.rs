use mova::game::Game;
use mova::input::Input;
use mova::renderer;
use mova::save::Save;
use ratatui::layout::Rect;

fn main() {
    for trial in 0..5 {
        let area = Rect::new(0, 0, 100, 30);
        let mut game = Game::new(Save::default());
        let (hx, hy) = renderer::view_half(area);
        game.set_view(hx, hy);
        game.start();
        let mut keys = Input::default();
        game.player.x = 0.0;
        game.player.y = 0.0;
        for _ in 0..120 {
            game.player.vx = 0.0;
            game.player.vy = game.player.cap() * 1.5;
            game.update(1.0 / 120.0, &mut keys);
            keys.clear_edges();
        }
        let cap = game.player.cap();
        game.fx.rings.clear();
        game.player.vx = cap * 1.5;
        game.player.vy = 0.0;
        game.player.bounce_t = 1.0;
        game.nova_tick();
        println!(
            "trial {trial}: cap {cap:.1} speed {:.1} ratio {:.3} bounce_t {} rings {}",
            game.player.speed(),
            game.player.speed_ratio(),
            game.player.bounce_t,
            game.fx.rings.len()
        );
    }
}
