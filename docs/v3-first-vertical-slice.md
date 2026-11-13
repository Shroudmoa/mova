# v3 — first vertical slice: what shipped and how it was built

The first slice of the planned v3 "Next Gen" upgrade. Scope chosen at the start
of the slice: **meta-progression and unlocks, two new powers, one new enemy.**
Everything else in the goal list is deliberately later.

## The slice's job

Prove, in one vertical chunk, that the v3 spine works end to end:

1. a run can pay out meta-currency,
2. the player can spend it permanently,
3. the new powers actually fire, and
4. the new enemy actually behaves differently.

If all four hold, the rest of the plan — more enemies, map events, juice, systems —
is content layered on top of a spine that is already honest.

## What was delivered

| Piece | Where | Notes |
| --- | --- | --- |
| Meta layer | `src/meta.rs` (new) | Catalog, costs, unlocks, drive accessors, payout. |
| Save fields | `src/save.rs` | `cores`, `unlocks` persisted as `cores=` / `unlocks=`. |
| Power rack | `src/power.rs` | Four slots; base two unlocked, new ones via shop bits. |
| Drive overrides | `src/player.rs` | Five permanent override fields + accessors; `from_save`. |
| New powers | `src/power.rs`, `src/game.rs` | Chain lightning + magnet hold. |
| New enemy | `src/enemy.rs` | Harrier: proximity-committed burst. |
| Game wiring | `src/game.rs` | `Phase::Shop`, `Game::buy`, `end_run` payout, chain, magnet pull. |
| Input | `src/input.rs` | `Shop`, `Buy(usize)`, chain/magnet edges, repeat debounce. |
| Renderer | `src/renderer.rs` | Shop screen, phase arm, locked power pips, power-bar placement. |
| Tests | `tests/v3.rs` (new) | Eleven green tests. |

## The tests

`tests/v3.rs`:

1. `a_run_pays_out_cores_and_the_shop_sells_permanently` — payout formula; buy
   only when affordable and not owned; ownership is one-way.
2. `the_rack_reads_its_unlocks` — base two on from birth; new slots via bits.
3. `drive_upgrades_are_permanent_overrides` — bought values reach the player
   through the accessors; an unbought player reads base tuning.
4. `a_kill_chains_to_the_next_enemy_in_range` — the bolt jumps to the next
   living thing in reach and takes it down.
5. `a_magnet_hold_pulls_a_pickup_home` — a held pickup inside pull radius lands
   in grab range and scores.
6. `the_harrier_commits_when_it_closes_in` — the charge arms on proximity, not
   on a timer.
7. `the_shop_renders_the_catalog` — the shop frame carries the title, an item
   name, and the controls.
8. `the_warden_is_a_ram_proof_elite` — the elite reads as an elite.
9. `the_warden_enrages_when_damaged` — the enrage threshold, derived not stored.
10. `the_harbinger_commits_when_it_closes_in` — the boss arms its charge on
    proximity, at siege scale.
11. `a_boss_wave_opens_with_a_harbinger` — wave seven is the first boss wave.

## Verification

- `cargo test` — 54 passing: map 9, play 22, render 12, v3 11. (Baseline before the
  slices was 43; the eleven new are in `tests/v3.rs`.)
- `cargo build --release` — clean.

## Slice 2 — the warden (delivered on top of slice 1)

The first elite from the enemy-depth line, built directly on slice 1's registry
style so later archetypes arrive the same way.

- `Kind::Warden`: hp 14, radius 2.2, speed multiplier 0.5, score multiplier 4.5,
  glyph `W`, ram-proof.
- Spawns from wave five, rare within the roll.
- Enrages when its hull drops below half health: the speed multiplier steps up,
  applied at movement time rather than stored, so the behaviour stays derived.

Tests: `the_warden_is_a_ram_proof_elite`, `the_warden_enrages_when_damaged`.

## Slice 3 — the harbinger (delivered on top of slice 2)

The mid-run boss, and the end of the enemy-depth line's first pass.

- `Kind::Harbinger`: hp 30, radius 3.0, speed multiplier 0.75, score multiplier
  6.0, glyph `H`, ram-proof.
- Boss waves: wave seven, then every fifth wave after that. A boss wave opens
  with one harbinger instead of the usual burst; regular spawning continues
  around it.
- Behavior: proximity-committed charge (commit range 14, fuse 1.2s, rush 3.5),
  with the warden's half-health enrage applied on top.

Tests: `the_harbinger_commits_when_it_closes_in`,
`a_boss_wave_opens_with_a_harbinger`.

## The plan, as executed

- **Meta-progression.** Catalog constants, costs, names, descs; bitmask save
  fields; `Game::end_run` payout; `Phase::Shop` + renderer + input + `Game::buy`.
- **Powers.** Roster widened; `ready` gained the unlock gate; chain implemented
  as recursive hop-from-kill; magnet implemented as hold-with-pull.
- **Enemy.** Harrier archetype with proximity-armed bursts.
- **Wiring.** Phase flow (menu / playing / over / shop), save round-trip of the
  new fields, HUD readouts switched to the bought-aware accessors.

## Deliberately out of the slices

- Boss escalation beyond the half-health enrage: arena-wide tells, a unique
  reward, and an encore rather than a bigger enemy.
- Map events and exploration rewards beyond the existing sector bonuses.
- Gun variants and passive slots.
- Juice beyond what the slices needed: particles/audio/death screen/daily seed.
- Technical follow-ups: headless replay, balance data file, performance pass.

Those are the remaining items from the v3 goal list, in priority order:

1. Enemy / wave depth — boss escalation and a boss reward loop.
2. Map / exploration — events, milestones, map influence on loot.
3. Build / loadout expression — passive slots, deeper meta build.
4. Juice / feedback — audio, death screen, daily seed, leaderboard.
5. Technical quality — headless mode, balance data, tests for the new systems.

The design vocabulary this slice built (per-kind stat tables, proximity-committed
behaviors, derived rather than stored state, registry-style constants) is what the
later content should be written against.
