# shunsai — FAQ

Why the code is the way it is, and not the other way. Written for the question
someone actually asks; if an answer stops being true, fix the answer — `git log`
holds the old one, the pull requests hold what happened, and `gh issue list`
holds what is next.

**A measurement is written once — here, or in [BENCHMARKS.md](./BENCHMARKS.md)
beside the method that produced it, never both.** A figure `benches/history/*.json`
already holds is cited by its bench id rather than repeated. A rule a doc comment
or a manifest already states is in neither.

The plan is [DESIGN.md](./DESIGN.md); the rules a session follows are
[CLAUDE.md](./CLAUDE.md).

## Bitboard and tables

### Why a crate-internal `u128` bitboard rather than `shogi_core::Bitboard`?

`shogi_core` 0.1.5 does ship a `[u64; 2]` bitboard, but the optimization phase
changes exactly this layer, and the *set of operations needed* differs per slider
technique: Qugiy wants byte-swapped pairs and subtraction tricks, magic wants
multiply and shift on raw words, SIMD wants explicit lanes. The upstream is dormant
and cannot be extended.

The `u128` is not free: `MoveSet::write_into` walks one directly, and on aarch64
both `u128::trailing_zeros` and `x & (x - 1)` cost roughly twice their 64-bit
counterparts.

### Why is there no public `Bitboard` ↔ `u128` interop?

`from_bits` is the one constructor that does not structurally guarantee bits 81..
are clear, and only a `debug_assert!` holds it — `for_each_square`'s safety argument
names that assert, and names a **debug** `cargo test` as part of its own guard. A
public `from_u128` would hand a release-build consumer unsound `Square` construction,
so it would have to be `unsafe` or mask on the way in, and nothing has asked for
either. "Interop via `to_u128` stays trivial" is a reason to keep the crate's own
bitboard, not a promise of shipped API.

### Why are the attack tables const-evaluated, when that measured neutral?

Every id moved within noise: on an already-initialized `LazyLock` the check is a
perfectly predicted branch that LTO hoists out of the hot loops. Kept because it is a
**simplification, not a complexity-for-speed trade** — no heap init, no runtime
indirection — and because the slider backends need this infrastructure for their own
much larger tables.

The builders index raw `array_index` arithmetic, since `Square::shift` and
`Square::all` are not `const fn`; the old `Square`-based builders are retained as
test-only references every const table is asserted equal to.

### Why is `attacks_of` a piece-indexed table, when per-piece-kind generation loops were rejected?

Different changes. The rejected one walked the 13 non-king kind bitboards so the kind
would be a loop constant, and lost the single dense pass over `our`: a typical
position spreads ~20 pieces over ~8 kinds, so walking 13 mostly-empty boards
outweighed what it saved. `attacks_of` folds the dispatch **inside** that same dense
pass, which is untouched. It costs 40.5 KiB of `.rodata`, additive to the per-kind
tables the reverse-lookup scans still want — which matters only to the deferred
magic-versus-Qugiy re-run under cache pressure.

**Rejected: moving the origin loops onto `Bitboard::for_each_square`** — the candidate this
work was planned around. The delegation itself is neutral on the initial position, but
`generate_normal`'s two loops cost about +20 % and the `king_danger` / `check_info` scans
about +14 %. The former costs the *materializing* path as much as the counting one, which is
what one would expect if the extra closure layer stops the listener inlining. **The scan
conversion touches no listener and its cost is not explained** — the accumulators becoming
`&mut` captures is the untested suspect. Recorded as measured, not as understood.

The general shape: `for_each_square` earns its keep **draining** a set (30–593
destinations) and loses on a loop of ~20 origins whose body is large. The walk is not
what those loops cost — turning them into closures is.

## Slider backends

### Why is the swap boundary the attack-function signatures rather than a trait over `Bitboard`?

**A trait over `Bitboard` was rejected**: the required operation set varies per
technique, so the abstraction would leak or widen every time, and generics would
either infect the public API (`Position<B>`) or force dyn dispatch into hot loops.

### Why magic bitboards rather than Qugiy — and when is that worth re-running?

Decided by measurement, and **the losing numbers are kept deliberately**
(`benches/history/2026-07-27-8de28d8.json`). Qugiy is **within ~10 % of magic while
needing no attack tables at all**, against magic's ~486 KiB of `.rodata`. If cache
pressure ever outweighs raw latency — a real search, unlike a perft microbenchmark —
the decision is worth **re-running rather than re-deriving**. Both backends stay
compiled, so that is a measurement, not a rewrite. The shared-cache condition is
`rinsai`'s first TT-backed search bench.

**The bake-off ran on the architecture most favourable to Qugiy.** Its `o - 2r` needs
the board mirrored for the downward ray, i.e. `u128::reverse_bits`, and aarch64 has a
single-instruction bit reverse where x86-64 has none — and `line_attacks` mirrors
twice per line, so a bishop pays it four times. Magic's cost is architecture-neutral.
**An x86-64 re-run should expect the gap to widen, and should measure `pext` (BMI2) as
a third backend** — with the caveat that `pext` is microcoded and slow on AMD
Zen1/Zen2.

### Why does `magics.rs` hold only the multipliers?

A multiplier is the output of a search and cannot be derived; a magic's mask and both
shifts *can* be, from the line geometry the crate already computes. So `magic.rs`
derives them at compile time and `magics.rs` holds three bare `[u64; 81]` arrays.
Drift between the generated file and the board geometry is then not detected but
**impossible**.

### Why two guards on the magics, when the build already rejects a bad one?

They answer different questions: the compile-time `magic::line_table` check asks *does
each multiplier work?*, and `gen_magics -- --check` in CI asks *are these **our
generator's** multipliers?*

**The second is needed because the first is satisfiable by numbers we did not produce,
and by far more of them than intuition suggests.** Validity is a *loose* condition: a
magic need not gather the relevant bits perfectly, only avoid mapping two occupancies
with different attacks onto one slot, so constructive collisions are allowed. Measured
over all **15552** single-bit corruptions of the committed constants (243 magics × 64
bits): **11299, or 72.7 %, remain valid** — they compile, build a correct table, and
pass every test including deep perft. **So "it works" is close to no evidence of "we
generated it."** The `--check` guard is a licensing guard, not a correctness one.

### Why does the generator write the file itself, rather than `build.rs` or `quote`?

Redirecting stdout into `magics.rs` cannot work: the shell truncates the file before
cargo builds the library it is part of. Writing to `CARGO_MANIFEST_DIR` directly is
also what makes `--check` possible.

**Rejected: `proc-macro2` / `quote`** — needs `syn` + `prettyplease` for readable
output, renders `u64` in decimal so hex literals would be built as strings anyway, and
would quadruple a dependency tree that is otherwise just `shogi_core`, all to format
243 integers.

**Rejected: `build.rs`** — the search is deterministic, so every downstream build would
repeat work whose answer never changes; and, decisive given that licensing is this
project's top constraint, the constants would no longer be visible in the tree or in a
diff, which is exactly how the "our own generator, not transcribed" claim is
demonstrated.

## The generation API

### Why a callback yielding a `MoveSet` per origin, rather than an iterator?

**Rejected: external iteration (`impl Iterator<Item = MoveSet>`).** It needs the
generator's nested state rewritten as an explicit state machine reloaded on every
`next()`, and it does *not* buy back the one thing the callback costs: an iterator
borrowing `&Position` blocks `do_move` exactly as the closure does. `gen` blocks would
give iterator ergonomics with generator code but remain unstable as of 1.94.
Cross-check: haitaka arrives at the same listener shape independently.

**Drop filtering had to move to bitboards in the same change**, or the grouping is a
regression: it made drops pay twice — build the destination bitboard square by square,
then walk it again — on the drop-heavy matsuri position.

### Why is `MoveSet` 48 bytes, and why not shrink it?

The size invites the usual engine instinct that a move object should be small. **That
instinct applies to move objects that are *stored*** — yasai's went into move lists,
size × count. What this crate stores is `shogi_core::Move`, 3 bytes.

haitaka encodes the same information in 32 bytes. The extra 16 buy exact O(1) counting:
with a single destination set, whether a square yields one or two moves is undecided, so
haitaka documents `PieceMoves::len` as *not* the move count and its `ExactSizeIterator`
needs per-piece-kind special cases plus a per-destination `PromotionStatus::new`.

Two halves of the original argument have since been qualified:

- The premise "nothing collects `MoveSet`s" was withdrawn once the consumer became a
  search — a move ordering collects them.
- The inlining claim holds for the **counting** listener only. There the call inlines,
  SROA shreds the struct before it ever has an address, and `piece` / `from` / the tag
  are dead-code-eliminated. The **materializing** listener spills; see `gh issue list`.

**Do not shrink it speculatively** — that optimizes for a caller that does not exist
yet and costs the `len()`-as-two-popcounts the 48 bytes buy.

**Rejected as a no-op, not a trade-off: `FnMut(&MoveSet)`.** AAPCS already passes
anything over 16 bytes indirectly, and `by_value` / `by_ref` probes of an identical
struct compiled to byte-identical code.

### Why does the listener return `ControlFlow<()>` rather than `bool`?

`true` meaning *stop* is unreadable at the call site, and returning `()` threw away the
one bit the caller most wants: whether the walk finished.

**`ControlFlow<B>` was tried and rejected on inference, not taste.** The overwhelmingly
common call is a full walk in statement position whose result is discarded, leaving `B`
unconstrained and failing with `E0282` (verified on 1.94); every counting caller would
need a turbofish to use an API shape it does not want. A value-carrying `find_move` can
be added later.

### Why does `MoveSet` keep its public fields?

Rust has no private field on a public variant, so opacity is all-or-nothing, and a
consumer's move ordering wants to `match` `Normal` against `Drop`. The 48 bytes follow
from the *design* — two destination bitboards — not from field visibility, so hiding
them would not buy the freedom to shrink it. What they do buy is concrete: **a caller
can filter a set by a target mask today**, which is much of what staged generation
would otherwise be asked for first.

### Why does `write_into` decide drop-versus-board once per set?

Everything it removes is per-move, so **the gain tracks moves per `MoveSet`** — the
initial position gains least and the in-check sweep nothing, evasions being restricted
to capturing the checker or interposing. That also resolves an anomaly once recorded as
a refutation: maxmoves cost *more* per move than matsuri only because the iterator's
per-item drop dispatch fell hardest on the drop-heavy position.

**The theory that motivated the change is still wrong.** The wasted-promotion-pop
argument predicted the initial position would gain *most*, since nothing can promote
there so all 30 moves pay the failed pop. startpos gains least. The change succeeded
for a different reason than the one that suggested it.

⚠️ **`perft/matsuri-cb/3` is not comparable across that change.** It reads high with
huge σ under whole-suite conditions but reproduces tightly in isolation at *every*
revision tested, and its non-allocating twin is flat throughout. The id is measuring
the allocator, not generation. **Which reading is the anomaly is undecided.**

### Why is there no `reserve` before a bulk push?

`write_into`'s own doc carries the argument; what it does not carry is the measurement.
`out.reserve(self.len())` was in the first version and made small-set positions worse:
startpos slightly, the in-check sweep about twice as much.

Sizing `legal_moves()`'s `Vec` to 593 is **not** the same thing: that is one sizing per
*call* removing real reallocations, not a per-*set* cost buying nothing.

### Why does threading a reusable buffer through perft buy nothing?

It was once credited with a matsuri-d3 gain; that was an artifact of single-shot timing
and is withdrawn. **The ceiling is a count, not a percentage, and counting settled it
faster than re-measuring.** A counting global allocator over the `-cb` driver reports
**931** allocations for startpos-d4, **208** plus ~200 growth reallocs for matsuri-d3,
and **1** for maxmoves-d2 — leaves are bulk-counted at depth 1, so only internal nodes
collect. **Quote the counts, not a percentage**: the share of runtime they buy rises as
the crate gets faster.

Two things generalize:

- **The claimed gains ran *inverse* to allocation density.** startpos allocates far more
  often per unit of runtime than matsuri yet was credited with much less. Getting the
  ordering backwards is the signature of measuring something that is not allocation.
- **The residual is real and points the other way.** After pin legality, `-cb-buf` is a
  hair slower on startpos-d4: `while i < buf.len()` reloads the length (the recursive
  call takes `&mut Vec<Move>`, so it cannot be hoisted) and `buf[i]` bounds-checks,
  where `-cb`'s `for mv in moves` is a pointer bump. That is per *move* against a saving
  that is per *allocation*, so shrinking the surrounding work flips the sign.

The buffer's own append-only ids are what caught the error, rather than folding it into
`-cb`.

## Legality and check information

### Why does one danger bitboard decide the king's destinations, and why is it filtered?

**On its own the bitboard is a regression on two positions of three, and that is the
useful result.** One pass over ~20 enemy pieces is a **fixed** cost where the test it
replaces was paid **per candidate square**, and a king has two or three candidates.
Filtering the loop — by the box a bounded-reach attacker must sit in — is what turned
both regressions into gains.

The general shape, which is not unconditionally good: **"replace a per-item test with
one bulk computation" trades a per-item cost for a fixed one, and loses wherever the
item count is small.**

**The `maxmoves` single-position ids moved for reasons that are not this mechanism.**
That root has no enemy slider at all, so the filter cannot save an iteration;
`movegen/maxmoves-cb` improved anyway and `movegen/maxmoves-buf` regressed, both from
code layout. Recorded as measured, not as understood. `perft/maxmoves-cb/2` *is*
mechanism: at depth 2 those five sliders become the enemy's.

### Why does `king_danger` return a partial attack map?

Because of that filter: the map is valid only next to the king. This is the function a
search would take its full attacked-squares set from, and dropping the filter is one
line — but it costs what the filter buys. **The condition: a search that wants the full
attack map must re-measure this filter, not assume it.** That condition may never fire
from evaluation at all, NNUE consuming piece placement rather than an attack map, which
leaves move ordering as its only plausible claimant.

### Why doesn't `check_info` hand its slider union to `king_danger`?

**Rejected, and losing is itself the refutation.** Both build the same five-kind union,
so passing it instead — a third `CheckInfo` field — looked free. It measured **eight of
ten ids worse**. If the union were genuinely being built twice, deleting one
construction could not make anything *slower*. It was not: the five loads read memory
nothing writes between the two call sites, and the compiler was already sharing them.
What the change did was grow `CheckInfo` from 32 to 48 bytes and thread it deeper.

This is the reference result for **"the source computes this twice"**, which is
re-derivable by reading two functions side by side and can be wrong. It also bounds a
`Position`-cached slider union at **≤1 %**.

**The prediction was several times too large**, and the reason generalizes: a per-call
figure from the `internals/*` sweep over-states what that call costs in a hot loop. The
sweep walks 81 origins where `check_info` asks about **one** square every node, so its
table lines are already warm — the obvious explanation, **not verified**.

### Why is caching a union in `Position` the wrong side of a trade?

Measured, and this is the reference trade for any future "cache it in `Position`"
proposal. A cached gold union (the five gold-moving kinds OR-ed once and maintained by
`put_piece` / `remove_piece`) did what it promised — `internals/attackers-to` improved —
but maintaining it cost **`do_undo` +7 %** and the recommended perft path came out
slower. A branchless variant recovered the perft loss but left `do_undo` down. **The
union is four ORs of already-hot bitboards; paying for it on every piece placement to
save it on every attack query is the wrong side of that trade.**

### Why does the pawn-drop-mate simulation not clone?

It used to, and removing the clone is what made generation allocate nothing. `with_drop`
survives rather than folding into clone-and-`do_move`, which it is now nearly identical
to: it touches the pawn-drop-mate hot path, so deleting it wants its own measurement
rather than a ride on someone else's.

⚠️ **What covers this at all** is the differential oracle,
`rules::pawn_drop_mate_is_excluded`, and `perft::max_moves_position_deep` — **no
default-depth perft value does**, and that deep value is `#[ignore]`d, so CI's
`--ignored` step is the only *perft* guard on pawn-drop-mate exclusion. `main`'s ruleset
does not require `perft-deep`, so that step can go red without stopping a merge; what
still blocks is the other two, both inside `check`.

### Why does `do_move` return an `Undo`?

So `Position` owns nothing on the heap: the `states: Vec<State>` stack moved out to the
caller. That break was spent before v0.1.0, when it was still free.

⚠️ **A guard was spent, not just an API changed.** `states.pop()` panicked on an empty
stack, so an unwind past the bottom of that stack announced itself; passing the wrong
`Undo` is silent unless it happens to trip `remove_from_hand`'s `hand underflow`.
`do_move` is deliberately not `#[must_use]` either — replaying a game forward is a
first-class use — so nothing catches a dropped one.

**It costs `do_undo/games-v1` about 4 %, and that is the whole of the durable cost** — the
`Undo` round-trips through the caller where it used to be written once into the position's
own `Vec`. (Five order-alternating passes per binary, every head reading above every base
reading, controls at −1.5..−0.05 %.)

**Rejected: writing `with_drop`'s copy out by hand instead of `self.clone()`** — the obvious
suspect, since the clone replaced an explicit field copy that inlined. Measured three ways
against base: it moves nothing. Every field of `Position` is `Copy`, so the derived `Clone`
was already lowering to a copy.

**The `movegen/*-buf` swings around that change are code layout, not mechanism.** The
`perft/*-cb-buf` walks thread the identical buffer through a whole tree and are flat; the
single-call `movegen/*-buf` ids scatter **in both directions**, and a mechanism pushes one
way.

### Why doesn't `Position` derive `Copy`?

Every field now would allow it. At 368 bytes it is small enough to copy deliberately and
large enough that copying it *by accident* in a search's hot loop would not announce
itself; `.clone()` at a copy-make site puts the cost in the source. Adding `Copy` later
is a break — `cargo-semver-checks`' `copy_impl_added` classifies it `Major`, for the
closure move-semantics reason in
[rust-lang/rust#100905](https://github.com/rust-lang/rust/issues/100905) — so it was the
freeze window's decision rather than a default, and **the item most likely to be
revisited**, copy-make being exactly the caller that would want it.

## Fixtures and guards

### Why do the fixtures come from real games rather than random playouts?

Two reasons, the second decisive: playout positions skew unrealistic (scattered
material, inflated hands), and playout *reproduction* depends on `legal_moves()`
ordering, so any ordering change would silently change the workload and corrupt the
history. Committed SFEN/USI text is stable forever.

Licensing: game records are factual data — positions and moves are not copyrightable
expression — the pipeline is our own permissive code, and raw kifu files are never
committed.

### What do the guards actually cover, and what has slipped past them?

**Establish a guard's worth by sabotage.** Every coverage claim here was made by breaking
the code and watching which tests fail. An assertion that a rare configuration was
*reached* is what stops a fixture list drifting into silent non-coverage.

**Perft is a real net but a coverage-dependent one.** It only reports a mistake where
some position in the tree actually exercises it, and which holes it has is not
guessable:

| sabotage | what caught it |
|---|---|
| `checkers \|= single(sniper)` → `=` | **nothing.** A double check by *two sliders* is reachable from no fixture and from none of the three deep perft trees. Two fixtures now close it, and the test **asserts it reached each configuration**. |
| a `STEP_ATTACKS` row filed under the wrong kind (`ProSilver` given silver's table) | the `shogi_legality_lite` differential **alone** — it moves no perft value at any of the three fixtures. |
| dropping `ProRook` from `king_danger`'s carried union | the differential **alone**. The fixture list accordingly gained its only position with promoted sliders. |
| giving `ROOK_RAYS` the diagonal steps | broadly — but **not** by the initial-position or matsuri default-depth perft values. |

**Correction: a `king_danger` under-report is *not* structurally invisible to perft.** The
reasoning had been that since `king_danger` only subtracts, an omission yields an illegal
king move and no change in node count. That does not follow — an extra generated move *is*
an extra node, and three sabotages of the slider filter are each rejected by all three deep
values. The dragon omission shows something narrower: perft reports the mistake only where
some position has the omitted piece bearing on a king destination.

`empty_board_rays_match_the_naive_backend` holds all three ray tables to `sliders::naive`
— **to `naive` rather than the live backend, so the guard does not rest on the thing
`sliders/tests.rs` is itself checking**.

### Why are the Zobrist keys from an inline splitmix64, and why const?

Not a seedable RNG crate: no extra runtime dependency, and keys stay byte-for-byte
reproducible independent of any crate's version — `rand`'s `StdRng` / `SmallRng` explicitly
do not guarantee algorithm stability across versions. It also converts trivially to
`const fn`, which is what was eventually cashed in. splitmix64 is public domain (CC0), by
Sebastiano Vigna.

**The keys had to come out byte-identical, which is a consumer constraint rather than a
correctness one.** Any distinct keys hash correctly, so renumbering is invisible here and
rebaselines every transposition-table result `rinsai` has recorded.

⚠️ **`the_draw_order_is_fixed` was first written to pin three witnesses "bracketing" the
sequence — first draw, last hand draw, last draw — and it did not work.** The first and last
elements of a sequence are fixed points of *any* permutation of it, so endpoints catch a
change in the **number** of draws and never a reordering. Re-nesting the two outer board
loops renumbers every board key and moves `startpos`'s from `0xb360d0a33ad0e6a7` to
`0xd28066a68dbe9fc9` — and the whole suite stayed green, differential oracle included.

**Correction: the acquire was never the barrier.** The open question this answered called
every `board_key` read "an acquire load" whose barrier stopped dead-store elimination. This
tree emits **no `ldar` and no `dmb` at all** on aarch64 — what stood between the loads and
DSE was `LazyLock`'s initialization check. The conclusion was right and the mechanism
described wrongly, worth keeping because the same wrong reason would be re-derived for any
other `LazyLock`.

## Packaging and releasing

### How is a release cut, and what can go wrong?

[release-plz](https://release-plz.dev) reads the commit log, opens a release PR, and
publishes over Trusted Publishing; `release-plz.yml`'s comments carry the token rules. What
follows is what nothing in the repository can state or check.

⚠️ **Two halves of the crates.io configuration are invisible from here and fail
silently.** The workflow **file name** is matched by the trusted publisher configuration,
so renaming `release-plz.yml` stops publishing; and its environment field must stay empty
because the release job declares none. The operator-side procedure is deliberately not in
this repository, so this is the only warning a future session gets before renaming that
file.

⚠️ **A hand-published version has to be tagged before the next push to `main`.**
release-plz bounds a release's range by the previous tag, by ancestry of the publish sha in
`.cargo_vcs_info.json`, or by a commit whose packaged files equal the registry copy — and a
hand publish can miss all three. v0.1.0 did: the first release PR took the whole repository
as its changelog and a `chore!` with it, proposing **0.2.0 while `cargo-semver-checks`
reported the API compatible**. **Nothing in CI notices this**; the release PR is the only
place it shows.

⚠️ **A stray `!` ships a wrong version, not a bad changelog line.** release-plz takes the
bump from the log and raises it only when `cargo-semver-checks` finds the API
incompatible; it never lowers one. `!` is a guard `rinsai` depends on.

v0.1.0 was **published by hand**, crates.io only accepting a Trusted Publishing
configuration against a crate that already exists, and its tarball was built from a commit
`main` cannot contain — `main` allows squash merges only. It is tagged by hand at
[`7e569f8`](https://github.com/sugyan/shunsai/commit/7e569f8), one changelog line short of
what shipped, and that entry stays hand-written: release-plz cannot generate one for a
release it did not cut.

### What does the packaging guard not catch?

Its limits are stated on the step itself in `ci.yml`. The consequence is that `include`
names the `src` **directory** rather than `src/**/*.rs`: an asset dropped by an
extension-scoped glob would go unnoticed until a consumer enabled the feature that reads it.

**`examples/gen_magics.rs` deliberately does not ship**, the provenance guard being
repository-level. Shipping it (~9 KiB) stays open if a downstream ever needs the claim
checkable from the artifact alone. Note the class of bug that closed: a relative-URL link is
invisible to `broken_intra_doc_links`, so `-D warnings` cannot catch the next one —
`grep -rn '](\.\./' src/` is the check.

⚠️ **Declaring `rust-version` changed dependency resolution with no artifact to notice it
by**: `Cargo.lock` is untracked, so there is no lockfile diff, and **criterion runs from
before and after it are not comparable**. The floor is bracketed rather than reasoned
about: 1.85 and 1.87 fail, 1.88 builds.

### Why aren't the backend flags renamed the way the bench feature was?

They are functional, API-preserving build knobs — each swaps the live backend at one call
site — so a reader of the feature list should see them. Nor would renaming be free:
`cargo-semver-checks`' `feature_missing` exempts only names starting with `_` or matching
`^(?:unstable|nightly|bench)(?:[-_].*)?$`. `_bench-internals` is exempt twice over, so
dropping its old name was not a break; `slider-*` are exempt by neither, and renaming them
would ship 0.2.0. ⚠️ **That exemption list constrains every future feature name.** A
maintainer-only flag named outside it cannot later be renamed or removed without a major
bump, whatever its documentation says it is.

0.1.0 and 0.1.1 shipped `bench-internals` unprefixed and their docs.rs index entries cannot
be revised, which is why `CHANGELOG.md`'s `[0.1.0]` note still spells it that way.

**Rejected: gating `magic` out of the override builds too.** Its geometry helpers
(`LineKind`, `relevant_mask`, the multipliers) live in `sliders.rs` and `magics.rs`, so
removing `magic` under `slider-naive` / `slider-qugiy` leaves those dead — trading one
suppressed warning for four.

**Rejected: `[package.metadata.docs.rs] all-features = false`.** It is docs.rs's default,
and the backend selection is `pub(crate)` while docs.rs does not pass
`--document-private-items`, so docs.rs could not have rendered the oracle either way. The
hazard it claimed to close was never live.

### What is running the override backends in CI actually worth?

Narrower than "it catches backend bugs", and settled by sabotage. A broken backend
*function* is already caught by plain `cargo test`, the lib's own tests compiling `naive`
under `cfg(test)` and holding every backend to it. What the flagged runs add is a defect
**local to the flagged configuration** — gate the same breakage on `cfg(feature =
"slider-naive")` and only the flagged run fails. Hence the whole suite rather than chosen
targets: `tables.rs` re-exports the attack functions from `active`, so the lib's tests reach
it too.

⚠️ **They cannot catch an `active` arm wired to the wrong backend.** Measured: point the
`slider-qugiy` arm at `magic` and every test in the repository stays green, because all
three backends are asserted equal. The guard is that a flagged build generates legal moves
at all, not that the flag selects the backend it names.

### What does the provenance scan establish, and what does it not?

**The scan lives in the local-only benchmarks repository**, for the reason the perft
harness does: it cannot run without the GPL corpus. Apparatus sits with the corpus; this
file keeps the result.

**Result: no verbatim reuse**, every hit accounted for. The 243 magic multipliers appear in
none of the seven pinned GPL projects — the check that carries the weight, and the
independent half of a pair with CI's `--check`. Three constants matched cshogi:
splitmix64's, public domain. Line overlap at ≥ 40 characters is four lines with yasai and
three with rshogi — two and zero at ≥ 60, so it collapses as the threshold rises — each
either a signature Rust forces to be written one way, or shared *data*.

⚠️ **What it does not establish.** It rules out a pasted table and a copied block, which is
the obligation. Being a trimmed-substring search rather than a similarity measure, it would
not catch a transliteration that renamed as it went; the defence against that is the
incremental history. **Re-run before each release** — the corpus moves.

⚠️ **It is deliberately output-safe, and a re-implementation must keep that.** `grep -o`
echoes the matched *pattern*, always ours, never the corpus line — so running it does not
put GPL source in front of whoever, or whatever, reads the output. CLAUDE.md's top rule
forbids the sessions writing this crate from reading those sources at all, which a scan
that dumped matching lines would defeat.

## Working in this repository

### Why is the README not a second copy of the record?

The cross-engine standing came out of it because that restated BENCHMARKS.md, the file
that owns measured figures, and a standing written in two places is one that starts
disagreeing with itself the next time the harness runs. The README keeps a link and no
numbers — and names no other engine: haitaka and apery_rust are what BENCHMARKS.md
measures against, and yasai survives only as the 野菜 the name comes from.

**`#![doc = include_str!("../README.md")]` was rejected.** crates.io resolves a README's
relative links against the repository; rustdoc does not, so those same links would break
on docs.rs, and the badges and the naming section would land on the crate's front doc
page.

### Why is there no prose plugin, reviewer agent or CI job for the hooks?

**Rejected: the `prose-discipline` plugin.** Its marketplace is a local directory with no
remote, so a cloud checkout resolved none of it — skill, agent, commands and hook alike,
silently. Over this whole tree its checker returns **two hits, both false positives**, and
zero true ones. `rinsai` measured the other half: its review loop ran ten passes on one
branch, **five of which changed no executable line**. Publishing the marketplace fixes the
cloud half and keeps the loop that is the cost.

**Rejected: a CI job for either hook, and this one is settled rather than deferred.**
These checks exist because an *agent* is careless in ways someone writing this by hand is
not; the repository's CI is the project's, and a machine's own discipline is the wrong
thing to put in a job every contributor pays for. **A check that only matters because
Claude Code is editing lives in a hook and nowhere else.** Both scripts take `--all` so a
CI step remains one line away if that ever changes.

**What the hooks are worth, measured rather than assumed.** Before the comment rule was
written, three `.rs` comments carried a measured figure and one an instruction the code
cannot enforce; none has since. The history-narration check has **never fired** — kept
because a grep is free, not because anything says it works. `corpus-path.sh` has never
fired either, and by construction cannot on a checkout without the corpus.

⚠️ **What a hook costs, stated rather than glossed.** It fires only when Claude Code does
the editing. A hand-written commit, another tool and CI see none of it, and nothing obliges
anyone to run `--all`.

### Why is the backlog a GitHub issue rather than a section here?

Because "what is next" and "why the code is this way" go stale on different clocks, and a
candidate list kept beside the answers is the part that regrew this file twice. The
optimization candidates are `gh issue list`; each carries what bounds it, what sized it,
and which caller is waiting.

What that costs, and it is real: **an issue is not read before touching the code, is not in
the diff, and is not seen by the hooks.** A cloud session carries no memory outside the
repository, so it must go and look. Nothing that has to be *true now* went there — this
file and CLAUDE.md keep that.

**Reopens if** a candidate turns out to need a fact with no home in either place.

### Why did the decision log become a FAQ?

It was maintained more than it was used: 62 KB across 35 dated entries, with the rejected
candidates scattered through them and the open-questions preamble rather than collected
anywhere — unreadable against the one question the file existed to answer, *was this
already tried?* It had been pruned twice for exactly that and regrew both times.

Dissolved: the dates, the supersession machinery, the retelling of commits git already
holds. Kept: every measured figure with no other home, every rejected alternative, every
reopening condition, and every coverage hole a sabotage established. Written against the
tree rather than transcribed — moving prose is what produces false claims when a document
is shortened.
