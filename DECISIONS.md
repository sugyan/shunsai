# shunsai — Decisions

Why the code is the way it is, what was tried and rejected, and what is still open.
Recorded so decisions are revisited deliberately instead of re-litigated.

- The **design** is in [DESIGN.md](./DESIGN.md); **how measurement works**, the rules for
  reading a run, and every recorded number are in [BENCHMARKS.md](./BENCHMARKS.md).
- **Figures appear here only when a future decision depends on the number itself.**
  Everything else is in `benches/history/*.json`, keyed by the append-only bench ids —
  that file is the record, this one is the reasoning.
- **If the commit says it, link the commit instead of retelling it.** An adopted change
  already has a primary source: the commit that made it, written against the tree it
  describes, with the figures that were true of it. Retelling it here produces a lossier
  copy that nothing re-checks — the 2026-07-30 entry said "roughly a quarter" where its
  commit says 23.7 %. So an adopted change gets a heading, a link, and then only the parts
  the commit does not hold.
- **What has no primary source is written out properly, and that is most of this file's
  value**: candidates that were *rejected* (there is no commit to find, which is why
  "check it was not already tried" has to land here), coverage holes and open conditions,
  corrections that span two commits, decisions taken with no code to point at, and
  findings that outlived the change that produced them.
- New entries go at the end. If an entry's conclusion is later generalized or superseded,
  compress it rather than appending a correction.

---

## Open questions

The current candidate list. **None of these is measured** — they are sized by disassembly
of the committed bench binary, or bounded by an earlier measurement. The 2026-08-06
decomposition sized `king_danger` and `check_info` on the initial position only; nothing
has sized the rest, so this order is a judgement, not a measurement.

1. **`check_info`'s step-checker half is dead work in 82 % of sampled positions** (33 of 40; 28 of the 31 not in check). Gateable on `their & (king_attacks | knight_attacks)`. This is the one item the 2026-08-06 decomposition ranked, and it ranked it first.
2. **`MoveSet::write_into` is not being inlined.** A standalone symbol at 14–16 call sites, so every materializing listener spills the 48-byte `MoveSet` to the stack and pays a prologue per set. This contradicts the 2026-07-28 finding that the 48 bytes "never materialise" — that was verified on the *counting* listener and holds only there.
3. **`zobrist.rs` is still `LazyLock<Box<Keys>>`.** The 2026-07-27 const-ification covered `tables.rs` alone. Every `board_key` is an acquire load plus a `Box` deref, and the acquire is a barrier that stops LLVM dead-store-eliminating the three Zobrist read-modify-writes in `undo_move` that `self.key = undo.key` immediately overwrites. `splitmix64` is `const fn`-able.
4. **The specialized pawn-drop-mate test** — a pawn checks from an *adjacent* square, so the only legal replies are a king move or a capture of the pawn; blocking and drops are impossible. Worth about a fifth of `maxmoves-d2` (it was four fifths before pin legality and the clone removal).
5. **A `Position`-cached slider union.** `king_danger` makes five `piece_kind_bb` reads per node, grouped into two unions. Removing that construction entirely was bounded at **≤1 %** on 2026-08-07, and the grouping into two unions plus the two zone ANDs it cannot touch puts the real upside below that. The trade is the one the cached gold union lost on — it needs a much cheaper maintenance story before it is worth measuring.
6. **Incremental maintenance of the danger bitboard across `do_move`** ([DESIGN.md](./DESIGN.md) §3's "incremental AttackInfo"). Now has a per-node rebuild to beat rather than the old per-destination test.
7. **`Position::remove_from_hand` is the one mutator not inlined**, while `put_piece`, `remove_piece` and `add_to_hand` all are.
8. **`info.pinned.contains(from)`** costs 10 instructions and a branch per non-king origin, unswitched.
9. **`do_move`/`undo_move` has never been optimized.** Flat in every recorded run since the M2 baseline — ~10 % of perft, and, unlike anything else on this list, called at every node of a search.
10. **Copy-make `Position`** — unmeasured, bounded by do-undo's ~10 %, and it must beat `-cb` rather than `-cb-buf`. haitaka runs this branch: its `Board` is a pure value, so it recurses *inside* its listener. Since 2026-08-11 `Position` is a plain value too, so this is a **driver change rather than a redesign**, and nothing is left blocking a measurement.

**Closed:** the hybrid per-destination king test (2026-08-07 — the danger pass it would replace has lost its slider loop on most nodes, so the crossover moved below one destination).

**Rejected on inspection, so they are not re-derived:** SWAR nifu (the nine-iteration fold is already unrolled and branchless, and the `u128` SWAR form is longer), `count()` → bit tricks (InstCombine already folds `checkers.count() < 2`), an `and_not`/`bic` helper (identical IR), a hand present-kinds bitmask (its premise — that `Hand::count` is `extern "C"` — is false), and a closure-free two-word `SquareIter` (bounded well below what this shape has already cost; see 2026-08-06).

---

## What optimizing this crate has taught

Findings that outlived the change that produced them. Rules about *measuring* are in
[BENCHMARKS.md](./BENCHMARKS.md) — this list is about the code.

- **"Replace a per-item test with one bulk computation" is not unconditionally good.** The king-danger bitboard on its own was a regression on two positions of three: a fixed cost replacing a cost that was paid per candidate square, where the king has two or three candidates.
- **`Bitboard::for_each_square` earns its keep draining a set (30–593 destinations) and loses on a loop of ~20 origins whose body is large.** The walk is not what those loops cost — turning them into closures is. See 2026-08-06 for what is and is not explained about that.
- **`reserve` before a bulk push is a reflex that measured negative here.** `Vec::push` checks capacity per element regardless, so reserving only avoids a reallocation — and `write_into` exists for callers that own a sized buffer, where there is none to avoid.
- **"The source computes this twice" is re-derivable by reading two functions side by side, and can be wrong.** The compiler was already sharing the loads. If a redundancy were real, deleting one of the two constructions could not make anything *slower* — losing is itself the refutation.
- **Perft is a real net but a coverage-dependent one.** It only reports a mistake where some position in the tree actually exercises it, and which holes it has is not guessable: a double check by two sliders is unreachable from every fixture, and a `STEP_ATTACKS` row filed under the wrong kind moves no perft value at any of the three. **The `shogi_legality_lite` differential caught the second. Nothing caught the first** — it needed new fixtures plus assertions that they are still reached.
- **Establish a guard's worth by sabotage.** Every entry below that claims coverage did so by breaking the code and watching which tests fail. An assertion that a rare configuration was *reached* is what stops a fixture list drifting into silent non-coverage.
- **One optimization per change.** Batching destroys the attribution that makes the committed history readable.

---

## Log

### 2026-07-23 — own `u128` Bitboard instead of `shogi_core::Bitboard`

`shogi_core` 0.1.5 does ship a `[u64; 2]` bitboard. We keep a crate-internal type because the optimization phase changes exactly this layer: both the representation and the *set of operations needed* differ per slider technique (Qugiy wants byte-swapped pairs and subtraction tricks, magic wants multiply/shift on raw words, SIMD wants explicit lanes), and the dormant upstream cannot be extended. Bit order matches `Square::array_index()`, so interop via `to_u128` stays trivial.

### 2026-07-23 — the slider swap boundary is the attack-function signatures, not a Bitboard trait

Backends are feature-switched behind identical `lance_attacks` / `bishop_attacks` / `rook_attacks` signatures. **A trait over `Bitboard` was rejected**: the required operation set varies per technique, so the abstraction would leak or widen every time, and generics would either infect the public API (`Position<B>`) or force dyn dispatch into hot loops.

### 2026-07-23 — benchmark targets: rustshogi replaced by rshogi; Fairy-Stockfish added

rustshogi's `search_moves` is pseudo-legal (no self-check filtering, no pawn-drop-mate exclusion — confirmed by source inspection) with no option to enable full legality, so it cannot produce comparable legal-perft numbers. [rshogi](https://github.com/SH11235/rshogi) replaces it; Fairy-Stockfish is added as an independent-implementation cross-check (speed reference-only, being a generalized variant engine).

Surveyed and passed over: the WCSC/Denryu-sen open-source engines reduce to the YaneuraOu / Apery / dlshogi movegen families already covered; Gikou (dormant, no perft), GPS/OSL (dormant), Bonanza (non-OSS license), nozaq/shogi-rs (no true perft).

### 2026-07-23 — Zobrist keys from an inline fixed-seed splitmix64

Not a seedable RNG crate (as the old yasai used): no extra runtime dependency, and keys stay byte-for-byte reproducible independent of any crate's version — `rand`'s `StdRng`/`SmallRng` explicitly do not guarantee algorithm stability across versions. It also converts trivially to `const fn`. splitmix64 is public-domain (CC0), by Sebastiano Vigna.

### 2026-07-24 — benchmark history is committed, keyed by append-only criterion ids

criterion's own baselines live in `target/`, are machine-local and volatile, so they cannot serve as a durable improvement history. `scripts/bench_snapshot.py` summarizes each run into `benches/history/*.json`. **Bench ids are append-only** — renames and reuse forbidden; new APIs and new fixture versions get new ids — so a metric's time series always measures the same thing.

### 2026-07-24 — internals exposed to benches via a `bench-internals` feature

`src/internals.rs` is a `#[doc(hidden)]` wrapper module. Wrapper functions because `pub use` of a `pub(crate)` item is rejected (E0365); a *feature* rather than making them `pub` because the public API must stay the `Position`-level surface — the backends swap behind these signatures and must not become de-facto public API.

### 2026-07-24 — movegen/do-undo fixtures come from floodgate real games

Extracted by our own `examples/gen_bench_positions.rs` into versioned, frozen files.

Real games over fixed-seed random playouts for two reasons: playout positions skew unrealistic (scattered material, inflated hands), and — decisive — playout *reproduction* depends on `legal_moves()` ordering, so any ordering change would silently change the workload and corrupt the history. Committed SFEN/USI text is stable forever.

Licensing: game records are factual data (positions and moves are not copyrightable expression), the pipeline is our own permissive code, and raw kifu files are never committed.

### 2026-07-27 — attack tables const-evaluated (`LazyLock` dropped); measured neutral, kept anyway

Every id moved within noise: on an already-initialized lock the check is a perfectly predicted branch that LTO hoists out of the hot loops. **Kept because it is a simplification, not a complexity-for-speed trade** — less machinery, no heap init, no runtime indirection — and because the slider backends need this const table-generation infrastructure for their own much larger tables. Recorded so the neutral result is not rediscovered later.

The builders index raw `array_index` arithmetic since `Square::shift`/`Square::all` are not `const fn`; the old `Square`-based builders are retained as test-only references that every const table is asserted equal to.

### 2026-07-27 — slider attacks: magic bitboards adopted over Qugiy, decided by measurement — [`efc399a`](https://github.com/sugyan/shunsai/commit/efc399a) (#4)

**The losing numbers are kept deliberately** (`benches/history/2026-07-27-8de28d8.json`). Qugiy is **within ~10 % of magic while needing no attack tables at all**, against magic's ~486 KiB of `.rodata`. If cache pressure ever outweighs raw latency — a real search, unlike a perft microbenchmark — the decision is worth **re-running rather than re-deriving**.

- **The bake-off ran on the architecture most favourable to qugiy.** Its `o - 2r` needs the board mirrored for the downward ray, i.e. `u128::reverse_bits`, and aarch64 has a single-instruction bit reverse where x86-64 has none — and `line_attacks` mirrors twice per line, so a bishop pays it four times. Magic's cost is architecture-neutral. **A future x86-64 re-run should expect the gap to widen, and should measure `pext` (BMI2) as a third backend** — with the caveat that `pext` is microcoded and slow on AMD Zen1/Zen2.
- **Shared by all backends.** The layout is file-major, so a *file* is nine **contiguous** bits: the file direction and lances index a small table directly, no multiply and no magic, identical across backends so the comparison isolates the strided lines. Each line has at most 7 blockable squares — the far end of a ray is attacked whether or not it is occupied — so every magic index is 7 bits wide.
- **Correctness.** Each backend is asserted equal to `naive` *exhaustively over the relevant occupancy* of every line (all 2^k subsets, k ≤ 7, for all 81 origins), plus random full-board occupancies. `naive` is kept as the oracle, compiled under `cfg(any(test, feature = "slider-naive", feature = "bench-internals"))`.

#### 2026-07-28 — only the multipliers are generated, and two independent guards cover them

A multiplier is the output of a search and cannot be derived; a magic's mask and both shifts *can* be, from the line geometry the crate already computes. So `magic.rs` derives them at compile time and `magics.rs` holds three bare `[u64; 81]` arrays. Drift between the generated file and the board geometry is then not detected but **impossible**.

Two questions remain, with different answers:

| question | guard | where |
|---|---|---|
| *does each multiplier work?* | `magic::line_table` rejects any two occupancies sharing a slot without sharing an attack set — a corrupted number fails the build with `E0080` | compile time, always |
| *are these **our generator's** multipliers?* | `cargo run --example gen_magics -- --check` re-runs the search and diffs | CI only |

**The second guard is needed because the first is satisfiable by numbers we did not produce, and by far more of them than intuition suggests.** Validity is a *loose* condition — a magic need not gather the relevant bits perfectly, only avoid mapping two occupancies with different attacks onto one slot, so constructive collisions are allowed. Measured over all 15552 single-bit corruptions of the committed constants (243 magics × 64 bits): **11299, or 72.7 %, remain valid** — they compile, build a correct table, and pass every test including deep perft. **So "it works" is close to no evidence of "we generated it."** Keep both guards.

#### 2026-07-28 — the generator writes the file itself; no `quote`, no `build.rs`

Redirecting the generator's stdout into `magics.rs` cannot work: the shell truncates the file before cargo builds the library it is part of. It writes to `CARGO_MANIFEST_DIR` directly, which is also what makes `--check` possible.

**Rejected: `proc-macro2`/`quote`** — needs `syn` + `prettyplease` for readable output, renders `u64` in decimal so hex literals would be built as strings anyway, and would quadruple a dependency tree that is otherwise just `shogi_core`, all to format 243 integers.

**Rejected: `build.rs`** — the search is deterministic, so every downstream build would repeat work whose answer never changes; and, decisive given that licensing is this project's top constraint, the constants would no longer be visible in the tree or in a diff, which is exactly how the "our own generator, not transcribed" claim is demonstrated.

### 2026-07-27 — the callback API yields a `MoveSet` per origin, promotions as a separate bitboard (M3) — [`86168d2`](https://github.com/sugyan/shunsai/commit/86168d2) (#5)

The shape worth recording is `MoveSet::Normal { promotions, non_promotions }` rather than one destination bitboard plus a flag: **the two sets overlap exactly where promotion is optional**, so a square in `promotions` alone is a compulsory promotion and one in `non_promotions` alone cannot promote at all. That encodes shogi's forced-promotion rule as set membership, which is what let the per-destination rank tests become mask ANDs. `MoveSet::len()` is then two popcounts, making perft's leaf bulk counting free of any `Move` construction.

- **Drop filtering had to move to bitboards in the same change.** Grouping made drops pay twice (build the destination bitboard square by square, then walk it again), a regression on the drop-heavy matsuri position. Both per-square tests are now set operations: the squares where a dropped piece could never move again are *exactly* the squares that force promotion for a board move, so one `forced_promotion_zone` mask covers pawn, lance and knight; and since only a checking pawn can be a pawn-drop mate, the reverse-lookup trick names the single square that gives check, so the expensive simulation runs at most once per position.
- **Rejected: external iteration (`impl Iterator<Item = MoveSet>`).** Needs the generator's nested state rewritten as an explicit state machine reloaded on every `next()`, and it does *not* buy back the one thing the callback costs: an iterator borrowing `&Position` blocks `do_move` exactly as the closure does. `gen` blocks would give iterator ergonomics with generator code but remain unstable as of 1.94. Cross-check: haitaka arrives at the same listener shape independently.
- **Two bitboards vs one bitboard plus a flag, checked against haitaka.** haitaka encodes the same information in 32 bytes to our 48. The 16 bytes buy exact O(1) counting: with a single destination set, whether a square yields one or two moves is undecided, so haitaka documents `PieceMoves::len` as *not* the move count and its `ExactSizeIterator` needs per-piece-kind special cases plus a per-destination `PromotionStatus::new`. The one measurement of the two layouts on the materializing path is 2026-07-31's cross-engine run: shunsai's expansion is cheaper per move on matsuri and maxmoves and dearer on startpos. ⚠️ Not settled — see Open question 2.

#### 2026-07-28 — `size_of::<MoveSet>() == 48` is not a cost for a counting listener

The size is real (two `u128` bitboards at align 16, plus tag/`piece`/`from` in the padding) and invites the usual engine instinct that a move object should be small. **That instinct applies to move objects that are *stored*** — yasai's went into move lists, size × count. Verified in the emitted aarch64 for the bulk-count path: the call inlines, SROA shreds the struct before it ever has an address, and `piece`/`from`/the tag are dead-code-eliminated. What *is* stored is `shogi_core::Move` — 3 bytes.

**Rejected as a no-op, not a trade-off: `FnMut(&MoveSet)`.** AAPCS already passes anything over 16 bytes indirectly, and `by_value`/`by_ref` probes of an identical struct compiled to byte-identical code.

⚠️ **Both halves have since been qualified.** The premise "nothing collects `MoveSet`s" was withdrawn on 2026-07-29 (a search ordering moves would), and the inlining claim holds for the counting listener only — the materializing one spills (Open question 2). **Do not shrink it speculatively** — that optimizes for a caller that does not exist yet and costs the `len()`-as-two-popcounts the 48 bytes buy.

#### 2026-07-28 — the listener returns `ControlFlow<()>`, not `bool`, and deliberately not `ControlFlow<B>`

`true` meaning *stop* is unreadable at the call site, and returning `()` threw away the one bit the caller most wants: whether the walk finished.

**`ControlFlow<B>` was tried and rejected on inference, not taste.** The overwhelmingly common call is a full walk in statement position whose result is discarded, leaving `B` unconstrained and failing with `E0282` (verified on 1.94); every counting caller would need a turbofish to use an API shape it does not want. A value-carrying `find_move` can be added later. The cost is that `ControlFlow` is `#[must_use]`, so full-walk callers write `let _ = ...` — noise the compiler enforces in exchange for making early exit impossible to ignore.

### 2026-07-27 — legality is decided per position, not per move — [`dc3c36e`](https://github.com/sugyan/shunsai/commit/dc3c36e) (#6)

### 2026-07-27 — two generation candidates measured and *rejected*

Both were on the M4 candidate list; both made things worse. Recorded so they are not re-derived.

- **A cached gold union in `Position`** (the five gold-moving kinds OR-ed once and maintained by `put_piece`/`remove_piece`). It did what it promised — `internals/attackers-to` improved — but maintaining it cost **`do_undo` +7 %** and the recommended perft path came out slower. A branchless variant recovered the perft loss but left `do_undo` down. **The union is four ORs of already-hot bitboards; paying for it on every piece placement to save it on every attack query is the wrong side of that trade.** This is the reference trade for any future "cache it in `Position`" proposal.
- **Per-piece-kind generation loops** (walk the 13 non-king kind bitboards so the kind is a loop constant). Uniformly worse on every id. A typical position spreads ~20 pieces over ~8 kinds, so the fixed cost of walking 13 mostly-empty boards — and the loss of the single dense pass over `our` — outweighs what it saves.

### 2026-07-29 — the reusable perft buffer is worth nothing; a −7.4 % claim withdrawn

The M3 entry had credited threading one reusable `Vec` through the perft recursion with a matsuri-d3 gain. It was an artifact of single-shot timing; the committed `-cb-buf` ids are right.

- **The effect has a hard ceiling far below the claim, and the durable form of that ceiling is a count, not a percentage.** A counting global allocator over the `-cb` driver reports **931** allocations for startpos-d4, **208** plus ~200 growth reallocs for matsuri-d3, and **1** for maxmoves-d2 — leaves are bulk-counted at depth 1, so only internal nodes ever collect. Multiplied by the measured cost of an allocation and of a growth realloc, that is a fraction of a percent of runtime, and maxmoves-d2's two drivers differ by a single `Vec`. **Quote the counts**: the share of runtime they buy rises as the crate gets faster.
- **The claimed gains ran *inverse* to allocation density** — startpos allocates far more often per unit of runtime than matsuri yet was credited with much less. **Getting the ordering backwards is the signature of measuring something that is not allocation.**
- **The residual is real and points the other way.** *After pin legality*, `-cb-buf` is consistently a hair slower on startpos-d4: `while i < buf.len()` reloads the length every iteration — the recursive call takes `&mut Vec<Move>`, so it cannot be hoisted — and `buf[i]` bounds-checks, where `-cb`'s `for mv in moves` is a pointer bump. That cost is per *move* against a saving that is per *allocation*, and pin legality shrank the work it was hiding behind, which is why the sign flips. On the M3 tree the sign is the other way.
- **Incidental, and worth more than the buffer ever was**: the driver's `Vec` is not where this crate allocated. maxmoves-d2 performed hundreds of allocations per perft(2) *inside* the library, because `is_pawn_drop_mate` cloned the position. That is the next entry.

The reasoning for giving the buffer its own append-only ids, rather than folding it into `-cb`, is what caught the error and stands.

### 2026-07-29 — the pawn-drop-mate simulation no longer clones; generation allocates nothing — [`4a17c50`](https://github.com/sugyan/shunsai/commit/4a17c50) (#8)

- **Rejected: `do_move`/`undo_move` on `&mut Position`.** It would avoid the copy too, but only by making `generate_moves`, `legal_moves` and `has_legal_moves` take `&mut self` — and the callback contract is shaped *around* the listener's shared borrow. Generation demanding unique access would stop callers holding the position immutably while generating.
- **This reversed the follow-up.** Before pin legality, the mate test was most of `maxmoves-d2` and the allocation a small part of it, which made the specialized mate test look like the obvious next change. Pin legality made the `has_legal_moves()` walk inside the test far cheaper without touching the clone, so the allocation became the *majority* of what was left. The specialized test is **demoted, not dropped** (Open question 4).
- **Guards.** `position_after_drop_matches_do_move` holds `with_drop` to clone-and-`do_move` over every hand piece on every square it may legally occupy; `Position`'s `PartialEq` compares every field, so a step `do_move`'s drop path gains and `with_drop`'s tail does not fails it. By sabotage, the tests that cover this function at all are the differential oracle, `rules::pawn_drop_mate_is_excluded`, and `perft::max_moves_position_deep` — **none of the default-depth perft values do**, and that deep value is `#[ignore]`d, so CI's `--ignored` step is the only perft guard on pawn-drop-mate exclusion.

### 2026-07-29 — the consumer is a search engine, which re-opens two closed decisions

Every optimization so far was judged against *perft*, and perft is the measuring instrument, not the customer. The engine is a separate crate, so the non-goals stand unchanged — **what changes is the standard of evidence. "Free" now has to mean free under a search.** Neither decision below is reversed; what is withdrawn is the reason to stop looking.

- **`MoveSet`'s 48 bytes are no longer settled** — that entry dismissed the size *because* nothing stores a `MoveSet`, and named the one caller that would change that: "move ordering in a search would, but search is a non-goal". That premise is gone. **Settled again 2026-08-11**: the fields are public and stay so, and the size follows from the design rather than from their visibility.
- **magic-vs-qugiy should be re-run under a search.** The slider entry already flagged the condition; a search sharing cache with a transposition table is precisely the case a perft microbenchmark cannot create. Both backends stay compiled, so this is a measurement, not a rewrite.
- **What a search needs that perft never exercises**, hence unmeasured and mostly unbuilt: repetition detection (千日手, including the perpetual-check distinction, which needs history rather than a position), a static exchange evaluation or capture-ordering hook, and **exposing** check / pin / attacked information rather than recomputing it per node. `king_danger` produces exactly the attacked-squares set a search wants, so the largest remaining perft hot spot and the first search-facing API need are the same piece of work.
- **Make-unmake is already the search-friendly shape.** `do_move`/`undo_move` is ~10 % of perft runtime, so the copy-make alternative is bounded by that. It also cost an API break at the time; 2026-08-11 spent that break for other reasons, so what remains is the bound.

### 2026-07-30 — the king's destinations are decided by one danger bitboard — [`05b59e4`](https://github.com/sugyan/shunsai/commit/05b59e4) (#10)

- **On its own the bitboard is a regression on two positions of three, and that is the useful result.** One pass over ~20 enemy pieces is a **fixed** cost where the test it replaces was paid **per candidate square** — and the initial position's king has three, matsuri's two.
- **The fix is to filter the loop, not to abandon the bitboard.** Only attacks landing on the king's eight neighbours can survive the mask, and a piece with bounded reach whose target is one step from the king provably cannot bear on a destination from outside a **two-file by three-rank box**. That turned both regressions into gains. *(The slider half of this filter was wrong until 2026-08-07 — see that entry.)*
- **The cost is that `king_danger` returns a partial attack map**, valid only next to the king — and this is the function a search would take its full attacked-squares set from. Dropping the filter is one line but costs what the filter buys. **The condition: a search that wants the full attack map must re-measure this filter, not assume it.**

- **Guards, by sabotage.** Zeroing `danger` is caught by the differential oracle, all three default-depth perft values, and three `rules` tests. Shrinking the box is caught by `step_attacker_zone_covers_every_step_piece` and by `rules::distant_knight_still_covers_a_king_escape`, which places a knight at the exact corner. Two tests cover the capture argument **in both directions** — the king may not take a defended pawn, and it **must** be allowed to take an undefended one. The second is what a merely-conservative danger bitboard would break, and zeroing `danger` does not fail it.
- ⚠️ **One sniper cannot tell accumulation from assignment, and the fixtures did not cover it.** Turning `checkers |= single(sniper)` into `=` passed **the entire suite, including the differential oracle and the `#[ignore]`d deep perft values**: a double check by *two sliders* is reachable from no fixture and from none of the three deep perft trees. Two fixtures now close it, and the test **asserts it reached each configuration**, so removing a fixture fails loudly instead of silently reducing coverage.

### 2026-07-31 — the consumer's roadmap, and the re-measurements it schedules here

The engine is NNUE + αβ first, DL/MCTS deferred behind explicit conditions. That plan lives in `rinsai`'s own design document; only the schedule it imposes **here** belongs in this log:

- **E0** (USI shell, material eval, αβ + TT + qsearch) **requires no shunsai change, deliberately.** SFEN stays external, and repetition resolves engine-side — 千日手 needs game history, so the engine stacks `(key(), Hand, in_check())` per ply. E0 shipping against a frozen shunsai is the layering's first contact with a real consumer.
- **E1** (ordering, null move, LMR, SEE) **is when the API additions land**, each carrying its recorded measurement: `attackers_to` and public `Bitboard` iteration (SEE's prerequisite); staged generation, which is the caller Open question 2 is waiting for; `gives_check`; null move; exposed `checkers`/`pinned`. **The first TT-backed search bench is the shared-cache condition the magic-vs-qugiy entry named** — that re-run happens here, not in perft.
- **E4** runs the x86-64 batch this log prescribed: magic vs qugiy vs `pext` under TT pressure. **The `king_danger` full-attack-map condition may never fire from evaluation at all** — NNUE consumes piece placement, not an attack map, which leaves move ordering as its only plausible claimant.
- **The engine repo adopts §7 verbatim plus one sharpening: run-vs-link.** GPL engines and servers may be *run* as separate processes, because nothing GPL is linked or distributed. Reading-to-reimplement stays allowed, porting stays forbidden.

### 2026-08-04 — the consumer is named `rinsai`, and v0.1.0 becomes a prerequisite of E0

The engine's name, repository layout and full roadmap live in `rinsai`'s own design document — only what imposes a schedule **here** belongs here. Two things do.

- **shunsai is published to crates.io, and `rinsai` depends on released versions rather than a git pin.** The plan had assumed a git dependency with a rev pin plus a local `[patch]` override, on the assumption that a release per API addition was a cost to avoid. It is not: shunsai is a library with third-party value and belongs on crates.io regardless. **A consequence that constrains this crate: an API addition E1 wants is a version of shunsai, so it carries semver.**
- **v0.1.0 therefore moves from "Later" to a prerequisite of E0.** E0 still requires no API change, but it needs something to build against.

### 2026-08-03/04 — `MoveSet::write_into` decides drop-versus-board once per set — [`aa44f8b`](https://github.com/sugyan/shunsai/commit/aa44f8b) (#13)

- **`Bitboard::for_each_square` is the other half, and it is where the `u128` choice finally cost something.** `pop()` walks the `u128` directly: on aarch64 both `u128::trailing_zeros` and `x & (x - 1)` cost roughly twice their 64-bit counterparts. The 81 bits are contiguous, so the bulk walk takes the low word to exhaustion and then the high word, which is usually empty. It also builds squares unchecked where `pop` goes through the `extern "C"` `Square::from_u8`, which re-checks a range this type's invariant already guarantees.
- **The gain tracks moves per `MoveSet`.** Everything removed is per-move, so a position whose sets are large collects most of it and one whose sets hold one or two moves collects almost none — startpos gains least, the in-check sweep nothing, because evasions are restricted to capturing the checker or interposing. **That also resolves an anomaly recorded as a refutation**: maxmoves cost *more* per move than matsuri only because the iterator's per-item drop dispatch fell hardest on the drop-heavy position. With it gone the ordering is monotone in set size again.
- **The theory that motivated the change is still wrong.** The wasted-promotion-pop argument predicted the initial position would gain *most*, since nothing can promote there so all 30 moves pay the failed pop. **startpos gains least.** The change succeeded for a different reason than the one that suggested it.
- **`out.reserve(self.len())` was in the first version and made small-set positions worse** — startpos slightly, the in-check sweep about twice as much. See *What optimizing this crate has taught*.
- ⚠️ **`perft/matsuri-cb/3` is not comparable across this boundary.** It reads high with huge σ under whole-suite conditions but reproduces tightly in isolation at *every* revision tested, and its non-allocating twin is flat throughout. The id is measuring the allocator, not generation. **Which reading is the anomaly is undecided.** Treat that one id's series as broken across 2026-08-03/04.

### 2026-08-06 — the per-*set* path: piece-indexed dispatch and per-origin promotion — [`bdf2836`](https://github.com/sugyan/shunsai/commit/bdf2836) (#15)

**The decomposition came first, and it inverted the plan.** `movegen/<pos>-cb` measures one generation call on one position, so stubbing a component does **not** change the shape of any tree. At startpos both `check_info` and `king_danger` are output-preserving — no checks, no pins, all three king candidates already safe — so those cells are exact rather than indicative.

Shares of one generation call **on the initial position**, 19 non-king origins:

| component | share |
|---|---|
| `attacks_of` + the target/check/pin masks | **41 %** |
| `emit_normal` + the listener | 21 % |
| `king_danger` | 20 % |
| `check_info` | 15 % |
| the origin bitboard walk + the mailbox load | 12 % |

The three origin-loop rows are additive and account for the whole loop; the two outer components are separately stubbed and over-count against the baseline by a few percent, which is why the column sums above 100 %. At startpos 15 of the 19 origins are step pieces, so after charging the four sliders the **step path was costing far more per origin than one indexed table load should** — which made the dispatch, not the walk, the thing to attack. The plan had ranked the walk first.

- **Adopted: `attacks_of` serves the nine non-slider kinds from one piece-indexed row.** It was a ten-arm `match` on `PieceKind`, which LLVM lowers to a jump table — an indirect branch whose target is whatever piece the mailbox yielded, and therefore poorly predicted. Costs 40.5 KiB of `.rodata`, additive to the per-kind tables the reverse-lookup scans still want, which matters only to the deferred magic-versus-qugiy re-run under cache pressure. **This is not the per-piece-kind generation loop rejected on 2026-07-27** — that walked 13 mostly-empty bitboards and lost the single dense pass over `our`; the dense pass is untouched here.
- **Adopted: promotion is decided per origin, not per set.** **Promotion is legal when a move starts *or* ends in the zone, which reads as two conditions on the destinations but is one fact about the origin**, so `PROMOTION_MASK[colour][from]` bakes it and the branch disappears.
- **Adopted: `legal_moves()` was asking for 128 moves' worth of `Vec` when the maximum is 593**, so drop-heavy positions grew three times inside the call. **This is not the `out.reserve` rejected above** — that was a per-*set* cost buying nothing; this is one sizing per *call* that removes real reallocations.
- **Rejected, and it is the candidate this work was planned around: moving the origin loops onto `Bitboard::for_each_square`.** Splitting it three ways, **on the initial position**: the delegation itself is neutral, `generate_normal`'s two loops cost about +20 %, and `king_danger`/`check_info`'s scans about +14 %. Both shrink on the other two positions, and the scan conversion is roughly neutral on maxmoves. The `generate_normal` conversion costs the *materializing* path as much as the counting one, which is what one would expect if the extra closure layer stops the listener inlining. **The scan conversion touches no listener, and its cost is not explained** — the accumulators becoming `&mut` captures is the obvious suspect and is untested. **Recorded as measured, not as understood.**
- **Guards, by sabotage.** `attacks_of_matches_the_per_kind_tables` holds the folded dispatch to the `match` it replaced; `piece_index_matches_as_u8` pins the index arithmetic against `Piece::as_u8`, an upstream representation this crate now reads through a mask. **One sabotage is caught by the differential alone**: a row filed under the wrong kind (`ProSilver` given silver's table) moves **no perft value at any of the three fixtures**.

### 2026-08-07 — `check_info`'s sniper scan comes out of ray tables; sharing the slider union is a loss — [`ffd4056`](https://github.com/sugyan/shunsai/commit/ffd4056) (#16)

- **Rejected: carrying the enemy-slider union from `check_info` to `king_danger`.** Both build the same five-kind union, so passing it instead (a third `CheckInfo` field) looked free. It measured **eight of ten ids worse**. **Losing is itself the refutation** — if the union were genuinely being built twice, deleting one construction could not make anything slower. It was not: the five loads read memory nothing writes between the two call sites, and the compiler was already sharing them. What the change did was grow `CheckInfo` from 32 to 48 bytes and thread it deeper. **This bounds Open question 5 at ≤1 %.**
- **The prediction was several times too large**, and the reason generalizes: a per-call figure from the `internals/*` sweep over-states what that call costs in a hot loop. The sweep walks 81 origins where `check_info` asks about **one** square every node, so its table lines are already warm — the obvious explanation, **not verified**.
- **Guards, by sabotage.** `empty_board_rays_match_the_naive_backend` holds all three tables to `sliders::naive` — **to `naive` rather than the live backend, so the guard does not rest on the thing `sliders/tests.rs` is itself checking**. Giving `ROOK_RAYS` the diagonal steps is caught broadly, but **not** by the initial-position or matsuri default-depth perft values.
- ⚠️ **A value consumed only by `king_danger` can hide from every perft value.** Dropping `ProRook` from the carried union — so a dragon's sliding attacks never enter the danger set — was caught by the differential **alone**, with all three deep perft values holding. The reason is coverage, not structure (next entry). The fixture list accordingly gained its only position with promoted sliders.

### 2026-08-07 — `king_danger`'s sliders are filtered by where they could bear on the king — [`1a41320`](https://github.com/sugyan/shunsai/commit/1a41320) (#17)

- ⚠️ **The step term applies to every enemy piece, not only the non-sliders, and that is load-bearing.** A horse's orthogonal sidesteps and a dragon's diagonal ones lie on *neither* piece's rays, so the two slider zones miss them. **Deleting the term as redundant is tidying that would silently break this.** `slider_attacker_zones_cover_every_slider` asserts the pair covers every slider attack on every king neighbour, exhaustively, plus the monotonicity of slider attacks in occupancy — which is what makes an empty-board sweep sufficient, and which nothing else in the suite pinned.
- **Correction: a `king_danger` under-report is *not* structurally invisible to perft.** The reasoning had been that because `king_danger` only ever subtracts, an omission produces an illegal king move and no change in node count. **That does not follow** — an extra generated move *is* an extra node, and `perft(1)` counts generated moves directly. Three sabotages of this filter are each rejected by **all three deep perft values**. What the dragon omission above demonstrated is narrower: perft only reports the mistake where some position in the tree has the omitted piece bearing on a king destination. **Perft is a real net here and a coverage-dependent one.**
- **The `maxmoves` single-position ids moved for reasons that are not this mechanism.** That root has **no enemy slider at all**, so the filter cannot save an iteration and strictly adds two loads and two ANDs; `movegen/maxmoves-cb` improved anyway and `movegen/maxmoves-buf` regressed, both from code layout. **Recorded as measured, not as understood.** `perft/maxmoves-cb/2` *is* mechanism: at depth 2 the side to move changes and those five sliders become the enemy's.
- **The control is imperfect, and the honest reason is this change rather than the machine.** `internals/bishop-attacks-magic`, which this change cannot reach, moved — the suspect being that two new `static` tables shift `.rodata`, and the `internals/*` ids sweep the magic backend's ~486 KiB. The signal is several times the control drift and opposite in sign, which is what makes the run recordable; **the in-check subset is the thinnest cell and the first to re-measure if this read is ever doubted.**

### 2026-08-11 — v0.1.0's packaging: what ships, what the MSRV is, and what declaring it costs — [`f41ccee`](https://github.com/sugyan/shunsai/commit/f41ccee) (#25)

Three of DESIGN.md §6's release prerequisites, decided together because one CI step checks all three.

- **The tarball is the library.** `include` ships `src`, `README.md`, `CHANGELOG.md` and the two licences — 21 files. The three documents, `tests/`, `examples/`, `scripts/` and the committed `benches/history/*.json` snapshots are for people working in this repository and are the bulk of the tree. `include` names the `src` **directory**, not `src/**/*.rs`: the packaging guard only compiles the default-feature lib, so a future generated table or `include_str!` asset dropped by an extension-scoped glob would not be noticed until a consumer enabled the feature that reads it.
- **`examples/gen_magics.rs` deliberately does not ship, and the shipped code no longer points at it.** The provenance guard for the multipliers is the generator plus CI's `--check` (2026-07-28), and both are repository-level. `magics.rs` had been instructing its reader to run a command the tarball cannot execute, and `magic.rs` linked `../../examples/gen_magics.rs`, which 404s on docs.rs; both now name the file without implying it is present. **Shipping the generator (~9 KiB) stays open** if a downstream ever needs the claim checkable from the artifact alone. Note the class: a relative-URL link is invisible to `broken_intra_doc_links`, so `-D warnings` cannot catch the next one — `grep -rn '](\.\./' src/` is the check.
- **MSRV is 1.88, and the edition does not set it.** Bracketed rather than reasoned about: 1.85 and 1.87 both fail with `error[E0658]: 'let' expressions in this position are unstable`; 1.88 builds. The single cause is the let-chain in `movegen.rs`'s pawn-drop-mate filter — edition 2024 alone would have suggested 1.85, **so the floor is one rewrite away from 1.85 if a consumer ever needs it.**
- ⚠️ **Declaring `rust-version` is not free: it switches the resolver to MSRV-aware mode.** Every job, and every contributor without a lockfile, silently drops from `Locking … to latest compatible versions` to `… to latest Rust 1.88 compatible versions`. Three costs, none of them the one intended: the stable jobs stop testing the graph a consumer resolves, criterion runs before and after the change stop being comparable, and the `msrv` job can never fail over a dependency that raises **its** floor — it selects the older release instead. CI sets `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=allow` to undo it. **`Cargo.lock` is untracked, so nothing else would have recorded the change.**
- **The backend flags are made exclusive, so `--all-features` is a build error and CI no longer uses it anywhere.** Cargo features are additive and cannot express "pick one"; the priority order standing in for it let `slider-naive` win, so every tool handed `--all-features` silently inspected the oracle — which is how clippy came to lint only the oracle's configuration and how docs.rs would have documented it. Preparing a feature to be *exclusive* and then asking for *all* of them is incoherent, and the several comments warning about it were handling a self-inflicted wound. A `compile_error!` replaces the order. CI names `--features bench-internals`, which compiles all three backends with `magic` live, and adds a `cargo check --lib` per override flag because each selecting arm is a single `use … as active` that nothing else compiles. `qugiy.rs` and `naive.rs` lost `#![allow(dead_code)]` in the process and are dead-code-checked for the first time.
- **Rejected: gating `magic` out of the override builds too.** Symmetry would say it should be, but its geometry helpers (`LineKind`, `relevant_mask`, the multipliers) live in `sliders.rs` and `magics.rs`, so removing `magic` under `slider-naive` / `slider-qugiy` leaves those dead — trading one suppressed warning for four. `magic` stays unconditional as the unflagged default, and `magic.rs` keeps the `allow` with that as its stated reason.
- ⚠️ **Cost of `compile_error!`, accepted: it is unfriendly under feature unification.** Two crates in one dependency graph each enabling a different flag break the build with no fix available locally — which is the usual reason the priority-order pattern exists. Both flags are documented maintainer-only knobs that no legitimate consumer enables, and **v0.1.0 freezes them into the published surface**, so the guard belongs before the release rather than after.
- Making the default-features doc run possible needed `sliders.rs`'s intra-doc link to `naive` dropped: the module is cfg-gated, so the link cannot resolve in the configuration docs.rs renders.
- **Rejected: `[package.metadata.docs.rs] all-features = false`.** Written first, then removed — it is docs.rs's default, and the backend selection is `pub(crate)` while docs.rs does not pass `--document-private-items`, so docs.rs could not have rendered the oracle either way. The hazard it claimed to close was never live.
- **Guards, and their edges.** `cargo publish --dry-run` on every PR fails on an `include` that drops a source the default-feature **lib** needs — and only that: the `warning: ignoring … is not included in the published package` lines for the dropped examples, tests and bench are warnings cargo exits 0 on. The `msrv` job runs `cargo check --lib --features bench-internals` — not `--all-features`, which making the backend flags exclusive turned into a build error — because `bench-internals` is what reaches `naive` and `qugiy`, the lib modules gated on `any(test, …)` that a default 1.88 build never compiles; dev-dependencies stay out because they do not constrain a consumer. **`main`'s ruleset requires `check` and `msrv`, with no bypass actors**, and the dry run is a step *of* `check`, so both guards block a merge. `perft-deep` is **not** required — so the `--ignored` step that the pawn-drop-mate simulation entry (2026-07-29, [`4a17c50`](https://github.com/sugyan/shunsai/commit/4a17c50)) names as the only *perft* guard on pawn-drop-mate exclusion can go red without stopping anything. What still blocks in that case is `rules::pawn_drop_mate_is_excluded` and the differential oracle, both inside `check`.

### 2026-08-11 — `do_move` returns an `Undo`, so `Position` owns nothing on the heap — [`e039cd7`](https://github.com/sugyan/shunsai/commit/e039cd7) (#22)

- **`do_move` is deliberately not `#[must_use]`.** Replaying a game forward and never taking it back is a first-class use — `tests/differential.rs`, `tests/rules.rs` and `examples/gen_bench_positions.rs` all do it — and marking it would put `let _ =` on legitimate code.
- **`Undo`'s fields stay private**, so the representation is free to change; nothing outside needs them.
- **The mispairing that `undo_move` used to catch is now the caller's to avoid.** `states.pop()` panicked on an empty stack; passing the wrong `Undo` is silent. The contract is documented on `do_move`, and the shape that makes it hard to get wrong — an `Undo` bound in the same scope as the `do_move` that produced it — is the one every driver here uses.
- **`with_drop` survives** rather than folding into clone-and-`do_move`, which it is now nearly identical to. It touches the pawn-drop-mate hot path, so deleting it wants its own measurement rather than a ride on this one.
- **Unblocks copy-make (open question 10).** It was described as needing "a `Position` redesign, not a driver tweak"; it is now a driver tweak.
- **It costs `do_undo/games-v1` about 4 %, and that is the whole of the durable cost.** Measured 2026-08-12: a full-suite paired A/B, then — on a quiet machine, five order-alternating passes per binary against the ids in question — **every head reading above every base reading**, controls at −1.5..−0.05 %. The `Undo` now round-trips through the caller (returned by value, stored, reloaded, passed back) where it used to be written once into the position's own `Vec`. Small, explained, and the price of the shape; open question 9 is where do/undo gets optimized, and it now has a reason to be taken.
- **The `movegen/*-buf` swings are code layout, not mechanism, and the test that separates them is in the suite already.** The three `perft/*-cb-buf` walks thread the identical buffer through a whole tree and are flat (+0.7..+2.4 %, inside the control band). The five single-call `movegen/*-buf` ids, running that same code once per iteration, scatter **−2.0 % to +19.0 % — in both directions**. A mechanism pushes one way. `movegen/maxmoves-buf` is the id 2026-08-07 already named the largest single mover in either direction; it is this crate's most layout-volatile measurement and should be read as one.
- **Rejected: writing `with_drop`'s copy out again instead of `self.clone()`.** The obvious suspect, since the clone replaced an explicit field copy that inlined. Built and measured three ways against base: it moves nothing (`maxmoves-buf` +19.3 % vs the clone's +20.8 %, `sampled-v1-buf` +8.5 % vs +8.5 %, `do_undo` +3.8 % vs +3.9 %). Every field of `Position` is `Copy`, so the derived `Clone` was already lowering to a copy. **The hypothesis was wrong and the change is not kept** — recorded so it is not re-derived.

### 2026-08-11 — what v0.1.0 freezes, and what it deliberately leaves out — [`67e454a`](https://github.com/sugyan/shunsai/commit/67e454a) (#23)

v0.1.0 is the last point at which an API break is free, so the public surface was audited once, deliberately, rather than an item at a time as each consumer asks. The `states: Vec<State>` move is recorded in its own entry; this one is the rest, including what was left alone and why.

**The instrument.** `examples/search.rs` is a fixed-depth alpha-beta — material evaluation, captures-first ordering, a transposition table on `key()` — importing `shunsai::Position` and `shogi_core` and nothing else. It is built on **default features**, which is what makes the check real: `internals` is feature-gated, so a slip into crate internals cannot compile there. It exists because **perft is not a consumer**: it never keeps a key, never cuts off a subtree, and never asks what a move captures. (`tests/` were already public-API-only, being integration tests. What none of them were is *search-shaped*.)

**It found no gap that needs a break.** Everything it wanted and could not get — `gives_check`, `attackers_to`, staged generation, a `Position` → `PartialPosition` conversion — is a pure addition, and under Cargo's semver for 0.x an addition ships in 0.1.x with no version change on the consumer side. This is why "predict what `rinsai` will need" is the wrong question to answer before the freeze: **only breaks have a deadline**, and the E1 list can arrive whenever E1 does, each with its measurement.

**`MoveSet` keeps its public fields, and with them its representation.** Rust has no private field on a public variant, so opacity is all-or-nothing, and a consumer's move ordering wants to `match` `Normal` against `Drop`. The 48 bytes are fixed by the *design* — two destination bitboards — not by field visibility, so hiding the fields would not buy the freedom to shrink it. What the public fields do buy is concrete: **a caller can filter a set by a target mask today**, off `promotions` and `non_promotions`, which is much of what staged generation would otherwise be asked for first. This closes the `MoveSet`-size question 2026-07-29 reopened.

**`Position` does not derive `Copy`**, though every field now would allow it. At 368 bytes it is small enough to copy deliberately and large enough that copying it *by accident* in a search's hot loop would not announce itself; `.clone()` at a copy-make site puts the cost in the source, where a reader will meet it. Adding `Copy` later is a break, so this is the window's decision rather than a default — and it is the one item here most likely to be revisited, because copy-make is exactly the caller that would want it.

**`Hash` is not derived, and `MoveSetIter` gains no public constructor.** Both are additions if ever wanted. A search wants `key()` rather than `Hash`, and `MoveSetIter` is reachable through `into_iter`.

**No `Bitboard` u128 interop.** `from_bits` is the one constructor that does not structurally guarantee bits 81.. are clear, and only a `debug_assert!` holds it — `for_each_square`'s safety argument names it, and names a **debug** `cargo test` as part of its own guard. A public `from_u128` would therefore hand a release-build consumer unsound `Square` construction, so it would have to be an `unsafe fn` or mask on the way in, and nothing has asked for either. The 2026-07-23 entry's "interop via `to_u128` stays trivial" is a reason to keep the crate's own bitboard, **not a promise of shipped API** — there is no contradiction to resolve.

### 2026-08-11 — provenance scan before v0.1.0

[DESIGN.md](./DESIGN.md) §7 requires one before publishing, and it had never been run. Done, against the pinned GPL submodules of the local benchmarks repository — apery, apery_rust, YaneuraOu, cshogi, rshogi, Fairy-Stockfish and the old yasai — over their Rust, C, C++ and Python sources.

**The scan lives in that repository rather than this one**, for the reason the cross-engine perft harness does: it cannot run without the corpus, so a copy here would be a script nobody but its author can execute, going stale unwatched. Apparatus sits with the corpus; this file keeps the result. The method is three fixed-string sweeps — the magic multipliers, the other long hex constants, and every non-comment source line of 40 characters or more with its leading whitespace trimmed so a re-indented copy still matches — plus the submodule pins it ran against, so a result is reproducible rather than asserted.

**Result: no verbatim reuse.** Every hit is accounted for.

- **The 243 magic multipliers appear in none of the seven.** This is the check that carries the weight: the tables are the one part of the tree worth copying, and the one part whose origin CI already asserts by regenerating them. The scan is the independent half of that pair — `--check` proves our generator produces these numbers, and this proves nobody else's source contains them.
- **Three constants matched cshogi.** They are splitmix64's — the golden-ratio increment and the two mixing multipliers. Having them *is* using splitmix64, which is public domain (CC0); the 2026-07-23 entry records why it was chosen. cshogi using it too is not evidence of anything.
- **Verbatim line overlap is four lines with yasai and three with rshogi at ≥ 40 characters**, two and zero at ≥ 60. Each is either a signature Rust forces to be written exactly one way — `Debug::fmt`, `Iterator::size_hint`, `bitxor_assign`, a `derive` line — or shared *data* rather than expression: the `use shogi_core::{…}` import both crates need from the dependency they share, and the max-moves SFEN, whose provenance §6 already gives.

⚠️ **What this establishes, and what it does not.** It rules out a pasted table and a copied block, which is what the obligation in §7 is about. It is a trimmed-substring search, not a token-level similarity measure, so it would not catch a transliteration that renamed as it went. The defence against *that* is the one §7 already names — an incremental development history — not this scan. Re-run it before each release; the corpus moves, and the active upstreams among these move most.

**It is deliberately output-safe**, and a re-implementation should keep that property. `grep -o` echoes the matched *pattern*, which is always ours, never the corpus line it was found in — so running it does not put GPL source in front of whoever, or whatever, reads the output. That matters here because CLAUDE.md's top rule forbids the sessions writing this crate from reading those sources at all, which a scan that dumped matching corpus lines would defeat.

### 2026-08-12 — release-plz is wired up, and the release PR needs a token that is not `GITHUB_TOKEN`

The commit convention adopted 2026-08-11 (#24) named release-plz as the machine that
reads the log, and nothing implemented it: no workflow, no config, no `CHANGELOG.md`,
while `include` already promised one. The convention had no consumer. This closes that.

- ⚠️ **The `release-pr` job must use a token that is not the default `GITHUB_TOKEN`, and this is load-bearing rather than a preference.** A pull request opened with `GITHUB_TOKEN` triggers no workflow runs, and `main`'s ruleset requires `check` and `msrv` **with no bypass actors** — so the release PR would carry no CI, satisfy no required check, and be unmergeable by anyone including the owner, with no override available. `RELEASE_PLZ_TOKEN` (a GitHub App token, or a PAT) is therefore a prerequisite of the workflow working at all, not a hardening step. The `release` job keeps the default token: it publishes and tags, and triggers nothing that needs to run.
- **The 0.1.0 changelog entry is hand-written, and later ones are not.** release-plz cannot generate the entry for a release it does not cut, and a missing `include` entry is skipped silently rather than failing, so without this the first tarball would have shipped without the file the manifest names. Writing it up front also beats what git-cliff would make of 29 commits of initial development. **Its date must match the day of the actual publish.**
- **`commit_parsers` is overridden because the default sweeps `perf` into "other"** — the one type this project exists to produce. Every type CLAUDE.md declares gets a group. **Nothing is skipped**, including `chore`: #25 was a `chore!` that moved the MSRV, which is exactly the kind of change a consumer must see, so "internal" here means grouped-low, not hidden.
- **`cargo-semver-checks` has no baseline at 0.1.0** — there is no published version to compare against, so the guard CLAUDE.md leans on for `rinsai` first does real work at 0.1.1. That is also when `rinsai` starts consuming releases, so the two line up; it is not a gap to close before publishing.
- **crates.io authentication is Trusted Publishing, so there is no registry secret at all.** release-plz mints a short-lived token from the job's OIDC identity — `id-token: write`, no `CARGO_REGISTRY_TOKEN`, and specifically *not* the `rust-lang/crates-io-auth-action` step, whose API calls release-plz implements itself. **The cost is that v0.1.0 must be published by hand**: crates.io only accepts a trusted publisher configuration against a crate that already exists, so the first upload cannot use the mechanism that secures every later one.
- ⚠️ **Two halves of that configuration live on crates.io, are invisible from this repository, and fail silently.** The workflow **file name** is matched, so renaming `release-plz.yml` stops publishing; and the configuration's environment field must stay empty because the release job declares none. Nothing here can check either, which is why the constraint is stated on the job itself as well — the operator-side procedure is deliberately **not** in this repository, so this bullet is the only warning a future session gets before renaming that file.
- **The one prerequisite that is a secret is `RELEASE_PLZ_TOKEN`.** The repository had **no secrets at all** when this landed.

### 2026-08-13 — v0.1.0 is on crates.io, published by hand, and nothing automated has been proven yet

- ⚠️ **The `release` job's first run fails, and that is the ordering rather than a defect to chase.** On the push that merged [#30](https://github.com/sugyan/shunsai/pull/30) it asked crates.io for a Trusted Publishing token, was told `No Trusted Publishing config found for repository sugyan/shunsai`, fell through to `cargo publish` with no registry secret, and exited 1 ([run](https://github.com/sugyan/shunsai/actions/runs/31691190341)). That is the 2026-08-12 constraint on Trusted Publishing seen from the other side, and the order that satisfies it is: publish by hand, configure the trusted publisher against the crate that now exists, and let every later version go out through it.
- **What this release leaves unexercised**: Trusted Publishing itself; `RELEASE_PLZ_TOKEN`, because a `release-pr` run has nothing to open a pull request about while nothing is published; and `cargo-semver-checks`, which only now has a baseline. **0.1.1 is the first release that tests any of the three**, and it is one release PR away.
- **The published tarball was built from a commit that `main` cannot contain.** `.cargo_vcs_info.json` in the uploaded crate names `615f389`, a local commit correcting the changelog date, and `main`'s ruleset allows squash merges only — so that sha never becomes an ancestor of `main`, and the same tree arrives under a different one. Consequence: **v0.1.0 has no git tag and no GitHub release**, and the tag history starts at `v0.1.1`.
- **A missing tag does not disturb the next changelog.** release-plz decides what is unreleased by [comparing the local package against the copy on the registry](https://release-plz.dev/docs/usage/update) rather than by reading tags.

### 2026-08-13 — the README is the crate's front page, not a second copy of the record

- **The cross-engine standing came out of the README.** It restated [BENCHMARKS.md](./BENCHMARKS.md), the file that owns measured figures, and a standing written in two places is one that starts disagreeing with itself the next time the harness runs. The README keeps a link and no numbers — and names no other engine: haitaka and apery_rust are what BENCHMARKS.md measures against, and yasai survives only as the 野菜 the name comes from.
- **`#![doc = include_str!("../README.md")]` was rejected.** crates.io resolves a README's relative links against the repository, so `./DECISIONS.md` works there; rustdoc does not, so those same links would break on docs.rs, and the badges and the naming section would land on the crate's front doc page. The crate root keeps its own worked examples, and the README keeps one.
