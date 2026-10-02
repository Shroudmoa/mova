//! World state, wave pressure, and the rules that connect everything.

use crate::enemy::{Enemy, Kind};
use crate::goal::{self, RunStats};
use crate::input::Input;
use crate::pickup::{self, Kind as Pick, Pickup};
use crate::player::{self, Player};
use crate::projectile::{self, Projectile};
use crate::region;
use crate::save::{self, Save};
use crate::score::{Fx, PopKind, Score, EMBER};
use crate::solid::SolidGrid;

/// Half-extents of the play space in world units. There is no wall at either:
/// the arena is a torus of `PERIOD_X` by `PERIOD_Y`, and running off one side
/// brings you in on the other. These are the sizes the camera eases between and
/// the sizes everything else is expressed in.
pub const ARENA_HX: f32 = crate::solid::PERIOD_X * 0.5;
pub const ARENA_HY: f32 = crate::solid::PERIOD_Y * 0.5;
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

/// How far from an ember still counts as running into the trail, in units, before
/// the enemy's own radius is added. Wide enough that the tail is a line and not a
/// dotted one — the embers are spaced one and a bit units apart, so anything
/// narrower would leave gaps you can stand an enemy in.
const WAKE_REACH: f32 = 1.6;

/// Seconds before the same enemy can be cut twice. Long enough that a body
/// travelling along a trail gets hit once rather than once per ember it passes,
/// and short enough that a trail wrapped back onto itself still cuts.
const WAKE_CD: f32 = 0.55;

/// What a wake kill pays, before the enemy's own multiplier. Above a bullet and
/// below a ram: the line is worth more than the gun and less than the contact.
const WAKE_MULT: f32 = 1.35;

/// The widest enemy on the board, so one reach covers all of them.
fn max_enemy_r(enemies: &[Enemy]) -> f32 {
    enemies.iter().map(|e| e.r).fold(0.0f32, f32::max)
}

/// How far a nova reaches, before the enemy's own radius. Big against the wake,
/// which is a blade's width: the whole difference between the two is that the
/// wake is where you have been and a nova is where you are, and a shockwave you
/// cannot see the edge of is worth reaching for in a crowd.
const NOVA_REACH: f32 = 11.0;

/// How far a nova throws whatever it catches. Enough to take an enemy off the
/// ground MOVA is about to cross, which is the point: a nova is not damage, it is
/// an opening.
const NOVA_PUSH: f32 = 26.0;

/// What a nova kill pays, before the enemy's own multiplier. Under a ram, over
/// the wake: it is the biggest thing MOVA owns that does not require touching the
/// enemy, and the payment says so.
const NOVA_MULT: f32 = 1.7;

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

    /// Folds a position back into `(-h, h]`. The seam is not a wall, so this is
    /// not a collision resolve — it is a teleport to the far side, and every
    /// caller that cares about it (the camera) has to take the short way round.
    #[inline]
    pub fn wrap_axis(v: f32, h: f32) -> f32 {
        (v + h).rem_euclid(h * 2.0) - h
    }

    /// Signed distance from `from` to `to` the short way round the seam.
    ///
    /// Without this the camera eases the long way when MOVA crosses, which on a
    /// world this size means a five-second pan across the whole arena every time
    /// you run off one edge.
    #[inline]
    pub fn delta_axis(from: f32, to: f32, h: f32) -> f32 {
        let d = (to - from).rem_euclid(h * 2.0);
        if d > h {
            d - h * 2.0
        } else {
            d
        }
    }

    #[inline]
    pub fn wrap(&self, x: f32, y: f32) -> (f32, f32) {
        (Self::wrap_axis(x, self.hx), Self::wrap_axis(y, self.hy))
    }

    /// Straight-line distance between two points, measured the short way round.
    #[inline]
    pub fn distance(&self, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
        let (dx, dy) = (
            Self::delta_axis(ax, bx, self.hx),
            Self::delta_axis(ay, by, self.hy),
        );
        (dx * dx + dy * dy).sqrt()
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
        self.nova_tick();
        self.wake_tick();
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

    /// Eases the camera toward MOVA the short way round the seam.
    ///
    /// There is nothing to clamp against any more, which is the whole point of
    /// the torus: the camera is free to follow MOVA anywhere and forever, and
    /// there is no corner of the map where it stops.
    ///
    /// Public for the same reason [`Self::set_view`] is: it is a step of the
    /// simulation, and a test that has to settle the camera should be able to
    /// run it without also simulating twenty seconds of a run.
    pub fn cam_follow(&mut self, dt: f32) {
        // Framerate independent easing, so the camera feels the same at any step.
        // The factor 40 ensures the camera keeps up with MOVA at cap speed
        // (44 units/frame) on the borderless torus arena.
        let k = 1.0 - (-40.0 * dt).exp();
        // Taken as a wrapped delta, so crossing the seam is a step and not a
        // pan. Both ends of the ease are inside the period, so the camera lands
        // on MOVA's side of it rather than drifting off on its own.
        self.cam_x += Arena::delta_axis(self.cam_x, self.player.x, self.arena.hx) * k;
        self.cam_y += Arena::delta_axis(self.cam_y, self.player.y, self.arena.hy) * k;
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
        self.fx.ring(self.player.x, self.player.y, true, true);
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

    /// Spawns on a ring just outside the view. If that ring runs past the seam
    /// or lands inside a pillar, the position is folded back in and the angle is
    /// re-rolled until it is both valid and clear of MOVA.
    ///
    /// Everything is measured with [`Arena::distance`], so an enemy that comes
    /// in from the far side of the seam counts as arriving, which is what it is.
    fn spawn_enemy(&mut self) {
        let (px, py) = (self.player.x, self.player.y);
        let view_r = (self.view_hx * self.view_hx + self.view_hy * self.view_hy).sqrt();
        let mut x = px;
        let mut y = py;

        for _ in 0..6 {
            let a = self.rnd() * std::f32::consts::TAU;
            let r = view_r * (1.04 + self.rnd() * 0.22);
            let (cand_x, cand_y) = self.arena.wrap(px + a.cos() * r, py + a.sin() * r);
            x = cand_x;
            y = cand_y;
            let far_enough = self.arena.distance(px, py, x, y) > 25.0;
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
            // Do not stack two pickups into one unreadable pile. Measured across
            // the seam, so two either side of the wrap do not land on top of
            // each other on screen.
            if self
                .pickups
                .iter()
                .any(|p| self.arena.distance(p.x, p.y, x, y) < 10.0)
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

            let dx = Arena::delta_axis(p.x, px, self.arena.hx);
            let dy = Arena::delta_axis(p.y, py, self.arena.hy);
            let d2 = dx * dx + dy * dy;
            if d2 < magnet2 && d2 > 0.001 {
                let d = d2.sqrt();
                // The pull is refused if it would drag the pickup into a
                // pillar. Cheap, and it stops an orb being sucked into a wall
                // and becoming unreachable.
                let (nx, ny) = self.arena.wrap(
                    p.x + dx / d * pickup::MAGNET_PULL * dt,
                    p.y + dy / d * pickup::MAGNET_PULL * dt,
                );
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
                self.fx.ring(p.x, p.y, true, true);
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
                self.fx.ring(p.x, p.y, false, true);
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
            e.wake_cd = (e.wake_cd - dt).max(0.0);
            e.update(dt, px, py, t, &self.arena, &self.solids);
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
            // Wrapped rather than culled. A shot fired at the right edge comes
            // back in from the left, which is both what the player expects and
            // what lets a lane be shot end to end.
            (b.x, b.y) = self.arena.wrap(b.x, b.y);
            if b.life <= 0.0 || b.spent || blocked {
                self.bullets.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    // ---- resolution -----------------------------------------------------

    /// A wall is a weapon, if you hit it hard enough.
    ///
    /// v3 gives the furniture its speed back on a reflection, which made the walls
    /// the best way to turn without paying for the turn — the one thing on a
    /// wrap-around map that is worth more than it costs, since there is no corner
    /// to lose momentum in and the furniture is the only corner there is. This
    /// pays for it. A bounce at ram speed throws a nova: damage, and a shove that
    /// clears a space in front of MOVA.
    ///
    /// The shove is the reason it is not just a second wake. Damage on contact is
    /// something you aim; a nova opens ground you were not going to get to, and
    /// the interesting bounce on a torus is always the one taken *into* a crowd
    /// with the crowd behind it.
    ///
    /// Gated at [`RAM_AT`], the same threshold the ram uses and the one the hint
    /// and the gauge already speak. Slow drift, lean, drift-off-a-post does not
    /// fire it, because a shockwave that came with every contact would make the
    /// furniture loud instead of useful.
    ///
    /// Consumes the bounce signal rather than polling it, so two reflections
    /// inside one step are one nova. There was one reflection; there should be one
    /// ring.
    pub fn nova_tick(&mut self) {
        if self.player.bounce_t <= 0.0 {
            return;
        }
        self.player.bounce_t = 0.0;
        if self.player.speed_ratio() < RAM_AT {
            return;
        }
        let (px, py) = (self.player.x, self.player.y);
        let (hx, hy) = (self.arena.hx, self.arena.hy);
        let reach = NOVA_REACH + max_enemy_r(&self.enemies);
        let reach2 = reach * reach;
        self.fx.ring(px, py, true, true);
        for _ in 0..8 {
            let a = self.rnd() * std::f32::consts::TAU;
            self.fx
                .particle(px, py, a.cos() * 30.0, a.sin() * 30.0, 0.24, 0);
        }

        for ei in 0..self.enemies.len() {
            // Wrapped for the same reason the wake is: the ring is drawn wrapped,
            // and MOVA has just come off a wall near the seam as often as not.
            let dx = Arena::delta_axis(px, self.enemies[ei].x, hx);
            let dy = Arena::delta_axis(py, self.enemies[ei].y, hy);
            let d2 = dx * dx + dy * dy;
            if self.enemies[ei].dead || d2 > reach2 {
                continue;
            }
            // Out along the away-normal, and never out along a zero one: an enemy
            // at the exact centre of a nova has no direction to be thrown, and
            // normalising there would throw it at one of the walls of the world.
            let d = d2.sqrt();
            let (ux, uy) = if d > 0.001 {
                (dx / d, dy / d)
            } else {
                (1.0, 0.0)
            };
            let e = &mut self.enemies[ei];
            e.x += ux * NOVA_PUSH;
            e.y += uy * NOVA_PUSH;
            (e.x, e.y) = self.arena.wrap(e.x, e.y);
            e.hit = 0.10;
            e.hp = e.hp.saturating_sub(1);
            let killed = e.hp == 0;
            e.dead = killed;
            let (mut nx, mut ny, r) = (e.x, e.y, e.r);
            // Into anything it landed in, the shove has to give way or the enemy
            // is stored inside a wall until its own update notices. The three
            // values come out of the roster first because the grid and the roster
            // are fields of the same struct, and passing `&mut self.enemies[ei].x`
            // next to `self.enemies[ei].r` is two borrows of one struct at once.
            self.solids.push_out(&mut nx, &mut ny, r);
            self.enemies[ei].x = nx;
            self.enemies[ei].y = ny;
            if killed {
                self.nova_kill(ei);
            }
        }
    }

    /// What a nova kill pays, before the enemy's own multiplier.
    fn nova_kill(&mut self, ei: usize) {
        let (x, y) = (self.enemies[ei].x, self.enemies[ei].y);
        let kind = self.enemies[ei].kind;
        let (pts, _) = self
            .score
            .kill_at(self.player.speed_ratio(), NOVA_MULT * kind.mult());
        self.fx.burst(x, y, 20.0);
        self.fx.ring(x, y, false, true);
        self.fx.pop(x, y, pts, PopKind::Ram);
        self.stats.kills += 1;
    }

    /// MOVA's wake is a weapon.
    ///
    /// The trail was already there and already the longest thing he draws, and it
    /// was the one part of the picture with nothing to do. At the cap it is now
    /// what happens to anything that runs into it, which changes what the cap is
    /// *for*: it was immunity — a thing to hold while you got out — and it is now
    /// the strongest thing you own. A line drawn across a crowd from one side to
    /// the other is worth more than the same crowd met head-on, because the ram
    /// spends the contact while the line is already past everything behind you.
    ///
    /// Only at the cap. Below it the trail is not drawn and not a weapon, and the
    /// threshold is the same one every other part of the game reads, so this needs
    /// no state of its own to stay consistent with the gauge on the screen.
    ///
    /// Measured against the embers rather than a separate trail object, so what
    /// cuts something is exactly what you can see cutting it. Anything can be
    /// overwritten between an ember and the frame, but not between the ember and
    /// the hit check — and there is no second shape to keep in sync with the first.
    pub fn wake_tick(&mut self) {
        if !self.player.blazing() {
            return;
        }
        let (hx, hy) = (self.arena.hx, self.arena.hy);
        let reach = WAKE_REACH + max_enemy_r(&self.enemies);
        let reach2 = reach * reach;
        // The trail's positions, taken before anything can be killed: a kill
        // spawns a burst and a ring, which grow `parts`, and a body held across a
        // call that pushes into it is a body the borrow checker is right about.
        let embers: Vec<(f32, f32)> = self
            .fx
            .parts
            .iter()
            .filter(|p| p.kind == EMBER)
            .map(|p| (p.x, p.y))
            .collect();
        for (px, py) in &embers {
            for ei in 0..self.enemies.len() {
                let e = &self.enemies[ei];
                if e.dead || e.wake_cd > 0.0 {
                    continue;
                }
                // Wrapped, because the trail is drawn wrapped. An ember left
                // against one edge of the map and an enemy standing just past
                // the other are drawn touching, so a wake that measured them
                // raw would be a weapon with a hole in it exactly where the
                // arena is least like itself.
                let dx = Arena::delta_axis(*px, e.x, hx);
                let dy = Arena::delta_axis(*py, e.y, hy);
                if dx * dx + dy * dy > reach2 {
                    continue;
                }
                self.enemies[ei].wake_cd = WAKE_CD;
                self.enemies[ei].hit = 0.10;
                self.enemies[ei].hp = self.enemies[ei].hp.saturating_sub(1);
                if self.enemies[ei].hp == 0 {
                    self.enemies[ei].dead = true;
                    self.wake_kill(ei);
                }
            }
        }
    }

    /// What a wake kill pays. Short of a ram on purpose: MOVA is not touching the
    /// thing, so it should not pay as though he is.
    fn wake_kill(&mut self, ei: usize) {
        let (x, y) = (self.enemies[ei].x, self.enemies[ei].y);
        let kind = self.enemies[ei].kind;
        let (pts, _) = self
            .score
            .kill_at(self.player.speed_ratio(), WAKE_MULT * kind.mult());
        self.fx.burst(x, y, 12.0);
        self.fx.ring(x, y, false, true);
        self.fx.pop(x, y, pts, PopKind::Kill);
        self.stats.kills += 1;
    }

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
                // Across the seam, so a shot fired at the edge connects with what
                // MOVA is chasing on the other side of the map.
                let dx = Arena::delta_axis(bx, ex, self.arena.hx);
                let dy = Arena::delta_axis(by, ey, self.arena.hy);
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
            // Contact is measured the short way round, so touching an enemy that
            // has just come through the wrap from the other side of the screen
            // is contact, not a near miss on the far side of the world.
            let dx = Arena::delta_axis(px, ex, self.arena.hx);
            let dy = Arena::delta_axis(py, ey, self.arena.hy);
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
                self.fx.ring(ex, ey, false, true);
                continue;
            }

            if self.player.damage(ex, ey) {
                self.stats.hits += 1;
                self.score.break_combo();
                self.fx.burst(px, py, 20.0);
                self.fx.ring(px, py, true, false);
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
        self.fx.ring(x, y, kind.heavy(), false);
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
        self.fx.ring(x, y, true, true);
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
