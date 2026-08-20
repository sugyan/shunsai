# shunsai — Decisions

Why the code is the way it is, what was tried and rejected, and what is still open.
Recorded so decisions are revisited deliberately instead of re-litigated.

**What belongs here**: candidates that were *rejected*, coverage holes and open
conditions, corrections spanning two commits, decisions with no code to point at, and
findings that outlived the change that produced them. An adopted change gets a heading,
its commit link, and only what the commit does not hold — retelling one produces a
lossier copy that nothing re-checks, and the 2026-07-30 entry said "roughly a quarter"
where its commit says 23.7 %. Pruned twice for that: 70 % of this log was retelling in
2026-08-12, and again on 2026-08-14. New entries go at the end; supersede by rewriting.

The design is in [DESIGN.md](./DESIGN.md), measurement in
[BENCHMARKS.md](./BENCHMARKS.md), and the rules these follow in
[CLAUDE.md](./CLAUDE.md).

---

## Open questions

The current candidate list. **None of these is measured** — they are sized by disassembly
of the committed bench binary, or bounded by an earlier measurement. The 2026-08-06
decomposition sized `king_danger` and `check_info` on the initial position only; nothing
has sized the rest, so this order is a judgement, not a measurement.
Taking the `zobrist.rs` item said nothing about that ordering either way — it sat
**third** of the ten, so paying −21.7 % is not evidence that a low rank is worth turning
over.

1. **`check_info`'s step-checker half is dead work in 82 % of sampled positions** (33 of 40; 28 of the 31 not in check). Gateable on `their & (king_attacks | knight_attacks)`. This is the one item the 2026-08-06 decomposition ranked, and it ranked it first.
2. **`MoveSet::write_into` is not being inlined.** A standalone symbol at 14–16 call sites, so every materializing listener spills the 48-byte `MoveSet` to the stack and pays a prologue per set. This contradicts the 2026-07-28 finding that the 48 bytes "never materialise" — that was verified on the *counting* listener and holds only there.
3. **The specialized pawn-drop-mate test** — a pawn checks from an *adjacent* square, so the only legal replies are a king move or a capture of the pawn; blocking and drops are impossible. Worth about a fifth of `maxmoves-d2` (it was four fifths before pin legality and the clone removal).
4. **A `Position`-cached slider union.** `king_danger` makes five `piece_kind_bb` reads per node, grouped into two unions. Removing that construction entirely was bounded at **≤1 %** on 2026-08-07, and the grouping into two unions plus the two zone ANDs it cannot touch puts the real upside below that. The trade is the one the cached gold union lost on — it needs a much cheaper maintenance story before it is worth measuring.
5. **Incremental maintenance of the danger bitboard across `do_move`** ([DESIGN.md](./DESIGN.md) §3's "incremental AttackInfo"). Now has a per-node rebuild to beat rather than the old per-destination test.
6. **`Position::remove_from_hand` is the one mutator not inlined**, while `put_piece`, `remove_piece` and `add_to_hand` all are.
7. **`info.pinned.contains(from)`** costs 10 instructions and a branch per non-king origin, unswitched.
8. **`do_move`/`undo_move` has never been optimized.** Flat in every recorded run since the M2 baseline — ~10 % of perft, and, unlike anything else on this list, called at every node of a search.
9. **Copy-make `Position`** — unmeasured, bounded by do-undo's ~10 %, and it must beat `-cb` rather than `-cb-buf`. haitaka runs this branch: its `Board` is a pure value, so it recurses *inside* its listener. Since 2026-08-11 `Position` is a plain value too, so this is a **driver change rather than a redesign**, and nothing is left blocking a measurement.

**Taken:** const-evaluating `zobrist.rs` (2026-08-18, below), which is also the first move on
what is now question 8.

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

rustshogi's `search_moves` is pseudo-legal with no option to enable full legality, so it cannot produce comparable legal-perft numbers. [rshogi](https://github.com/SH11235/rshogi) replaces it; Fairy-Stockfish is a speed-reference cross-check, being a generalized variant engine.

Surveyed and passed over, so the list is not re-derived: the WCSC/Denryu-sen open-source engines reduce to the YaneuraOu / Apery / dlshogi movegen families already covered; Gikou (dormant, no perft), GPS/OSL (dormant), Bonanza (non-OSS license), nozaq/shogi-rs (no true perft).

### 2026-07-23 — Zobrist keys from an inline fixed-seed splitmix64

Not a seedable RNG crate (as the old yasai used): no extra runtime dependency, and keys stay byte-for-byte reproducible independent of any crate's version — `rand`'s `StdRng`/`SmallRng` explicitly do not guarantee algorithm stability across versions. It also converts trivially to `const fn`. splitmix64 is public-domain (CC0), by Sebastiano Vigna.

### 2026-07-24 — movegen/do-undo fixtures come from floodgate real games

Real games over fixed-seed random playouts for two reasons: playout positions skew unrealistic (scattered material, inflated hands), and — decisive — playout *reproduction* depends on `legal_moves()` ordering, so any ordering change would silently change the workload and corrupt the history. Committed SFEN/USI text is stable forever.

Licensing: game records are factual data (positions and moves are not copyrightable expression), the pipeline is our own permissive code, and raw kifu files are never committed.

### 2026-07-27 — attack tables const-evaluated (`LazyLock` dropped); measured neutral, kept anyway

Every id moved within noise: on an already-initialized lock the check is a perfectly predicted branch that LTO hoists out of the hot loops. **Kept because it is a simplification, not a complexity-for-speed trade** — less machinery, no heap init, no runtime indirection — and because the slider backends need this const table-generation infrastructure for their own much larger tables. Recorded so the neutral result is not rediscovered later.

The builders index raw `array_index` arithmetic since `Square::shift`/`Square::all` are not `const fn`; the old `Square`-based builders are retained as test-only references that every const table is asserted equal to.

### 2026-07-27 — slider attacks: magic bitboards adopted over Qugiy, decided by measurement — [`efc399a`](https://github.com/sugyan/shunsai/commit/efc399a) (#4)

**The losing numbers are kept deliberately** (`benches/history/2026-07-27-8de28d8.json`). Qugiy is **within ~10 % of magic while needing no attack tables at all**, against magic's ~486 KiB of `.rodata`. If cache pressure ever outweighs raw latency — a real search, unlike a perft microbenchmark — the decision is worth **re-running rather than re-deriving**.

- **The bake-off ran on the architecture most favourable to qugiy.** Its `o - 2r` needs the board mirrored for the downward ray, i.e. `u128::reverse_bits`, and aarch64 has a single-instruction bit reverse where x86-64 has none — and `line_attacks` mirrors twice per line, so a bishop pays it four times. Magic's cost is architecture-neutral. **A future x86-64 re-run should expect the gap to widen, and should measure `pext` (BMI2) as a third backend** — with the caveat that `pext` is microcoded and slow on AMD Zen1/Zen2.
- **`naive` is the oracle and stays compiled** wherever something can reach it; every backend is asserted equal to it exhaustively over each line's relevant occupancy.

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

- **Drop filtering had to move to bitboards in the same change**, or the grouping is a regression: it made drops pay twice — build the destination bitboard square by square, then walk it again — on the drop-heavy matsuri position.
- **Rejected: external iteration (`impl Iterator<Item = MoveSet>`).** Needs the generator's nested state rewritten as an explicit state machine reloaded on every `next()`, and it does *not* buy back the one thing the callback costs: an iterator borrowing `&Position` blocks `do_move` exactly as the closure does. `gen` blocks would give iterator ergonomics with generator code but remain unstable as of 1.94. Cross-check: haitaka arrives at the same listener shape independently.
- **Two bitboards vs one bitboard plus a flag, checked against haitaka.** haitaka encodes the same information in 32 bytes to our 48. The 16 bytes buy exact O(1) counting: with a single destination set, whether a square yields one or two moves is undecided, so haitaka documents `PieceMoves::len` as *not* the move count and its `ExactSizeIterator` needs per-piece-kind special cases plus a per-destination `PromotionStatus::new`. The one measurement of the two layouts on the materializing path is 2026-07-31's cross-engine run: shunsai's expansion is cheaper per move on matsuri and maxmoves and dearer on startpos. ⚠️ Not settled — see Open question 2.

#### 2026-07-28 — `size_of::<MoveSet>() == 48` is not a cost for a counting listener

The size invites the usual engine instinct that a move object should be small. **That instinct applies to move objects that are *stored*** — yasai's went into move lists, size × count. Verified in the emitted aarch64: for the bulk-count path the call inlines, SROA shreds the struct before it ever has an address, and `piece`/`from`/the tag are dead-code-eliminated. What *is* stored is `shogi_core::Move`, 3 bytes.

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

- **The ceiling is a count, not a percentage, and counting settled it faster than re-measuring.** A counting global allocator over the `-cb` driver reports **931** allocations for startpos-d4, **208** plus ~200 growth reallocs for matsuri-d3, and **1** for maxmoves-d2 — leaves are bulk-counted at depth 1, so only internal nodes collect. Against the measured cost of an allocation that is a fraction of a percent. **Quote the counts**: the share of runtime they buy rises as the crate gets faster.
- **The claimed gains ran *inverse* to allocation density** — startpos allocates far more often per unit of runtime than matsuri yet was credited with much less. **Getting the ordering backwards is the signature of measuring something that is not allocation.**
- **The residual is real and points the other way.** *After pin legality*, `-cb-buf` is a hair slower on startpos-d4: `while i < buf.len()` reloads the length (the recursive call takes `&mut Vec<Move>`, so it cannot be hoisted) and `buf[i]` bounds-checks, where `-cb`'s `for mv in moves` is a pointer bump. That is per *move* against a saving that is per *allocation*, so pin legality shrinking the surrounding work flips the sign.
- **The buffer's own append-only ids are what caught the error**, rather than folding it into `-cb`. The allocations that mattered were inside the library, not in the driver — the next entry.

### 2026-07-29 — the pawn-drop-mate simulation no longer clones; generation allocates nothing — [`4a17c50`](https://github.com/sugyan/shunsai/commit/4a17c50) (#8)

- **This reversed the follow-up.** Before pin legality the mate test was most of `maxmoves-d2` and the allocation a small part of it, which made the specialized mate test the obvious next change. Pin legality made the `has_legal_moves()` walk inside the test far cheaper without touching the clone, so the allocation became the *majority* of what was left. The specialized test is **demoted, not dropped** (Open question 3).
- ⚠️ **Guards, by sabotage.** `position_after_drop_matches_do_move` holds `with_drop` to clone-and-`do_move` over every hand piece on every square it may legally occupy, and `Position`'s `PartialEq` compares every field. What covers this function at all is the differential oracle, `rules::pawn_drop_mate_is_excluded`, and `perft::max_moves_position_deep` — **no default-depth perft value does**, and that deep value is `#[ignore]`d, so CI's `--ignored` step is the only perft guard on pawn-drop-mate exclusion.

### 2026-07-29 — the consumer is a search engine, which re-opens two closed decisions

Every optimization so far was judged against *perft*, which is the measuring instrument, not the customer. The non-goals stand — **what changes is the standard of evidence: "free" now has to mean free under a search.** Neither decision below is reversed; what is withdrawn is the reason to stop looking.

- **`MoveSet`'s 48 bytes are no longer settled** — that entry dismissed the size *because* nothing stores a `MoveSet`, and named a search's move ordering as the caller that would. **Settled again 2026-08-11.**
- **magic-vs-qugiy should be re-run under a search**, which is the shared-cache case a perft microbenchmark cannot create. Both backends stay compiled, so this is a measurement, not a rewrite.
- **`king_danger` produces exactly the attacked-squares set a search wants**, so the largest remaining perft hot spot and the first search-facing API need are the same work. (The rest of what a search needs and perft never exercises is the 2026-07-31 roadmap's E0 and E1 lists.)

### 2026-07-30 — the king's destinations are decided by one danger bitboard — [`05b59e4`](https://github.com/sugyan/shunsai/commit/05b59e4) (#10)

- **On its own the bitboard is a regression on two positions of three, and that is the useful result.** One pass over ~20 enemy pieces is a **fixed** cost where the test it replaces was paid **per candidate square**, and a king has two or three candidates. Filtering the loop — by the two-file by three-rank box a bounded-reach attacker must sit in — is what turned both regressions into gains. *(The slider half of that filter was wrong until 2026-08-07.)*
- **The cost is that `king_danger` returns a partial attack map**, valid only next to the king — and this is the function a search would take its full attacked-squares set from. Dropping the filter is one line but costs what the filter buys. **The condition: a search that wants the full attack map must re-measure this filter, not assume it.**
- **Guards, by sabotage.** Zeroing `danger` is caught by the differential oracle, all three default-depth perft values, and three `rules` tests. Shrinking the box is caught by `step_attacker_zone_covers_every_step_piece` and `rules::distant_knight_still_covers_a_king_escape`, which places a knight at the exact corner. Two tests cover the capture argument **in both directions** — the king may not take a defended pawn and **must** be allowed to take an undefended one. The second is what a merely-conservative danger bitboard would break, and zeroing `danger` does not fail it.
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

- **The `u128` bitboard finally cost something here**: `pop()` walks it directly, and on aarch64 both `u128::trailing_zeros` and `x & (x - 1)` cost roughly twice their 64-bit counterparts.
- **The gain tracks moves per `MoveSet`**, everything removed being per-move — so startpos gains least and the in-check sweep nothing, evasions being restricted to capturing the checker or interposing. **That also resolves an anomaly recorded as a refutation**: maxmoves cost *more* per move than matsuri only because the iterator's per-item drop dispatch fell hardest on the drop-heavy position. With it gone the ordering is monotone in set size again.
- **The theory that motivated the change is still wrong.** The wasted-promotion-pop argument predicted the initial position would gain *most*, since nothing can promote there so all 30 moves pay the failed pop. **startpos gains least.** The change succeeded for a different reason than the one that suggested it.
- **`out.reserve(self.len())` was in the first version and made small-set positions worse** — startpos slightly, the in-check sweep about twice as much. See *What optimizing this crate has taught*.
- ⚠️ **`perft/matsuri-cb/3` is not comparable across this boundary.** It reads high with huge σ under whole-suite conditions but reproduces tightly in isolation at *every* revision tested, and its non-allocating twin is flat throughout. The id is measuring the allocator, not generation. **Which reading is the anomaly is undecided.** Treat that one id's series as broken across 2026-08-03/04.

### 2026-08-06 — the per-*set* path: piece-indexed dispatch and per-origin promotion — [`bdf2836`](https://github.com/sugyan/shunsai/commit/bdf2836) (#15)

**The decomposition came first, and it inverted the plan.** Stubbing a component in `movegen/<pos>-cb` changes no tree shape, and at startpos both `check_info` and `king_danger` are output-preserving, so those cells are exact rather than indicative. Shares of one generation call **on the initial position**, 19 non-king origins:

| component | share |
|---|---|
| `attacks_of` + the target/check/pin masks | **41 %** |
| `emit_normal` + the listener | 21 % |
| `king_danger` | 20 % |
| `check_info` | 15 % |
| the origin bitboard walk + the mailbox load | 12 % |

The three origin-loop rows are additive; the two outer components are separately stubbed and over-count by a few percent, which is why the column sums above 100 %. With 15 of the 19 origins being step pieces, the **step path was costing far more per origin than one indexed table load should** — so the dispatch, not the walk, was the thing to attack. The plan had ranked the walk first.

- **The piece-indexed `attacks_of` costs 40.5 KiB of `.rodata`**, additive to the per-kind tables the reverse-lookup scans still want, which matters only to the deferred magic-versus-qugiy re-run under cache pressure. **It is not the per-piece-kind generation loop rejected on 2026-07-27** — that walked 13 mostly-empty bitboards and lost the single dense pass over `our`; the dense pass is untouched here.
- **Sizing `legal_moves()`'s `Vec` to 593 is not the `out.reserve` rejected above** — that was a per-*set* cost buying nothing; this is one sizing per *call* that removes real reallocations.
- **Rejected, and it is the candidate this work was planned around: moving the origin loops onto `Bitboard::for_each_square`.** On the initial position the delegation itself is neutral, `generate_normal`'s two loops cost about +20 %, and `king_danger`/`check_info`'s scans about +14 %; both shrink on the other two positions, and the scan conversion is roughly neutral on maxmoves. The `generate_normal` conversion costs the *materializing* path as much as the counting one, which is what one would expect if the extra closure layer stops the listener inlining. **The scan conversion touches no listener, and its cost is not explained** — the accumulators becoming `&mut` captures is the untested suspect. **Recorded as measured, not as understood.**
- **Guards, by sabotage.** `attacks_of_matches_the_per_kind_tables` holds the folded dispatch to the `match` it replaced; `piece_index_matches_as_u8` pins the index arithmetic against `Piece::as_u8`, an upstream representation this crate now reads through a mask. **One sabotage is caught by the differential alone**: a row filed under the wrong kind (`ProSilver` given silver's table) moves **no perft value at any of the three fixtures**.

### 2026-08-07 — `check_info`'s sniper scan comes out of ray tables; sharing the slider union is a loss — [`ffd4056`](https://github.com/sugyan/shunsai/commit/ffd4056) (#16)

- **Rejected: carrying the enemy-slider union from `check_info` to `king_danger`.** Both build the same five-kind union, so passing it instead (a third `CheckInfo` field) looked free. It measured **eight of ten ids worse**. **Losing is itself the refutation** — if the union were genuinely being built twice, deleting one construction could not make anything slower. It was not: the five loads read memory nothing writes between the two call sites, and the compiler was already sharing them. What the change did was grow `CheckInfo` from 32 to 48 bytes and thread it deeper. **This bounds Open question 4 at ≤1 %.**
- **The prediction was several times too large**, and the reason generalizes: a per-call figure from the `internals/*` sweep over-states what that call costs in a hot loop. The sweep walks 81 origins where `check_info` asks about **one** square every node, so its table lines are already warm — the obvious explanation, **not verified**.
- **Guards, by sabotage.** `empty_board_rays_match_the_naive_backend` holds all three tables to `sliders::naive` — **to `naive` rather than the live backend, so the guard does not rest on the thing `sliders/tests.rs` is itself checking**. Giving `ROOK_RAYS` the diagonal steps is caught broadly, but **not** by the initial-position or matsuri default-depth perft values.
- ⚠️ **A value consumed only by `king_danger` can hide from every perft value.** Dropping `ProRook` from the carried union — so a dragon's sliding attacks never enter the danger set — was caught by the differential **alone**, with all three deep perft values holding. The reason is coverage, not structure (next entry). The fixture list accordingly gained its only position with promoted sliders.

### 2026-08-07 — `king_danger`'s sliders are filtered by where they could bear on the king — [`1a41320`](https://github.com/sugyan/shunsai/commit/1a41320) (#17)

- ⚠️ **The step term applies to every enemy piece, not only the non-sliders, and that is load-bearing.** A horse's orthogonal sidesteps and a dragon's diagonal ones lie on *neither* piece's rays, so the two slider zones miss them. **Deleting the term as redundant is tidying that would silently break this.** `slider_attacker_zones_cover_every_slider` asserts the pair covers every slider attack on every king neighbour, plus the monotonicity of slider attacks in occupancy — which is what makes an empty-board sweep sufficient, and which nothing else in the suite pinned.
- **Correction: a `king_danger` under-report is *not* structurally invisible to perft.** The reasoning had been that because `king_danger` only subtracts, an omission produces an illegal king move and no change in node count. **That does not follow** — an extra generated move *is* an extra node. Three sabotages of this filter are each rejected by all three deep perft values. What the dragon omission demonstrated is narrower: perft reports the mistake only where some position in the tree has the omitted piece bearing on a king destination.
- **The `maxmoves` single-position ids moved for reasons that are not this mechanism.** That root has **no enemy slider at all**, so the filter cannot save an iteration; `movegen/maxmoves-cb` improved anyway and `movegen/maxmoves-buf` regressed, both from code layout. **Recorded as measured, not as understood.** `perft/maxmoves-cb/2` *is* mechanism: at depth 2 those five sliders become the enemy's.
- **The control is imperfect, and the honest reason is this change rather than the machine.** `internals/bishop-attacks-magic`, which this change cannot reach, moved — the suspect being that two new `static` tables shift `.rodata` under the `internals/*` sweep. The signal is several times the control drift and opposite in sign, which is what makes the run recordable; **the in-check subset is the thinnest cell and the first to re-measure if this read is doubted.**

### 2026-08-11 — v0.1.0's packaging: what ships, what the MSRV is, and what declaring it costs — [`f41ccee`](https://github.com/sugyan/shunsai/commit/f41ccee) (#25)

Three of DESIGN.md §6's release prerequisites, decided together because one CI step checks all three. `Cargo.toml`'s and `ci.yml`'s comments carry what ships and why the resolver is overridden; what follows is what they do not.

- **`include` names the `src` **directory**, not `src/**/*.rs`.** The packaging guard only compiles the default-feature lib, so a generated table or `include_str!` asset dropped by an extension-scoped glob would go unnoticed until a consumer enabled the feature that reads it.
- **`examples/gen_magics.rs` deliberately does not ship**, the provenance guard being repository-level (2026-07-28); the shipped code names it without implying it is present. **Shipping the generator (~9 KiB) stays open** if a downstream ever needs the claim checkable from the artifact alone. Note the class of bug it fixed: a relative-URL link is invisible to `broken_intra_doc_links`, so `-D warnings` cannot catch the next one — `grep -rn '](\.\./' src/` is the check.
- **The MSRV floor is one rewrite away from 1.85**, being a single let-chain in `movegen.rs`'s pawn-drop-mate filter, if a consumer ever needs it. Bracketed rather than reasoned about: 1.85 and 1.87 fail, 1.88 builds.
- ⚠️ **Declaring `rust-version` changed dependency resolution with no artifact to notice it by**: `Cargo.lock` is untracked, so there is no lockfile diff, and **criterion runs from before and after it are not comparable**.
- **The backend flags became exclusive because the priority order standing in for exclusivity let `slider-naive` win**, so every tool handed `--all-features` silently inspected the *oracle* — clippy linted only that configuration, and docs.rs would have documented it. `qugiy.rs` and `naive.rs` lost `#![allow(dead_code)]` in the process and are dead-code-checked for the first time.
- **Rejected: gating `magic` out of the override builds too.** Symmetry would say it should be, but its geometry helpers (`LineKind`, `relevant_mask`, the multipliers) live in `sliders.rs` and `magics.rs`, so removing `magic` under `slider-naive` / `slider-qugiy` leaves those dead — trading one suppressed warning for four.
- ⚠️ **Cost of `compile_error!`, accepted: it is unfriendly under feature unification.** Two crates in one dependency graph each enabling a different flag break the build with no fix available locally — which is the usual reason the priority-order pattern exists. Both flags are documented maintainer-only knobs, and **v0.1.0 freezes them into the published surface**, so the guard belongs before the release rather than after.
- **Rejected: `[package.metadata.docs.rs] all-features = false`.** Written first, then removed — it is docs.rs's default, and the backend selection is `pub(crate)` while docs.rs does not pass `--document-private-items`, so docs.rs could not have rendered the oracle either way. The hazard it claimed to close was never live.
- ⚠️ **Guards, and their edges.** `cargo publish --dry-run` fails on an `include` that drops a source the default-feature **lib** needs, and only that — the lines for dropped examples, tests and benches are warnings cargo exits 0 on. And `main`'s ruleset requires `check` and `msrv` but **not `perft-deep`**, so the `--ignored` step that 2026-07-29 names as the only *perft* guard on pawn-drop-mate exclusion can go red without stopping a merge; what still blocks is `rules::pawn_drop_mate_is_excluded` and the differential oracle, both inside `check`.

### 2026-08-11 — `do_move` returns an `Undo`, so `Position` owns nothing on the heap — [`e039cd7`](https://github.com/sugyan/shunsai/commit/e039cd7) (#22)

- ⚠️ **A guard was spent, not just an API changed.** `states.pop()` panicked on an empty stack, so an unwind past the bottom of that stack announced itself; passing the wrong `Undo` is silent unless it happens to trip `remove_from_hand`'s `hand underflow`. `do_move` is deliberately not `#[must_use]` either — replaying a game forward is a first-class use — so nothing catches a dropped one. Two shapes keep it right: an `Undo` bound in the same scope as its `do_move`, or the caller's own `Vec<Undo>` popped in reverse, which is what `do_undo`'s bench driver does and what `Undo`'s doc comment points a search at.
- **`with_drop` survives** rather than folding into clone-and-`do_move`, which it is now nearly identical to. It touches the pawn-drop-mate hot path, so deleting it wants its own measurement rather than a ride on this one.
- **Unblocks copy-make (open question 9)**, which was described as needing a `Position` redesign and is now a driver change.
- **It costs `do_undo/games-v1` about 4 %, and that is the whole of the durable cost** (measured 2026-08-12, five order-alternating passes per binary: every head reading above every base reading, controls at −1.5..−0.05 %). The `Undo` round-trips through the caller where it used to be written once into the position's own `Vec`. That is the price of the shape, and it gives open question 8 a reason to be taken.
- **The `movegen/*-buf` swings are code layout, not mechanism, and the suite separates them already.** The `perft/*-cb-buf` walks thread the identical buffer through a whole tree and are flat; the single-call `movegen/*-buf` ids, running that same code once per iteration, scatter **in both directions**. A mechanism pushes one way. `movegen/maxmoves-buf` is this crate's most layout-volatile id and should be read as one.
- **Rejected: writing `with_drop`'s copy out again instead of `self.clone()`.** The obvious suspect, since the clone replaced an explicit field copy that inlined. Built and measured three ways against base: it moves nothing (`maxmoves-buf` +19.3 % vs the clone's +20.8 %, `sampled-v1-buf` +8.5 % vs +8.5 %, `do_undo` +3.8 % vs +3.9 %). Every field of `Position` is `Copy`, so the derived `Clone` was already lowering to a copy. **The hypothesis was wrong and the change is not kept** — recorded so it is not re-derived.

### 2026-08-11 — what v0.1.0 freezes, and what it deliberately leaves out — [`67e454a`](https://github.com/sugyan/shunsai/commit/67e454a) (#23)

v0.1.0 is the last point at which an API break is free, so the public surface was audited once, deliberately, rather than an item at a time as each consumer asks. The `states: Vec<State>` move is recorded in its own entry; this one is the rest, including what was left alone and why.

**It found no gap that needs a break.** The instrument was `examples/search.rs`, a search-shaped consumer on default features, which is what makes it real — `internals` is feature-gated, so a slip into crate internals cannot compile there. Everything it wanted and could not get — `gives_check`, `attackers_to`, staged generation, a `Position` → `PartialPosition` conversion — is a pure addition, and under Cargo's semver for 0.x an addition ships in 0.1.x with no version change on the consumer side. **Only breaks have a deadline**, which is why predicting what `rinsai` will need was the wrong question to answer before the freeze.

**`MoveSet` keeps its public fields, and with them its representation.** Rust has no private field on a public variant, so opacity is all-or-nothing, and a consumer's move ordering wants to `match` `Normal` against `Drop`. The 48 bytes follow from the *design* — two destination bitboards — not from field visibility, so hiding them would not buy the freedom to shrink it. What they do buy is concrete: **a caller can filter a set by a target mask today**, which is much of what staged generation would otherwise be asked for first. This closes the `MoveSet`-size question 2026-07-29 reopened.

**`Position` does not derive `Copy`**, though every field now would allow it. At 368 bytes it is small enough to copy deliberately and large enough that copying it *by accident* in a search's hot loop would not announce itself; `.clone()` at a copy-make site puts the cost in the source. Adding `Copy` later is a break — `cargo-semver-checks`' `copy_impl_added` classifies it `Major`, for the closure move-semantics reason in rust-lang/rust#100905 — so it is the freeze window's decision rather than a default, and **the item here most likely to be revisited**, copy-make being exactly the caller that would want it.

**No `Bitboard` u128 interop, and that is not a contradiction to resolve.** `from_bits` is the one constructor that does not structurally guarantee bits 81.. are clear, and only a `debug_assert!` holds it — `for_each_square`'s safety argument names it, and names a **debug** `cargo test` as part of its own guard. A public `from_u128` would hand a release-build consumer unsound `Square` construction, so it would have to be `unsafe` or mask on the way in, and nothing has asked for either. The 2026-07-23 entry's "interop via `to_u128` stays trivial" is a reason to keep the crate's own bitboard, not a promise of shipped API. (`Hash` and a public `MoveSetIter` constructor are likewise additions if ever wanted.)

### 2026-08-11 — provenance scan before v0.1.0

[DESIGN.md](./DESIGN.md) §7 requires one before publishing. Run against the pinned GPL submodules of the local benchmarks repository — apery, apery_rust, YaneuraOu, cshogi, rshogi, Fairy-Stockfish, the old yasai. **The scan lives in that repository**, for the reason the perft harness does: it cannot run without the corpus. Apparatus sits with the corpus; this file keeps the result.

**Result: no verbatim reuse**, every hit accounted for. The 243 magic multipliers appear in none of the seven — the check that carries the weight, and the independent half of a pair with CI's `--check`. Three constants matched cshogi: splitmix64's, which is public domain (CC0). Line overlap at ≥ 40 characters is four lines with yasai and three with rshogi — two and zero at ≥ 60, so it collapses as the threshold rises — each either a signature Rust forces to be written one way or shared *data* — the `use shogi_core::{…}` import both crates need, and the max-moves SFEN.

⚠️ **What it does not establish.** It rules out a pasted table and a copied block, which is the §7 obligation. Being a trimmed-substring search rather than a similarity measure, it would not catch a transliteration that renamed as it went; the defence against that is the incremental history §7 already names. **Re-run before each release** — the corpus moves.

⚠️ **It is deliberately output-safe, and a re-implementation must keep that.** `grep -o` echoes the matched *pattern*, always ours, never the corpus line — so running it does not put GPL source in front of whoever, or whatever, reads the output. CLAUDE.md's top rule forbids the sessions writing this crate from reading those sources at all, which a scan that dumped matching lines would defeat.

### 2026-08-12 — release-plz is wired up, and the release PR needs a token that is not `GITHUB_TOKEN`

The commit convention adopted 2026-08-11 (#24) named release-plz as the machine that
reads the log, and nothing implemented it. This closes that. The job comments in
`release-plz.yml` carry the token rules; these are the constraints nothing in the
repository can state or check.

- ⚠️ **Two halves of the crates.io configuration are invisible from here, and fail silently.** The workflow **file name** is matched by the trusted publisher configuration, so renaming `release-plz.yml` stops publishing; and its environment field must stay empty because the release job declares none. The operator-side procedure is deliberately **not** in this repository, so this bullet is the only warning a future session gets before renaming that file.
- **crates.io authentication is Trusted Publishing, so there is no registry secret at all** — release-plz mints a short-lived token from the job's OIDC identity, and specifically *not* via the `rust-lang/crates-io-auth-action` step, whose API calls it implements itself. **The cost is that v0.1.0 must be published by hand**: crates.io only accepts that configuration against a crate that already exists.
- **The 0.1.0 changelog entry is hand-written, and later ones are not** — release-plz cannot generate an entry for a release it does not cut, and a missing `include` entry is skipped silently rather than failing, so without it the first tarball would have shipped without the file the manifest names. **Its date must match the day of the actual publish.**
- **`cargo-semver-checks` has no baseline at 0.1.0**, so it first does real work at 0.1.1 — which is also when `rinsai` starts consuming releases. Not a gap to close before publishing.

### 2026-08-13 — v0.1.0 is on crates.io, published by hand, and nothing automated has been proven yet

- ⚠️ **The `release` job's first run fails, and that is the ordering rather than a defect to chase.** On the push that merged [#30](https://github.com/sugyan/shunsai/pull/30) it asked crates.io for a Trusted Publishing token, was told `No Trusted Publishing config found for repository sugyan/shunsai`, fell through to `cargo publish` with no registry secret, and exited 1 ([run](https://github.com/sugyan/shunsai/actions/runs/31691190341)). That is the 2026-08-12 constraint on Trusted Publishing seen from the other side, and the order that satisfies it is: publish by hand, configure the trusted publisher against the crate that now exists, and let every later version go out through it.
- **What this release leaves unexercised**: Trusted Publishing itself; `RELEASE_PLZ_TOKEN`, because a `release-pr` run has nothing to open a pull request about while nothing is published; and `cargo-semver-checks`, which only now has a baseline. **0.1.1 is the first release that tests any of the three**, and it is one release PR away.
- **The published tarball was built from a commit that `main` cannot contain.** `.cargo_vcs_info.json` in the uploaded crate names `615f389`, a local commit correcting the changelog date, and `main`'s ruleset allows squash merges only — so that sha never becomes an ancestor of `main`, and no commit on `main` carries the tree that shipped. `v0.1.0` is tagged by hand at [`7e569f8`](https://github.com/sugyan/shunsai/commit/7e569f8) instead — the commit before it, on `main`'s line and one changelog line short of what shipped. The tag is not decoration; the entry below is what it costs to skip it.

### 2026-08-13 — the README is the crate's front page, not a second copy of the record

- **The cross-engine standing came out of the README.** It restated [BENCHMARKS.md](./BENCHMARKS.md), the file that owns measured figures, and a standing written in two places is one that starts disagreeing with itself the next time the harness runs. The README keeps a link and no numbers — and names no other engine: haitaka and apery_rust are what BENCHMARKS.md measures against, and yasai survives only as the 野菜 the name comes from.
- **`#![doc = include_str!("../README.md")]` was rejected.** crates.io resolves a README's relative links against the repository, so `./DECISIONS.md` works there; rustdoc does not, so those same links would break on docs.rs, and the badges and the naming section would land on the crate's front doc page. The crate root keeps its own worked examples, and the README keeps one.

### 2026-08-14 — a hand-published version has to be tagged before the next push to `main`

- ⚠️ **Tag it, or the next release PR is computed from the whole history.** release-plz bounds that range by the previous version's tag, by ancestry of the publish sha in `.cargo_vcs_info.json`, or by a commit whose packaged files equal the registry copy — and a hand publish can miss all three, as this one did. The first release PR listed every commit in the repository as its changelog and took #25's `chore!` with it, proposing **0.2.0 while `cargo-semver-checks` reported the API compatible** ([#32](https://github.com/sugyan/shunsai/pull/32), closed unmerged); tagging `v0.1.0` and re-running the same workflow produced [#33](https://github.com/sugyan/shunsai/pull/33), 0.1.1 with one line. **Nothing in CI notices this** — the release PR is the only place it shows. Pushing the publish commit to any branch would have served as well; it need not be `main`.
- ⚠️ **A stray `!` ships a wrong version, not a bad changelog line.** release-plz takes the bump from the log and raises it only when `cargo-semver-checks` finds the API incompatible; it never lowers one. `!` is a guard `rinsai` depends on, and CLAUDE.md said the opposite until this run.

### 2026-08-14 — the bench feature is hidden from docs.rs by its name; the backend flags deliberately are not

Not the surface 2026-08-11's `all-features` bullet is about. That one is what docs.rs *renders*; this is its feature-flags page, which lists every feature a published crate declares whatever the items behind it are. docs.rs filters that page on the name alone — `Feature::is_private` is `name.starts_with('_')` — and Cargo has no private-feature mechanism ([rust-lang/cargo#10882](https://github.com/rust-lang/cargo/issues/10882)), so the name is the only lever there is. 0.1.0 and 0.1.1 shipped `bench-internals` and their index entries cannot be revised, which is why `CHANGELOG.md`'s `[0.1.0]` note still spells it that way and must keep doing so.

- **Rejected: prefixing `slider-naive` / `slider-qugiy` the same way.** They are functional, API-preserving build knobs — each swaps the live backend at one call site — so a reader of the feature list should see them. Nor would it be free: `cargo-semver-checks`' `feature_missing` exempts only names starting with `_` or matching `^(?:unstable|nightly|bench)(?:[-_].*)?$`. `_bench-internals` is exempt twice over, by its underscore now and its `bench-` prefix before, so dropping the old name is not a break; `slider-*` are exempt by neither, and renaming them would ship 0.2.0.
- ⚠️ **That exemption list constrains every future feature name.** A maintainer-only flag named outside it cannot later be renamed or removed without a major bump, whatever its documentation says it is.
- **What running the override backends in CI is worth is narrower than "it catches backend bugs", and was settled by sabotage.** A broken backend *function* is already caught by plain `cargo test`, the lib's own tests compiling `naive` under `cfg(test)` and holding every backend to it. What the new runs add is a defect **local to the flagged configuration**: gate the same breakage on `cfg(feature = "slider-naive")` and both `cargo test` and `cargo check --lib --features slider-naive` — the step they replace — stay green, while the flagged run fails — **`--lib` among the targets that do**, which is why the runs are the whole suite rather than chosen targets: `tables.rs` re-exports the attack functions from `active`, so the lib's tests reach it too, and a target list is a second place to keep that right. Which targets fail depends on the break: emptying `naive::bishop_attacks` leaves the doc tests green where emptying `rook_attacks` does not. Until this, **nothing in CI had executed** a build in which `sliders::active` is not `magic` — every `cargo bench` step the workflow has ever had is `--no-run`, and `check`, `clippy` and `doc` do not run what they compile. Whether a *local* run ever did is not recorded: `benches/history/*.json` stores no feature set.
- ⚠️ **They cannot catch an `active` arm wired to the wrong backend.** Measured: point the `slider-qugiy` arm at `magic` and every test in the repository stays green, because all three backends are asserted equal. The guard is that a flagged build generates legal moves at all, not that the flag selects the backend it names.

### 2026-08-18 — the v0.1.2 baseline, and const-evaluating the Zobrist keys — [`843a42c`](https://github.com/sugyan/shunsai/commit/843a42c) ([#37](https://github.com/sugyan/shunsai/pull/37))

**The record had no base to read against.** The committed history stopped at #17, so v0.1.0, v0.1.1 and v0.1.2 were all unmeasured, and #22's `Undo` cost lived in prose with no entry to check it. `2026-08-18-e8fe5ed.json` closes that. Read against #17 `do_undo` moves 10.9 → 11.2 ns/pair, which is the right direction and size for that recorded ~4 % — but cross-day, which BENCHMARKS.md calls the weak kind, so it corroborates rather than confirms.

- ⚠️ **Correction: the acquire was never the barrier.** The open question said every `board_key` was "an acquire load" whose barrier stopped the dead-store elimination. This tree emits **no `ldar` and no `dmb` at all** on aarch64 — what stood between the loads and DSE was `LazyLock`'s initialization check. The conclusion was right and the mechanism was described wrongly, which is worth keeping because the same wrong reason would have been re-derived for any other `LazyLock`.
- **The keys had to come out byte-identical, and that is a consumer constraint rather than a correctness one.** Any distinct keys hash correctly, so renumbering the table is invisible here and rebaselines every transposition-table result `rinsai` has recorded. Verified by running both implementations, and guarded by `the_draw_order_is_fixed`, whose expected digest comes from a **separate** implementation of splitmix64 so the test is evidence rather than an echo of the code it guards.
- ⚠️ **That guard was first written to pin three witnesses "bracketing" the sequence — the first draw, the last hand draw, the last draw — and it did not work.** The first and last elements of a sequence are fixed points of *any* permutation of it, so endpoints catch a change in the **number** of draws and never a reordering. Re-nesting the two outer board loops renumbers all 2268 board keys and moves `startpos`'s key from `0xb360d0a33ad0e6a7` to `0xd28066a68dbe9fc9` — and the whole suite stayed green, differential oracle included. It now folds all 2521 entries in canonical index order with an order-sensitive mix, so it reads which key sits in which slot; five sabotages fail it, that one among them. **This file already says a coverage claim is worth what its sabotage showed. The first version shipped the claim without the sabotage.**
- ⚠️ **`perft/maxmoves-mat/2` looked like a separated +2.07 % regression on three readings a side, and is not one.** At eleven readings — four base, seven head — the ranges overlap (135.5–139.6 against 136.3–143.0) though the medians still sit +2.2 % apart. The id is simply unstable: σ to 8.2 % within a pass and 4.3 % across four independent ones, which is why the 2026-08-20 entry says not to read that cell. **Full separation of two triples is weaker evidence than it looks on an id whose spread is of the same order**, and this is the second claim in this entry that more readings changed.
- **`movegen/maxmoves-buf` met neither recordability condition** (σ 15.7 %, 5.7 % across four independent runs), and the history entry's note says so. It is the id 2026-08-11 already named the most layout-volatile here; nothing has explained why it alone behaves this way.
- **What the six-pass alternating shape bought.** Machine load changed *during* the set, and alternating is what makes that harmless — the change lands on base and head alike. The absolute run recorded separately needed its own quiet window, which is a different requirement from the A/B and was learned the expensive way here.
