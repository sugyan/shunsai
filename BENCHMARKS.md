# Benchmarks

How `shunsai` is measured, what is frozen about the measurement, and every recorded
figure. Why a result was adopted or rejected is in [FAQ.md](./FAQ.md).

The comparison *targets* are pinned submodules in a **local-only, unpublished** sibling
repository — the harness lives there too, because it cannot run without them. This file
keeps the result.

## Method

- **Metrics**: perft (nodes/sec, and a correctness check), movegen alone (ns per
  position), do/undo throughput.
- **Conditions**: `--release`, `lto = "fat"`, `codegen-units = 1` (already configured);
  warm-up; a fixed position set; the same machine; CPU architecture noted. Every history
  entry records CPU, OS, rustc and criterion versions.
- **Position set (fixed SFEN)**: the initial position; the **matsuri** midgame position
  (指し手生成祭り, the standard movegen-benchmark position in the Japanese shogi-dev
  community, used by YaneuraOu's `bench`); the **maximum-legal-move** position; and
  check-adjacent positions, realized as the in-check subset of the sampled real-game
  fixture. The SFEN strings are in `benches/suite/common.rs` and `tests/perft.rs`.

### Fairness — normalize every library to the same work

Account for pseudo-legal-plus-validation versus fully-legal generation, callback versus
`Vec` APIs, and Python-binding boundary costs. **Pawn-drop-mate exclusion**: legal movegen
must not generate pawn drops that give immediate checkmate; engines differ here and it is
a known cause of perft mismatches, so a comparison must confirm each library does the
same.

⚠️ **"Leaf bulk counting" is two conventions, not one, and the difference decides the
comparison.** Every engine in the harness bulk-counts at leaf parents, yet they do not do
the same work to get the number:

| convention | how the count is obtained | engines |
|---|---|---|
| **count-only** | straight off the destination bitboards; **no `Move` is ever constructed** | shunsai (`MoveSet::len()`, two popcounts), haitaka (`PieceMoves::into_iter().len()`) |
| **materialize** | build every legal move, take the list's length | YaneuraOu, apery, apery_rust, yasai, rshogi, cshogi, Fairy-Stockfish |

Count-only is a genuine advantage of the callback API — it is what `MoveSet::len()` is
*for* — but it is one **perft collects and a search cannot**, since a search must have the
moves. So the two are not comparable, and the gap is not small: read the ratio off the
current standing below rather than quoting one from here, because it moves every time the
expansion path is optimized. The harness records `leaf` per row and carries `shunsai-mat`
and `haitaka-mat` columns (the same binaries with `--materialize`), so each pair isolates
exactly this difference.

**Never compare numbers produced under different conventions.** Mixing them is what made
the standing unreadable for a week.

Two engine-specific traps, both about reading a number rather than producing one:

- **YaneuraOu's self-reported `Elapsed Time` is an integer millisecond count plus a
  deliberate `+1`** (ゼロ割防止). The harness subtracts the 1 ms and falls back to wall
  time below a millisecond; charging the raw figure had been costing YaneuraOu up to +10 %
  on the matsuri cell.
- ⚠️ **Do not measure YaneuraOu with `test genmoves`.** It looks like the movegen-level
  command and is not convention-compatible: it loops `MoveList<EVASIONS>` /
  `MoveList<NON_EVASIONS>` rather than `LEGAL_ALL`, so its numbers cannot be compared with
  anything else in the table.

## Running

```bash
cargo bench --features _bench-internals
```

The whole suite is one bench target (`benches/suite/`); without the feature the
`internals` group is compiled out and the rest runs. Selecting a backend for the `perft/*`
and `movegen/*` ids needs a rebuild:

```bash
cargo bench --features _bench-internals,slider-qugiy
```

> ⚠️ **Name the backend a run wants.** `--all-features` is not the shortcut it looks like:
> the two backend flags select the same thing, so enabling both is a `compile_error!` and
> the crate does not build that way at all.

criterion baselines (`--save-baseline <name>` / `-- --baseline <name>`) live under
`target/criterion/` and are throwaway, for local A/B while developing. The durable record
is the [history](#history) below. The full suite takes roughly 3–5 minutes.

## Suite

| id | measures | throughput |
|---|---|---|
| `perft/startpos/4` | perft(4) from the initial position, leaf bulk counting | Elements = 719,731 nodes |
| `perft/matsuri/3` | perft(3) from the matsuri midgame position | Elements = 4,809,015 nodes |
| `perft/maxmoves/2` | perft(2) from the max-legal-moves position | Elements = 105,677 nodes |
| `perft/{startpos,matsuri,maxmoves}-cb/<d>` | the same trees through the callback API | as above |
| `perft/{startpos,matsuri,maxmoves}-cb-buf/<d>` | the same again, with one buffer reused for the whole tree instead of a `Vec` per internal node | as above |
| `perft/{startpos,matsuri,maxmoves}-mat/<d>` | the same again, but leaf parents **materialize** every move into that buffer instead of popcounting | as above |
| `perft/{startpos,matsuri,maxmoves}-mat-wi/<d>` | the same, materialized through `MoveSet::write_into` rather than the iterator | as above |
| `movegen/{startpos,matsuri,maxmoves}` | one `legal_moves()` call | — |
| `movegen/{startpos,matsuri,maxmoves}-cb` | the same, counted through the callback API | — |
| `movegen/{startpos,matsuri,maxmoves}-buf` | the same, materialized into a buffer allocated outside the measured closure | — |
| `movegen/{startpos,matsuri,maxmoves}-wi` | the same, through `MoveSet::write_into` | — |
| `movegen/sampled-v1` | `legal_moves()` over all 40 sampled real-game positions | Elements = positions |
| `movegen/sampled-v1-check` | same, restricted to the in-check subset (evasions) | Elements = positions |
| `movegen/sampled-v1{,-check}-{cb,buf,wi}` | the same two sweeps through each path above | Elements = positions |
| `do_undo/games-v1` | `do_move` all + `undo_move` all over 4 real games, the driver holding the `Undo` stack it allocated outside the measured loop | Elements = do+undo pairs |
| `internals/{bishop,rook,lance}-attacks` | the attack functions, 81 squares × 3 positions, against whichever backend is **live** | Elements = calls |
| `internals/attackers-to` | the reverse-lookup attacker test behind legality checking | Elements = calls |
| `internals/{bishop,rook}-attacks-{naive,qugiy,magic}` | the same sweep against each backend individually, in a single run | Elements = calls |

What the variants separate: `-cb` builds no `Move` and allocates only at internal nodes;
the plain ids build every move *and* allocate per node; `-mat` / `-buf` build every move
into a buffer the caller already owns. So **`-buf` minus `-cb` is the `MoveSet` → `Move`
expansion loop, and the plain id minus `-buf` is the allocation.** `-cb-buf` isolates the
one cost the callback API does not remove — internal-node collection, since `do_move`
needs `&mut Position` and the listener's borrow blocks it. That difference measures as
nil; FAQ.md holds the allocation counts that bound it.

The `-mat` ids exist because **the count-only path is the one a search will not use**, and
because only haitaka is on shunsai's footing in the cross-engine table.

Perft bench setup asserts the known node counts for **every** driver, so a wrong-depth
run, a broken movegen, or the drivers disagreeing all fail instead of recording garbage.

## Bench-id stability contract

History entries are keyed by criterion's `full_id` (`group/function[/value]`). Ids are
**append-only**:

- never rename or reuse an id — a renamed benchmark is a new id;
- a new API gets a new id (e.g. `movegen/matsuri-cb`), so old and new remain individually
  trackable;
- a new fixture version gets a new id (`movegen/sampled-v2`) and the old id is retired,
  never redefined.

Nothing checks this.

## Fixtures

Committed under `benches/positions/`, generated by
[`examples/gen_bench_positions.rs`](./examples/gen_bench_positions.rs) and **frozen**:
regeneration from the same inputs and seed is byte-identical, and any different set must
be committed as a new version with new bench ids. Each file's header records its
provenance and selection rules.

- `sampled-v1.sfen` — 40 positions sampled from floodgate real games (both players rated
  ≥ 3000), stratified by game phase (opening/middle/end = 12/16/12), at least 8 in check.
- `games-v1.usi` — 4 full games (≥ 120 plies) as USI move sequences, for the do/undo
  benchmark.

To regenerate (or build a v2), download one day of floodgate records and run the
generator:

```bash
curl -sL https://wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/ | grep -o 'href="wdoor[^"]*\.csa"' | sed 's/href="//;s/"$//' | sort -u > filelist.txt
```

```bash
xargs -n1 -I{} sh -c 'curl -sLO "https://wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/{}"; sleep 0.2' < filelist.txt
```

```bash
cargo run --release --example gen_bench_positions -- --csa-dir <download dir> --out-dir benches/positions
```

The generator validates every game move against `legal_moves()`, so a successful run
doubles as a differential test on real games.

## Quieting the machine first

A history entry is only worth the machine state during it, and **a plausible-looking
number is not evidence that the machine was quiet.** Two recorded entries needed
re-running for exactly this: once at σ = 46 % on `perft/matsuri/3`, once with two whole
suite runs discarded. Neither mean *looked* wrong — the σ = 46 % run landed on the
previous entry's value and would have recorded a null result where the truth was a large
gain.

- **Build before quieting, not after.** Compiling is itself a disturbance, and on a
  machine with an on-access scanner a freshly written binary keeps one busy for minutes
  afterwards. Get everything built (`--no-run`), let the machine settle, then measure with
  nothing left to compile.
- **Sample the load *during* the run, not before it.** An idle check at the start says
  nothing about the machine while the suite is executing, and a disturbance that shifts a
  mean uniformly does not show up in σ at all.
- **Close background applications and stop long-running local services.** Anything
  periodic is worse than anything constant, because it moves some ids and not others.

### What makes a run recordable

All three, not any one:

1. **σ ≤ 4 % on every id** — or, for the shortest `movegen/*` ids where σ is demonstrably
   not a function of duration, **agreement across three independent runs**. Re-rolling
   until every id happens to pass σ selects for lucky runs; agreement between runs is the
   stronger evidence. Readings that *drift monotonically* across runs are the machine
   changing state, not a property of the id.
2. **The control ids hold.** Quote the ids the change provably cannot reach and state
   their movement. **Pick the control per change, not from a list.**
   `internals/attackers-to` takes a `&Position`, so anything that moves `Position`'s
   layout reaches it and it is signal rather than control; the slider sweeps take a square
   and an occupancy, and are the controls that survive a layout change. **If the control
   drifts as much as the signal, the run is not recordable** — or, if it is recorded
   anyway, the entry must say so and give the ratio.
3. **Nothing is quoted across a base it was not measured against.** Gains quoted cross-day
   and losses quoted from same-run passes would flatter any change; give both bases when
   they differ.

Beyond that, an A/B wants **order-alternating passes**: machine load can change *during* a
set, and alternating is what makes that harmless, since the change lands on base and head
alike. An absolute run needs its own quiet window, which is a different requirement.

⚠️ **Full separation of two triples is weaker evidence than it looks on an id whose spread
is of the same order.** `perft/maxmoves-mat/2` looked like a separated +2.07 % regression
on three readings a side and is not one: at eleven readings the ranges overlap, the id
being unstable to σ 8.2 % within a pass and 4.3 % across four independent ones. Do not
read that cell. `movegen/maxmoves-buf` is this crate's most layout-volatile id and behaves
the same way; nothing has explained why.

Two traps this suite has actually sprung:

- **The `Vec` ids drift across days**; never read a change into them. Their `-cb` twins do
  the same generation without allocating and are the tell.
- **A screen's per-cell figures expire when anything else in the crate moves.** Numbers
  taken through one binary do not survive a change to another part of the crate —
  inlining and code layout shift underneath them. Re-measure on the tree that ships.

### Reading a figure against the committed history

⚠️ **State which estimator a quoted figure is.** `scripts/bench_snapshot.py` commits
criterion's **mean**; criterion's terminal output reports its **slope estimate**; the
snapshot also carries a median. The three differ by a few percent on the same id and must
not be read against each other — the same `do_undo` change reads −21.3 % on the committed
mean, −21.6 % on the median, and −21.7 % as criterion reported it.

## Recording a history entry

On a clean, committed tree, and a quiet machine (above):

```bash
cargo bench --features _bench-internals
```

```bash
python3 scripts/bench_snapshot.py --note "what changed"
```

The script summarizes `target/criterion/` into `benches/history/YYYY-MM-DD-<rev>.json`
(all ids, error bars, and machine meta) and regenerates the table below from all history
files. Commit both.

## Cross-engine standing

Run from the local-only benchmarks repository's perft harness, which shares no code with
criterion. Minimum kept, every cell validated against the known node counts. Seconds,
lower is better. **Read the `leaf` column** — count-only and materializing rows are not
comparable.

**Current, 2026-08-06** (shunsai rev `2be3a5b`, `results/2026-08-06.json`):

| engine | leaf | startpos d5 | matsuri d3 | maxmoves d3 |
|---|---|---|---|---|
| **shunsai** | count-only | **0.060926** | **0.002003** | **0.009771** |
| haitaka | count-only | 0.076580 | 0.002420 | 0.018260 |
| haitaka-bulk (control) | count-only | 0.075214 | 0.002379 | 0.017239 |
| yaneuraou (C++) | materialize | **0.090000** | 0.010000 | 0.082000 |
| **shunsai-mat** | materialize | 0.094221 | **0.005524** | **0.043669** |
| apery (C++) | materialize | 0.095169 | 0.009517 | 0.087326 |
| apery_rust | materialize | 0.100649 | 0.011468 | 0.110096 |
| haitaka-mat | materialize | 0.103839 | 0.015853 | 0.170039 |
| yasai 0.5.0 | materialize | 0.132825 | 0.011843 | 0.115836 |
| rshogi | materialize | 0.186331 | 0.016095 | 0.136277 |
| cshogi | materialize | 0.208250 | 0.014297 | 0.124539 |

**The goal is met.** On the count-only convention shunsai and haitaka share, shunsai is
ahead on **all three** — **1.26× / 1.21× / 1.87×**. apery_rust is beaten on all three
under the materializing convention (1.07× / 2.08× / 2.52×).

On startpos only YaneuraOu is still ahead. Like-for-like against its wall times
(0.089742 / 0.010291 / 0.082299): startpos a **1.05× loss**, matsuri **1.86×**, maxmoves
**1.88×**. Against haitaka-mat: 1.10× / 2.87× / 3.89×.

⚠️ This standing predates the changes recorded since; it is re-run deliberately, not per
commit, and the harness cannot resolve steps smaller than 15–25 % (below).

### Reading a harness run

- **The control decides whether a cross-day read is sound.** Every engine other than
  shunsai should reproduce its previous time; the run above held all of them to **2.4 %**,
  which is what made it quotable. When `haitaka-bulk` — which this repository does not
  touch — moved +9.2 % on matsuri the following day, the run could resolve nothing and the
  standing was left unchanged.
- **This harness is the instrument for 15–25 % steps, not for 3 % ones.** Perft trees
  spend only part of their time in generation, so a small generation change has no
  business showing here. A change of that size is a criterion-suite result, and where the
  two instruments have disagreed — including in sign — criterion is the one that resolves
  generation.
- **Two cells are structurally unreliable.** YaneuraOu's matsuri cell is a ~10 ms
  measurement on a 1 ms clock, and cshogi's carry Python binding overhead. Hedge them
  explicitly or quote a range.
- **Minimum-of-N is what makes a scattered harness run usable.** Noise only ever *adds*
  time, so min-of-N recovers the quiet-slot figure provided one repeat got a quiet slot —
  and cross-day reproduction of the other engines is the evidence that they did. A high
  spread is recorded rather than treated as disqualifying, but only alongside that
  control.

## History

Headline metrics per recorded run; full data in `benches/history/*.json`. This table is
generated by `scripts/bench_snapshot.py` — do not edit by hand.

**Read across a row, not down a column.** Two ids in one row were measured in the same run
on the same machine, so the difference between them is the code and nothing else. A column
read downwards also carries whatever moved between recording days — ids one change did not
touch at all have sat ~4 % below their previous figures, which is enough to hide a small
gain or invent a small regression. ⚠️ **The two conventions in a row are not comparable to
each other**: `-cb` counts leaves and `-mat-wi` builds every move.

**This table is a summary; the JSON is the record.** A question the columns cannot answer
is answered there rather than by widening the table. What earns a column is being readable
*for change* across rows, which is why the allocating `Vec` ids are absent.

<!-- BENCH_HISTORY_BEGIN -->
| date | rev | perft startpos-d4 -cb | perft startpos-d4 -mat-wi | perft matsuri-d3 -cb | perft matsuri-d3 -mat-wi | movegen sampled-v1 -cb | do_undo ns/pair | note |
|---|---|---|---|---|---|---|---|---|
| 2026-07-24 | 6858e24 | - | - | - | - | - | 10.9 | M2 naive baseline (M1 implementation) |
| 2026-07-27 | e101841 | - | - | - | - | - | 10.9 | const-evaluated attack tables (LazyLock removed) - neutral, … |
| 2026-07-27 | 8de28d8 | - | - | - | - | - | 11.4 | magic slider backend adopted (M4 bake-off vs qugiy/naive) |
| 2026-07-27 | d6ac964 | 104.1 | - | 784.3 | - | 0.34 | 10.9 | M3 callback API (-cb ids) + bitboard drop filtering |
| 2026-07-28 | abf8345 | 226.2 | - | 1702.1 | - | 0.13 | 11.1 | pin-based legality: checkers + pinned computed once per node |
| 2026-07-29 | b94d7b1 | 228.8 | - | 1728.5 | - | 0.13 | 10.8 | pawn-drop-mate simulation no longer clones Position; … |
| 2026-07-30 | 97e28b2 | 268.1 | - | 1844.2 | - | 0.09 | 10.8 | king danger bitboard (one per node, filtered to the king's … |
| 2026-08-03 | 045db51 | 267.5 | - | 1869.2 | - | 0.09 | 10.7 | materializing leaf convention: perft/*-mat and … |
| 2026-08-04 | ded13fc | 257.4 | 187.6 | 869.7 | 797.5 | 0.09 | 11.0 | MoveSet::write_into + legal_moves() routed through it; … |
| 2026-08-06 | 2be3a5b | 329.9 | 211.4 | 2210.0 | 828.3 | 0.07 | 10.8 | piece-indexed attacks_of dispatch + per-origin promotion … |
| 2026-08-07 | 658543b | 335.4 | 216.3 | 2211.6 | 794.1 | 0.07 | 10.8 | check_info's empty-board sniper scan served from ray … |
| 2026-08-07 | d056511 | 398.6 | 224.4 | 2451.3 | 847.1 | 0.06 | 10.9 | king_danger's slider half filtered by the … |
| 2026-08-18 | v0.1.2 | 403.1 | 232.6 | 2403.4 | 854.1 | 0.06 | 11.2 | v0.1.2 as released, measured after a stable quiet window. … |
| 2026-08-20 | v0.1.2-6-ge35956f | 408.5 | 239.0 | 2533.6 | 875.8 | 0.06 | 8.8 | Zobrist keys const-evaluated: do_undo -21.6 % against the … |
<!-- BENCH_HISTORY_END -->
