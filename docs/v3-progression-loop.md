# The v3 progression loop

How a run becomes a permanent change, and what the player does with it.

## The shape of the loop

```
                 +-----------------------+
                 |                       |
   death/run end  |   score -> payout     |  cores
   wave clear    |   -> save.cores      |--------> [ SHOP ]
                 |                       |             |
                 +-----------------------+             |
                                                          v
                                             drive upgrade override
                                             power unlock bit
                                                          |
                                                          v
                                                    next run
```

The loop is deliberately the same shape as a run itself: you move, you die or you
clear, something is paid out, and the next iteration starts from a slightly
different state. Meta-progression here is not a second game bolted on; it is the
same run economy with the consequences made permanent.

## The payout

A finished run pays out cores through `meta::run_payout(score, wave, clean)`:

```
payout = score / 25 + (clean ? 25 : 0) + wave * 10
```

- **`score / 25`** is the base economy. Cores are the shop's currency, and the
  score a player actually cares about — kills at speed, combos, near misses —
  translates one-to-one into spendable progress. Small divisions keep early
  runs meaningful without making late-game scores absurd.
- **`+25` for a clean run** is the performance bonus: no hits landed. It rewards
  the full-speed play the game is built around, and it is small enough that a
  risky line can never be bought back through spending alone.
- **`+ wave * 10`** is the floor. A run that ended on wave one still pays, so a
  new player who dies in the opening rush gets a first taste of the shop rather
  than a hard zero.

The sum is added with saturating arithmetic, so a very long late-game score
cannot overflow the wallet.

## The shop

Six items, flat cost, the same for every player — the shop is a vending machine,
not a personal economy.

| Item | Cost | Effect |
| --- | --- | --- |
| Overcharge Cell | 120 | The drive's burn tier lasts 3s longer. |
| Surge Cell | 100 | Surges hold 4s longer. |
| Momentum Core | 200 | Top speed +9%. |
| Dash Coil | 90 | Cooldown -20%, burst +10%. |
| Chain Cable | 150 | Unlocks the Chain Lightning rack slot. |
| Magnet Coil | 140 | Unlocks the Magnet hold rack slot. |

The catalog lives in `src/meta.rs` as constants (`ITEM_*`, `cost`, `item_name`,
`item_desc`, `ITEM_COUNT`). Adding the next tier of offers is new constants, not
new code paths.

## What persists

The save file carries two new fields alongside the map memory:

- `cores: u32` — spendable currency.
- `unlocks: u32` — one bit per shop item.

The bitmask is deliberately shared. Bit `N` is `ITEM_<N>`, and owning bit `N`
means both "this is a power unlock" (CHAIN, MAGNET) and "this is a drive upgrade"
(Overcharge Cell, Surge Cell, Momentum Core, Dash Coil) — the same bit serves
both readings, so the save never needs a second flag. Buying is checked against
ownership and the price; once set, the bit is never cleared, so an item is
effectively one-permanently-owned.

Drive upgrades reach the player through `Player::from_save`, which turns the
save's unlock bits into permanent field overrides (`drive_overcharge`,
`drive_surge`, `drive_max`, `drive_dash_cd`, `drive_dash_speed`). A player who
bought none reads exactly like the shipped base tuning; the overrides are `f32`
amounts applied at build time, which is what makes a "permanent drive upgrade"
just a number the shop picked.

## The loop in play

1. A run ends (death or wave clear) and `Game::end_run` pays out into the save.
2. From the menu or the game-over screen, `U` opens the shop (`Phase::Shop`).
3. The player buys with `1`-`6` (or the number keys) or backs out with `U`.
4. `Game::start` rebuilds the player and the power rack from the save, so bought
   things apply to the next run and only the next run.
5. A new map generates, the run begins, and the loop repeats.

The shop phase owns no simulation state; it is a renderer + input concern.
Simulation stays paused, the wave timers do not drain, and nothing in the world
moves while it is open.
