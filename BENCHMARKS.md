# Benchmarks

Micro-benchmarks for `shunsai` (criterion) and the committed history of
results. Method and conditions are defined in [DESIGN.md](./DESIGN.md) §4;
the cross-engine perft comparison (haitaka / apery_rust / YaneuraOu, ...)
lives in the local-only `../benchmarks` repository and is not part of this
suite.

## Running

```bash
cargo bench --features bench-internals
```

The whole suite is one bench target (`benches/suite/`); without the
feature, the `internals` group is compiled out and the rest runs.

> ⚠️ **Never record with `--all-features`.** Cargo features are additive, so
> the slider backend is chosen by a priority order in `src/sliders.rs`, and
> `slider-naive` wins it. `--all-features` therefore builds every `perft/*`
> and `movegen/*` id against the **naive oracle backend** — around 5× slower on
> bishops and 8× on rooks — and reports it without complaint. CI's
> `cargo bench --no-run --all-features` step is fine because it only
> type-checks, but a measurement run must name the features it wants
> (`--features bench-internals`, plus `slider-qugiy` or `slider-naive`
> deliberately when comparing backends).

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

Conditions (DESIGN.md §4): release profile with `lto = "fat"` and
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
that (see the 2026-07-29 decision-log entry in DESIGN.md, which retracts an
earlier ad-hoc −7.4 %), and as the baseline a copy-make driver would have to
beat.

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
than one. See DESIGN.md §4 ("leaf bulk counting is two conventions") and the
2026-07-30 entry under §6 M5.

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

A history entry is only worth the machine state during it, and a
plausible-looking number is not evidence that the machine was quiet — two
entries in DESIGN.md's decision log were re-run for exactly this reason
(σ = 46 % on `perft/matsuri/3` once; two whole suite runs discarded another
time). Neither mean *looked* wrong.

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

Acceptance for a recordable run: σ ≤ 4 % on every id, or — for the shortest
`movegen/*` ids, where σ is demonstrably not a function of duration —
agreement across three independent runs. Re-rolling until every id happens to
pass σ selects for lucky runs; agreement between runs is the stronger
evidence.

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

## History

Headline metrics per recorded run; full data in `benches/history/*.json`.
This table is generated by `scripts/bench_snapshot.py` — do not edit by
hand.

<!-- BENCH_HISTORY_BEGIN -->
| date | rev | startpos-d4 Mnps | startpos-d4 -cb | startpos-d4 -mat | matsuri-d3 Mnps | matsuri-d3 -cb | matsuri-d3 -mat | movegen matsuri µs | sampled-v1 µs/pos | sampled-v1 -cb | sampled-v1 -buf | do_undo ns/pair | note |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-07-24 | 6858e24 | 56.1 | - | - | 207.1 | - | - | 0.79 | 0.89 | - | - | 10.9 | M2 naive baseline (M1 implementation) |
| 2026-07-27 | e101841 | 56.3 | - | - | 213.1 | - | - | 0.79 | 0.88 | - | - | 10.9 | const-evaluated attack tables (LazyLock removed) - neutral, all deltas within noise |
| 2026-07-27 | 8de28d8 | 97.7 | - | - | 326.9 | - | - | 0.52 | 0.48 | - | - | 11.4 | magic slider backend adopted (M4 bake-off vs qugiy/naive) |
| 2026-07-27 | d6ac964 | 95.8 | 104.1 | - | 408.4 | 784.3 | - | 0.39 | 0.42 | 0.34 | - | 10.9 | M3 callback API (-cb ids) + bitboard drop filtering |
| 2026-07-28 | abf8345 | 158.8 | 226.2 | - | 539.6 | 1702.1 | - | 0.31 | 0.24 | 0.13 | - | 11.1 | pin-based legality: checkers + pinned computed once per node |
| 2026-07-29 | b94d7b1 | 160.9 | 228.8 | - | 546.5 | 1728.5 | - | 0.30 | 0.24 | 0.13 | - | 10.8 | pawn-drop-mate simulation no longer clones Position; movegen allocates nothing |
| 2026-07-30 | 97e28b2 | 184.4 | 268.1 | - | 556.9 | 1844.2 | - | 0.29 | 0.20 | 0.09 | - | 10.8 | king danger bitboard (one per node, filtered to the king's neighbourhood) + checkers/pins fused into one scan; movegen/maxmoves-cb at sigma 5.7%; recorded on 3-run agreement (reported time 101.94/101.11/100.84 ns), not on duration |
| 2026-08-03 | 045db51 | 188.1 | 267.5 | 187.4 | 554.1 | 1869.2 | 595.6 | 0.30 | 0.22 | 0.09 | 0.20 | 10.7 | materializing leaf convention: perft/*-mat and movegen/*-buf ids added; recorded on 3-run agreement (worst spread 3.8%, sigma up to 33% on movegen/* under background load) |
<!-- BENCH_HISTORY_END -->
