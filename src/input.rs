//! Input: raw key state plus a few one-shot actions.

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

/// Debounce for the pause toggle. Terminals repeat a held key, and without
/// this the pause flickers on and off for as long as `P` is down.
const REPEAT_GUARD: f32 = 0.22;

/// Continuous (held) state. Re-asserted every frame by the terminal's key repeat.
#[derive(Default, Clone, Copy)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// Edge triggered: consumed by the first simulation step that sees it.
    pub dash: bool,
}

impl Input {
    /// Movement as a unit-ish vector. Length is 1 when a single axis is held,
    /// `sqrt(2)` on the diagonal, which keeps diagonal movement honest.
    #[inline]
    pub fn axis(self) -> (f32, f32) {
        let mut x = f32::from(self.right) - f32::from(self.left);
        let mut y = f32::from(self.down) - f32::from(self.up);
        let len = (x * x + y * y).sqrt();
        if len > 1.0 {
            x /= len;
            y /= len;
        }
        (x, y)
    }

    #[inline]
    pub fn clear_edges(&mut self) {
        self.dash = false;
    }
}

pub enum Action {
    None,
    Confirm,
    /// Opens the goal ledger from the menu or the game-over screen.
    Goals,
    Quit,
    /// Freezes the simulation without leaving the run.
    Pause,
    /// Shows or hides the corner radar.
    Radar,
}

/// Rolling record of which action keys are down, so auto-repeat from the
/// terminal does not machine-gun the one-shot actions.
#[derive(Default)]
pub struct Repeat {
    quit: f32,
    goals: f32,
    pause: f32,
    radar: f32,
}

impl Repeat {
    /// Returns true when the action should fire, i.e. either a fresh press or
    /// the repeat window has elapsed.
    fn allow(slot: &mut f32, down: bool, dt: f32) -> bool {
        if !down {
            *slot = 0.0;
            return false;
        }
        if *slot <= 0.0 {
            *slot = REPEAT_GUARD;
            return true;
        }
        *slot -= dt;
        false
    }
}

/// Drains every pending event without blocking. One-shot actions are debounced
/// against key auto-repeat, so holding a key down does not retrigger it.
pub fn poll(dt: f32, rep: &mut Repeat) -> (Input, Action) {
    let mut input = Input::default();
    // Collected raw, then resolved once the frame is drained. A burst of events
    // in one frame should still produce exactly one action, not the last one.
    let mut want_quit = false;
    let mut want_goals = false;
    let mut want_pause = false;
    let mut want_radar = false;
    let mut want_confirm = false;

    let key = |k: crossterm::event::KeyEvent,
               input: &mut Input,
               quit: &mut bool,
               goals: &mut bool,
               pause: &mut bool,
               radar: &mut bool,
               confirm: &mut bool| {
        if k.kind == KeyEventKind::Release {
            // Not all terminals report releases, but honour them when they arrive.
            match k.code {
                KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => input.left = false,
                KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => input.right = false,
                KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => input.up = false,
                KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => input.down = false,
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => input.left = true,
            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => input.right = true,
            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => input.up = true,
            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => input.down = true,
            KeyCode::Char(' ') => input.dash = true,
            KeyCode::Enter => *confirm = true,
            KeyCode::Char('g') | KeyCode::Char('G') => *goals = true,
            KeyCode::Char('p') | KeyCode::Char('P') => *pause = true,
            KeyCode::Char('m') | KeyCode::Char('M') => *radar = true,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => *quit = true,
            _ => {}
        }
    };

    while event::poll(Duration::ZERO).unwrap_or(false) {
        match event::read() {
            Ok(Event::Key(k)) => key(
                k,
                &mut input,
                &mut want_quit,
                &mut want_goals,
                &mut want_pause,
                &mut want_radar,
                &mut want_confirm,
            ),
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let action = if Repeat::allow(&mut rep.quit, want_quit, dt) {
        Action::Quit
    } else if Repeat::allow(&mut rep.goals, want_goals, dt) {
        Action::Goals
    } else if Repeat::allow(&mut rep.pause, want_pause, dt) {
        Action::Pause
    } else if Repeat::allow(&mut rep.radar, want_radar, dt) {
        Action::Radar
    } else if want_confirm {
        Action::Confirm
    } else {
        Action::None
    };

    (input, action)
}
