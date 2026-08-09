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
| 2026-08-04 | ded13fc | 178.5 | 257.4 | 179.8 | 187.6 | 631.2 | 869.7 | 583.7 | 797.5 | 0.25 | 0.20 | 0.09 | 0.20 | 0.16 | 11.0 | MoveSet::write_into + legal_moves() routed through it; perft/matsuri-cb/3 unstable under whole-suite conditions (sigma 27%, cause unresolved) - its series is not comparable with 2026-08-03, see DESIGN.md; the other 45 ids are sigma <= 4% |
| 2026-08-06 | 2be3a5b | 181.2 | 329.9 | 203.8 | 211.4 | 758.7 | 2210.0 | 608.7 | 828.3 | 0.22 | 0.17 | 0.07 | 0.16 | 0.14 | 10.8 | piece-indexed attacks_of dispatch + per-origin promotion decision; generation -22.7% on startpos, -23.1% on the sampled-v1 real-game fixture; legal_moves() sized from 593. Recorded on agreement across four independent runs (44 of 46 ids within 3.4%) rather than sigma; perft/maxmoves-cb/2 reads 15% above the other three runs while its non-allocating twin perft/maxmoves-cb-buf/2 is stable to 0.6% across all four, so treat that one id's series as broken here - the same allocator artifact 2026-08-04 recorded for perft/matsuri-cb/3, which has itself recovered. |
| 2026-08-07 | 658543b | 190.0 | 335.4 | 201.0 | 216.3 | 731.1 | 2211.6 | 597.6 | 794.1 | 0.21 | 0.17 | 0.07 | 0.16 | 0.14 | 10.8 | check_info's empty-board sniper scan served from ray tables: generation -3.4% on the sampled-v1 real-game fixture, -3.0% on its in-check subset, -0.9..-1.8% on the three fixture positions, and -1.6..-2.3% on the perft trees. Recorded on agreement across four independent runs (42 of 46 ids within 4%) rather than sigma - the ids over the bar drifted monotonically across runs rather than scattering, so the median of the four is what the DESIGN.md entry quotes. Control: all 11 ids this change cannot reach (internals/*, do_undo) reproduce their 2026-08-06 figures within +-1.7%, which is what makes the cross-day read sound. |
| 2026-08-07 | d056511 | 182.1 | 398.6 | 223.0 | 224.4 | 771.0 | 2451.3 | 609.4 | 847.1 | 0.21 | 0.16 | 0.06 | 0.16 | 0.14 | 10.9 | king_danger's slider half filtered by the orthogonal/diagonal attacker zones. Figures are from the two order-reversed passes; this file's own deltas against 658543b are given alongside where the two differ, because the table is read standalone. Generation -16.4% on the initial position (cross-day -16.40%), -11.7% matsuri (-11.2%), -9.0% on the sampled-v1 real-game fixture (-9.05%), -6.4% on its in-check subset (-6.5%); perft/startpos-cb/4 -17.5% (-15.9%), maxmoves-cb/2 -14.4% (-21.2%, an id the 2026-08-06 row flagged as a broken series), matsuri-cb/3 -9.4% (-9.8%). Recorded on agreement across two order-reversed passes of both binaries (the measured ids agree to 0.4-2.7 pp; startpos-cb, the widest, -17.74/-15.03). Control is imperfect and the reason is the change itself: the two new tables shift .rodata, and internals/bishop-attacks-magic reproduces +4.4% across both passes with each binary self-reproducing to 0.7%. Signal is 1.5-4.0x that and opposite in sign - 3.7-4.0x on the two startpos cells, 2.0-2.7x on matsuri and the real-game fixture, 1.5x on the in-check subset, which is the thinnest cell here. Moving the wrong way and not named in the DESIGN.md bullets, all cross-day: perft/startpos/4 +4.4%, internals/* +0.8..+3.6%, and movegen/maxmoves-buf +25.6% (the passes read +18.4/+21.8%), the largest single movement in the run in either direction. do_undo +0.1% in pass 1; its +16.6% in pass 2 is a disturbance during that one measurement (the same binary self-reproduced +18.5% against its own pass-1 run). |
<!-- BENCH_HISTORY_END -->
