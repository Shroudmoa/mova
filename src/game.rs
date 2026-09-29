//! World state, wave pressure, and the rules that connect everything.

use crate::enemy::{Enemy, Kind};
use crate::goal::{self, RunStats};
use crate::input::Input;
use crate::pickup::{self, Kind as Pick, Pickup};
use crate::player::{self, Player};
use crate::projectile::{self, Projectile};
use crate::region;
use crate::save::{self, Save};
use crate::score::{Fx, PopKind, Score};
use crate::solid::SolidGrid;

/// Half-extents of the play space in world units. The camera follows MOVA
/// through this, so it is sized to give room to run rather than to fit a screen.
pub const ARENA_HX: f32 = 120.0;
pub const ARENA_HY: f32 = 70.0;
/// Seconds per wave.
pub const WAVE_TIME: f32 = 14.0;
const MAX_ENEMIES: usize = 140;
/// Enemies pushed out at the top of a run so wave one already has weight.
const OPENING_BURST: usize = 6;
/// Brushing past an enemy this close during a dash scores a near miss.
const GRAZE_RANGE: f32 = 3.4;
/// Speed fraction at which contact stops hurting and starts paying.
pub const RAM_AT: f32 = 0.55;
/// Ramming is worth roughly two bullets, so it is a choice and not a default.
const RAM_MULT: f32 = 1.95;

/// Points awarded the first time a run reaches a sector. Scales with how many
/// sectors are already known, so the last corner is worth the most.
const FOUND_BASE: u32 = 400;
const FOUND_STEP: u32 = 250;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Menu,
    Playing,
    Over,
}

pub struct Arena {
    pub hx: f32,
    pub hy: f32,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    pub const fn new() -> Self {
        Self {
            hx: ARENA_HX,
            hy: ARENA_HY,
        }
    }

    #[inline]
    pub fn clamp(&self, x: &mut f32, y: &mut f32) {
        *x = x.clamp(-self.hx, self.hx);
        *y = y.clamp(-self.hy, self.hy);
    }
}

pub struct Game {
    pub phase: Phase,
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub bullets: Vec<Projectile>,
    /// The map's furniture. Rebuilt from a fresh seed every run, so two runs
    /// are two different places to learn.
    pub solids: SolidGrid,
    pub pickups: Vec<Pickup>,
    pub score: Score,
    pub fx: Fx,
    pub arena: Arena,
    pub save: Save,

    pub stats: RunStats,
    pub wave: u32,
    /// Bitmask of goals unlocked by the run that just ended.
    pub new_goals: u32,

    wave_t: f32,
    spawn_t: f32,
    fire_t: f32,
    pickup_t: f32,
    /// Index of the sector MOVA is standing in.
    pub sector: usize,
    /// Seconds of banner left for a sector that has just been discovered.
    pub sector_t: f32,
    /// True while the sector banner is showing its "first find" wording.
    pub sector_new: bool,
    pub banner_t: f32,
    /// One-shot teaching beat the first time MOVA is fast enough to ram.
    pub hint_t: f32,
    hint_shown: bool,
    /// Whether the cap has been reached yet this run, which is when the second
    /// hint fires.
    blaze_shown: bool,
    /// Whether the corner radar is drawn. A screen preference rather than part
    /// of the run, so `start` deliberately leaves it alone.
    pub radar: bool,
    pub time: f32,
    pub run_t: f32,
    pub new_high: bool,

    /// Camera, in world units. Eased toward the player, then clamped so the
    /// view never drifts past the arena wall.
    pub cam_x: f32,
    pub cam_y: f32,
    /// Half-extents of what the terminal can currently show, in world units.
    view_hx: f32,
    view_hy: f32,

    rng: u32,
}

impl Game {
    pub fn new(save: Save) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9, |d| d.as_nanos() as u32)
            | 1;
        Self {
            phase: Phase::Menu,
            player: Player::new(),
            enemies: Vec::with_capacity(MAX_ENEMIES),
            bullets: Vec::with_capacity(projectile::MAX),
            solids: SolidGrid::new(seed ^ seed.rotate_left(15)),
            pickups: Vec::with_capacity(pickup::MAX),
            score: Score::new(),
            fx: Fx::new(),
            arena: Arena::new(),
            save,
            stats: RunStats::default(),
            wave: 1,
            new_goals: 0,
            wave_t: 0.0,
            spawn_t: 0.0,
            fire_t: 0.0,
            pickup_t: 0.0,
            sector: 0,
            sector_t: 0.0,
            sector_new: false,
            banner_t: 0.0,
            hint_t: 0.0,
            hint_shown: false,
            blaze_shown: false,
            radar: true,
            time: 0.0,
            run_t: 0.0,
            new_high: false,
            cam_x: 0.0,
            cam_y: 0.0,
            view_hx: 60.0,
            view_hy: 26.0,
            rng: seed,
        }
    }

    #[inline]
    fn rnd(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    /// One step of the xorshift. Split out from [`Self::rnd`] because the map
    /// generator wants the raw word rather than a float.
    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// A fresh word for the map generator. Two of these in a row are never
    /// equal, which is the only thing the map asks of a seed.
    ///
    /// Public so a test can watch the seeds themselves rather than infer them
    /// from the maps: a generator that silently stopped varying would still
    /// produce connected maps, and only the seeds say otherwise.
    pub fn map_seed(&mut self) -> u32 {
        self.next_u32() ^ self.next_u32().rotate_left(11)
    }

    /// Tells the simulation how much world the terminal can show, so spawns
    /// land just past the edge of the screen instead of inside it.
    pub fn set_view(&mut self, hx: f32, hy: f32) {
        self.view_hx = hx;
        self.view_hy = hy;
    }

    pub fn start(&mut self) {
        self.player = Player::new();
        // A new map every run. It has to be the first thing rebuilt: pickups,
        // spawns and the opening push all read the furniture, and rolling any
        // of them against the previous run's pillars would put a core inside a
        // block on the first frame.
        self.solids = SolidGrid::new(self.map_seed());
        self.enemies.clear();
        self.bullets.clear();
        self.fx.clear();
        self.score = Score::new();
        self.stats = RunStats::default();
        self.wave = 1;
        self.wave_t = 0.0;
        self.spawn_t = 0.35;
        self.fire_t = 0.0;
        self.pickup_t = 0.0;
        self.banner_t = 1.8;
        self.hint_t = 0.0;
        self.hint_shown = false;
        self.blaze_shown = false;
        self.run_t = 0.0;
        self.new_high = false;
        self.new_goals = 0;
        self.cam_x = 0.0;
        self.cam_y = 0.0;
        self.sector = region::at(0.0, 0.0, ARENA_HX, ARENA_HY);
        self.sector_t = 0.0;
        self.sector_new = false;
        self.pickups.clear();
        // A starting handful, so there is something to walk toward on the
        // first move rather than a blank map that fills in over time.
        for _ in 0..8 {
            if let Some(p) = self.roll_pickup(30.0) {
                self.pickups.push(p);
            }
        }
        self.phase = Phase::Playing;

        for _ in 0..OPENING_BURST {
            self.spawn_enemy();
        }
    }

    pub fn confirm(&mut self) {
        if self.phase != Phase::Playing {
            self.start();
        }
    }

    /// Advances only what the paused screen is allowed to animate. Everything
    /// that would affect the run is deliberately left alone, including the
    /// timers, so pausing is genuinely free.
    pub fn tick_idle(&mut self, dt: f32) {
        self.time += dt;
    }

    pub fn update(&mut self, dt: f32, input: &mut Input) {
        // Time always ticks so the menu keeps breathing between runs.
        self.time += dt;
        if self.phase != Phase::Playing {
            return;
        }
        self.banner_t = (self.banner_t - dt).max(0.0);
        self.hint_t = (self.hint_t - dt).max(0.0);
        self.sector_t = (self.sector_t - dt).max(0.0);
        self.run_t += dt;
        self.stats.seconds = self.run_t;

        self.player
            .update(dt, input, &self.arena, &self.solids, &mut self.fx);
        self.sector_tick();
        self.spawn_tick(dt);
        self.enemies_tick(dt);
        self.fire_tick(dt);
        self.bullets_tick(dt);
        self.pickups_tick(dt);
        self.collide();
        self.score.update(dt);
        self.fx.update(dt);

        let ratio = self.player.speed_ratio();
        self.stats.peak_speed = self.stats.peak_speed.max(ratio);
        if !self.hint_shown && ratio >= RAM_AT {
            // Shown once per run, the first time MOVA is fast enough to ram.
            self.hint_shown = true;
            self.hint_t = 2.2;
        }
        if !self.blaze_shown && self.player.blazing() {
            // And again at the cap, which is the state that actually makes
            // nothing land. Without this the invulnerability reads as a bug
            // rather than as a thing to hold on to.
            self.blaze_shown = true;
            self.hint_t = 2.4;
        }
        self.cam_follow(dt);
    }

    /// Eases the camera toward MOVA and keeps the view inside the arena.
    ///
    /// Public for the same reason [`Self::set_view`] is: it is a step of the
    /// simulation, and a test that has to settle the camera should be able to
    /// run it without also simulating twenty seconds of a run.
    pub fn cam_follow(&mut self, dt: f32) {
        // Framerate independent easing, so the camera feels the same at any step.
        let k = 1.0 - (-11.0 * dt).exp();
        self.cam_x += (self.player.x - self.cam_x) * k;
        self.cam_y += (self.player.y - self.cam_y) * k;
        // How far the view is allowed to travel before the arena wall stops it.
        // The view is far smaller than the arena on any normal terminal, so this
        // is normally the whole arena and the camera is free to follow MOVA all
        // the way to the corner. It only bites on a window big enough to see
        // past the edge, where pinning to the middle is what keeps the void
        // off screen.
        let lx = (ARENA_HX - self.view_hx).max(0.0);
        let ly = (ARENA_HY - self.view_hy).max(0.0);
        self.cam_x = self.cam_x.clamp(-lx, lx);
        self.cam_y = self.cam_y.clamp(-ly, ly);
    }

    // ---- sectors ---------------------------------------------------------

    /// Watches for MOVA crossing into a new sector. The first entry into any
    /// sector this run pays out, which is the whole reason to walk the map.
    fn sector_tick(&mut self) {
        let s = region::at(self.player.x, self.player.y, ARENA_HX, ARENA_HY);
        if s == self.sector {
            return;
        }
        self.sector = s;
        self.sector_t = 1.6;

        let bit = 1u32 << s;
        // Already seen in a previous run: the HUD already says where you are,
        // so a full banner would be repeating itself. Pay nothing.
        if self.save.sectors & bit != 0 {
            self.sector_new = false;
            self.sector_t = 0.0;
            return;
        }

        self.save.sectors |= bit;
        self.sector_new = true;
        self.stats.sectors += 1;
        // Worth more for each sector already charted, so the far corner is the
        // tempting one rather than an afterthought.
        let found = self.save.sectors.count_ones();
        let pts = FOUND_BASE + FOUND_STEP * (found - 1);
        self.score.points += pts;
        // The banner names the sector; the popup only carries the payout.
        self.fx
            .pop(self.player.x, self.player.y, pts, PopKind::Found);
        self.fx.ring(self.player.x, self.player.y, true);
    }

    // ---- pressure -------------------------------------------------------

    fn spawn_interval(&self) -> f32 {
        (0.74 * 0.905f32.powi(self.wave as i32 - 1)).clamp(0.09, 0.74)
    }

    fn enemy_speed(&self) -> f32 {
        (8.5 + self.wave as f32 * 0.75).min(26.0)
    }

    /// Picks what walks in. Runners show up from wave two, brutes from three.
    fn roll_kind(&mut self) -> Kind {
        let r = self.rnd();
        if self.wave >= 3 && r < 0.09 {
            Kind::Brute
        } else if self.wave >= 2 && r < 0.09 + 0.34 {
            Kind::Runner
        } else {
            Kind::Grunt
        }
    }

    /// The arena is much larger than the view now, so the count is set to keep
    /// the same on-screen crowding it had when everything fitted at once.
    fn max_alive(&self) -> usize {
        (26 + self.wave as usize * 4).min(MAX_ENEMIES)
    }

    fn spawn_tick(&mut self, dt: f32) {
        self.wave_t += dt;
        if self.wave_t >= WAVE_TIME {
            self.wave_t -= WAVE_TIME;
            self.wave += 1;
            self.banner_t = 1.8;
            // A wave opens with a burst so pressure arrives immediately.
            let burst = 4 + self.wave as usize;
            for _ in 0..burst {
                if self.enemies.len() < self.max_alive() {
                    self.spawn_enemy();
                }
            }
        }

        self.spawn_t -= dt;
        if self.spawn_t <= 0.0 {
            self.spawn_t = self.spawn_interval();
            if self.enemies.len() < self.max_alive() {
                self.spawn_enemy();
            }
        }
    }

    /// Spawns on a ring just outside the view. If that ring runs past the arena
    /// wall or lands inside a pillar, the position is folded back in and the
    /// angle is re-rolled until it is both valid and clear of MOVA.
    fn spawn_enemy(&mut self) {
        let (px, py) = (self.player.x, self.player.y);
        let view_r = (self.view_hx * self.view_hx + self.view_hy * self.view_hy).sqrt();
        let mut x = px;
        let mut y = py;

        for _ in 0..6 {
            let a = self.rnd() * std::f32::consts::TAU;
            let r = view_r * (1.04 + self.rnd() * 0.22);
            x = px + a.cos() * r;
            y = py + a.sin() * r;
            self.arena.clamp(&mut x, &mut y);
            let (dx, dy) = (x - px, y - py);
            let far_enough = dx * dx + dy * dy > 25.0 * 25.0;
            if far_enough && !self.solids.inside(x, y) {
                break;
            }
        }

        let kind = self.roll_kind();
        let speed = self.enemy_speed() * kind.speed() * (0.86 + self.rnd() * 0.3);
        let wob = self.rnd() * std::f32::consts::TAU;
        let turn = if self.rnd() < 0.5 { -1.0 } else { 1.0 };
        self.enemies.push(Enemy::new(x, y, kind, speed, wob, turn));
    }

    // ---- pickups --------------------------------------------------------

    /// Chooses a kind, weighted, with heal only ever offered when it would
    /// actually do something.
    fn roll_pick_kind(&mut self) -> Pick {
        let hurt = self.player.hp < self.player.max_hp;
        let total =
            Pick::Core.weight() + Pick::Surge.weight() + if hurt { Pick::Heal.weight() } else { 0 };
        let mut roll = self.rnd() * total as f32;
        for k in [Pick::Core, Pick::Surge, Pick::Heal] {
            if !hurt && k == Pick::Heal {
                continue;
            }
            roll -= k.weight() as f32;
            if roll <= 0.0 {
                return k;
            }
        }
        Pick::Core
    }

    /// Finds open floor for a pickup, at least `min_gap` from MOVA so nothing
    /// lands underfoot, and clear of any pillar.
    fn roll_pickup(&mut self, min_gap: f32) -> Option<Pickup> {
        let kind = self.roll_pick_kind();
        let gap2 = min_gap * min_gap;
        for _ in 0..24 {
            let x = (self.rnd() * 2.0 - 1.0) * (ARENA_HX - 4.0);
            let y = (self.rnd() * 2.0 - 1.0) * (ARENA_HY - 4.0);
            // Clear by a body's width, not by a point. A pickup that merely
            // has an open centre still renders half-buried once it bobs and
            // once the magnet has dragged it around.
            if self.solids.hit(x, y, 1.6) {
                continue;
            }
            let (px, py) = (self.player.x, self.player.y);
            let dx = x - px;
            let dy = y - py;
            if dx * dx + dy * dy < gap2 {
                continue;
            }
            // Do not stack two pickups into one unreadable pile.
            if self
                .pickups
                .iter()
                .any(|p| (p.x - x).powi(2) + (p.y - y).powi(2) < 100.0)
            {
                continue;
            }
            let phase = self.rnd() * std::f32::consts::TAU;
            return Some(Pickup::new(kind, x, y, phase));
        }
        None
    }

    fn pickups_tick(&mut self, dt: f32) {
        let (px, py) = (self.player.x, self.player.y);
        let lift = self.time;
        let magnet2 = pickup::MAGNET * pickup::MAGNET;
        let grab2 = (pickup::PICK_RADIUS + player::RADIUS) * (pickup::PICK_RADIUS + player::RADIUS);

        let mut i = 0;
        while i < self.pickups.len() {
            let p = &mut self.pickups[i];
            p.age += dt;
            // Drift home after any magnet pull, so a nudged pickup eases back
            // to its spot instead of being dragged around by the player.
            p.hx += (p.x - p.hx) * (1.0 - 3.0 * dt);
            p.hy += (p.y - p.hy) * (1.0 - 3.0 * dt);
            p.x = p.hx;
            p.y = p.hy + p.lift(lift);

            let (dx, dy) = (px - p.x, py - p.y);
            let d2 = dx * dx + dy * dy;
            if d2 < magnet2 && d2 > 0.001 {
                let d = d2.sqrt();
                // The pull is refused if it would drag the pickup into a
                // pillar. Cheap, and it stops an orb being sucked into a wall
                // and becoming unreachable.
                let nx = p.x + dx / d * pickup::MAGNET_PULL * dt;
                let ny = p.y + dy / d * pickup::MAGNET_PULL * dt;
                if !self.solids.hit(nx, ny, 1.0) {
                    p.x = nx;
                    p.y = ny;
                }
            }

            if d2 <= grab2 {
                let taken = self.take_pickup(i);
                if taken {
                    continue;
                }
            }
            i += 1;
        }

        // Refill up to the cap so the map is always worth a detour.
        self.pickup_t -= dt;
        if self.pickup_t <= 0.0 && self.pickups.len() < pickup::MAX {
            self.pickup_t = pickup::RESPAWN;
            if let Some(p) = self.roll_pickup(14.0) {
                self.pickups.push(p);
            }
        }
    }

    /// Collects one pickup. Returns false if it was already full health, in
    /// which case it is left on the floor.
    fn take_pickup(&mut self, i: usize) -> bool {
        let p = self.pickups[i];
        match p.kind {
            Pick::Core => {
                // Scored off the live combo and it refreshes the timer, so a
                // detour pays best when it happens mid-chain.
                let pts = self.score.core(pickup::CORE_VALUE);
                self.fx.pop(p.x, p.y, pts, PopKind::Core);
                self.fx.burst(p.x, p.y, 12.0);
                self.stats.cores += 1;
            }
            Pick::Surge => {
                self.player.surge();
                self.fx.note(p.x, p.y, "SURGE", PopKind::Surge);
                self.fx.ring(p.x, p.y, true);
                for _ in 0..8 {
                    let a = self.rnd() * std::f32::consts::TAU;
                    self.fx
                        .particle(p.x, p.y, a.cos() * 30.0, a.sin() * 30.0, 0.4, 1);
                }
                self.stats.surges += 1;
            }
            Pick::Heal => {
                if self.player.hp >= self.player.max_hp {
                    return false;
                }
                self.player.hp += 1;
                self.fx.note(p.x, p.y, "REPAIR", PopKind::Heal);
                self.fx.ring(p.x, p.y, false);
                self.stats.heals += 1;
            }
        }
        self.pickups.swap_remove(i);
        true
    }

    // ---- motion ---------------------------------------------------------

    fn enemies_tick(&mut self, dt: f32) {
        let (px, py) = (self.player.x, self.player.y);
        let t = self.time;
        for e in self.enemies.iter_mut() {
            e.update(dt, px, py, t, &self.solids);
        }
    }

    fn fire_tick(&mut self, dt: f32) {
        self.fire_t -= dt;
        if self.fire_t > 0.0 {
            return;
        }
        let surging = self.player.surging();
        self.fire_t = if surging {
            projectile::SURGE_INTERVAL
        } else {
            projectile::FIRE_INTERVAL
        };
        let Some((ux, uy)) = projectile::nearest(self.player.x, self.player.y, &self.enemies)
        else {
            return;
        };
        if self.bullets.len() < projectile::MAX {
            self.bullets.push(Projectile::new(
                self.player.x + ux * 1.6,
                self.player.y + uy * 1.6,
                ux,
                uy,
            ));
        }
    }

    fn bullets_tick(&mut self, dt: f32) {
        let hx = ARENA_HX + 6.0;
        let hy = ARENA_HY + 6.0;
        let mut i = 0;
        while i < self.bullets.len() {
            let b = &mut self.bullets[i];
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.life -= dt;
            // Bullets stop on pillars as well as on bodies. Without that a
            // cross-lane shot sails through the map and the pillars stop
            // reading as cover.
            let blocked = self.solids.hit(b.x, b.y, projectile::RADIUS);
            if blocked {
                self.fx.burst(b.x, b.y, 8.0);
            }
            if b.life <= 0.0 || b.spent || blocked || b.x.abs() > hx || b.y.abs() > hy {
                self.bullets.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    // ---- resolution -----------------------------------------------------

    fn collide(&mut self) {
        let dashing = self.player.dashing();
        let armed = self.player.armed();

        for bi in 0..self.bullets.len() {
            if self.bullets[bi].spent {
                continue;
            }
            let (bx, by) = (self.bullets[bi].x, self.bullets[bi].y);
            for ei in 0..self.enemies.len() {
                if self.enemies[ei].dead {
                    continue;
                }
                let (ex, ey, er) = (self.enemies[ei].x, self.enemies[ei].y, self.enemies[ei].r);
                let hit = projectile::RADIUS + er;
                let (dx, dy) = (ex - bx, ey - by);
                if dx * dx + dy * dy <= hit * hit {
                    self.bullets[bi].spent = true;
                    let e = &mut self.enemies[ei];
                    e.hp = e.hp.saturating_sub(1);
                    e.hit = 0.10;
                    if e.hp == 0 {
                        e.dead = true;
                        self.kill_enemy(ei);
                    }
                    break;
                }
            }
        }

        let (px, py) = (self.player.x, self.player.y);
        for ei in 0..self.enemies.len() {
            if self.enemies[ei].dead {
                continue;
            }
            let (ex, ey, er) = (self.enemies[ei].x, self.enemies[ei].y, self.enemies[ei].r);
            let (dx, dy) = (ex - px, ey - py);
            let d2 = dx * dx + dy * dy;

            if dashing {
                // Rewards cutting through the swarm instead of avoiding it.
                let graze_r2 = (er + GRAZE_RANGE) * (er + GRAZE_RANGE);
                if d2 <= graze_r2 && !self.enemies[ei].grazed {
                    self.enemies[ei].grazed = true;
                    let pts = self.score.near_miss();
                    self.fx.pop(ex, ey, pts, PopKind::Near);
                } else if d2 > graze_r2 * 2.6 {
                    self.enemies[ei].grazed = false;
                }
                continue;
            }

            let touch = er + player::RADIUS;
            if d2 > touch * touch {
                continue;
            }

            // Fast enough to break it. Speed is the weapon; standing still and
            // letting the gun work is the safe, poorer option. Heavy enemies
            // opt out, so momentum is a choice about *what* to run at.
            if armed && !self.enemies[ei].kind.heavy() {
                self.enemies[ei].dead = true;
                self.ram_kill(ei);
                continue;
            }

            if armed && self.enemies[ei].kind.heavy() {
                // Shoved off a brute you simply bounce. No damage, no points:
                // the answer is the gun, and the game says so by being quiet.
                let (ux, uy) = {
                    let l = d2.sqrt().max(0.001);
                    (dx / l, dy / l)
                };
                self.enemies[ei].x += ux * 2.2;
                self.enemies[ei].y += uy * 2.2;
                self.player.vx -= ux * 18.0;
                self.player.vy -= uy * 18.0;
                self.fx.ring(ex, ey, false);
                continue;
            }

            if self.player.damage(ex, ey) {
                self.stats.hits += 1;
                self.score.break_combo();
                self.fx.burst(px, py, 20.0);
                self.fx.ring(px, py, true);
                for i in 0..5 {
                    let a = i as f32 * (std::f32::consts::TAU / 5.0);
                    self.fx
                        .particle(px, py, a.cos() * 26.0, a.sin() * 26.0, 0.3, 2);
                }
                if !self.player.alive {
                    self.end_run();
                    return;
                }
            }
        }

        // Reclaim the dead. swap_remove keeps this allocation free.
        let mut i = 0;
        while i < self.enemies.len() {
            if self.enemies[i].dead {
                self.enemies.swap_remove(i);
            } else {
                i += 1;
            }
        }
        self.bullets.retain(|b| !b.spent);
    }

    fn kill_enemy(&mut self, ei: usize) {
        let (x, y) = (self.enemies[ei].x, self.enemies[ei].y);
        let kind = self.enemies[ei].kind;
        let ratio = self.player.speed_ratio();
        let (pts, fast) = self.score.kill_at(ratio, kind.mult());
        // Nudge sideways so kills in a crowd do not stack their popups.
        let jx = (self.rnd() - 0.5) * 5.0;
        self.fx.burst(x, y, 15.0);
        self.fx.ring(x, y, kind.heavy());
        self.fx.pop(
            x + jx,
            y,
            pts,
            if fast { PopKind::Fast } else { PopKind::Kill },
        );
        self.stats.kills += 1;
        if kind == Kind::Brute {
            self.stats.brutes += 1;
        }
    }

    fn ram_kill(&mut self, ei: usize) {
        let (x, y) = (self.enemies[ei].x, self.enemies[ei].y);
        let ratio = self.player.speed_ratio();
        let (pts, _) = self.score.kill_at(ratio, RAM_MULT);
        let jx = (self.rnd() - 0.5) * 4.0;
        self.fx.burst(x, y, 30.0);
        self.fx.ring(x, y, true);
        for _ in 0..4 {
            let a = self.rnd() * std::f32::consts::TAU;
            self.fx
                .particle(x, y, a.cos() * 34.0, a.sin() * 34.0, 0.26, 2);
        }
        self.fx.pop(x + jx, y, pts, PopKind::Ram);
        self.player.kick();
        self.stats.kills += 1;
        self.stats.rams += 1;
    }

    fn end_run(&mut self) {
        self.stats.wave = self.wave;
        self.stats.best_combo = self.score.best_combo;
        self.stats.score = self.score.points;
        self.new_high = self.save.absorb(&self.stats, self.score.points);

        // Any goal this run satisfies and no earlier run did gets ticked off,
        // permanently, and remembered for the game-over screen.
        self.new_goals = 0;
        for i in 0..goal::COUNT {
            let bit = goal::GOALS[i].bit;
            if self.save.goals & bit == 0 && goal::done(i, &self.stats, &self.save) {
                self.save.goals |= bit;
                self.new_goals |= bit;
            }
        }
        save::store(&self.save);

        self.fx.clear();
        self.phase = Phase::Over;
    }
}
