# shunsai — Design Document

Design, implementation approach, benchmarking method, comparison targets, milestones, and licensing policy for `shunsai`.
Current status: **M3 complete; M4 in progress** — callback move generation and magic slider attacks landed, both adopted by measurement against the committed benchmark history.

## 1. Background & goal

- [`yasai`](https://github.com/sugyan/yasai) ("Yet Another Shogi library, for AI") is a Rust library for fast legal move generation and position management. It is based on `apery_rust`, uses `shogi_core` types, and implements bitboards with hand-written per-platform SIMD. It has been **dormant since v0.5.0 (October 2023)**. Its main downstream user is `tsumeshogi-solver` (a DFPN mate solver).
- Since 2024, libraries such as [`haitaka`](https://github.com/tofutofu/haitaka) (based on [`cozy-chess`](https://github.com/analog-hors/cozy-chess): magic/Qugiy sliders, `no_std`, zero-allocation callback move generation, mostly-const tables) have moved the design state of the art ahead.
- **Goal**: rebuild yasai's internals from scratch to be one of the fastest shogi move generators in Rust, while staying compatible with `shogi_core`.

## 2. Scope

| | |
|---|---|
| **In scope** | Legal move generation (movegen); position management (do/undo, Zobrist, check/pin information) |
| **Out of scope (non-goals)** | Kifu I/O (SFEN/USI/KIF/CSA), evaluation, search, tsume solvers |

- Fundamental types stay on **`shogi_core` (MIT)** (`Color/Piece/Square/Move/Hand/PartialPosition`), so `tsumeshogi-solver` and others can migrate by swapping the dependency.

## 3. Implementation approach — simple first, then benchmark-driven optimization

Don't pick Qugiy or SIMD up front. First get a **simple, correct implementation** working, then compare candidates **while benchmarking** to decide the optimization strategy.

### Phase 1: correctness-first, naive implementation
- `u128` (or a straightforward multi-word) bitboard; naive occupancy-loop slider attacks.
- Position (do/undo, incremental Zobrist) on `shogi_core` types.
- Correctness is guaranteed by matching the known perft values (see §6).

### Phase 2: benchmark harness
- Measure perft / movegen / do-undo with `criterion`. Wire it into the local `../benchmarks` checkout (see §5) and compare side-by-side against the old yasai, haitaka, and apery_rust.
- Record the naive implementation as the **baseline**.

### Phase 3: optimization candidates, adopted by benchmark comparison
- **Slider attacks**: naive → **Qugiy** / **magic bitboards** / **hand-written SIMD**, chosen by measuring which is fastest (settle on one, or keep several behind feature flags).
- **Move-generation API**: `cozy-chess`/haitaka-style **callback generation** (yield `from + destination bitboard` grouped per piece; zero-allocation, early exit). High enough value to adopt early. `legal_moves() -> ArrayVec` is kept as a compatibility wrapper.
- **const tables** (replacing `once_cell` runtime init), **incremental AttackInfo** updates (the old yasai rebuilds it every move), and **bit layout** (e.g. file-major) are also candidates to measure.

### Planned module layout (implementation phase)

```
src/lib.rs
src/bitboard.rs   # bitboard + slider attacks
src/tables.rs     # attack / between tables (const-ified later)
src/zobrist.rs
src/position.rs   # Position: do/undo, Zobrist, check/pin
src/movegen.rs    # callback generation + legal_moves wrapper
examples/perft.rs
benches/          # movegen / perft
```

### Implementation decision log

Rationale for decisions made during implementation, recorded so they can be revisited deliberately instead of re-litigated.

- **2026-07-23 — own `u128` Bitboard instead of `shogi_core::Bitboard`.** `shogi_core` 0.1.5 does ship a `[u64; 2]` bitboard, and M1 alone could have been built on it. We keep a crate-internal type because the optimization phase (M4) changes exactly this layer: both the representation (`u128` / two words / SIMD lanes) and the *set of operations needed* differ per slider technique (Qugiy wants byte-swapped pairs and subtraction tricks, magic wants multiply/shift on raw words, SIMD wants explicit lanes), and the dormant upstream cannot be extended. Bit order matches `Square::array_index()`, so interop via `shogi_core::Bitboard::to_u128` stays trivial.
- **2026-07-23 — the swap boundary for slider-attack techniques is the attack-function signatures in `tables.rs`** (`lance_attacks` / `bishop_attacks` / `rook_attacks`, ...), not a Bitboard trait. M4 candidates are added as feature-switched backends with identical signatures. A trait over Bitboard was considered and rejected: the required operation set varies per technique (the abstraction would leak or widen every time), and generics would either infect the public API (`Position<B>`) or force dyn dispatch into hot loops.
- **2026-07-23 — benchmark targets: rustshogi replaced by rshogi; Fairy-Stockfish added.** rustshogi's `search_moves` turned out to be pseudo-legal (no self-check filtering, no pawn-drop-mate exclusion — confirmed by source inspection), so it cannot produce comparable legal-perft numbers and no option exists to enable full legality. [rshogi](https://github.com/SH11235/rshogi) (GPL-3.0, active, USI `go perft` built in) replaces it as the modern-Rust reference. Fairy-Stockfish (GPL-3.0, shogi variant with the pawn-drop-mate rule implemented, Stockfish-style `go perft`) is added as an independent-implementation cross-check; being a generalized variant engine, its speed is reference-only. Candidate survey at the time: WCSC/Denryu-sen open-source engines reduce to the YaneuraOu / Apery / dlshogi(=cshogi-core) movegen families already covered; Gikou (dormant since 2020, no perft, x86-oriented), GPS/OSL (dormant), Bonanza (non-OSS license), and nozaq/shogi-rs (no true perft; validate-on-make API) were considered and passed over.
- **2026-07-23 — Zobrist keys from an inline fixed-seed splitmix64**, not a seedable RNG crate (as the old yasai did): no extra runtime dependency, keys stay byte-for-byte reproducible independent of any crate's version (`rand`'s `StdRng`/`SmallRng` explicitly do not guarantee algorithm stability across versions), and it converts trivially to `const fn` when tables are const-ified (M4). Embedding a tiny PRNG for Zobrist init is standard engine practice; splitmix64 itself is a public-domain (CC0) algorithm by Sebastiano Vigna.
- **2026-07-24 — benchmark history is committed to the repo, keyed by append-only criterion ids.** criterion's own baselines (`--save-baseline`) live in `target/`, are machine-local and volatile, so they cannot serve as a durable "improvement history"; instead `scripts/bench_snapshot.py` summarizes each run into `benches/history/*.json` (mean/median/σ per id, plus git rev, rustc, criterion version, CPU, OS, fixture versions) and regenerates the headline table in BENCHMARKS.md. Bench ids are append-only (renames/reuse forbidden; new APIs and new fixture versions get new ids) so a metric's time series always measures the same thing. Baselines remain the tool for local A/B during development.
- **2026-07-24 — internals are exposed to benches via a `bench-internals` feature and a `#[doc(hidden)]` wrapper module** (`src/internals.rs`), wrapping exactly the M4 swap-boundary functions (`lance/bishop/rook_attacks`, `attacks_of`) plus `attackers_to`. Wrapper functions because `pub use` of a `pub(crate)` item is rejected (E0365); a feature (rather than making them `pub`) because the public API must stay exactly the `Position`-level surface — the M4 backends swap behind these signatures and must not become de-facto public API. The `internals` bench target sets `required-features`, so plain `cargo bench` simply skips it.
- **2026-07-24 — movegen/do-undo fixtures come from floodgate real games via our own Rust extractor** (`examples/gen_bench_positions.rs`), committed as versioned, frozen files (`benches/positions/sampled-v1.sfen`, `games-v1.usi`). Real games were chosen over fixed-seed random playouts for two reasons: playout positions skew unrealistic (scattered material, inflated hands), and — decisive — playout *reproduction* depends on `legal_moves()` ordering, so any M3/M4 ordering change would silently change the workload and corrupt the history; committed SFEN/USI text is stable forever, and versioning (v1 → v2 with new bench ids) keeps even a deliberate set change traceable. Licensing: game records are factual data (positions/moves are not copyrightable expression), the extraction pipeline is entirely our own permissive code on `shogi_core` serialization (no GPL tooling), and raw kifu files are never committed. The extractor validates every game move against `legal_moves()`, doubling as a real-game differential test; kifu I/O remains a library non-goal (the CSA parsing lives only in this dev example).
- **2026-07-27 — attack tables const-evaluated (`LazyLock` dropped); measured neutral, kept anyway.** Every table access used to pay a `LazyLock` acquire-load plus a branch, and `BETWEEN` additionally went through a `Box`. Replacing all of them with `const fn`-built `static`s changed nothing measurable: all 13 bench ids moved within 1–2σ (`benches/history/2026-07-27-e101841.json`), because on an already-initialized lock the check is a perfectly predicted branch that LTO hoists out of the hot loops. It is kept because it is a *simplification*, not a complexity-for-speed trade — less machinery, no heap init, no runtime indirection — and because the M4 slider backends need exactly this const table-generation infrastructure for their own (much larger) tables. Recorded here so the neutral result is not rediscovered later. The builders index raw `array_index` arithmetic since `Square::shift`/`Square::all` are not `const fn`; the old `Square`-based builders are retained as test-only reference implementations that every const table is asserted equal to.
- **2026-07-27 — slider attacks: magic bitboards adopted over Qugiy-style arithmetic, decided by measurement (M4).** The M2 baseline showed slider ray-walking was *the* bottleneck: 37.2 ns of an `attackers_to` call's 39.6 ns was `lance/bishop/rook_attacks` stepping square by square through `Square::shift` (which `shogi_core` exports as `extern "C"`, so it is not freely inlinable). `src/sliders/` now holds three interchangeable backends behind the unchanged `tables.rs` signatures — the swap boundary decided on 2026-07-23. Measured per call on the 81-square × 3-position sweep (`internals/*-attacks-*`, Apple M4 Pro): naive bishop 12.4 ns / rook 20.3 ns; qugiy 2.61 / 2.86; **magic 2.43 / 2.37**. Magic wins mostly on the rook, and end-to-end perft agreed (startpos d5 0.212 s vs qugiy's 0.217 s), so magic is the unflagged default and `slider-qugiy` / `slider-naive` remain as override flags. The losing numbers are kept here deliberately: qugiy is within ~10 % of magic while needing no attack tables at all (magic's three per-line tables cost ~486 KiB of `.rodata`), so if cache pressure ever matters more than raw latency — a real search, unlike a perft microbenchmark — the decision is worth re-running rather than re-deriving.
  - **The bake-off ran on the architecture most favourable to qugiy.** Its `o - 2r` needs the board mirrored to get the downward ray, i.e. `u128::reverse_bits`, and aarch64 has a single-instruction bit reverse: `mirror()` compiles to **5 instructions** (two `rbit`, an `extr`, an `lsr`). x86-64 has no bit-reverse instruction, so the same function becomes **44** (12 `and`, 7 `shr`, 2 `bswap`, …) — and `line_attacks` mirrors twice per line, so a bishop pays it four times. Magic's cost (one multiply, two shifts, a table load) is architecture-neutral. So magic winning on Apple Silicon is a *stronger* result than the 10 % gap suggests, and a future re-run on x86-64 should expect the gap to widen — and should measure `pext` (BMI2) as a third backend rather than just re-running these two, with the caveat that `pext` is microcoded and slow on AMD Zen1/Zen2.
  - **Two things are shared by all backends.** The layout is file-major, so a *file* is nine **contiguous** bits: the file direction and lances index a 2.3 KiB table directly, with no multiply and no magic, and this is deliberately identical across backends so the comparison isolates the strided lines (rank, and the two diagonals). And each line has at most 7 blockable squares — the far end of a ray is attacked whether or not it is occupied — so every magic index is 7 bits wide.
  - **Licensing / provenance.** The magic multipliers are brute-forced by our own `examples/gen_magics.rs` from a fixed splitmix64 seed and committed as constants; the attack tables are const-evaluated from them at compile time. Nothing is transcribed from another project (CLAUDE.md's "generate tables with our own generator"). Qugiy is written from the published description of the `o - 2r` technique, not from any engine's source.
  - **2026-07-28 — only the multipliers are generated, and two independent guards cover them.** A multiplier is the output of a search and cannot be derived; a magic's mask and both shifts *can* be, from the line geometry the crate already computes (`sliders::relevant_mask`). So `magic.rs` derives them at compile time and `magics.rs` holds three bare `[u64; 81]` arrays — 1485 lines of `Magic { .. }` literals became 93. Drift between the generated file and the board geometry is then not detected but **impossible**, which is strictly better than the `const` assert it replaces. Two things remain worth checking, and they are different questions with different answers: *does each multiplier work?* is asserted at compile time by `magic::line_table`, which now rejects any two occupancies that share a slot without sharing an attack set (a corrupted number fails the build with `E0080`); *are these our generator's multipliers?* is checked in CI by `cargo run --example gen_magics -- --check`, which re-runs the search (~0.4 s in debug) and diffs. The second guard is needed because the first is satisfiable by numbers we did not produce, and by far more of them than intuition suggests. Validity is a *loose* condition — a magic need not gather the relevant bits perfectly, only avoid mapping two occupancies with different attacks onto one slot, so constructive collisions are allowed and valid multipliers are plentiful. Measured over all 15552 single-bit corruptions of the committed constants (243 magics × 64 bits): **11299, or 72.7 %, remain valid** (rank 56.1 %, diagonals ~81 %) — they compile, build a correct table, and pass every test including deep perft. So "it works" is close to no evidence of "we generated it", and only `--check` distinguishes them. This replaces "a rerun reproduces `magics.rs` byte for byte (verified once, by hand)" with a property CI holds on every run. Note the division of labour: the compile-time check is what stops a *broken* number locally, `--check` only runs in CI.
  - **2026-07-28 — the generator writes the file itself; no `quote`, no `build.rs`.** The documented command used to be `cargo run --example gen_magics > src/sliders/magics.rs`, which cannot work: the shell truncates `magics.rs` before cargo builds the library that `magics.rs` is part of, so regeneration fails to compile. It now writes to `concat!(env!("CARGO_MANIFEST_DIR"), "/src/sliders/magics.rs")` directly, which also makes `--check` possible. Emitting through `proc-macro2`/`quote` was considered and rejected: it needs `syn` + `prettyplease` to produce readable output, `quote!` renders a `u64` in decimal so the hex literals would have to be built as strings anyway, and it would quadruple a dependency tree that is otherwise just `shogi_core` — all to format 243 integers. Shrinking the output was the better answer to the same complaint. Generating at build time via `build.rs` was also rejected: the search is deterministic, so every downstream build would repeat work whose answer never changes, and — decisive, given that licensing is this project's top constraint — the constants would no longer be visible in the tree or in a diff, which is exactly how the "our own generator, not transcribed" claim is demonstrated. `magics.rs` is marked `linguist-generated` in `.gitattributes`, deliberately without `-diff`.
  - **Correctness.** Each backend is asserted equal to `naive` *exhaustively over the relevant occupancy* of every line (all 2^k subsets, k ≤ 7, for all 81 origins), plus 20k random full-board occupancies; `naive` is therefore kept compiled forever as the oracle. All known perft values hold on every backend.
  - Result (`benches/history/2026-07-27-8de28d8.json`): `attackers_to` −76 %, perft startpos-d4 −42 %, matsuri-d3 −35 %, maxmoves-d2 −73 %, movegen evasions −56 %. `do_undo` reads +3.8 %, but it calls no slider code and moves the same amount under every backend (6.82–7.34 µs), i.e. code-layout noise rather than a regression.
- **2026-07-27 — the callback API yields a `MoveSet` per origin, with promotions as a *separate bitboard* (M3).** `Position::generate_moves(|set| -> bool)` hands out one `MoveSet` per origin (or per dropped piece kind) and stops early when the listener returns `true`; `legal_moves()` remains as the allocating wrapper. The shape worth recording is `MoveSet::Normal { promotions, non_promotions }` rather than a single destination bitboard plus a flag: the two sets **overlap** exactly where promotion is optional, so a square in `promotions` alone is a compulsory promotion and one in `non_promotions` alone cannot promote at all. That encodes shogi's forced-promotion rule as set membership, which is what let the per-destination `relative_rank` tests become promotion-zone mask ANDs. `MoveSet::len()` also makes perft's leaf bulk counting free of any `Move` construction. Early exit gave `has_legal_moves()`, so the pawn-drop-mate test stops at the opponent's first reply instead of building a whole move list.
  - **Drop filtering had to move to bitboards in the same change.** Grouping made drops pay twice (build the destination bitboard square by square, then walk it again to expand), a 23 % regression on the drop-heavy matsuri position. Both per-square tests are now set operations: the squares where a dropped piece could never move again are *exactly* the squares that force promotion for a board move, so one `forced_promotion_zone` mask covers pawn, lance and knight; and since only a checking pawn can be a pawn-drop mate, the reverse-lookup trick names the single square that gives check, so the expensive simulation runs at most once per position instead of once per candidate square. This was the drop half of the planned generation rewrite, pulled forward because it undid a regression this change introduced.
  - Result (`benches/history/2026-07-27-d6ac964.json`), against the slider entry: perft matsuri-d3 −20 % via `Vec` and −58 % via the callback, maxmoves-d2 −39 % / −57 %, movegen maxmoves −25 % / −75 %. Cumulatively from the M2 baseline: perft maxmoves-d2 −84 %, matsuri-d3 −49 % (−74 % via callback), startpos-d4 −41 %.
  - **Known and expected**: `movegen/sampled-v1-check-cb` is ~7 % *slower* than its `Vec` twin. In-check positions test every candidate move, and the legality filter now builds a "safe destinations" bitboard that the set expansion then walks again — the same double pass drops just shed. Pin-based legality removes the per-move test entirely, so this is left to that change rather than patched here. *(Resolved by the next entry: the id is now 21 % faster than its `Vec` twin.)*
  - **2026-07-28 — `size_of::<MoveSet>() == 48` is not a cost here, and passing it by reference would change nothing.** The size is real (two `u128` bitboards at align 16, plus tag/`piece`/`from` in 3 of the 14 padding bytes) and invites the usual engine instinct that a move object should be small — yasai kept its move in 64 bits. That instinct applies to move objects that are *stored*: yasai's went into move lists, size × count. A `MoveSet` is never stored. It is built by `emit_normal` and consumed by a listener whose type is known after monomorphization, so the call inlines and SROA shreds the struct before it ever has an address. Verified in the emitted aarch64: in the perft bulk-count path the two bitboards go straight from GPR pairs into `cnt.16b`/`addv.16b`, the only store is the node accumulator, and `piece`/`from`/the tag are dead-code-eliminated outright; `legal_moves` likewise inlines the whole generator (no call to `generate_legal` survives). What *is* stored is `shogi_core::Move` — 3 bytes, smaller than yasai's. Switching to `FnMut(&MoveSet)` was rejected as a no-op, not a trade-off: AAPCS already passes anything over 16 bytes indirectly, and `by_value`/`by_ref` probes of an identical struct compiled to byte-identical code (pointer in `x0`, loads through it). So the size only becomes real if a caller ever *collects* `MoveSet`s — move ordering in a search would, but search is a non-goal (§2), and the crate's own consumer collects `Move`s.
  - **Alternatives to the callback, and why none replaced it.** External iteration (`impl Iterator<Item = MoveSet>`) needs the generator's nested state — evasion vs normal × piece kind × board vs drop — rewritten as an explicit state machine reloaded on every `next()`, and it does *not* buy back the one thing the callback costs: an iterator borrowing `&Position` blocks `do_move` exactly as the closure does. `gen` blocks would give iterator ergonomics with generator code, but remain unstable as of 1.94, and this crate ships on stable. Cross-check: haitaka (MIT, the design cozy-chess brought to shogi) arrives at the same `generate_board_moves(&self, listener: impl FnMut(PieceMoves) -> bool)` shape independently, and uses an iterator only *inside* one set, exactly as `MoveSetIter` does.
  - **2026-07-28 — the listener returns `ControlFlow<()>`, not `bool`, and deliberately not `ControlFlow<B>`.** `true` meaning *stop* is unreadable at the call site, and `generate_moves` returning `()` threw away the one bit the caller most wants: whether the walk finished. `has_legal_moves` had to reconstruct it by capturing a flag; it is now `self.generate_moves(|_| ControlFlow::Break(())).is_break()`. The generic `ControlFlow<B>` — which would let `Break` carry a value, e.g. "the first move satisfying a predicate" — was tried and rejected on inference, not taste: the overwhelmingly common call is a full walk in statement position, whose result is discarded, leaving `B` unconstrained and failing with `E0282` (verified on 1.94). Every counting caller would need a turbofish or a bound annotation to use an API shape it does not want. `ControlFlow<()>` propagates the expected type into the closure, so `ControlFlow::Continue(())` just works; a value-carrying `find_move` can be added later as its own method if a caller ever needs one. The one cost is that `ControlFlow` is `#[must_use]`, so full-walk callers write `let _ = position.generate_moves(..)` — noise the compiler enforces in exchange for making early exit impossible to ignore. Internally the change also removes bookkeeping rather than adding it: `generate_normal`/`generate_drops` now propagate with `?` (`ControlFlow` implements `Try`) instead of returning `bool` and being tested at each call. Re-measured with the same harness as the size probe above: matsuri-d3 6.649 → 6.532 ms, maxmoves-d2 0.310 → 0.301, startpos-d4 6.900 → 6.846 — all within run-to-run noise, as expected for a one-byte enum that inlines away.
  - **Where the remaining headroom actually is: the caller's buffer, not the set.** Because the callback borrows the position, any driver deeper than one ply must collect first — and both `examples/perft.rs` and the `-cb` bench allocate a fresh `Vec` per internal node. Threading one reusable buffer through the recursion instead, measured 2026-07-28 on the same three positions: matsuri-d3 **−7.4 %**, maxmoves-d2 −1.7 %, startpos-d4 −0.5 %. That is a driver change, not an API change (the callback already supports it), and it is deliberately *not* folded into the existing `-cb` ids, which would silently redefine what the committed history measured — so it is added as its own `perft/<pos>-cb-buf/<depth>` ids under the append-only rule (§4), with the buffer allocated outside the measured closure.
    - **The deeper reason the collection exists at all is make-unmake, and haitaka shows the other branch.** `do_move` takes `&mut Position`, which the listener's `&Position` borrow blocks, so nothing deeper than one ply can recurse inside the callback. haitaka instead recurses *inside* its listener, because `board.clone()` conflicts with nothing — it is copy-make. Both position types are **400 bytes**, but the resemblance stops there: haitaka's `Board` is a pure value (a `Copy`-able `ZobristBoard` plus four bitboards), while `Position` owns `states: Vec<State>`, an undo stack, so cloning it would allocate. Adopting the copy-make recursion is therefore a `Position` redesign, not a driver tweak — recorded here as the alternative it is, to be measured against `-cb-buf` rather than assumed better.
  - **Two bitboards vs one bitboard plus a flag, checked against haitaka.** haitaka encodes the same information as `PieceMoves::BoardMoves { to: BitBoard, prom_status: PromotionStatus }` — 32 bytes to our 48. The 16 bytes buy exact O(1) counting: with a single destination set, whether a square yields one move or two is undecided, so haitaka documents `PieceMoves::len` as *not* the move count ("use `moves.into_iter()`") and its `ExactSizeIterator` needs per-piece-kind special cases (`len_for_pawn`, `len_for_knight`, `len_for_lance`) plus a per-destination `PromotionStatus::new` during iteration. `MoveSet::len()` is two popcounts and no branches. Given that the 48 bytes never materialise (above), that is the favourable side of the trade.
- **2026-07-27 — legality is decided per position, not per move.** Generation used to test each candidate with `attackers_to` on an adjusted occupancy, gated by an over-approximation: any of our pieces standing on a king ray counted as possibly pinned whether or not an enemy slider stood behind it, and in check *every* move was tested. Each node now computes the checkers and the genuinely `pinned_pieces` once, and those two bitboards make every non-king move legal by construction — a piece that is not pinned cannot expose its own king, because a pin is exactly the situation where it could; a pinned piece is masked to `line(king, from)`, which still lets it capture the pinner; a single check masks every non-king move to capturing the checker or interposing; a double check leaves only king moves. Only the king still needs a per-destination test, since it is what the test is about, and it is lifted out of `occupied` so it cannot retreat along a checking ray. Snipers are found by asking which enemy sliders would reach the king on an *empty* board and then counting blockers between; a dragon's or horse's one-step sidesteps can never pin (nothing fits between them and the king), so only true slider lines are searched, and lances are searched from the king with *our* colour — the same reverse lookup `attackers_to` uses.
  - Cost: one new 81×81 `LINE` table (~105 KiB, alongside `BETWEEN`) for the pin mask.
  - Result (`benches/history/2026-07-28-abf8345.json`), against the M3 entry: movegen evasions **−72 %** (`-cb` −79 %), `movegen/sampled-v1` −43 % (−62 %), perft startpos-d4 −40 % (−54 %), matsuri-d3 −24 % (−54 %), maxmoves-d2 −49 % (−69 %). Cumulatively from the M2 baseline: perft maxmoves-d2 **−92 %**, startpos-d4 −65 %, matsuri-d3 −62 %, movegen evasions −88 %.
  - Note on measurement: this entry took three full-suite runs. The first had σ = 46 % on `perft/matsuri/3`, the second σ = 18 % on `movegen/sampled-v1`, both because the machine was not quiet; only the third (every id at σ ≤ 4 %) was recorded. Neither noisy mean *looked* wrong — the first landed on the previous entry's value and would have recorded a spurious −0.2 % on matsuri-d3 instead of the real −24 %. A history entry is only worth the machine state during it, and a plausible-looking number is not evidence that the machine was quiet.
  - **The `-cb-buf` ids are recorded for the first time here, and they do not reproduce the ~6–7 % they were credited with.** Against their `-cb` twins in this run: startpos-d4 **+1.5 %**, matsuri-d3 +0.2 %, maxmoves-d2 −0.1 % — i.e. reusing one buffer across the tree is worth nothing measurable, where the ad-hoc measurement recorded in the M3 entry above saw matsuri-d3 −7.4 %. The difference is not explained yet, and the honest reading is that the earlier figure was an ad-hoc probe rather than a suite run, which is exactly why the append-only ids exist. Left as an open question rather than a conclusion: the M3 entry's −7.4 % should be treated as unconfirmed, and the copy-make alternative it was meant to be measured against still has no baseline.

## 4. Benchmarking method

- **Metrics**: (1) **perft** (nodes/sec; also serves as a correctness check); (2) **movegen alone** (ns per position); (3) **do/undo throughput**.
- **Tooling**: `criterion`. Measurement assets for each library live in `../benchmarks` (see §5).
- **Conditions**: `--release` / `lto = "fat"` / `codegen-units = 1`; same machine; with warm-up; a fixed position set; multiple trials with variance recorded; CPU architecture (x86_64 / aarch64) noted.
- **Position set (fixed SFEN)**:
  - initial position
  - **"matsuri" midgame position** (指し手生成祭り, the standard movegen-benchmark position in the Japanese shogi-dev community; used by YaneuraOu's `bench` / `test genmoves`): `l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1`
  - **maximum-legal-move position** (`R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`)
  - check- and mate-adjacent positions — realized (M2) as the in-check subset of the sampled real-game fixture (`movegen/sampled-v1-check`)
- **Fairness** — normalize every library to the same work before comparing:
  - **Full legal move generation** (account for pseudo-legal + validation vs fully-legal differences, callback vs `Vec`/`ArrayVec` API differences, and Python-binding boundary costs).
  - **Pawn-drop-mate (打ち歩詰め) exclusion**: legal movegen must not generate pawn drops that give immediate checkmate. Engines differ here and it is a known cause of perft mismatches (see the TalkChess thread in §6); shunsai excludes them, and comparisons must confirm each library does the same.
  - **Bulk counting**: perft harnesses must agree on leaf handling. haitaka's perft example bulk-counts at depth 1 (cozy-chess style) instead of playing out leaf moves; our cross-library perft comparisons standardize on **leaf bulk counting** (and note it in results).
  - **C++ engines' measurement method**: YaneuraOu is measured with its built-in `test genmoves` command (movegen throughput on the matsuri position) driven over USI; apery similarly via its own commands where available. cshogi ships no perft, so we write a small Python-side perft/movegen harness on its API (its Python-binding overhead is part of what the "practical stack" comparison measures — report it as such).

### Micro-benchmark suite (M2)

The in-repo criterion suite implementing the metrics above — bench targets,
ids, fixtures, the id-stability contract, and the workflow for recording
results — is documented in [BENCHMARKS.md](./BENCHMARKS.md). Key points:

- Bench ids are **append-only** (never renamed/reused); history entries in
  `benches/history/*.json` are keyed by them and committed, so improvements
  can be traced over time. criterion's local baselines are only for ad-hoc
  A/B during development.
- `movegen` is additionally measured over a **versioned fixture of real-game
  positions** (`benches/positions/sampled-v1.sfen`, floodgate games,
  phase-stratified), and `do_undo` replays committed real-game move
  sequences (`games-v1.usi`) — deterministic and independent of move
  ordering, so history comparisons stay valid across M3/M4 changes.
- Internal primitives (the M4 swap-boundary attack functions and
  `attackers_to`) are benchmarked via the `bench-internals` feature.

## 5. Comparison targets

(submodules under `../benchmarks` — a **local-only, unpublished** sibling git repository with no remote. It is not part of the shunsai repository and is never distributed, which is also why GPL projects may live there for benchmarking.)

| Category | Library | Role | License | Pin (as of 2026-07-22) | Upstream |
|---|---|---|---|---|---|
| **Main rival (Rust)** | **haitaka** | Target to **beat directly** on perft/movegen | MIT | v0.3.2+4 (2025-06-12) | dormant ~1 year (crates.io latest is also 0.3.2) |
| **Main rival (Rust)** | **apery_rust** | Target to **beat directly** on perft/movegen | GPL-3.0 | v2.1.0+8 (2024-06-23) | dormant; builds on stable toolchain |
| Self-baseline | **yasai (old 0.5.0)** | Basis for measuring the improvement | GPL-3.0 | 0.5.0+16 (2023-10-14) | dormant (intended: fixed baseline) |
| Reference (Rust) | rshogi | Modern Rust engine cross-check; ships USI `go perft` | GPL-3.0 | v1.3.0 (2026-07-23) | active |
| Reference (variant engine, C++) | Fairy-Stockfish | Independent-implementation cross-check; speed is reference-only (generalized variant movegen) | GPL-3.0 | master (pinned 2026-07-23) | active |
| Reference ceiling (C++) | YaneuraOu | A sense of "how close can we get" | GPL-3.0 | v9.40+ master | active |
| Reference ceiling (C++) | apery | A sense of "how close can we get" | GPL-3.0 | WCSC28+36 (2021-09-21) | dormant |
| Reference (practical) | cshogi | Comparison with a practical Python stack | GPL-3.0 | v1.0.4 (2026-07-18) | active |

**Pinning / update policy**: the `../benchmarks` (local-only) submodules are **pinned** commits recorded in that repository (see its README). Comparison numbers are only meaningful against a recorded pin. Updates are deliberate: bump a submodule intentionally, record the new pin and date, and re-run baselines — never benchmark against a silently-drifted checkout. Dormant upstreams (apery, apery_rust, yasai, haitaka) double as stable, reproducible targets.

Correctness oracle (not a speed target): [`shogi_legality_lite`](https://github.com/rust-shogi-crates/shogi_legality_lite) (MIT, same `shogi_core` types) — see §6.

## 6. Milestones

- **M0 (done)**: name & concept fixed; design documents. Licensing policy decided.
- **M1 (done)**: **simple, correct implementation** (Position + naive movegen) matching known perft values.
- **M2 (done)**: benchmark harness (criterion + `../benchmarks` integration); record the naive implementation as baseline. In-repo suite documented in [BENCHMARKS.md](./BENCHMARKS.md); cross-engine baseline recorded 2026-07-23 in the local benchmarks repository (§5).
- **M3 (done)**: move-generation API refined into the callback form — `Position::generate_moves(|MoveSet| -> bool)`, with `legal_moves()` kept as the allocating wrapper. Measured under the append-only `-cb` bench ids beside the `Vec` ones.
- **M4 (in progress)**: evaluate optimization candidates and **adopt by benchmark comparison**. Slider attacks done — magic adopted over Qugiy-style arithmetic and the naive walk (see the decision log); const tables done (measured neutral, kept as a simplification). Pin-based legality done. Remaining: per-piece-kind bulk generation, and `Position` bitboard layout (a cached gold union, per-colour piece boards).
- **M5**: numerically confirm we **beat** haitaka / apery_rust.
- **M6**: switch `tsumeshogi-solver` to depend on `shunsai`; validate the migration.
- **Later**: publish v0.1.0 on crates.io (`keywords = ["shogi","move-generation","bitboard","game","usi"]`, `categories = ["game-development","algorithms"]`).

### Known perft values (correctness checks for M1/M4)

| Position | depth 1 | depth 2 | depth 3 | depth 4 | depth 5 | depth 6 |
|---|---|---|---|---|---|---|
| Initial position | 30 | 900 | 25470 | 719731 | 19861490 | 547581517 |
| Matsuri position | 207 | 28684 | 4809015 | 516925165 | — | — |
| Max-moves position ※ | 593 | 105677 | 53393368 | 9342410965 | — | — |

※ `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`

- Initial-position values through depth 5–6 are cross-confirmed by multiple independent engines ([shogi-l thread](https://groups.google.com/g/shogi-l/c/U7hmtThbk1k), [TalkChess "Shogi Perft numbers"](https://www.talkchess.com/forum3/viewtopic.php?t=71550)); the max-moves values come from [this Qiita article](https://qiita.com/ak11/items/8bd5f2bb0f5b014143c8) (also used in yasai's tests).
- Matsuri values confirmed 2026-07-23 via the cross-engine perft harness in the local benchmarks repository (§5): nine independent implementations agree (shunsai, haitaka, yasai, apery_rust, rshogi, YaneuraOu, apery, cshogi, Fairy-Stockfish), matching the expected values hardcoded in YaneuraOu's own test suite.
- Max-moves depth 3–4 established 2026-07-23 by 8-engine consensus (same harness; depth 3 is also asserted in yasai's upstream bench). Fairy-Stockfish is excluded there by convention: it *generates* pawn-drop-mate moves and enforces the rule as a game result, so its counts run high on drop-heavy trees (+6369 at depth 3) — a live example of the §4 fairness warning.
- These counts assume **fully legal** generation, including **pawn-drop-mate (打ち歩詰め) exclusion** — a documented source of cross-engine perft disagreement (see the TalkChess thread).

### Correctness verification (M1)

Fixed perft values alone only cover a handful of positions. In addition:

- **Differential testing against `shogi_legality_lite`** (MIT, [rust-shogi-crates](https://github.com/rust-shogi-crates/shogi_legality_lite)): it is slow but straightforward, and it shares `shogi_core` types, so the full legal-move **sets** (not just counts) can be compared directly on arbitrary positions. Use it as a dev-dependency oracle: random playouts from the fixed position set, asserting set-equality of legal moves at every node.
- **Cross-perft against cshogi / YaneuraOu** for positions with no published values: agreement between independent implementations establishes the reference number; record it here once confirmed. *(Done for the matsuri position, 2026-07-23 — see the known-values table above. Max-moves depth 3–4 are consensus-only, recorded in the local benchmarks repository (§5).)*

## 7. Licensing policy (important)

**Chosen license: `MIT OR Apache-2.0` (permissive).**

### Actual licenses of the compared libraries (verified)

| permissive (MIT) | copyleft (GPL-3.0) |
|---|---|
| haitaka, cozy-chess, shogi_core | yasai, apery_rust, apery, YaneuraOu, cshogi, rshogi, Fairy-Stockfish |

### Principle

Copyright protects **expression (the actual code)**, not ideas, algorithms, or techniques.

- **Adopting only a technique** (implementing Qugiy, magic bitboards, etc. yourself from public write-ups) → not bound by the source's license.
- **Copying / line-by-line porting of code** → creates a derivative work and **inherits GPLv3**.
- ⚠️ The old yasai is itself GPLv3 (derived from apery_rust). **Porting yasai's code would make shunsai GPLv3 too**, so to stay permissive we reimplement yasai as well.

### Rule: do not reuse GPL code

To keep the permissive license clean, both human and AI contributors must:

- **May reference**: haitaka / cozy-chess / shogi_core (MIT), plus **public algorithm write-ups** (the Qugiy appeal document, magic-bitboard articles, etc.).
- **Must not copy**: apery / apery_rust / YaneuraOu / cshogi / rshogi / Fairy-Stockfish / the old yasai (GPL-3.0). Understanding the technique is fine; copying the code is not.
- **Generate tables / magic numbers with our own generator** (never paste them from elsewhere).
- When reusing from MIT sources, **retain the copyright notices** (approximate / partial reuse is fine).

### Provenance of AI-generated code

Most of the code will be written by AI (Claude Code). The question is not "was GPL in the training data" but "**does the shipped code substantially reproduce the *expression* of a specific GPL work.**" In an infringement claim the burden is on the **rights holder** (to show substantial similarity + access); we are not required to prove non-copying in advance. In this domain the core is **algorithms (not copyrightable)** plus **tables (which we generate ourselves)**, and boilerplate expression is thinly protected. In practice:

- Keep primary sources restricted to **MIT code + public write-ups** (per the rule above).
- **Run a provenance scan before release** (distinctive-string search / code-similarity scanner to check for verbatim copies).
- Keep a **git history** that shows incremental, original development.

> This section is a general summary of how licensing works, not legal advice. The copyright status of AI training data and generated output is an evolving area. For commercial or high-stakes releases, consult a professional.

## 8. Risks & mitigations

- **`shogi_core` is dormant** (latest release 0.1.5, published 2022-08; no releases since). shunsai builds its public API on it, so:
  - Treat its API as frozen — depend only on what 0.1.5 already provides; do not plan around hoped-for upstream changes.
  - If a blocking bug or missing capability appears, it is MIT: forking (or vendoring the needed types) is an acceptable fallback that preserves the "swap the dependency" migration story for `tsumeshogi-solver`.
- **Perft-convention mismatches can fake regressions or wins.** Pawn-drop-mate handling and leaf bulk-counting differ across libraries (see §4 Fairness, §6). Every cross-library number must state the convention used; never compare numbers produced under different conventions.
- **Benchmark-target drift.** Comparison targets are pinned submodules in `../benchmarks`; results are only comparable against a recorded pin (see §5 pinning policy). Active upstreams (YaneuraOu, cshogi, rshogi, Fairy-Stockfish) will move — bump deliberately and re-baseline.
- **Feasibility of the "beat haitaka / apery_rust" goal** (assessed 2026-07-22): haitaka self-describes as experimental (no engine adoption yet, tuned/tested mainly on Apple M2, no hand-written SIMD); yasai demonstrated that hand-tuned SIMD is competitive with apery_rust. Combining haitaka-style const tables / Qugiy with yasai-style SIMD (reimplemented, per §7) leaves clear headroom, so the goal is considered achievable — but it is validated empirically at M5, not assumed.
