# Benchmarks

Everything about measuring `shunsai`: the method, the in-repo criterion suite,
the fixtures, the committed history, and the cross-engine standing.

The comparison *targets* are pinned submodules in the local-only
`../benchmarks` repository (see [DESIGN.md](./DESIGN.md) §5); why a given
result was adopted or rejected is in [DECISIONS.md](./DECISIONS.md).

## Method

- **Metrics**: (1) **perft** (nodes/sec; also a correctness check); (2) **movegen alone** (ns per position); (3) **do/undo throughput**.
- **Tooling**: `criterion` in-repo; the cross-engine perft harness in `../benchmarks/perft`.
- **Conditions**: `--release` / `lto = "fat"` / `codegen-units = 1`; same machine; with warm-up; a fixed position set; multiple trials with variance recorded; CPU architecture (x86_64 / aarch64) noted.
- **Position set (fixed SFEN)**:
  - initial position
  - **"matsuri" midgame position** (指し手生成祭り, the standard movegen-benchmark position in the Japanese shogi-dev community; used by YaneuraOu's `bench` / `test genmoves`): `l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1`
  - **maximum-legal-move position**: `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`
  - check- and mate-adjacent positions — realized as the in-check subset of the sampled real-game fixture (`movegen/sampled-v1-check`)

### Fairness — normalize every library to the same work

- **Full legal move generation.** Account for pseudo-legal + validation vs fully-legal differences, callback vs `Vec` API differences, and Python-binding boundary costs.
- **Pawn-drop-mate (打ち歩詰め) exclusion.** Legal movegen must not generate pawn drops that give immediate checkmate. Engines differ here and it is a known cause of perft mismatches; shunsai excludes them, and comparisons must confirm each library does the same.
- ⚠️ **"Leaf bulk counting" is two conventions, not one, and the difference decides the comparison.** Every engine in the harness bulk-counts at leaf parents, yet they do not do the same work to get the number:

  | convention | how the count is obtained | engines |
  |---|---|---|
  | **count-only** | straight off the destination bitboards; **no `Move` is ever constructed** | shunsai (`MoveSet::len()`, two popcounts), haitaka (`PieceMoves::into_iter().len()`) |
  | **materialize** | build every legal move, take the list's length | YaneuraOu (`MoveList<LEGAL_ALL>`, `source/perft.h`), apery (`MoveList<LegalAll>`), apery_rust (`leaf_mlist.generate::<LegalAllType>`), yasai, rshogi, cshogi, Fairy-Stockfish (`MoveList<LEGAL>`) |

  Count-only is a genuine advantage of the callback API — it is what `MoveSet::len()` is *for* — but it is one **perft collects and a search cannot**, since a search must have the moves. So a count-only number and a materializing number are not comparable, and the gap is not small: on the current run below, `shunsai-mat` costs **1.55× / 2.76× / 4.47×** what `shunsai` does. Read that ratio off the current table rather than quoting it from here — it has moved every time the expansion path was optimized. The harness records `leaf` per row and carries `shunsai-mat` and `haitaka-mat` columns (the same binaries with `--materialize`), so each pair isolates exactly this difference.

  **Never compare numbers produced under different conventions.** Mixing them is what made the standing unreadable for a week — see [Cross-engine standing](#cross-engine-standing).

### How the C++ engines are measured

YaneuraOu is driven over USI with its built-in **`go perft <d>`** (`Benchmark::perft`), which uses `MoveList<LEGAL_ALL>` and bulk-counts at leaf parents — the same tree under the materializing convention. Its self-reported `Elapsed Time` is an *integer millisecond* count plus a deliberate `+1` (ゼロ割防止), so the harness subtracts the 1 ms and falls back to wall time below a millisecond; charging the raw figure had been costing YaneuraOu up to +10 % on the matsuri cell.

⚠️ **Do not measure YaneuraOu with `test genmoves`.** It looks like the movegen-level command and is not convention-compatible: it loops `MoveList<EVASIONS>` / `MoveList<NON_EVASIONS>` rather than `LEGAL_ALL`, so its numbers cannot be compared with anything else in the table.

apery ships no perft at all, so it gets a small driver of our own on `MoveList<LegalAll>`. cshogi likewise has none, so we drive its `Board` API from Python — its binding overhead is part of what the "practical stack" comparison measures, and should be reported as such.

## Running

```bash
cargo bench --features bench-internals
```

The whole suite is one bench target (`benches/suite/`); without the
feature, the `internals` group is compiled out and the rest runs.

> ⚠️ **Name the backend a run wants**: `--features bench-internals`, plus
> `slider-qugiy` or `slider-naive` deliberately when comparing backends.
> `--all-features` is not the shortcut it looks like — the two backend flags
> select the same thing, so enabling both is a `compile_error!` and the crate
> does not build that way at all. It used to resolve by a priority order that
> `slider-naive` won, which built every `perft/*` and `movegen/*` id against
> the **naive oracle backend** — several times slower on both bishop and rook
> — and reported it without complaint.

Useful variants:

```bash
cargo bench --features bench-internals movegen
```

```bash
cargo bench --features bench-internals -- --save-baseline before
```

criterion baselines (`--save-baseline <name>` / `-- --baseline <name>`)
live under `target/criterion/` and are throwaway, for local A/B comparison
while developing. The durable record is the [history](#history) below.

Conditions (see [Method](#method)): release profile with `lto = "fat"` and
`codegen-units = 1` (already configured), warm-up, a quiet machine, and the
same machine for comparable numbers — every history entry records CPU, OS,
rustc, and criterion versions. The full suite takes roughly 3–5 minutes.

## Suite

| id | measures | throughput |
|---|---|---|
| `perft/startpos/4` | perft(4) from the initial position, leaf bulk counting | Elements = 719,731 nodes |
| `perft/matsuri/3` | perft(3) from the matsuri midgame position | Elements = 4,809,015 nodes |
| `perft/maxmoves/2` | perft(2) from the max-legal-moves position | Elements = 105,677 nodes |
| `perft/{startpos,matsuri,maxmoves}-cb/<d>` | the same trees through the callback API | as above |
| `perft/{startpos,matsuri,maxmoves}-cb-buf/<d>` | the same again, with one buffer reused for the whole tree instead of a `Vec` per internal node | as above |
| `perft/{startpos,matsuri,maxmoves}-mat/<d>` | the same again, but leaf parents **materialize** every move into that buffer instead of popcounting — the convention the other engines use | as above |
| `perft/{startpos,matsuri,maxmoves}-mat-wi/<d>` | the same, materialized through `MoveSet::write_into` rather than the iterator | as above |
| `movegen/startpos` | one `legal_moves()` call | — |
| `movegen/matsuri` | one `legal_moves()` call | — |
| `movegen/maxmoves` | one `legal_moves()` call | — |
| `movegen/{startpos,matsuri,maxmoves}-cb` | the same, counted through the callback API | — |
| `movegen/{startpos,matsuri,maxmoves}-buf` | the same, materialized into a buffer allocated outside the measured closure | — |
| `movegen/{startpos,matsuri,maxmoves}-wi` | the same, through `MoveSet::write_into` rather than the iterator | — |
| `movegen/sampled-v1` | `legal_moves()` over all 40 sampled real-game positions | Elements = positions |
| `movegen/sampled-v1-check` | same, restricted to the in-check subset (evasions) | Elements = positions |
| `movegen/sampled-v1{,-check}-cb` | the same two sweeps through the callback API | Elements = positions |
| `movegen/sampled-v1{,-check}-buf` | the same two sweeps, materialized into a reused buffer | Elements = positions |
| `movegen/sampled-v1{,-check}-wi` | the same two sweeps, through `MoveSet::write_into` | Elements = positions |
| `do_undo/games-v1` | `do_move` all + `undo_move` all over 4 real games | Elements = do+undo pairs |
| `internals/bishop-attacks` | `bishop_attacks(sq, occ)`, 81 squares × 3 positions | Elements = calls |
| `internals/rook-attacks` | `rook_attacks(sq, occ)`, same sweep | Elements = calls |
| `internals/lance-attacks` | `lance_attacks(color, sq, occ)`, both colors | Elements = calls |
| `internals/attackers-to` | the reverse-lookup attacker test behind legality checking | Elements = calls |
| `internals/{bishop,rook}-attacks-{naive,qugiy,magic}` | the same sweep against each M4 slider backend individually | Elements = calls |

`internals/bishop-attacks` and `internals/rook-attacks` track whichever
backend is *live*, so they are the improvement time series; the
`-naive` / `-qugiy` / `-magic` ids measure all three in a single run and are
what a backend-adoption decision reads. Selecting a backend for the
`perft/*` and `movegen/*` ids needs a rebuild:

```bash
cargo bench --features bench-internals,slider-qugiy
```

The plain `perft/*` and `movegen/*` ids measure the allocating
`legal_moves()` wrapper; the `-cb` ids measure the same work through the M3
callback API (`generate_moves`), where nothing is allocated and no `Move` is
built — `perft/*-cb` counts leaves straight off `MoveSet::len()`. Perft
bench setup asserts the known node counts for **every** driver, so a
wrong-depth run, a broken movegen, or the drivers disagreeing all fail
instead of recording garbage.

The `-cb-buf` ids isolate the one cost the callback API does *not* remove.
`do_move` needs `&mut Position`, which the listener's borrow blocks, so
anything deeper than one ply has to collect the moves before playing them —
and `-cb` does that with a fresh `Vec` per internal node. `-cb-buf` runs the
identical tree with a single buffer threaded through the recursion, each ply
taking a slice off the end and truncating back on the way out, allocated once
outside the measured closure. The leaf path is the same bulk count in both,
so the difference is purely internal-node collection.

That difference measures as **nil**, and it is structurally tiny: bulk
counting means only internal nodes allocate, which is 931 allocations for
startpos-d4 and exactly 1 for maxmoves-d2. Note the durable form of that
bound is the *count*, not a percentage — the share of runtime it can buy
rises as the crate gets faster (~0.2 % on the M3 tree where these ids were
introduced, ~0.5 % at this branch's speed), so quote the allocation counts
rather than the percentage. The ids are kept as the standing evidence for
that (see the 2026-07-29 entry in [DECISIONS.md](./DECISIONS.md), which
retracts an earlier ad-hoc −7.4 %), and as the baseline a copy-make driver
would have to beat.

The `-mat` and `-buf` ids separate two things the `-cb`-vs-plain pair had been
conflating: whether a `Move` is **built** at all, and whether the list it goes
into is **allocated**. `-cb` builds nothing and allocates only at internal
nodes; the plain ids build every move *and* allocate per node; `-mat` / `-buf`
build every move into a buffer the caller already owns. So `-buf` minus `-cb`
is the `MoveSet` → `Move` expansion loop, and the plain id minus `-buf` is the
allocation.

They matter because **the count-only path is the one a search will not use**:
a search needs the moves. It is also the reason the cross-engine table
flatters shunsai — YaneuraOu, apery, apery_rust, yasai, rshogi and cshogi all
count leaf moves by building them, so only haitaka is on the same footing.
The `-mat` ids exist so the committed history tracks both conventions rather
than one. See [Fairness](#fairness--normalize-every-library-to-the-same-work)
above and [Cross-engine standing](#cross-engine-standing) below.

## Bench-id stability contract

History entries are keyed by criterion's `full_id` (`group/function[/value]`).
Ids are **append-only**:

- never rename or reuse an id — a renamed benchmark is a new id;
- a new API gets a new id (e.g. `movegen/matsuri-cb`), so old and new remain
  individually trackable;
- a new fixture version gets a new id (`movegen/sampled-v2`) and the old id
  is retired, never redefined.

## Fixtures

Committed under `benches/positions/`, both generated by
[`examples/gen_bench_positions.rs`](./examples/gen_bench_positions.rs) and
**frozen**: regeneration from the same inputs and seed is byte-identical,
and any different set must be committed as a new version (v2, ...) with new
bench ids. Each file's header records its provenance and selection rules.

- `sampled-v1.sfen` — 40 positions sampled from floodgate real games
  (both players rated ≥ 3000), stratified by game phase
  (opening/middle/end = 12/16/12), at least 8 in-check positions.
- `games-v1.usi` — 4 full games (≥ 120 plies) as USI move sequences, for
  the do/undo benchmark.

Sampled positions are factual game data; the whole extraction pipeline is
this repository's own permissive Rust code (no GPL tooling involved), and
raw kifu files are never committed. To regenerate (or build a v2), download
one day of floodgate records and run the generator:

```bash
curl -sL https://wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/ | grep -o 'href="wdoor[^"]*\.csa"' | sed 's/href="//;s/"$//' | sort -u > filelist.txt
```

```bash
xargs -n1 -I{} sh -c 'curl -sLO "https://wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/{}"; sleep 0.2' < filelist.txt
```

```bash
cargo run --release --example gen_bench_positions -- --csa-dir <download dir> --out-dir benches/positions
```

The generator validates every game move against `legal_moves()`, so a
successful run doubles as a differential test on real games.

## Quieting the machine first

A history entry is only worth the machine state during it, and **a
plausible-looking number is not evidence that the machine was quiet.** Two
recorded entries needed re-running for exactly this reason: once at σ = 46 % on
`perft/matsuri/3`, once with two whole suite runs discarded. Neither mean
*looked* wrong — the σ = 46 % run landed on the previous entry's value and would
have recorded a null result where the truth was a large gain.

Three things are worth doing rather than assuming:

- **Build before quieting, not after.** Compiling is itself a disturbance,
  and on a machine with an on-access scanner a freshly written binary keeps
  one busy for minutes afterwards. Get everything built
  (`cargo bench --features bench-internals --no-run`), then let the machine
  settle, then measure with nothing left to compile.
- **Sample the load *during* the run, not before it.** An idle check at the
  start says nothing about the machine while the suite is executing, and a
  disturbance that shifts a mean uniformly does not show up in σ at all.
- **Close background applications and stop long-running local services**
  before a run that will be recorded. Anything periodic is worse than
  anything constant, because it moves some ids and not others.

### What makes a run recordable

All three, not any one:

1. **σ ≤ 4 % on every id** — or, for the shortest `movegen/*` ids where σ is
   demonstrably not a function of duration, **agreement across three
   independent runs**. Re-rolling until every id happens to pass σ selects for
   lucky runs; agreement between runs is the stronger evidence. Readings that
   *drift monotonically* across runs are the machine changing state, not a
   property of the id.
2. **The control ids hold.** Quote the ids the change provably cannot reach
   (`internals/*`, `do_undo/*` for a movegen change) and state their movement.
   **If the control drifts as much as the signal, the run is not recordable** —
   or, if it is recorded anyway, the entry must say so and give the ratio.
3. **Nothing is quoted across a base it was not measured against.** Gains
   quoted cross-day and losses quoted from same-run passes would flatter any
   change; give both bases when they differ.

Two traps this suite has actually sprung:

- **The `Vec` ids drift across days**; never read a change into them. Their
  `-cb` twins do the same generation without allocating and are the tell.
- **A screen's per-cell figures expire when anything else in the crate moves.**
  Numbers taken through one binary do not survive a change to another part of
  the crate — inlining and code layout shift underneath them. Re-measure on the
  tree that ships before recording.

### Reading a figure against the committed history

`scripts/bench_snapshot.py` commits criterion's **mean**; criterion's terminal
output reports its **slope estimate**. The two differ by a few percent on the
same id and must not be read against each other — a session comparing its
console output to `benches/history/*.json` will otherwise see a phantom
regression.

## Recording a history entry

On a clean, committed tree, and a quiet machine (above):

```bash
cargo bench --features bench-internals
```

```bash
python3 scripts/bench_snapshot.py --note "what changed"
```

The script summarizes `target/criterion/` into
`benches/history/YYYY-MM-DD-<rev>.json` (all ids, error bars, and machine
meta) and regenerates the table below from all history files. Commit both.

## Cross-engine standing

Run from the local-only `../benchmarks/perft` harness, which shares no code
with criterion:

```bash
./perft --measure --repeat 5
```

Minimum kept, every cell validated against the known node counts (DESIGN.md §6).
Seconds, lower is better. **Read the `leaf` column** — count-only and
materializing rows are not comparable (see [Fairness](#fairness--normalize-every-library-to-the-same-work)).

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

**M5 is met.** On the count-only convention shunsai and haitaka share, shunsai
is ahead on **all three** — **1.26× / 1.21× / 1.87×**. apery_rust is beaten on
all three under the materializing convention (1.07× / 2.08× / 2.52×).

**startpos moves from fifth of nine to second**, passing apery, apery_rust and
haitaka-mat; only YaneuraOu is still ahead. Like-for-like against its wall
times (0.089742 / 0.010291 / 0.082299): startpos a **1.05× loss**, matsuri
**1.86×**, maxmoves **1.88×**. Against haitaka-mat: 1.10× / 2.87× / 3.89×.

### How the standing got here

| date | shunsai count-only | shunsai-mat | vs haitaka (count-only) | vs YaneuraOu (materialize) |
|---|---|---|---|---|
| 2026-07-30 | 0.075326 / 0.002407 / 0.010716 | — | 1.00× / 0.98× / 1.67× | *not comparable — conventions mixed* |
| 2026-07-31 | 0.074960 / 0.002407 / 0.010969 | 0.108286 / 0.007764 / 0.066976 | 1.01× / 0.98× / 1.70× | 0.80× / 1.11× / 1.17× |
| 2026-08-04 | 0.075095 / 0.002503 / 0.010930 | 0.102947 / 0.006144 / 0.043976 | 1.005× / 0.97× / 1.67× | 0.86× / 1.40…1.64× / 1.85× |
| 2026-08-06 | 0.060926 / 0.002003 / 0.009771 | 0.094221 / 0.005524 / 0.043669 | **1.26× / 1.21× / 1.87×** | **0.95× / 1.86× / 1.88×** |

⚠️ **The 2026-07-30 row is why this table has a `leaf` column at all.** It read
"1.19× / 4.57× / 7.84× faster than YaneuraOu" by comparing a shunsai that counts
leaves with two popcounts against engines that build every move to count it.
Those numbers were not wrong, but they were not a statement about move
generation — and the one comparison in that table that *was* already
like-for-like is the one against the main rival, haitaka. Established from
source, not from timings: only haitaka's bulk path (`moves.into_iter().len()`)
constructs nothing either.

### Reading a harness run

- **The control decides whether a cross-day read is sound.** Every engine other than shunsai should reproduce its previous time; 2026-08-06 held all of them to **2.4 %**, which is what made that read quotable. When `haitaka-bulk` — which this repository does not touch — moved +9.2 % on matsuri (2026-08-07), the run could resolve nothing and the standing was left unchanged.
- **This harness is the instrument for 15–25 % steps, not for 3 % ones.** Perft trees spend only part of their time in generation, so a small generation change has no business showing here. A change of that size is a criterion-suite result. Where the two instruments have disagreed — including in sign, on the materializing initial position in 2026-08-07 — criterion is the one that resolves generation.
- **Two cells are structurally unreliable.** YaneuraOu's matsuri cell is a ~10 ms measurement on a 1 ms clock, and cshogi's carry Python binding overhead. Hedge them explicitly or quote a range.
- **Minimum-of-N is what makes a scattered harness run usable.** `spread_pct` can be poor while the minimum is sound, because noise only ever *adds* time — min-of-N recovers the quiet-slot figure provided one repeat got a quiet slot, and cross-day reproduction of the other engines is the evidence that they did. A high spread is therefore recorded rather than treated as disqualifying, but only alongside that control.

## History

Headline metrics per recorded run; full data in `benches/history/*.json`.
This table is generated by `scripts/bench_snapshot.py` — do not edit by
hand.

**Read the paired columns across a row, not a column down the table.** Where
a change adds a *new* path beside the old one — `-mat` next to `-mat-wi`,
`-buf` next to `-wi` — the pair is measured in the same run on the same
machine, so the difference between them is the change and nothing else. A
column read downwards also carries whatever moved between recording days:
on 2026-08-04, ids this crate did not touch at all (`perft/startpos-cb/4`,
`movegen/sampled-v1-cb`) sat **~4 % below** their 2026-08-03 figures, which
is enough to hide a small gain or invent a small regression. That is also
why a row can show a new path winning while the old path's own column drifts
down: only the first of those is a statement about the code.

<!-- BENCH_HISTORY_BEGIN -->
| date | rev | perft startpos-d4 Mnps | perft startpos-d4 -cb | perft startpos-d4 -mat | perft startpos-d4 -mat-wi | perft matsuri-d3 Mnps | perft matsuri-d3 -cb | perft matsuri-d3 -mat | perft matsuri-d3 -mat-wi | movegen matsuri µs | movegen sampled-v1 µs/pos | movegen sampled-v1 -cb | movegen sampled-v1 -buf | movegen sampled-v1 -wi | do_undo ns/pair | note |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-07-24 | 6858e24 | 56.1 | - | - | - | 207.1 | - | - | - | 0.79 | 0.89 | - | - | - | 10.9 | M2 naive baseline (M1 implementation) |
| 2026-07-27 | e101841 | 56.3 | - | - | - | 213.1 | - | - | - | 0.79 | 0.88 | - | - | - | 10.9 | const-evaluated attack tables (LazyLock removed) - neutral, all deltas within noise |
| 2026-07-27 | 8de28d8 | 97.7 | - | - | - | 326.9 | - | - | - | 0.52 | 0.48 | - | - | - | 11.4 | magic slider backend adopted (M4 bake-off vs qugiy/naive) |
| 2026-07-27 | d6ac964 | 95.8 | 104.1 | - | - | 408.4 | 784.3 | - | - | 0.39 | 0.42 | 0.34 | - | - | 10.9 | M3 callback API (-cb ids) + bitboard drop filtering |
| 2026-07-28 | abf8345 | 158.8 | 226.2 | - | - | 539.6 | 1702.1 | - | - | 0.31 | 0.24 | 0.13 | - | - | 11.1 | pin-based legality: checkers + pinned computed once per node |
| 2026-07-29 | b94d7b1 | 160.9 | 228.8 | - | - | 546.5 | 1728.5 | - | - | 0.30 | 0.24 | 0.13 | - | - | 10.8 | pawn-drop-mate simulation no longer clones Position; movegen allocates nothing |
| 2026-07-30 | 97e28b2 | 184.4 | 268.1 | - | - | 556.9 | 1844.2 | - | - | 0.29 | 0.20 | 0.09 | - | - | 10.8 | king danger bitboard (one per node, filtered to the king's neighbourhood) + checkers/pins fused into one scan; movegen/maxmoves-cb at sigma 5.7%; recorded on 3-run agreement (reported time 101.94/101.11/100.84 ns), not on duration |
| 2026-08-03 | 045db51 | 188.1 | 267.5 | 187.4 | - | 554.1 | 1869.2 | 595.6 | - | 0.30 | 0.22 | 0.09 | 0.20 | - | 10.7 | materializing leaf convention: perft/*-mat and movegen/*-buf ids added; recorded on 3-run agreement (worst spread 3.8%, sigma up to 33% on movegen/* under background load) |
| 2026-08-04 | ded13fc | 178.5 | 257.4 | 179.8 | 187.6 | 631.2 | 869.7 | 583.7 | 797.5 | 0.25 | 0.20 | 0.09 | 0.20 | 0.16 | 11.0 | MoveSet::write_into + legal_moves() routed through it; perft/matsuri-cb/3 unstable under whole-suite conditions (sigma 27%, cause unresolved) - its series is not comparable with 2026-08-03, see DECISIONS.md; the other 45 ids are sigma <= 4% |
| 2026-08-06 | 2be3a5b | 181.2 | 329.9 | 203.8 | 211.4 | 758.7 | 2210.0 | 608.7 | 828.3 | 0.22 | 0.17 | 0.07 | 0.16 | 0.14 | 10.8 | piece-indexed attacks_of dispatch + per-origin promotion decision; generation -22.7% on startpos, -23.1% on the sampled-v1 real-game fixture; legal_moves() sized from 593. Recorded on agreement across four independent runs (44 of 46 ids within 3.4%) rather than sigma; perft/maxmoves-cb/2 reads 15% above the other three runs while its non-allocating twin perft/maxmoves-cb-buf/2 is stable to 0.6% across all four, so treat that one id's series as broken here - the same allocator artifact 2026-08-04 recorded for perft/matsuri-cb/3, which has itself recovered. |
| 2026-08-07 | 658543b | 190.0 | 335.4 | 201.0 | 216.3 | 731.1 | 2211.6 | 597.6 | 794.1 | 0.21 | 0.17 | 0.07 | 0.16 | 0.14 | 10.8 | check_info's empty-board sniper scan served from ray tables: generation -3.4% on the sampled-v1 real-game fixture, -3.0% on its in-check subset, -0.9..-1.8% on the three fixture positions, and -1.6..-2.3% on the perft trees. Recorded on agreement across four independent runs (42 of 46 ids within 4%) rather than sigma - the ids over the bar drifted monotonically across runs rather than scattering, so the median of the four is what the DECISIONS.md entry quotes. Control: all 11 ids this change cannot reach (internals/*, do_undo) reproduce their 2026-08-06 figures within +-1.7%, which is what makes the cross-day read sound. |
| 2026-08-07 | d056511 | 182.1 | 398.6 | 223.0 | 224.4 | 771.0 | 2451.3 | 609.4 | 847.1 | 0.21 | 0.16 | 0.06 | 0.16 | 0.14 | 10.9 | king_danger's slider half filtered by the orthogonal/diagonal attacker zones. Figures are from the two order-reversed passes; this file's own deltas against 658543b are given alongside where the two differ, because the table is read standalone. Generation -16.4% on the initial position (cross-day -16.40%), -11.7% matsuri (-11.2%), -9.0% on the sampled-v1 real-game fixture (-9.05%), -6.4% on its in-check subset (-6.5%); perft/startpos-cb/4 -17.5% (-15.9%), maxmoves-cb/2 -14.4% (-21.2%, an id the 2026-08-06 row flagged as a broken series), matsuri-cb/3 -9.4% (-9.8%). Recorded on agreement across two order-reversed passes of both binaries (the measured ids agree to 0.4-2.7 pp; startpos-cb, the widest, -17.74/-15.03). Control is imperfect and the reason is the change itself: the two new tables shift .rodata, and internals/bishop-attacks-magic reproduces +4.4% across both passes with each binary self-reproducing to 0.7%. Signal is 1.5-4.0x that and opposite in sign - 3.7-4.0x on the two startpos cells, 2.0-2.7x on matsuri and the real-game fixture, 1.5x on the in-check subset, which is the thinnest cell here. Moving the wrong way and not named in the DECISIONS.md bullets, all cross-day: perft/startpos/4 +4.4%, internals/* +0.8..+3.6%, and movegen/maxmoves-buf +25.6% (the passes read +18.4/+21.8%), the largest single movement in the run in either direction. do_undo +0.1% in pass 1; its +16.6% in pass 2 is a disturbance during that one measurement (the same binary self-reproduced +18.5% against its own pass-1 run). |
<!-- BENCH_HISTORY_END -->
