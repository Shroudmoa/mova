//! Projectiles. MOVA fires these on its own; there is no aim and no ammo.

pub const SPEED: f32 = 62.0;
pub const RADIUS: f32 = 0.55;
/// Deliberately slow. MOVA's gun is the limiter that keeps the arena crowded.
pub const FIRE_INTERVAL: f32 = 0.30;
/// Fired while a Surge is live. The pickup is a real detour only if it makes
/// the map more dangerous as well as faster.
pub const SURGE_INTERVAL: f32 = 0.19;
pub const LIFE: f32 = 1.5;
/// Ceiling on live bullets. Past this the gun simply stops adding to the swarm.
pub const MAX: usize = 72;

pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub spent: bool,
}

impl Projectile {
    #[inline]
    pub fn new(x: f32, y: f32, ux: f32, uy: f32) -> Self {
        Self {
            x,
            y,
            vx: ux * SPEED,
            vy: uy * SPEED,
            life: LIFE,
            spent: false,
        }
    }
}

/// Direction to the closest living enemy, if there is one.
///
/// Heavy targets are ignored while anything else is in range. A brute is a
/// six-hit commitment, and the gun should not open with one while a runner is
/// right there; it still takes the shot once it is the only thing left.
pub fn nearest(px: f32, py: f32, enemies: &[crate::enemy::Enemy]) -> Option<(f32, f32)> {
    let mut best = f32::MAX;
    let mut best_heavy = f32::MAX;
    let mut light = None;
    let mut heavy = None;

    for e in enemies.iter() {
        if e.dead {
            continue;
        }
        let dx = e.x - px;
        let dy = e.y - py;
        let d = dx * dx + dy * dy;
        if e.kind.heavy() {
            if d < best_heavy {
                best_heavy = d;
                heavy = Some((dx, dy));
            }
        } else if d < best {
            best = d;
            light = Some((dx, dy));
        }
    }

    let dir = light.or(heavy)?;
    let len = dir.0.hypot(dir.1);
    (len > 0.001).then_some((dir.0 / len, dir.1 / len))
}
