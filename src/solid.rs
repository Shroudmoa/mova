//! Fixed furniture.
//!
//! The arena is a torus. There is no edge to run out of: every lattice cell
//! tiles the world, so a wall leaving the right of the screen is waiting on the
//! left. That is what takes the cage out of the map — there is nowhere to be
//! cornered against, only somewhere to come back around to.
//!
//! Generated from a seed, and given a fresh one every run, so each map is a
//! new place to learn. The seed is the only input: the same word always builds
//! the same arena, which is what lets a test pin a layout down and lets a
//! layout worth complaining about be replayed.
//!
//! Shapes are stamped, not sampled. Sampling a lattice from noise gives
//! confetti that happens to clump — a lot of small cubes and nothing worth
//! naming. v3 stamps walls, rooms and pillar fields onto an empty torus and
//! then erodes them, so what comes out is a place with corridors and arenas
//! rather than a texture.

/// Centre-to-centre spacing of the block lattice, in world units.
pub const CELL: f32 = 12.0;
/// Lattice counts. Both odd, so the plaza sits exactly on a cell and the torus
/// seams fall between cells rather than through them.
pub const COLS: i32 = 33;
pub const ROWS: i32 = 21;
/// How far the world repeats. Anything that wraps has to wrap by one of these.
pub const PERIOD_X: f32 = COLS as f32 * CELL;
pub const PERIOD_Y: f32 = ROWS as f32 * CELL;

/// Half-extent of a lone pillar. Two neighbours leave a five-unit lane between
/// them, which is a lane MOVA can build speed down and a dash can cross.
const POST_HALF: f32 = 3.4;
/// Half-extent of a stamped wall. Wider than half a cell on purpose: at this
/// size consecutive cells overlap in all eight directions, so a run of them
/// fuses into one continuous surface instead of a dotted line. Diagonals fuse
/// too, which is what lets a wall run at forty-five degrees and still read as a
/// wall rather than as a staircase.
const WALL_HALF: f32 = 7.6;
/// MOVA's starting plaza is never built on.
/// Cells of clear ground in every direction from the spawn.
///
/// Not merely "where MOVA appears". This is the other half of the promise: that
/// he opens onto ground he can see and run out of on *any* heading, without
/// turning first. A three-cell square cannot say that, which is the whole reason
/// this exists — anything off to one side of the box was never inside it.
pub const SIGHT: i32 = 3;

/// Ceiling on push-out iterations. Only reached when a body starts buried
/// several blocks deep, which only a test does; nothing in play gets that far.
const MAX_PASSES: usize = 24;

/// How far past touching a resolve leaves a body. See the use in `resolve`.
const SKIN: f32 = 0.02;

/// Occupancy codes. Also the render's idea of what a block looks like: a post is
/// a thing with space around it, a wall is a surface that joins up.
pub const EMPTY: u8 = 0;
pub const POST: u8 = 1;
pub const WALL: u8 = 2;

/// A rectangular block. A box that matches what you see beats a circle here:
/// the collision and the glyph are the same shape.
#[derive(Clone, Copy)]
pub struct Solid {
    pub x: f32,
    pub y: f32,
    pub hw: f32,
    pub hh: f32,
    /// `WALL` or `POST`. Carried rather than inferred from the extents so the
    /// renderer can draw a fused surface without re-deriving what is next door.
    pub kind: u8,
}

impl Solid {
    #[inline]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x > self.x - self.hw && x < self.x + self.hw && y > self.y - self.hh && y < self.y + self.hh
    }

    /// True when this block is one of a fused run, and so has no free face of
    /// its own to catch the light.
    #[inline]
    pub fn wall(&self) -> bool {
        self.kind == WALL
    }
}

// ---- the lattice ----------------------------------------------------------

/// Flat index of lattice cell `(i, j)`, wrapping on both axes. Every lookup in
/// the file goes through this, which is what makes the torus seamless: there is
/// no path that reads a neighbour without going round the seam.
#[inline]
fn cell(i: i32, j: i32) -> usize {
    let i = (i + COLS / 2).rem_euclid(COLS);
    let j = (j + ROWS / 2).rem_euclid(ROWS);
    (j * COLS + i) as usize
}

#[inline]
fn cell_i(k: usize) -> i32 {
    (k as i32 % COLS) - COLS / 2
}

#[inline]
fn cell_j(k: usize) -> i32 {
    (k as i32 / COLS) - ROWS / 2
}

// ---- the seed -------------------------------------------------------------

/// A small xorshift, local to the generator so a map's randomness cannot reach
/// into the run's.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// `0..1`.
    #[inline]
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }

    /// Inclusive on both ends.
    #[inline]
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1) as u32) as i32
    }

    #[inline]
    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

// ---- stamping -------------------------------------------------------------

/// The eight lattice directions, in order, so turning is a step round the ring.
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Folds a point back into the fundamental rectangle of the torus.
///
/// Every query runs this first. The lattice indices already wrap, so this is
/// not about the buckets — it is because the blocks themselves are stored at
/// fixed world positions, and a block at `x = 192` has to answer for a body at
/// `x = 588`. Without it the seam is invisible in the index arithmetic and then
/// turns up as a real hole in the collision surface one period out.
#[inline]
fn fold(x: f32, y: f32) -> (f32, f32) {
    (fold_axis(x, PERIOD_X * 0.5), fold_axis(y, PERIOD_Y * 0.5))
}

#[inline]
fn fold_axis(v: f32, h: f32) -> f32 {
    (v + h).rem_euclid(h * 2.0) - h
}

#[inline]
fn put(occ: &mut [u8], i: i32, j: i32, v: u8) {
    let k = cell(i, j);
    // A post never downgrades a wall. Order of stamping must not decide whether
    // a pillar field eats the end of a wall run.
    if occ[k] == WALL || v == WALL {
        occ[k] = WALL;
    } else if occ[k] == EMPTY {
        occ[k] = v;
    }
}

/// A long wall built out of straight runs joined at the knees.
///
/// This is the map's spine, and its shape is the whole argument for building it
/// out of segments rather than one cell at a time. Turning at every cell — which
/// is what this did first — gives eight directions of freedom per step, and the
/// result curls into a blob: forty cells of scribble, no run longer than three,
/// nothing to read at speed. Here each segment is four to eight cells long and
/// the join turns by at most forty-five degrees, and always the same way round.
/// A spine that curves is a landmark you can see coming and plan a line at; a
/// spine that doubles back on itself is just noise at a larger scale.
fn spine(occ: &mut [u8], rng: &mut Rng, i: i32, j: i32, len: i32) {
    // One handedness for the whole chain. Choosing per joint lets a long run
    // unwind itself, which is the scribble again in slower motion.
    let hand = if rng.chance(0.5) { 1 } else { -1 };
    let mut d = rng.range(0, 7);
    let (mut i, mut j) = (i, j);
    let mut left = len;
    while left > 0 {
        let run = left.min(rng.range(4, 8));
        for _ in 0..run {
            put(occ, i, j, WALL);
            let (di, dj) = DIRS[d as usize];
            i += di;
            j += dj;
        }
        left -= run;
        if left > 0 {
            // Zero or one step round the ring. Never more, never reversed.
            d = (d + hand * rng.range(0, 1)).rem_euclid(8);
        }
    }
}

/// A single straight run of wall, attached to nothing.
///
/// The best ground in the game at dash speed, and it is worth its own feature
/// because nothing else here is shaped like it. A ring is a room you go round;
/// a bar is a wall you run *along*, which is a different problem: you are
/// reading the lane beside it at two hundred units a second and correcting into
/// it late. Stamped straight rather than snaked on purpose — a bar you can see
/// the far end of is a bar you can aim at.
fn bar(occ: &mut [u8], rng: &mut Rng, i: i32, j: i32, len: i32) {
    let (di, dj) = DIRS[rng.range(0, 7) as usize];
    for n in 0..len {
        put(occ, i + di * n, j + dj * n, WALL);
    }
}

/// A chunk of building with two lanes cut clean through it.
///
/// v3 had this as a plain filled disc of up to two and a half cells. Sixty units
/// of unbroken wall, and on screen it is a slab with nothing to read: no
/// silhouette to aim at, no gap to judge, nothing to go round but the whole
/// thing. Worse next to the spawn, where it can fill a third of the view and turn
/// the first three seconds of a run into a decision about one shape.
///
/// The cross is what makes it architecture. One cell wide, running the full width
/// of the block so both lanes come out into open ground and it is never a dead
/// end — two lines through a chunk, which is the whole idea of this game: pick a
/// lane, hold it, and see what is at the end of it.
fn room(occ: &mut [u8], i: i32, j: i32, r: f32) {
    let n = r.ceil() as i32;
    for dj in -n..=n {
        for di in -n..=n {
            if (di * di + dj * dj) as f32 > r * r + 0.25 {
                continue;
            }
            put(
                occ,
                i + di,
                j + dj,
                if di == 0 || dj == 0 { EMPTY } else { WALL },
            );
        }
    }
}

/// A hollow room: walls with a doorway-sized gap already knocked out of one
/// side. These are the best ground in the game — something to run round, with
/// sightlines into it — so the generator lays down a few deliberately.
fn arena_ring(occ: &mut [u8], rng: &mut Rng, i: i32, j: i32, r: f32) {
    let n = r.ceil() as i32;
    let door = rng.range(0, 7);
    for dj in -n..=n {
        for di in -n..=n {
            let d2 = (di * di + dj * dj) as f32;
            if d2 > (r + 0.5) * (r + 0.5) || d2 < (r - 1.4) * (r - 1.4) {
                continue;
            }
            // Exactly one gap, on one side. A ring with no way in is a lid.
            if r >= 2.0 && (di, dj) == DIRS[door as usize] && di.abs() + dj.abs() == 1 {
                continue;
            }
            put(occ, i + di, j + dj, WALL);
        }
    }
}

/// A rectangular block of lone pillars on the full lattice — a colonnade.
///
/// Every pillar stands on the twelve-unit grid, so the gaps between them are a
/// fixed five and a bit units: a lane MOVA can build speed down and a dash can
/// cross. `erode` then takes a few of them out, so the field is a grid with
/// gaps in it rather than a checkerboard.
///
/// This replaced a scatter of pillars dropped at random offsets, which is the
/// one shape that cannot be read at all: the eye gets no rows and no columns, so
/// at speed it is only ever "is there something there", and the lane between two
/// pillars happens to be five units or happens to be thirty. A grid is a
/// different proposition. You can count across it and pick a column.
fn colonnade(occ: &mut [u8], i: i32, j: i32, w: i32, h: i32) {
    for dj in 0..h {
        for di in 0..w {
            put(occ, i + di, j + dj, POST);
        }
    }
}

/// A spot for a landmark to sit on.
///
/// Three things are wanted and none of them come from picking a coordinate and
/// stamping on it. The spot has to be spread across the whole torus rather than
/// the middle, or the spawn point sits in the furniture and the edges are bare.
/// It has to be clear of the plaza, or a ring gets stamped across the spawn and
/// comes out with a doorway and a missing wall segment, which reads as a mistake.
/// And it has to be clear of what has already been built, or features land on
/// each other and fuse into one undifferentiated mass — which is what this map
/// looked like before: a hundred and sixty wall cells in a handful of heaps, and
/// a lot of bare floor between them.
///
/// So: draw sixteen candidates, score each on how much of the ground it would
/// cover is already occupied, and take the first that is nearly clear. Falling
/// back to the best of a bad set rather than giving up, because a map with two
/// landmarks sharing a corner is a worse map but not a broken one.
/// `reach` is how far the feature actually extends from the spot it is given,
/// in cells, and it is what keeps the feature off the spawn. It used to be the
/// edge margin instead, which was a square test: it rejected a spot only if
/// *both* coordinates were near the plaza, so a candidate one cell east of the
/// origin sailed through and a room centred there put sixty units of wall twenty
/// units from MOVA's nose — a third of the screen, before he has touched a key.
fn spot(occ: &[u8], rng: &mut Rng, clear: i32, keep: i32, reach: i32) -> (i32, i32) {
    // The spawn's own keep-out, in cells, as a radius rather than a box. `SIGHT`
    // so a landmark cannot even begin where the open ground ends, and `reach` so
    // its near edge lands outside the sightline. `open_sight` would catch the
    // result either way; this is here so the map is not built by laying
    // something down and then cutting a hole in it.
    let keepout = SIGHT + reach + 1;
    let mut fallback = (0, 0, f32::MAX);
    for _ in 0..16 {
        let (i, j) = (
            rng.range(-COLS / 2 + clear, COLS / 2 - clear),
            rng.range(-ROWS / 2 + clear, ROWS / 2 - clear),
        );
        if i * i + j * j <= keepout * keepout {
            continue;
        }
        let (mut busy, mut total) = (0, 0);
        for dj in -keep..=keep {
            for di in -keep..=keep {
                if di * di + dj * dj > keep * keep {
                    continue;
                }
                total += 1;
                if occ[cell(i + di, j + dj)] != EMPTY {
                    busy += 1;
                }
            }
        }
        let share = busy as f32 / total as f32;
        if share < 0.12 {
            return (i, j);
        }
        if share < fallback.2 {
            fallback = (i, j, share);
        }
    }
    (fallback.0, fallback.1)
}

/// Knocks holes in what was stamped. Without this the geometry is too tidy to
/// run through: every corridor is exactly as wide as it was drawn, and nothing
/// can be improvised. Eight percent is enough to put doorways and notches in
/// the walls without turning them back into noise.
fn erode(occ: &mut [u8], rng: &mut Rng) {
    for cell in occ.iter_mut() {
        if *cell != EMPTY && rng.chance(0.08) {
            *cell = EMPTY;
        }
    }
}

/// Clears the spawn plaza. Runs after everything else, so no amount of stamping
/// can wall MOVA in on the first frame.
/// Clears the spawn's sightlines, enforced rather than negotiated.
///
/// Every stamp is placed through `spot`, which keeps its centre off the plaza —
/// but a centre is not a silhouette. A room of radius two stamped five cells out
/// reaches back to three; a bar laid along the seam of a keep-out crosses it
/// while its own spot stays legal. So the last word goes to the sightline: after
/// the final stamp, everything inside it is cleared, whatever asked for it.
///
/// A disc, not the three-cell square the spawn used to be cleared to, because the
/// square is exactly the shape of the bug this replaces.
fn open_sight(occ: &mut [u8]) {
    for j in -SIGHT..=SIGHT {
        for i in -SIGHT..=SIGHT {
            if i * i + j * j <= SIGHT * SIGHT {
                occ[cell(i, j)] = EMPTY;
            }
        }
    }
}

/// Floods the floor outward from the plaza over the torus, eight-connected.
fn reach(occ: &[bool]) -> Vec<bool> {
    let mut seen = vec![false; occ.len()];
    let start = cell(0, 0);
    if occ[start] {
        return seen;
    }
    let mut stack = vec![start];
    seen[start] = true;
    while let Some(k) = stack.pop() {
        let (i, j) = (cell_i(k), cell_j(k));
        for dj in -1..=1i32 {
            for di in -1..=1i32 {
                let m = cell(i + di, j + dj);
                if seen[m] || occ[m] {
                    continue;
                }
                seen[m] = true;
                stack.push(m);
            }
        }
    }
    seen
}

/// Fills in every patch of floor the flood could not reach.
///
/// On a torus this does less than it did in v2, because the arena wraps and the
/// things that used to be sealed pockets are now joined up at the seams. What
/// it still catches is the real case: two parallel wall runs that close off a
/// band. Cheap, and it keeps the guarantee that the floor is one space.
fn seal(occ: &mut [u8]) {
    let blocked: Vec<bool> = occ.iter().map(|v| *v != EMPTY).collect();
    let seen = reach(&blocked);
    for (k, s) in seen.iter().enumerate() {
        if !*s && occ[k] != EMPTY {
            continue;
        }
        if !*s {
            occ[k] = WALL;
        }
    }
}

// ---- the grid -------------------------------------------------------------

/// Every block that can move the run, indexed by lattice cell. The bucket index
/// is the cell index, so a query that walks the lattice wraps for free.
#[derive(Clone)]
pub struct SolidGrid {
    pub solids: Vec<Solid>,
    buckets: Vec<Vec<u16>>,
}

impl SolidGrid {
    pub fn new(seed: u32) -> Self {
        let n = (COLS * ROWS) as usize;
        let mut occ = vec![EMPTY; n];
        let mut rng = Rng::new(seed);

        // Spines first and largest, because they are what the rest is laid out
        // around. Three of them: one would be a fence, two can cross but not
        // reliably, and it is the crossing and the near-miss that make a run
        // read as a place with a shape rather than a field.
        for _ in 0..3 {
            let (i, j) = spot(&occ, &mut rng, 3, 6, 11);
            let len = rng.range(14, 22);
            spine(&mut occ, &mut rng, i, j, len);
        }

        // Rings before rooms, so a room landing on top of one eats it rather
        // than the other way round — a ring with its doorway filled in is just a
        // room, and we have a stamp for those.
        for _ in 0..3 {
            let (i, j) = spot(&occ, &mut rng, 4, 4, 4);
            let r = 2.0 + rng.unit() * 0.9;
            arena_ring(&mut occ, &mut rng, i, j, r);
        }
        for _ in 0..2 {
            let (i, j) = spot(&occ, &mut rng, 3, 3, 3);
            let r = 1.3 + rng.unit() * 0.8;
            room(&mut occ, i, j, r);
        }

        // Free-standing bars, angled so they are rarely parallel to the axes the
        // lanes already run along.
        for _ in 0..3 {
            let (i, j) = spot(&occ, &mut rng, 4, 5, 7);
            let len = rng.range(5, 10);
            bar(&mut occ, &mut rng, i, j, len);
        }

        // Colonnades. Sparse on purpose. A colonnade is a regular grid, and the
        // rhythm is the point: you count across the field and pick a lane. That
        // only works with empty ground between fields, because what you are
        // really doing is choosing a line through the arena and a grid every
        // twelve units turns the choice into noise. The count is what sets how
        // much of the view is floor, and at this speed the view wants to be
        // mostly floor.
        for _ in 0..5 {
            let (i, j) = spot(&occ, &mut rng, 3, 3, 4);
            let (w, h) = (rng.range(3, 5), rng.range(3, 5));
            colonnade(&mut occ, i, j, w, h);
        }

        erode(&mut occ, &mut rng);
        open_sight(&mut occ);
        seal(&mut occ);

        let mut solids = Vec::with_capacity(n / 5);
        let mut buckets = vec![Vec::new(); n];
        for (k, v) in occ.iter().enumerate() {
            if *v == EMPTY {
                continue;
            }
            let half = if *v == WALL { WALL_HALF } else { POST_HALF };
            buckets[k].push(solids.len() as u16);
            solids.push(Solid {
                x: cell_i(k) as f32 * CELL,
                y: cell_j(k) as f32 * CELL,
                hw: half,
                hh: half,
                kind: *v,
            });
        }

        Self { solids, buckets }
    }

    /// What is stamped into lattice cell `(i, j)`. The renderer uses this to
    /// decide whether a block still has a free top edge worth lighting, which
    /// is what stops a fused wall from rendering as a stack of stripes.
    #[inline]
    pub fn cell_kind(&self, i: i32, j: i32) -> u8 {
        self.buckets[cell(i, j)]
            .first()
            .map_or(EMPTY, |n| self.solids[*n as usize].kind)
    }

    /// Lattice cells a query circle could touch. The bounds are grown by the
    /// largest half-extent as well as the radius, because a block is bigger than
    /// the cell it is indexed by.
    #[inline]
    fn span(&self, x: f32, y: f32, r: f32) -> (i32, i32, i32, i32) {
        let (ex, ey) = (r + WALL_HALF, r + WALL_HALF);
        let i0 = ((x - ex) / CELL).floor() as i32;
        let i1 = ((x + ex) / CELL).ceil() as i32;
        let j0 = ((y - ey) / CELL).floor() as i32;
        let j1 = ((y + ey) / CELL).ceil() as i32;
        (i0, i1, j0, j1)
    }

    /// True when a circle of radius `r` overlaps anything at all.
    pub fn hit(&self, x: f32, y: f32, r: f32) -> bool {
        let (x, y) = fold(x, y);
        let (i0, i1, j0, j1) = self.span(x, y, r);
        let r2 = r * r;
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &n in &self.buckets[cell(i, j)] {
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
        let (x, y) = fold(x, y);
        let (i0, i1, j0, j1) = self.span(x, y, 0.0);
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &n in &self.buckets[cell(i, j)] {
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
    /// rather than piling into it, and MOVA bounces off it.
    pub fn push_out(&self, x: &mut f32, y: &mut f32, r: f32) -> Option<(f32, f32)> {
        // Fold first, resolve in the fundamental rectangle, fold back. A body
        // resolved against a block a whole period away has to be pushed toward
        // that block in its own frame or it is pushed the wrong way entirely.
        let (wx, wy) = (*x, *y);
        let (fx, fy) = fold(wx, wy);
        let mut px = fx;
        let mut py = fy;
        let deepest = self.resolve(&mut px, &mut py, r);
        (*x, *y) = fold(px + wx - fx, py + wy - fy);
        deepest
    }

    fn resolve(&self, x: &mut f32, y: &mut f32, r: f32) -> Option<(f32, f32)> {
        let mut deepest: Option<(f32, f32)> = None;
        let mut worst = 0.0f32;
        // Runs until the circle is clear, not for a fixed number of passes.
        //
        // v2's fixed three were enough because blocks were separate cubes with
        // gaps between them. v3's walls fuse, so a body buried at the centre of
        // one can only be stepped out one block at a time — it lands inside the
        // next block and has to be pushed again. A pass count is a guess about
        // how deep the furniture goes, and the map decides that. Exiting on the
        // first clear pass keeps the common case at one or two iterations, since
        // nothing in normal play is ever buried in the first place.
        for _ in 0..MAX_PASSES {
            let mut moved = false;
            let (i0, i1, j0, j1) = self.span(*x, *y, r);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    for &n in &self.buckets[cell(i, j)] {
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
                            // Centre is inside the block, so it has to leave by a face —
                            // nearest first, so a body never tunnels out the far side.
                            //
                            // "Nearest" is not enough on its own, and the reason is worth
                            // writing down because it cost an afternoon. Walls fuse, so a block
                            // can have another block sitting directly behind two opposite
                            // faces: a lane a cell wide with building either side of it, which
                            // is exactly what a room's cross cuts. A body buried in such a block
                            // finds both its nearest faces walled in, steps out into the
                            // neighbour, the neighbour steps it straight back, and the two trade
                            // it between them until the pass limit runs out — leaving MOVA
                            // stuck inside a wall with a normal to bounce off.
                            //
                            // So the four faces are candidates rather than a tie-break. The first
                            // one whose exit is clear of everything wins, in a fixed order, so the
                            // same body always leaves the same way; the nearest face is only the
                            // fallback for a block with every exit blocked, which is the one case
                            // where stepping out at all is the right move.
                            let mut nearest: Option<(f32, f32, f32)> = None;
                            let mut escape: Option<(f32, f32, f32)> = None;
                            for (nx, ny) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                            {
                                let pen = if nx != 0.0 {
                                    s.x + nx * s.hw - *x
                                } else {
                                    s.y + ny * s.hh - *y
                                };
                                if pen < 0.0 {
                                    continue;
                                }
                                let travel = pen + r;
                                if nearest.is_none_or(|(_, _, best)| travel < best) {
                                    nearest = Some((nx, ny, travel));
                                }
                                if escape.is_none() {
                                    let (ex, ey) =
                                        (*x + nx * (travel + SKIN), *y + ny * (travel + SKIN));
                                    if !self.hit(ex, ey, r) {
                                        escape = Some((nx, ny, travel));
                                    }
                                }
                            }
                            // Every face is behind another block, so the block is a pocket rather
                            // than a wall and the body has to climb out of the pocket. Nearest
                            // face still makes progress; the next pass does the rest.
                            escape.or(nearest).unwrap_or((0.0, 1.0, r))
                        };
                        if pen > worst {
                            worst = pen;
                            deepest = Some((nx, ny));
                        }
                        // Overshoot the contact distance by a skin.
                        //
                        // Pushing out to exactly `r` leaves the body flush with
                        // the surface, which puts it one rounding step away from
                        // the strict overlap test in `hit`. In f32 that is not a
                        // corner case: it is what `push_out` produced every time,
                        // so a body resting against a wall reported a hit
                        // against it. The skin clears it, and it is far too
                        // small to see.
                        let push = pen + SKIN;
                        *x += nx * push;
                        *y += ny * push;
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
