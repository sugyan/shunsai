# shunsai — Decisions

Why the code is the way it is, what was tried and rejected, and what is still open.
Recorded so decisions are revisited deliberately instead of re-litigated.

- The **design** is in [DESIGN.md](./DESIGN.md); **how measurement works** and every recorded number is in [BENCHMARKS.md](./BENCHMARKS.md).
- Figures here are the ones that carried a decision. Full per-run data lives in `benches/history/*.json`, keyed by the append-only bench ids.
- New entries go at the end of the dated log, and update **Open questions** above it.

---

## Open questions

The current candidate list, in the order the 2026-08-06 cost decomposition and the
2026-08-07 measurements argue for. **None of these is measured** — they are sized by
disassembly of the committed bench binary, or bounded by an earlier measurement.

1. **`MoveSet::write_into` is not being inlined.** A standalone symbol (676 B / 169 instructions) at 14–16 call sites, so every materializing listener spills the 48-byte `MoveSet` to the stack and pays a six-register prologue per set. This contradicts the 2026-07-28 finding that the 48 bytes "never materialise" — that was verified on the *counting* listener and holds only there.
2. **`zobrist.rs` is still `LazyLock<Box<Keys>>`.** The 2026-07-27 const-ification covered `tables.rs` alone. Every `board_key` is an acquire load plus a `Box` deref, and the acquire is a barrier that stops LLVM dead-store-eliminating the three Zobrist read-modify-writes in `undo_move` that `self.key = state.key` immediately overwrites. `splitmix64` is `const fn`-able; the table is ~20 KiB.
3. **`check_info`'s step-checker half is dead work in 82 % of sampled positions** (33 of 40; 28 of the 31 not in check). Gateable on `their & (king_attacks | knight_attacks)`.
4. **The specialized pawn-drop-mate test** — a pawn checks from an *adjacent* square, so the only legal replies are a king move or a capture of the pawn; blocking and drops are impossible. Worth about a fifth of `maxmoves-d2` (it was four fifths before pin legality and the clone removal).
5. **A `Position`-cached slider union.** `king_danger` still makes five `piece_kind_bb` reads per node, now grouped into two unions rather than one. Upside is bounded at **≤1 %** scaled to about three-quarters (see 2026-08-07), and the trade is the one the cached gold union lost on (`do_undo` +7 %) — it needs a much cheaper maintenance story before it is worth measuring again.
6. **Incremental maintenance of the danger bitboard across `do_move`** (§3's "incremental AttackInfo"). Now has a per-node rebuild to beat rather than the old per-destination test.
7. **`Position::remove_from_hand` is the one mutator not inlined**, while `put_piece`, `remove_piece` and `add_to_hand` all are.
8. **`info.pinned.contains(from)`** costs 10 instructions and a branch per non-king origin, unswitched.
9. **`do_move`/`undo_move` has never been optimized.** 10.7–11.4 ns/pair in every recorded run since the M2 baseline — ~10 % of perft, and, unlike anything else on this list, called at every node of a search.
10. **Copy-make `Position`** — still unmeasured, bounded by do-undo's ~10 %, and it must now beat `-cb` rather than `-cb-buf`.

> ⏳ **One item has a deadline rather than a size.** Moving `states: Vec<State>` out of `Position`, so `do_move` returns an `Undo`, makes `Position` a 368-byte pure value whose `Clone` does not allocate and collapses `with_drop` into `*self` — which is what would put copy-make within reach of a driver experiment. It is an **API break, so it is free only before v0.1.0 ships**.

**Closed:** the hybrid per-destination king test (closed 2026-08-07 — the danger pass it would replace has lost its slider loop on most nodes, so the crossover moved below one destination).

**Rejected on inspection, so they are not re-derived:** SWAR nifu (the nine-iteration fold is already fully unrolled and branchless at ~38 instructions; the `u128` SWAR form is ~44), `count()` → bit tricks (InstCombine already folds `checkers.count() < 2`), an `and_not`/`bic` helper (identical IR), a hand present-kinds bitmask (its premise — that `Hand::count` is `extern "C"` — is false; it is an ordinary `#[inline] pub fn`), and a closure-free two-word `SquareIter` (bounded at ~4 % against the +20.0 % / +14.5 % this shape has already cost).

---

## Standing rules established by measurement

Cross-cutting findings that apply to work not yet done. Each was paid for once.

**On measuring**

- **Single-shot `Instant` timing on this machine scatters −8.8 % to +53 %** on a matsuri-d3 A/B. One trial can report −7.4 % by luck alone; a real −7.4 % was withdrawn on exactly this. Screen with alternating binaries, seven passes, minimum kept.
- **A plausible-looking number is not evidence that the machine was quiet.** A run with σ = 46 % on `perft/matsuri/3` landed its *mean* on the previous entry's value and would have recorded −0.2 % where the truth was −24 %.
- **Agreement between independent runs beats σ within one run.** Re-rolling until every id passes σ selects for lucky runs. Monotonic drift across readings is the machine changing state, not a property of the id.
- **Control ids — ones the change provably cannot reach — are what make a cross-day read sound.** Quote them. If the control drifts as much as the signal, the run is not recordable.
- **The allocating `Vec` bench ids drift across days**; never read a change into them. Their `-cb` twins do the same generation without allocating and are the tell.
- **A ~3 % generation change is a criterion-suite result, not a cross-engine one.** The cross-engine harness runs whole trees, which spend only part of their time in generation; it is the instrument for 15–25 % steps.
- **`perft/*` stubs inflate the tree and so understate what they measure**; `movegen/*-cb` stubs do not change the shape of anything and are exact where the stubbed component is output-preserving.
- **A per-call figure from the `internals/*` sweep over-states what the same call costs in a hot loop** that keeps hitting one table row. It predicted 2–3× too much for the ray-table change.

**On correctness**

- **Perft is a real net but a coverage-dependent one.** It only reports a mistake where some position in the tree actually exercises it. A `king_danger` under-report *is* visible in principle — an extra generated move is an extra node, and `perft(1)` counts generated moves directly — but the 2026-08-07 dragon omission slipped past all three deep values because no fixture tree contained the configuration. **The `shogi_legality_lite` differential is what closes it.**
- **Which holes the perft values have is not guessable.** A double check by two sliders is unreachable from every fixture; a `STEP_ATTACKS` row filed under the wrong kind moves no perft value at any of the three fixtures. Both were caught by the differential alone.
- **Establish a guard's worth by sabotage.** Every entry below that claims coverage did so by breaking the code and watching which tests fail. Assertions that a rare configuration was *reached* (liveness counters) are what stop a fixture list drifting into silent non-coverage.

**On optimizing**

- **"Replace a per-item test with one bulk computation" is not unconditionally good.** The king-danger bitboard on its own was +15 % / +24 % on two positions of three: a fixed cost replacing a cost that was paid per candidate square, where the king has two or three candidates.
- **`Bitboard::for_each_square` earns its keep draining a set (30–593 destinations) and loses on a loop of ~20 origins whose body is large.** Converting the origin loops measured +18 %. The walk is not what those loops cost — turning them into closures is, and the suspected mechanism is the listener no longer inlining, which is what the whole `MoveSet` design rests on.
- **`reserve` before a bulk push is a reflex that measured strictly negative here.** `Vec::push` checks capacity per element regardless, so reserving only avoids a reallocation — and `write_into` exists for callers that own a sized buffer, where there is none to avoid. It cost small-set positions +3.4…+8.1 %.
- **"The source computes this twice" is re-derivable by reading two functions side by side, and can be wrong.** The compiler was already sharing the loads. If a redundancy were real, deleting one of the two constructions could not make anything *slower* — losing is itself the refutation.
- **One optimization per change.** Batching destroys the attribution that makes the committed history readable.

---

## Log

### 2026-07-23 — own `u128` Bitboard instead of `shogi_core::Bitboard`

`shogi_core` 0.1.5 does ship a `[u64; 2]` bitboard, and M1 alone could have been built on it. We keep a crate-internal type because the optimization phase changes exactly this layer: both the representation (`u128` / two words / SIMD lanes) and the *set of operations needed* differ per slider technique (Qugiy wants byte-swapped pairs and subtraction tricks, magic wants multiply/shift on raw words, SIMD wants explicit lanes), and the dormant upstream cannot be extended. Bit order matches `Square::array_index()`, so interop via `shogi_core::Bitboard::to_u128` stays trivial.

### 2026-07-23 — the slider swap boundary is the attack-function signatures, not a Bitboard trait

M4 candidates are added as feature-switched backends with identical `lance_attacks` / `bishop_attacks` / `rook_attacks` signatures. **A trait over `Bitboard` was considered and rejected**: the required operation set varies per technique, so the abstraction would leak or widen every time, and generics would either infect the public API (`Position<B>`) or force dyn dispatch into hot loops.

### 2026-07-23 — benchmark targets: rustshogi replaced by rshogi; Fairy-Stockfish added

rustshogi's `search_moves` is pseudo-legal (no self-check filtering, no pawn-drop-mate exclusion — confirmed by source inspection) with no option to enable full legality, so it cannot produce comparable legal-perft numbers. [rshogi](https://github.com/SH11235/rshogi) replaces it as the modern-Rust reference; Fairy-Stockfish is added as an independent-implementation cross-check (its speed is reference-only, being a generalized variant engine).

Candidates surveyed and passed over: WCSC/Denryu-sen open-source engines reduce to the YaneuraOu / Apery / dlshogi(=cshogi-core) movegen families already covered; Gikou (dormant since 2020, no perft, x86-oriented), GPS/OSL (dormant), Bonanza (non-OSS license), nozaq/shogi-rs (no true perft; validate-on-make API).

### 2026-07-23 — Zobrist keys from an inline fixed-seed splitmix64

Not a seedable RNG crate (as the old yasai used): no extra runtime dependency, and keys stay byte-for-byte reproducible independent of any crate's version — `rand`'s `StdRng`/`SmallRng` explicitly do not guarantee algorithm stability across versions. It also converts trivially to `const fn`. splitmix64 is a public-domain (CC0) algorithm by Sebastiano Vigna.

### 2026-07-24 — benchmark history is committed, keyed by append-only criterion ids

criterion's own baselines (`--save-baseline`) live in `target/`, are machine-local and volatile, so they cannot serve as a durable improvement history. `scripts/bench_snapshot.py` summarizes each run into `benches/history/*.json` and regenerates the headline table in BENCHMARKS.md. **Bench ids are append-only** — renames and reuse forbidden; new APIs and new fixture versions get new ids — so a metric's time series always measures the same thing.

### 2026-07-24 — internals exposed to benches via a `bench-internals` feature

`src/internals.rs` is a `#[doc(hidden)]` wrapper module over exactly the swap-boundary functions plus `attackers_to`. Wrapper functions because `pub use` of a `pub(crate)` item is rejected (E0365); a *feature* rather than making them `pub` because the public API must stay the `Position`-level surface — the backends swap behind these signatures and must not become de-facto public API. The `internals` bench target sets `required-features`, so plain `cargo bench` skips it.

### 2026-07-24 — movegen/do-undo fixtures come from floodgate real games

Extracted by our own `examples/gen_bench_positions.rs` into versioned, frozen files (`benches/positions/sampled-v1.sfen`, `games-v1.usi`).

Real games over fixed-seed random playouts for two reasons: playout positions skew unrealistic (scattered material, inflated hands), and — decisive — playout *reproduction* depends on `legal_moves()` ordering, so any ordering change would silently change the workload and corrupt the history. Committed SFEN/USI text is stable forever, and versioning (v1 → v2 with new bench ids) keeps a deliberate set change traceable.

Licensing: game records are factual data (positions and moves are not copyrightable expression), the pipeline is our own permissive code on `shogi_core` serialization, and raw kifu files are never committed. The extractor validates every game move against `legal_moves()`, doubling as a real-game differential test.

### 2026-07-27 — attack tables const-evaluated (`LazyLock` dropped); measured neutral, kept anyway

All 13 bench ids moved within 1–2σ: on an already-initialized lock the check is a perfectly predicted branch that LTO hoists out of the hot loops. **Kept because it is a simplification, not a complexity-for-speed trade** — less machinery, no heap init, no runtime indirection — and because the slider backends need exactly this const table-generation infrastructure for their own much larger tables. Recorded so the neutral result is not rediscovered later.

The builders index raw `array_index` arithmetic since `Square::shift`/`Square::all` are not `const fn`; the old `Square`-based builders are retained as test-only references that every const table is asserted equal to.

### 2026-07-27 — slider attacks: magic bitboards adopted over Qugiy, decided by measurement

The M2 baseline showed slider ray-walking was *the* bottleneck: 37.2 ns of an `attackers_to` call's 39.6 ns was stepping square by square through `Square::shift` (which `shogi_core` exports as `extern "C"`, so it is not freely inlinable).

Measured per call over the 81-square × 3-position sweep: naive bishop 12.4 ns / rook 20.3 ns; qugiy 2.61 / 2.86; **magic 2.43 / 2.37**. Magic wins mostly on the rook, and end-to-end perft agreed. Magic is the unflagged default; `slider-qugiy` / `slider-naive` remain as override flags.

**The losing numbers are kept deliberately.** Qugiy is within ~10 % of magic while needing *no* attack tables at all, against magic's ~486 KiB of `.rodata`. If cache pressure ever outweighs raw latency — a real search, unlike a perft microbenchmark — the decision is worth **re-running rather than re-deriving**.

- **The bake-off ran on the architecture most favourable to qugiy.** Its `o - 2r` needs the board mirrored for the downward ray, i.e. `u128::reverse_bits`, and aarch64 has a single-instruction bit reverse: `mirror()` compiles to **5 instructions**. x86-64 has none, so the same function becomes **44** — and `line_attacks` mirrors twice per line, so a bishop pays it four times. Magic's cost (one multiply, two shifts, a table load) is architecture-neutral. **A future x86-64 re-run should expect the gap to widen, and should measure `pext` (BMI2) as a third backend** — with the caveat that `pext` is microcoded and slow on AMD Zen1/Zen2.
- **Shared by all backends.** The layout is file-major, so a *file* is nine **contiguous** bits: the file direction and lances index a 2.3 KiB table directly, no multiply and no magic, identical across backends so the comparison isolates the strided lines. Each line has at most 7 blockable squares — the far end of a ray is attacked whether or not it is occupied — so every magic index is 7 bits wide.
- **Correctness.** Each backend is asserted equal to `naive` *exhaustively over the relevant occupancy* of every line (all 2^k subsets, k ≤ 7, for all 81 origins), plus 20k random full-board occupancies. `naive` is therefore kept compiled forever as the oracle.

#### 2026-07-28 — only the multipliers are generated, and two independent guards cover them

A multiplier is the output of a search and cannot be derived; a magic's mask and both shifts *can* be, from the line geometry the crate already computes (`sliders::relevant_mask`). So `magic.rs` derives them at compile time and `magics.rs` holds three bare `[u64; 81]` arrays — 1485 lines of struct literals became 93. Drift between the generated file and the board geometry is then not detected but **impossible**.

Two questions remain, and they have different answers:

| question | guard | where |
|---|---|---|
| *does each multiplier work?* | `magic::line_table` rejects any two occupancies sharing a slot without sharing an attack set — a corrupted number fails the build with `E0080` | compile time, always |
| *are these **our generator's** multipliers?* | `cargo run --example gen_magics -- --check` re-runs the search (~0.4 s in debug) and diffs | CI only |

**The second guard is needed because the first is satisfiable by numbers we did not produce, and by far more of them than intuition suggests.** Validity is a *loose* condition — a magic need not gather the relevant bits perfectly, only avoid mapping two occupancies with different attacks onto one slot, so constructive collisions are allowed. Measured over all 15552 single-bit corruptions of the committed constants (243 magics × 64 bits): **11299, or 72.7 %, remain valid** (rank 56.1 %, diagonals ~81 %) — they compile, build a correct table, and pass every test including deep perft. **So "it works" is close to no evidence of "we generated it."** Keep both guards.

#### 2026-07-28 — the generator writes the file itself; no `quote`, no `build.rs`

`cargo run --example gen_magics > src/sliders/magics.rs` cannot work: the shell truncates `magics.rs` before cargo builds the library it is part of. It now writes to `concat!(env!("CARGO_MANIFEST_DIR"), ...)` directly, which is also what makes `--check` possible.

**Rejected: emitting through `proc-macro2`/`quote`** — needs `syn` + `prettyplease` for readable output, `quote!` renders a `u64` in decimal so hex literals would be built as strings anyway, and it would quadruple a dependency tree that is otherwise just `shogi_core`, all to format 243 integers.

**Rejected: `build.rs`** — the search is deterministic, so every downstream build would repeat work whose answer never changes; and, decisive given that licensing is this project's top constraint, the constants would no longer be visible in the tree or in a diff, which is exactly how the "our own generator, not transcribed" claim is demonstrated. `magics.rs` is marked `linguist-generated` in `.gitattributes`, deliberately without `-diff`.

### 2026-07-27 — the callback API yields a `MoveSet` per origin, promotions as a separate bitboard (M3)

`Position::generate_moves(|set| ...)` hands out one `MoveSet` per origin (or per dropped piece kind) and stops early; `legal_moves()` remains the allocating wrapper.

The shape worth recording is `MoveSet::Normal { promotions, non_promotions }` rather than one destination bitboard plus a flag: **the two sets overlap exactly where promotion is optional**, so a square in `promotions` alone is a compulsory promotion and one in `non_promotions` alone cannot promote at all. That encodes shogi's forced-promotion rule as set membership, which is what let the per-destination `relative_rank` tests become mask ANDs. `MoveSet::len()` is then two popcounts, making perft's leaf bulk counting free of any `Move` construction. Early exit gave `has_legal_moves()`, so the pawn-drop-mate test stops at the opponent's first reply.

- **Drop filtering had to move to bitboards in the same change.** Grouping made drops pay twice (build the destination bitboard square by square, then walk it again), a 23 % regression on matsuri. Both per-square tests are now set operations: the squares where a dropped piece could never move again are *exactly* the squares that force promotion for a board move, so one `forced_promotion_zone` mask covers pawn, lance and knight; and since only a checking pawn can be a pawn-drop mate, the reverse-lookup trick names the single square that gives check, so the expensive simulation runs at most once per position.
- **Rejected: external iteration (`impl Iterator<Item = MoveSet>`).** Needs the generator's nested state — evasion vs normal × piece kind × board vs drop — rewritten as an explicit state machine reloaded on every `next()`, and it does *not* buy back the one thing the callback costs: an iterator borrowing `&Position` blocks `do_move` exactly as the closure does. `gen` blocks would give iterator ergonomics with generator code but remain unstable as of 1.94. Cross-check: haitaka arrives at the same listener shape independently, and uses an iterator only *inside* one set, exactly as `MoveSetIter` does.
- **Two bitboards vs one bitboard plus a flag, checked against haitaka.** haitaka encodes the same information in 32 bytes to our 48. The 16 bytes buy exact O(1) counting: with a single destination set, whether a square yields one or two moves is undecided, so haitaka documents `PieceMoves::len` as *not* the move count and its `ExactSizeIterator` needs per-piece-kind special cases plus a per-destination `PromotionStatus::new` during iteration. ⚠️ *Qualified 2026-08-07: this trade is not settled — see Open question 1.*

#### 2026-07-28 — `size_of::<MoveSet>() == 48` is not a cost here, and passing by reference would change nothing

The size is real (two `u128` bitboards at align 16, plus tag/`piece`/`from` in 3 of the 14 padding bytes) and invites the usual engine instinct that a move object should be small. **That instinct applies to move objects that are *stored*** — yasai's went into move lists, size × count. A `MoveSet` is built by `emit_normal` and consumed by a listener whose type is known after monomorphization, so the call inlines and SROA shreds the struct before it ever has an address. Verified in the emitted aarch64: in the bulk-count path the two bitboards go straight from GPR pairs into `cnt.16b`/`addv.16b`, and `piece`/`from`/the tag are dead-code-eliminated. What *is* stored is `shogi_core::Move` — 3 bytes.

**Rejected as a no-op, not a trade-off: `FnMut(&MoveSet)`.** AAPCS already passes anything over 16 bytes indirectly, and `by_value`/`by_ref` probes of an identical struct compiled to byte-identical code.

⚠️ **Both halves of this have since been qualified.** The premise "nothing collects `MoveSet`s" was withdrawn on 2026-07-29 (a search ordering moves would), and the inlining claim was found to hold for the counting listener only (Open question 1). **Do not shrink it speculatively** — that optimizes for a caller that does not exist yet and costs the `len()`-as-two-popcounts the 48 bytes buy. Re-measure when a search actually orders moves.

#### 2026-07-28 — the listener returns `ControlFlow<()>`, not `bool`, and deliberately not `ControlFlow<B>`

`true` meaning *stop* is unreadable at the call site, and returning `()` threw away the one bit the caller most wants: whether the walk finished.

**`ControlFlow<B>` was tried and rejected on inference, not taste.** The overwhelmingly common call is a full walk in statement position whose result is discarded, leaving `B` unconstrained and failing with `E0282` (verified on 1.94); every counting caller would need a turbofish to use an API shape it does not want. A value-carrying `find_move` can be added later as its own method. The one cost is that `ControlFlow` is `#[must_use]`, so full-walk callers write `let _ = ...` — noise the compiler enforces in exchange for making early exit impossible to ignore. Internally the change removes bookkeeping: `generate_normal`/`generate_drops` propagate with `?` instead of returning `bool` and being tested at each call.

### 2026-07-27 — legality is decided per position, not per move

Generation used to test each candidate with `attackers_to` on an adjusted occupancy, gated by an over-approximation: any of our pieces standing on a king ray counted as possibly pinned whether or not an enemy slider stood behind it, and in check *every* move was tested.

Each node now computes the checkers and the genuinely pinned pieces once, and those two bitboards make every non-king move legal by construction:

- a piece that is not pinned cannot expose its own king, because a pin is exactly the situation where it could;
- a pinned piece is masked to `line(king, from)`, which still lets it capture the pinner;
- a single check masks every non-king move to capturing the checker or interposing; a double check leaves only king moves.

Only the king still needs a test, since it is what the test is about, and it is lifted out of `occupied` so it cannot retreat along a checking ray. Snipers are found by asking which enemy sliders would reach the king on an *empty* board and counting blockers between; a dragon's or horse's one-step sidesteps can never pin (nothing fits between them and the king), and lances are searched from the king with *our* colour — the same reverse lookup `attackers_to` uses.

Cost: one 81×81 `LINE` table (~105 KiB, alongside `BETWEEN`) for the pin mask.

### 2026-07-27 — two generation candidates measured and *rejected*

Both were on the M4 candidate list; both made things worse. Recorded so they are not re-derived.

- **A cached gold union in `Position`** (the five gold-moving kinds OR-ed once and maintained by `put_piece`/`remove_piece`). It did what it promised — `internals/attackers-to` −13 % — but maintaining it cost `do_undo` **+7 %** and `perft/startpos-cb/4` **+3 %**. A branchless variant recovered the perft loss but left `do_undo` ~10 % down. **The union is four ORs of already-hot bitboards; paying for it on every piece placement to save it on every attack query is the wrong side of that trade.** This is the reference trade for any future "cache it in `Position`" proposal.
- **Per-piece-kind generation loops** (walk the 13 non-king kind bitboards so the kind is a loop constant). Uniformly **+5 % to +12 %** on every id. A typical position spreads ~20 pieces over ~8 kinds, so the fixed cost of walking 13 mostly-empty boards — and the loss of the single dense pass over `our` — outweighs what it saves.

### 2026-07-29 — the reusable perft buffer is worth nothing; a −7.4 % figure withdrawn

The M3 entry had credited threading one reusable `Vec` through the perft recursion with matsuri-d3 −7.4 %. It is an artifact of single-shot timing, and the committed `-cb-buf` ids are right.

- **The effect has a hard ceiling far below the claim.** A counting global allocator over the `-cb` driver reports **931** allocations for startpos-d4, **208** for matsuri-d3, and **1** for maxmoves-d2 — leaves are bulk-counted at depth 1, so only internal nodes ever collect. At 17.1 ns per `Vec::with_capacity(128)` + drop and 41.6 ns per growth realloc, that bounds the *entire* effect at ~0.2 % of runtime. The claimed gains also ran *inverse* to allocation density — startpos allocates 4× more often per unit of runtime yet was credited with 15× less. **Getting the ordering backwards is the signature of measuring something that is not allocation.**
- **The residual is real and points the other way.** `-cb-buf` is consistently a hair *slower* on startpos-d4 (+1.4…+1.9 %): `while i < buf.len()` reloads the length every iteration — the recursive call takes `&mut Vec<Move>`, so it cannot be hoisted — and `buf[i]` bounds-checks, where `-cb`'s `for mv in moves` is a pointer bump. That cost is per *move* against a saving that is per *allocation*.
- **Incidental, and worth more than the buffer ever was**: the driver's `Vec` is not where this crate allocates. maxmoves-d2 performed ~543 allocations per perft(2) *inside* the library, because `is_pawn_drop_mate` cloned the position. That is the next entry.

The reasoning for giving the buffer its own append-only ids, rather than folding it into `-cb`, is what caught the error and stands.

### 2026-07-29 — the pawn-drop-mate simulation no longer clones; generation allocates nothing

`is_pawn_drop_mate` simulated the drop with `position.clone()` + `do_move`, and `Position` owns `states: Vec<State>`, so cloning allocates. It was **the only allocating step anywhere in generation**. Two operations per simulated drop, not one: `Vec::clone` gives the copy *exact* capacity, so `do_move`'s own `states.push` reallocates immediately.

`Position::with_drop` instead copies the position by value and starts from an empty `Vec`, applying the drop without recording undo state — the simulated position is discarded, so it never needs to be un-done. All three `-cb-buf` walks are now **0 allocations**. Worth maxmoves-d2 **−19.5 %**; every other id is flat, correctly — the simulation is reached ~17 times in all of matsuri-d3 and **never** at startpos, which needs a pawn in hand and a legal drop square adjacent to the enemy king.

- **Rejected: `do_move`/`undo_move` on `&mut Position`.** It would avoid the ~400-byte copy too, but only by making `generate_moves`, `legal_moves` and `has_legal_moves` take `&mut self` — and the callback contract is shaped *around* the listener's shared borrow. Generation demanding unique access would stop callers holding the position immutably while generating.
- **This reversed the follow-up.** Before pin legality, stubbing the mate test out took maxmoves-cb-d2 from 298.5 µs to 50.7 µs — ~83 % of the benchmark, of which the allocation was a ninth, which made the specialized mate test look like the obvious next change. Pin legality made the `has_legal_moves()` walk inside the test far cheaper without touching the clone, so the allocation became the *majority* of what was left. The specialized test is **demoted, not dropped**: now worth about a fifth of maxmoves-d2 rather than four fifths.
- **Guards.** `position_after_drop_matches_do_move` holds `with_drop` to clone-and-`do_move` over every hand piece on every square it may legally occupy; `Position`'s `PartialEq` compares every field *except* the undo stack, so a field added to `Position` and missed in `with_drop` fails it. By sabotage, the tests that cover this function at all are the differential oracle, `rules::pawn_drop_mate_is_excluded`, and `perft::max_moves_position_deep` — **none of the default-depth perft values do**, and that deep value is `#[ignore]`d, so CI's `--ignored` step is the only perft guard on pawn-drop-mate exclusion.

### 2026-07-29 — the consumer is a search engine, which re-opens two closed decisions

Every optimization so far was judged against *perft*, and perft is the measuring instrument, not the customer. The engine is a separate crate, so the non-goals stand unchanged — **what changes is the standard of evidence. "Free" now has to mean free under a search.** Neither decision below is reversed; what is withdrawn is the reason to stop looking.

- **`MoveSet`'s 48 bytes are no longer settled** — that entry dismissed the size *because* nothing stores a `MoveSet`, and named the one caller that would change that: "move ordering in a search would, but search is a non-goal". That premise is gone.
- **magic-vs-qugiy should be re-run under a search.** The slider entry already flagged the condition; a search sharing cache with a transposition table is precisely the case a perft microbenchmark cannot create. Both backends stay compiled, so this is a measurement, not a rewrite.
- **What a search needs that perft never exercises**, hence unmeasured and mostly unbuilt: repetition detection (千日手, including the perpetual-check distinction, which needs history rather than a position), a static exchange evaluation or capture-ordering hook, and **exposing** check / pin / attacked information rather than recomputing it per node. The last is worth noting: `king_danger` produces exactly the attacked-squares set a search wants, so the largest remaining perft hot spot and the first search-facing API need are the same piece of work.
- **Make-unmake is already the search-friendly shape.** `do_move`/`undo_move` is ~9–11 % of perft runtime, so the copy-make alternative is bounded by that and costs an API break.

### 2026-07-30 — the king's destinations are decided by one danger bitboard

Three changes, measured and adopted separately, **because the first on its own loses**.

**Why this was the target.** Generation tested each king destination separately, rebuilding occupancy per move and calling the full `attackers_to` — up to eight times a node. Stubbing it out measured it at **23.7 % / 19.6 % / 58.7 %** of startpos-d5, matsuri-d3 and maxmoves-d3, against 8.2 / 8.2 / 23.2 % for `checkers`. haitaka's remaining lead was 1.18× on startpos, so this one site was larger than the whole gap.

**The destinations collapse into one mask, exactly.** The per-destination test built `occupied_after = (occupied ^ single(from)) | single(to)` and `enemy_mask = player_bb(them) & !single(to)`. Neither edit can change whether `to` itself is attacked:

- a piece standing on a square only shortens rays **beyond** it, and what reaches `to` depends on the pieces **between**;
- no shogi piece attacks the square it stands on, so the piece being captured was never among `to`'s attackers.

Only lifting our king out of `occupied` matters — otherwise it could retreat along a checking ray while still blocking it with the body it is trying to save — and that does not depend on the destination.

- **Built lazily, inside `generate_king_moves`.** `has_legal_moves` breaks at the first move set and `generate_normal` runs first, so a walk that stops early usually never reaches the king — and that walk is the pawn-drop-mate test. Building the bitboard in `generate_legal` would hand that back. A `candidates.is_empty()` guard covers a king boxed in by its own pieces.
- **On its own the bitboard is a regression on two positions of three, and that is the useful result.** startpos-d4 **+15.1 %**, matsuri-d3 **+24.4 %**, against maxmoves-d2 −23.5 %. One pass over ~20 enemy pieces is a **fixed** cost where the test it replaces was paid **per candidate square** — and the initial position's king has three, matsuri's two.
- **The fix is to filter the loop, not to abandon the bitboard.** The result is only ever masked with `king_attacks(king)`, so only attacks landing on the king's eight neighbours can survive. A piece moving a fixed number of steps has bounded reach — the knight's is longest at one file and two ranks — and its target is itself one step from the king, so anything outside a **two-file by three-rank box** provably cannot bear on a king destination.
- **The cost is that `king_danger` returns a partial attack map**, valid only next to the king — and this is the function a search would take its full attacked-squares set from. Dropping the filter is one line but costs what the filter buys. **The condition: a search that wants the full attack map must re-measure this filter, not assume it.**

**Checkers and pins are the same slider question at different blocker counts.** `checkers` asked the three slider tables against the real occupancy; the pin scan asked the same three against an *empty* board and counted blockers. The empty-board pass **subsumes** the other — a slider on `s` attacks the king through `occupied` exactly when `s` lies on a slider line from the king **and** `between(king, s)` is clear — so one walk gives both: 0 blockers is a checker, 1 is a pin, 2+ is neither. `attackers_to` stays for `in_check` and the internals bench.

- **Guards, by sabotage.** Zeroing `danger`: caught by the differential oracle, all three default-depth perft values, and three `rules` tests. Shrinking the box by one rank or file: caught by `step_attacker_zone_covers_every_step_piece` (which checks the superset property against `attacks_of` itself over every king square × neighbour × origin × non-slider kind × colour) and by `rules::distant_knight_still_covers_a_king_escape`, which places a knight at the exact corner. Two tests cover the capture argument **in both directions**: the king may not take a pawn whose defending rook's ray *stops on* it, and it **must** be allowed to take an undefended one — the second is what a merely-conservative danger bitboard would break, and zeroing `danger` does not fail it.
- ⚠️ **One sniper cannot tell accumulation from assignment, and the fixtures did not cover it.** Turning `checkers |= single(sniper)` into `= ` passed **the entire suite, including the differential oracle and the `#[ignore]`d deep perft values**: a double check by *two sliders* is not reachable within two plies of any fixture, nor anywhere in the three deep perft trees. Two fixtures now close both cases deterministically, and the test **asserts it reached each configuration**, so removing a fixture fails loudly instead of silently reducing coverage.

### 2026-07-31 — the consumer's roadmap, and the re-measurements it schedules here

The route was decided against the 2022–26 championship record: DL-based winners at WCSC32/33/36, NNUE winners at WCSC34/35, and a DL+NNUE hybrid 2nd in 36 — both routes reach the top, so the deciding constraint for a solo developer is training budget, and it points at NNUE: its data generation runs on CPU, where shunsai's movegen speed converts directly into training-data throughput, and its training fits a single mid-tier GPU. DL/MCTS is deferred behind explicit conditions (rating plateau + a temporary budget multiple + tournament inference hardware), not rejected.

What this log needs is the schedule the engine phases impose here:

- **E0** (USI shell, material eval, αβ + TT + qsearch) **requires no shunsai change, deliberately.** SFEN stays external. Repetition resolves engine-side without any API: 千日手 needs game history, so the engine stacks `(key(), Hand, in_check())` per ply. E0 shipping against a frozen shunsai is the layering's first contact with a real consumer.
- **E1** (ordering, null move, LMR, SEE) **is when the API additions land**, each carrying its recorded measurement: expose `attackers_to` plus public `Bitboard` iteration (SEE's prerequisite); staged generation (captures / evasions / quiets) — the change that makes a search *collect* move sets, which is precisely the caller the 48-byte entry was waiting for; `gives_check`; `do_null_move`/`undo_null_move`; expose `checkers`/`pinned`. **The first TT-backed search bench is the shared-cache condition the magic-vs-qugiy entry named** — that re-run happens here, not in perft.
- **E3–E4** settle the rest. NNUE deltas are computed engine-side from `piece_at` reads before `do_move` first; a `DirtyPieces`-returning variant waits for a profile, not an argument. E4 runs the x86-64 batch this log prescribed — magic vs qugiy vs `pext` under TT pressure. **The `king_danger` filter condition may never fire from evaluation at all**: NNUE consumes piece placement, not an attack map, which leaves ordering experiments as the only plausible claimant for the full map.
- **The engine repo adopts the licensing policy verbatim plus one sharpening: run-vs-link.** GPL engines and servers may be *run* as separate processes — sparring ladders, CSA bridges, local match servers — because nothing GPL is linked or distributed. Reading-to-reimplement stays allowed, porting stays forbidden, and there is no major permissive reference for *search* code, so search is written from CPW, papers and first principles. Verified: Ayane (the USI match runner) is Apache-2.0; `usi`/`csa`/`shogi_usi_parser` are MIT; fastchess and cutechess have no USI support.

### 2026-08-04 — the consumer is named `rinsai`, and v0.1.0 becomes a prerequisite of E0

The engine's name, repository layout and full roadmap live in `rinsai`'s own design document — they are the *engine's* design, and only what imposes a schedule **here** belongs here. Two things do.

- **shunsai is published to crates.io, and `rinsai` depends on released versions rather than a git pin.** The plan had assumed a git dependency with a rev pin plus a local `[patch]` override, on the assumption that a release per API addition was a cost to avoid. It is not: shunsai is a library with third-party value and belongs on crates.io regardless. The prototype loop is unchanged — try it on a branch, measure it on this crate's bench, adopt it — but it now ends in a **release** rather than a rev bump. **A consequence that constrains this crate: an API addition E1 wants is a version of shunsai, so it carries semver.**
- **v0.1.0 therefore moves from "Later" to a prerequisite of E0.** E0 still requires no API change, but it needs something to build against, and the engine's crates are `publish = false`.

### 2026-08-03/04 — `MoveSet::write_into` decides drop-versus-board once per set

Materializing the moves — what every engine except haitaka does at leaf parents, and what a search must do — cost **2.14×** on `sampled-v1`, and the expansion loop, not the allocation, is where it went. Nothing had ever optimized that loop.

**What the iterator was paying per move.** `MoveSetIter::next` matches on `Option<Square>` to decide drop-versus-board on *every* call, and on the board path probes `promotions` first and falls through, so **every non-promoting move pays a failed pop**. `write_into` makes both decisions once per set and drains each destination bitboard in its own loop with `promote` a loop constant.

The iterator stays — it is the right shape for a caller that consumes lazily or stops early. **`legal_moves()` is not that caller**, so it drives `write_into` too, which is where the public materializing API picks the gain up (matsuri −14.5 %, maxmoves −18.5 %). That also gives `write_into` tree-level coverage: `callback_and_vec_apis_agree` and the differential both run through `legal_moves()`.

Result: `movegen` maxmoves **−43…−47 %**, matsuri −24 %, `sampled-v1` **−16 %**, startpos −5 %. Materializing perft matsuri-d3 and maxmoves-d2 both **−25 %**.

- **`Bitboard::for_each_square` is the other half, and it is where the `u128` choice finally cost something.** `pop()` walks the `u128` directly: on aarch64 `u128::trailing_zeros` needs an `rbit`/`clz` pair on *each* half plus a select, and `x & (x - 1)` needs a borrow chain, so both run about twice their 64-bit cost. The 81 bits are contiguous, so the bulk walk takes the low word to exhaustion and then the high word, which is usually empty. It also builds squares with `Square::from_u8_unchecked` where `pop` goes through the `extern "C"` `Square::from_u8`, which re-checks a range this type's invariant already guarantees.
- **The gain tracks moves per `MoveSet`.** Everything removed is per-move, so a position whose sets are large collects most of it and one whose sets hold one or two moves collects almost none — startpos −5 %, the in-check sweep nil, because evasions are restricted to capturing the checker or interposing. **That also resolves an anomaly recorded as a refutation**: maxmoves cost *more* per move than matsuri (1.090 against 0.871 ns) only because the iterator's per-item drop dispatch fell hardest on the drop-heavy position. With it gone the ordering is monotone in set size again.
- **The theory that motivated the change is still wrong.** The wasted-promotion-pop argument predicted the initial position would gain *most*, since nothing can promote there so all 30 moves pay the failed pop. **startpos gains least.** The change succeeded for a different reason than the one that suggested it.
- **`out.reserve(self.len())` was in the first version and made small-set positions worse.** See *Standing rules*.
- ⚠️ **`perft/matsuri-cb/3` is not comparable across this boundary.** It reads 5.4–5.65 ms at σ ≈ 27 % under whole-suite conditions but 2.59 ms at σ ≤ 1.3 % in isolation at *every* revision tested, and its non-allocating twin sits at 2.60 ms throughout. The id is measuring the allocator, not generation. **Which reading is the anomaly is undecided.** Treat that one id's series as broken across 2026-08-03/04.

### 2026-08-06 — the per-*set* path: piece-indexed dispatch and per-origin promotion

Generation is **−22.7 % on the initial position and −23.1 % on the 40 sampled real-game positions**; startpos moved to second of nine and haitaka was beaten on all three positions.

**The decomposition came first, and it inverted the plan.** `movegen/<pos>-cb` measures one generation call on one position, so stubbing a component does **not** change the shape of any tree. At startpos both `check_info` and `king_danger` are output-preserving — no checks, no pins, all three king candidates already safe — so those cells are exact rather than indicative.

| component of startpos's 85.44 ns, 19 non-king origins | ns | share | per origin |
|---|---|---|---|
| `attacks_of` + the target/check/pin masks | **34.94** | **41 %** | 1.84 |
| `emit_normal` + the listener | 17.56 | 21 % | 0.92 per set |
| `king_danger` | 17.05 | 20 % | 4.3 per relevant piece (~4) |
| `check_info` | 12.77 | 15 % | — |
| the origin bitboard walk + the mailbox load | 10.21 | 12 % | 0.54 |

The three origin-loop rows are **exactly additive**. At startpos 15 of 19 origins are step pieces, so after charging four sliders ~2.4 ns each the **step path was costing ~1.7 ns per origin for what ought to be one indexed table load** — which made the dispatch, not the walk, the thing to attack. The plan had ranked the walk first.

- **Adopted: `attacks_of` serves the nine non-slider kinds from one piece-indexed row.** It was a ten-arm `match` on `PieceKind`, which LLVM lowers to a jump table — an indirect branch whose target is whatever piece the mailbox yielded, and therefore poorly predicted. `STEP_ATTACKS[piece.as_u8() & 31][square]` folds colour and kind into one index, reached through a single direct branch against a `SLIDER_KINDS` bitmask. **startpos −16…−18 %.** Costs 40.5 KiB of `.rodata`, additive to the per-kind tables the reverse-lookup scans still want in their two-row shape — which matters only to the deferred magic-versus-qugiy re-run under cache pressure.
  - **This is not the per-piece-kind generation loop rejected on 2026-07-27.** That walked 13 mostly-empty bitboards and lost the single dense pass over `our`. The dense pass is untouched here; only the dispatch inside it moves.
- **Adopted: promotion is decided per origin, not per set.** `emit_normal` runs once per generated set and made three rule decisions each time. **Promotion is legal when a move starts *or* ends in the zone, which reads as two conditions on the destinations but is one fact about the origin**, so `PROMOTION_MASK[colour][from]` bakes it and the branch disappears. **startpos −5.5 %** on top; cumulative −22…−23 %.
- **Adopted: `legal_moves()` was asking for 128 moves' worth of `Vec` when the maximum is 593**, so drop-heavy positions grew three times inside the call. Sized from the real maximum: matsuri −12.5 %, maxmoves −24.0 %. **This is not the `out.reserve` rejected above** — that was a per-*set* cost buying nothing; this is one sizing per *call* that removes reallocations the call really was performing.
- **Rejected, and it is the candidate this work was planned around: moving the origin loops onto `Bitboard::for_each_square`.** See *Standing rules*. Splitting it three ways: the delegation itself is neutral (−0.01 %), `generate_normal`'s two loops cost **+20.0 %**, and `king_danger`/`check_info`'s scans **+14.5 %**. The `generate_normal` conversion costs the *materializing* path as much as the counting one, which is what one would expect if the extra closure layer stops the listener inlining. The scan conversion costs the counting path 14.5 % and leaves the materializing path alone, and **that is not explained** — the accumulators becoming `&mut` captures is the obvious suspect and is untested. **Recorded as measured, not as understood.**
- **Guards, by sabotage.** `attacks_of_matches_the_per_kind_tables` holds the folded dispatch to the `match` it replaced over every piece × square × 66 occupancies; `piece_index_matches_as_u8` pins the index arithmetic against `Piece::as_u8` — an upstream representation this crate now reads through a mask. **One sabotage is caught by the differential alone**: a row filed under the wrong kind (`ProSilver` given silver's table) moves **no perft value at any of the three fixtures**.

### 2026-08-07 — `check_info`'s sniper scan comes out of ray tables; sharing the slider union is a loss

**Adopted: the three "which sliders would reach the king on an *empty* board" lookups have no occupancy to consult, so they are fixed by the king square** — by the king square *and* `us` for the lance, which only attacks forwards, which is why `LANCE_RAYS` is the one table of the three with a colour row. They were nevertheless going through the live slider backend: for the rook a magic multiply, a shift and two loads for the rank plus the file table, and two such lookups for the bishop. `ROOK_RAYS` / `BISHOP_RAYS` / `LANCE_RAYS` make them one load apiece, at **~5.1 KiB** against the magic backend's ~486 KiB. `movegen/sampled-v1-cb` **−3.4 %**, in-check subset −3.0 %.

- **Rejected: carrying the enemy-slider union from `check_info` to `king_danger`.** `king_danger` builds `their & (Lance|Bishop|Rook|ProBishop|ProRook)` from five reads and four ORs, and `check_info` had already split exactly those five kinds. Passing the union instead (a third `CheckInfo` field) measured **eight of ten ids worse**, up to +5.6 % on `startpos-wi`. **Losing is itself the refutation** — if the union were genuinely being built twice, deleting one construction could not make anything slower. It was not: the five loads read memory nothing writes between the two call sites, and the compiler was already sharing them. What the change did was grow `CheckInfo` from 32 to 48 bytes and thread it through `generate_normal` into `generate_king_moves`.
  - **This bounds Open question 5** rather than settling it: removing the union construction from `king_danger` entirely is worth **≤1 %**.
- **The prediction was 2–3× too large.** It reasoned from `internals/bishop-attacks` at 2.376 ns/call that three magic lookups were 2.5–3.5 ns of `check_info`'s 12.77 ns. Measured, the whole change is ~1.2 ns. The `internals/*` sweep walks 81 origins where `check_info` asks about **one** square every node, so its table lines are already warm — the obvious explanation, **not verified**. See *Standing rules*.
- **Guards, by sabotage.** `empty_board_rays_match_the_naive_backend` holds all three tables to `sliders::naive` over 81 squares × both colours — **to `naive` rather than the live backend, so the guard does not rest on the thing `sliders/tests.rs` is itself checking**. Giving `ROOK_RAYS` the diagonal steps is caught broadly, but **not** by the initial-position or matsuri default-depth perft values.
- ⚠️ **A value consumed only by `king_danger` is nearly invisible to perft.** Dropping `ProRook` from the carried union — so a dragon's sliding attacks never enter the danger set — was caught by the differential **alone**: every unit test passed, every `rules` test passed, and all three deep perft values held. *(The structural reason first given for this was wrong; see the correction in the next entry.)* The fixture list accordingly gained its only position with promoted sliders.

### 2026-08-07 — `king_danger`'s sliders are filtered by where they could bear on the king

The entry above named `king_danger` as the largest single fixed per-node cost (20–29 %) and left two candidates inside it, both about making the *loop body* cheaper. **Neither was the opportunity. The loop's trip count was.** It skipped a step piece unless it stood in `STEP_ATTACKER_ZONE`, but took every enemy slider wherever it stood, on the stated grounds that "sliders reach from anywhere and are always included".

**Reaching from anywhere is not reaching from anywhere *to here*.** The squares a rook or bishop can bear on a *king neighbour* from are as fixed by the king square as a knight's are: the union, over the eight neighbours, of the rays reaching each one on an empty board. `ORTHOGONAL_ATTACKER_ZONE` / `DIAGONAL_ATTACKER_ZONE` are that union, at **2.5 KiB** and a mean of 42.3 and 44.7 of 81 squares. The means average over a wide spread — the diagonal zone is 26 squares in a corner and **65** in the centre — so **the filter earns most of its keep while the king is still at home**. At the initial position **all four** enemy sliders are dropped and the loop runs **zero** times. A lance rides in the orthogonal set (its ray is a subset of the rook's, so the zone is a superset for it) rather than getting a colour-keyed table.

Generation **−16.4 % on the initial position**, −11.2 % matsuri, −9.0 % on the sampled real-game positions, −6.5 % on its in-check subset.

- ⚠️ **The step term now applies to every enemy piece, not only the non-sliders, and that is load-bearing.** A horse's orthogonal sidesteps and a dragon's diagonal ones lie on *neither* piece's rays, so the two slider zones miss them; both reach only a neighbour of a neighbour, which the two-file-by-three-rank box contains. **Deleting the term as redundant is tidying that would silently break this.** `slider_attacker_zones_cover_every_slider` asserts the pair covers every slider attack on every king neighbour over 440,640 configurations, plus the monotonicity of slider attacks in occupancy — which is what makes an empty-board sweep sufficient, and which nothing else in the suite pinned.
- **Correction: a `king_danger` under-report is *not* structurally invisible to perft.** The previous entry reasoned that because `king_danger` only ever subtracts, an omission "produces an illegal king move and no change in node count at all". **The second half does not follow from the first** — an extra generated move *is* an extra node. Measured by sabotage: dropping the step term, swapping the two slider zones, and building the zones from the king's own rays instead of its neighbours' are each rejected by **all three deep perft values**. What the dragon omission demonstrated is narrower and still worth having: perft only reports the mistake where some position in the tree has the omitted piece bearing on a king destination.
- **The `maxmoves` single-position ids moved for reasons that are not this mechanism.** `movegen/maxmoves-cb` reads −13.5…−15.4 %, but that root has **no enemy slider at all**, so the filter cannot save an iteration and strictly adds two loads and two ANDs — the gain is code layout. In the same run `movegen/maxmoves-buf` reproduces **+18.4 % / +21.8 %**, a real regression of the same origin and opposite sign. **Recorded as measured, not as understood.** `perft/maxmoves-cb/2` (−14.4 %) *is* mechanism: at depth 2 the side to move changes and those five sliders become the enemy's.
- **The control is imperfect, and the honest reason is this change rather than the machine.** `internals/bishop-attacks-magic`, which this change cannot reach, reads **+4.4 %** — the suspect being that two new `static` tables shift `.rodata`, and the `internals/*` ids sweep the magic backend's ~486 KiB. The signal is 1.5–4.0× the control drift and opposite in sign, which is what makes the run recordable; the margin is not uniform, and **the in-check subset at 1.5× is the thinnest cell and the first to re-measure if this read is ever doubted.**
