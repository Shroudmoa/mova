//! Fixed furniture.
//!
//! Generated from a seed, and given a fresh one every run, so each map is a
//! new place to learn. The seed is the only input: the same word always builds
//! the same arena, which is what lets a test pin a layout down and lets a
//! layout worth complaining about be replayed.

/// Centre-to-centre spacing of the block lattice, in world units.
pub const CELL: f32 = 12.0;
/// Half-extents of one block. Eight units across leaves lanes between
/// neighbours wide enough to dash straight down.
const HALF: f32 = 4.0;
/// Lattice extents, sized so a clear margin of floor sits against every wall.
const COLS: i32 = 19;
const ROWS: i32 = 11;
/// MOVA's starting plaza is never built on.
const PLAZA: i32 = 1;
/// A candidate map is only accepted once this share of its floor is reachable
/// from the plaza. Whatever is left over gets filled in, so there are no traps.
const OPEN_RATIO: f32 = 0.94;
/// Density knob. Walked upward until the floor above is connected enough.
/// Tuned by eye against the render: dense enough that the map has landmarks
/// and cover, open enough that there are real lanes to run.
const FILL: f32 = 0.56;

// Bucket grid. One block fits well inside its own lattice cell, so a query only
// ever has to look at the buckets its radius actually reaches.
const BX0: i32 = -COLS / 2 - 1;
const BY0: i32 = -ROWS / 2 - 1;
const BXW: i32 = COLS + 2;
const BYH: i32 = ROWS + 2;

/// A rectangular block. A box that matches what you see beats a circle here:
/// the collision and the glyph are the same shape.
#[derive(Clone, Copy)]
pub struct Solid {
    pub x: f32,
    pub y: f32,
    pub hw: f32,
    pub hh: f32,
}

impl Solid {
    #[inline]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x > self.x - self.hw && x < self.x + self.hw && y > self.y - self.hh && y < self.y + self.hh
    }
}

/// Cheap integer hash to `0..1`. Deterministic, so one seed always builds one
/// arena.
#[inline]
fn hash(i: i32, j: i32, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x2545_F491)
        ^ (j as u32).wrapping_mul(0x9E37_79B9)
        ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}

/// A cell is solid when a fine and a coarse sample agree. Mixing octaves is
/// what turns scattered confetti into blobs with lanes running between them.
#[inline]
fn block_at(i: i32, j: i32, salt: u32, fill: f32) -> bool {
    if i.abs() <= PLAZA && j.abs() <= PLAZA {
        return false;
    }
    let fine = hash(i, j, salt);
    let coarse = hash(i.div_euclid(2), j.div_euclid(2), salt ^ 0x51ED_2701);
    fine * 0.55 + coarse * 0.45 > fill
}

/// Every block that can move the run, indexed by lattice cell.
pub struct SolidGrid {
    pub solids: Vec<Solid>,
    buckets: Vec<Vec<u16>>,
}

impl SolidGrid {
    pub fn new(seed: u32) -> Self {
        let n = (COLS * ROWS) as usize;
        let mut fill = FILL;
        let mut occ = vec![false; n];
        for _ in 0..14 {
            for (k, o) in occ.iter_mut().enumerate() {
                *o = block_at(idx_i(k), idx_j(k), seed, fill);
            }
            let free = occ.iter().filter(|b| !**b).count();
            if free > 0 && reach(&occ) as f32 / free as f32 >= OPEN_RATIO {
                break;
            }
            fill += 0.04;
        }
        // Pocket anything the flood could not get to. The arena is one space.
        seal(&mut occ);

        let mut solids = Vec::with_capacity(n / 3);
        for (k, o) in occ.iter().enumerate() {
            if *o {
                solids.push(Solid {
                    x: idx_i(k) as f32 * CELL,
                    y: idx_j(k) as f32 * CELL,
                    hw: HALF,
                    hh: HALF,
                });
            }
        }

        let mut buckets = vec![Vec::new(); (BXW * BYH) as usize];
        for (n, s) in solids.iter().enumerate() {
            let bi = (s.x / CELL).floor() as i32 - BX0;
            let bj = (s.y / CELL).floor() as i32 - BY0;
            buckets[(clamp_i(bj, BYH) * BXW + clamp_i(bi, BXW)) as usize].push(n as u16);
        }

        Self { solids, buckets }
    }

    /// Buckets a query circle could touch.
    ///
    /// The bounds are grown by the block's half extent as well as the radius.
    /// A block is centred in its bucket but is not smaller than it, so an
    /// unexpanded range misses the block a body is standing inside — which is
    /// exactly the case that has to resolve, since that is a body buried in a
    /// pillar.
    #[inline]
    fn span(&self, x: f32, y: f32, r: f32) -> (i32, i32, i32, i32) {
        let (ex, ey) = (r + HALF, r + HALF);
        let i0 = clamp_i(((x - ex) / CELL).floor() as i32 - BX0, BXW);
        let i1 = clamp_i(((x + ex) / CELL).floor() as i32 - BX0, BXW);
        let j0 = clamp_i(((y - ey) / CELL).floor() as i32 - BY0, BYH);
        let j1 = clamp_i(((y + ey) / CELL).floor() as i32 - BY0, BYH);
        (i0, i1, j0, j1)
    }

    /// True when a circle of radius `r` overlaps anything at all.
    pub fn hit(&self, x: f32, y: f32, r: f32) -> bool {
        let (i0, i1, j0, j1) = self.span(x, y, r);
        let r2 = r * r;
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &n in &self.buckets[(j * BXW + i) as usize] {
                    let s = &self.solids[n as usize];
                    let cx = x.clamp(s.x - s.hw, s.x + s.hw);
                    let cy = y.clamp(s.y - s.hh, s.y + s.hh);
                    let (dx, dy) = (x - cx, y - cy);
                    if dx * dx + dy * dy < r2 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// True when the point itself is buried. Spawning uses this; nothing moves
    /// the point, so a zero radius test is the honest one.
    pub fn inside(&self, x: f32, y: f32) -> bool {
        let (i0, i1, j0, j1) = self.span(x, y, 0.0);
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &n in &self.buckets[(j * BXW + i) as usize] {
                    if self.solids[n as usize].contains(x, y) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Slides a circle clear of anything it overlaps and returns the surface
    /// normal of the deepest contact, or `None` if it was already clear.
    ///
    /// Callers use the normal for two different things: enemies blend their
    /// heading toward the wall's tangent so the swarm flows around a block
    /// rather than piling into it, and MOVA's dash loses its speed.
    pub fn push_out(&self, x: &mut f32, y: &mut f32, r: f32) -> Option<(f32, f32)> {
        let mut deepest: Option<(f32, f32)> = None;
        let mut worst = 0.0f32;
        // Corners can hold a circle against two faces at once; a couple of
        // passes settle that instead of leaving it jittering in the seam.
        for _ in 0..3 {
            let mut moved = false;
            let (i0, i1, j0, j1) = self.span(*x, *y, r);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    for &n in &self.buckets[(j * BXW + i) as usize] {
                        let s = &self.solids[n as usize];
                        let cx = x.clamp(s.x - s.hw, s.x + s.hw);
                        let cy = y.clamp(s.y - s.hh, s.y + s.hh);
                        let (dx, dy) = (*x - cx, *y - cy);
                        let d2 = dx * dx + dy * dy;
                        let (nx, ny, pen) = if d2 > 1e-6 {
                            let d = d2.sqrt();
                            if d >= r {
                                continue;
                            }
                            (dx / d, dy / d, r - d)
                        } else {
                            // Centre is inside the block: leave by the nearest
                            // face, so a body never tunnels out the far side.
                            let (px, py) = (s.x + s.hw - *x, s.y + s.hh - *y);
                            let (nx_, ny_) = (*x - (s.x - s.hw), *y - (s.y - s.hh));
                            if px <= nx_ && px <= py && px <= ny_ {
                                (1.0, 0.0, px + r)
                            } else if nx_ <= py && nx_ <= ny_ {
                                (-1.0, 0.0, nx_ + r)
                            } else if py <= ny_ {
                                (0.0, 1.0, py + r)
                            } else {
                                (0.0, -1.0, ny_ + r)
                            }
                        };
                        if pen > worst {
                            worst = pen;
                            deepest = Some((nx, ny));
                        }
                        *x += nx * pen;
                        *y += ny * pen;
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        deepest
    }
}

#[inline]
fn idx_i(k: usize) -> i32 {
    k as i32 % COLS - COLS / 2
}

#[inline]
fn idx_j(k: usize) -> i32 {
    k as i32 / COLS - ROWS / 2
}

#[inline]
fn clamp_i(v: i32, n: i32) -> i32 {
    v.clamp(0, n - 1)
}

/// Lattice cell `(i, j)` to a flat index. The offsets are what map world
/// coordinates back to the grid, so they must be added, not subtracted.
#[inline]
fn cell_index(i: i32, j: i32) -> usize {
    ((j + ROWS / 2) * COLS + (i + COLS / 2)) as usize
}

/// Floods the floor outward from the plaza, eight-connected.
fn reach(occ: &[bool]) -> usize {
    let start = cell_index(0, 0);
    let mut seen = vec![false; occ.len()];
    if occ[start] {
        return 0;
    }
    let mut stack = vec![start];
    seen[start] = true;
    let mut count = 1;
    while let Some(k) = stack.pop() {
        let (i, j) = (idx_i(k), idx_j(k));
        for dj in -1..=1i32 {
            for di in -1..=1i32 {
                let (ni, nj) = (i + di, j + dj);
                if ni < -COLS / 2 || nj < -ROWS / 2 || ni > COLS / 2 || nj > ROWS / 2 {
                    continue;
                }
                let m = cell_index(ni, nj);
                if seen[m] || occ[m] {
                    continue;
                }
                seen[m] = true;
                count += 1;
                stack.push(m);
            }
        }
    }
    count
}

/// Fills in every patch of floor the flood could not reach. Cheap, and it
/// guarantees the arena is a single connected space with no hidden pockets.
fn seal(occ: &mut [bool]) {
    let start = cell_index(0, 0);
    if occ[start] {
        return;
    }
    let mut seen = vec![false; occ.len()];
    let mut stack = vec![start];
    seen[start] = true;
    while let Some(k) = stack.pop() {
        let (i, j) = (idx_i(k), idx_j(k));
        for dj in -1..=1i32 {
            for di in -1..=1i32 {
                let (ni, nj) = (i + di, j + dj);
                if ni < -COLS / 2 || nj < -ROWS / 2 || ni > COLS / 2 || nj > ROWS / 2 {
                    continue;
                }
                let m = cell_index(ni, nj);
                if seen[m] || occ[m] {
                    continue;
                }
                seen[m] = true;
                stack.push(m);
            }
        }
    }
    for (k, o) in occ.iter_mut().enumerate() {
        if !*o && !seen[k] {
            *o = true;
        }
    }
}
