# v3 new-system designs

Designs for the systems added in the first v3 vertical slice. Each section is
the contract the implementation kept, and each names the constants that carry the
behavior, so a later tier of content can arrive without new mechanics.

## Meta layer (`src/meta.rs`)

The permanent layer. Owns the catalog, the economy, and the accessors the rest
of the game reads through.

- `ITEM_COUNT` — the catalog size. Fixed at six for the first slice; later tiers
  add constants, not code paths.
- `ITEM_*` — the item indices. Drive upgrades double as their unlock bit; power
  unlocks occupy their own bits.
- `cost`, `item_name`, `item_desc` — the shop's face. Display strings live here
  so the renderer never formats catalog data itself.
- `owns` / `buy` — the save-level transaction. Rejects already-owned items and
  unaffordable ones without touching the save.
- `power_unlocked` — maps a rack slot to the bit that must be set. The base two
  (`VIRUS`, `OVERCHARGE`) are unconditionally on; the new slots are gated.
- Drive accessors — `overcharge_secs`, `surge_secs`, `max_speed`, `dash_cd`,
  `dash_speed` — resolve a bought value over a base value, for the player build.

Design note: "bought" and "applied" stay two honest steps. The save stores what
was bought; the accessors translate into live numbers when a player is built.
That separation is what lets a later tier of meta-spend mean something else
without touching the game rules.

## Power rack (`src/power.rs`)

The rack went from two slots to four.

- `COUNT = 4`, `CHARGE`, `NAMES`, `KEYS` arrays widened in the same move; adding
  the next power is one line in each.
- `Power::new(unlocks: u32)` takes the save's bitmask. The default rack for a
  fresh player contains the two base slots.
- `ready(slot)` requires both the unlock bit and a full charge. A locked slot's
  key is simply not the player's, which is the cheapest possible lock UI.
- `fire(slot)` spends charge and returns whether it happened.

## Chain lightning

One press, a line instead of a punctuation mark.

- `CHAIN_REACH = 18.0` — how far a bolt arcs before stopping. Enough to skip a
  lane, short enough that a crowd answers to one press.
- `CHAIN_HOPS = 3` — how many times a single bolt keeps jumping after landing.
  A kill at full speed earns a chain; a kill in open ground should not become a
  nuke.
- `CHAIN_DMG = 2.0` — what one hop costs before the enemy's own multiplier. A hop
  is a bullet pair, so it finishes the grunts the gun needed two bullets for.

The bolt starts at the victim's position — where the player already is — and
spreads from there, so it skips the lane the victim crossed rather than spraying
backwards along the player's wake.

`Game::chain(x, y, hops)` walks the nearest living enemy within reach, applies
the hop damage, and recurses with `hops - 1` on a kill. A hop that lands but
doesn't finish is the honest end of the chain.

Chain is wired into every kill path that should start one: gun kills, wake kills,
and ram kills — a ram is the fastest possible kill, which is exactly when the
chain should earn its keep.

## Magnet hold

The player is held, not the world. Everything within reach slides home, which is
the difference between hoarding a pickup and having a reason to roam.

- `Player::magnet()` sets `magnet_t = MAGNET_TIME` (6s) for one charge.
- While `magneting()`, `Game::pickups_tick` pulls any pickup inside
  `pickup::MAGNET` (7.0) toward the player at `pickup::MAGNET_PULL` (30.0) per
  second, refused if the pull would drag the pickup into a pillar.
- Pulled pickups ease back to their spawn position after the hold ends, so a
  nudged pickup does not get dragged around by the player forever.

## The Harrier

The wave's first mind of scheme. It does not weave and it does not orbit: it
closes, and once it closes it commits.

- Stats: hp 2, radius 0.9, speed multiplier 1.45, score multiplier 1.7, glyph `!`.
- Burst timing: a window of `0.8 + wob * 0.3`, recharging after each burst.
- Commitment: proximity, not a clock. Closing within `9.0` units arms the burst
  and pins the charge at speed; while the burst holds, the harrier holds its
  heading.

The distinction from the Runner is the design point: the Runner fires bursts off
a free-running timer the player learns to read; the Harrier arms off the geometry
of the approach, which is a different gamble and one the player reads off the
direction of travel rather than a countdown.

## The Harbinger (mid-run boss)

The boss wears the game's existing vocabulary rather than a new rulebook: it is
the harrier's commitment at siege scale.

- Stats: hp 30, radius 3.0, speed multiplier 0.75, score multiplier 6.0, glyph
  `H`, ram-proof.
- Behavior: walks at the player slowly, then arms a charge by proximity within
  `14.0` units. The fuse is `1.2 + wob * 0.3`, the committed rush `3.5`, and it
  enrages below half health like the warden.
- Boss waves: `wave >= 7 && (wave - 7) % 5 == 0`. A boss wave opens with a single
  harbinger instead of the usual burst; regular spawning continues around it, so
  the boss is a pressure peak rather than a separate arena.

The boss answers a question the roster already asked: what happens when the
committed charge has a hull you cannot ram? The answer is the same as everything
else in MOVA — you shoot it, and the chain makes the crowd around it answer too.

## Drive overrides

Permanent meta-bought tuning, stored as `f32` overrides on the player and read
through accessors (`surge_time`, `overcharge_time`, `max_speed`, `dash_cd`,
`dash_speed`). Zero means base tuning when the player was built fresh; the shop
writes through the save instead. Every override funnels through the same
accessor, so a bought value and a base value are handled identically at the
point of use — only the number differs.
