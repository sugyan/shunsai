# shunsai — FAQ

Why the code is the way it is, and not the other way. Written for the question someone
actually asks; if an answer stops being true, fix the answer.

**This file is an index, not a record.** This project writes its reasoning into commit
messages, so the argument, the measurement and the conditions of a run live there — an
answer here names the commit and adds only what the commit does not hold: a later
correction, a condition that would reopen it, or a view that spans several commits.
`git show <sha>` is the second half of every answer below. `gh issue list` holds what is
next; `benches/history/*.json` holds every recorded figure.

The plan is [DESIGN.md](./DESIGN.md); measurement is [BENCHMARKS.md](./BENCHMARKS.md);
the rules a session follows are [CLAUDE.md](./CLAUDE.md).

## Bitboard and tables

### Why a crate-internal `u128` bitboard rather than `shogi_core::Bitboard`?

`shogi_core` 0.1.5 does ship a `[u64; 2]` bitboard, but the *set of operations needed*
differs per slider technique — Qugiy wants byte-swapped pairs, magic wants multiply and
shift on raw words, SIMD wants lanes — and the upstream is dormant, so it cannot be
extended.

⚠️ The `u128` is not free, and the bill arrives in the expansion loop: on aarch64 both
`u128::trailing_zeros` and `x & (x - 1)` cost roughly twice their 64-bit counterparts
([`aa44f8b`](https://github.com/sugyan/shunsai/commit/aa44f8b)).

### Why is there no public `Bitboard` ↔ `u128` interop?

`from_bits` is the one constructor that does not structurally guarantee bits 81.. are
clear, and only a `debug_assert!` holds it — so a public `from_u128` would hand a
release-build consumer unsound `Square` construction. It would have to be `unsafe` or mask
on the way in, and nothing has asked for either
([`67e454a`](https://github.com/sugyan/shunsai/commit/67e454a)).

### Why are the attack tables const-evaluated, when that measured neutral?

Because it is a **simplification, not a complexity-for-speed trade** — no heap init, no
runtime indirection — and because the slider backends need the same infrastructure for
their own much larger tables. Recorded so the neutral result is not rediscovered as a
reason to undo it.

### Why is `attacks_of` a piece-indexed table, when per-piece-kind generation loops were rejected?

Different changes. The rejected one walked the 13 non-king kind bitboards and lost the
single dense pass over `our`; `attacks_of` folds the dispatch **inside** that same dense
pass ([`bdf2836`](https://github.com/sugyan/shunsai/commit/bdf2836), which also carries
the cost decomposition that reordered the work and the `.rodata` it costs).

**Rejected there and worth not re-deriving: moving the origin loops onto
`Bitboard::for_each_square`.** The general shape is that `for_each_square` earns its keep
**draining** a set (30–593 destinations) and loses on a loop of ~20 origins whose body is
large — the walk is not what those loops cost, turning them into closures is. ⚠️ That
explains the generation loops and **not** the scan loops, which touch no listener and got
dearer anyway. Measured, not understood.

## Slider backends

### Why is the swap boundary the attack-function signatures rather than a trait over `Bitboard`?

**A trait over `Bitboard` was rejected**: the required operation set varies per technique,
so the abstraction would leak or widen every time, and generics would either infect the
public API (`Position<B>`) or force dyn dispatch into hot loops. `src/sliders.rs` states
what each backend may assume.

### Why magic bitboards rather than Qugiy — and when is that worth re-running?

Adopted by bake-off, and **the losing numbers are kept deliberately**
(`benches/history/2026-07-27-8de28d8.json`,
[`efc399a`](https://github.com/sugyan/shunsai/commit/efc399a) for the trade and the
architecture argument).

What the commit does not hold is the **condition**: Qugiy is close enough, and needs no
tables at all, that the decision is worth **re-running rather than re-deriving** the first
time cache pressure outweighs raw latency — which a perft microbenchmark cannot create.
Both backends stay compiled, so that is a measurement, not a rewrite. The trigger is
`rinsai`'s first TT-backed search bench; an x86-64 run should measure `pext` (BMI2) as a
third backend at the same time.

### Why does `magics.rs` hold only the multipliers, and why two guards on them?

A multiplier is the output of a search; a magic's mask and both shifts can be derived from
geometry the crate already computes, so drift between the generated file and the board is
not detected but impossible. The two guards answer different questions — *does each
multiplier work?* (compile time, always) and *are these **our generator's** multipliers?*
(`gen_magics -- --check`, CI only) — and
[`efc399a`](https://github.com/sugyan/shunsai/commit/efc399a) carries the measurement that
settles why both are needed: validity is a loose condition, so most single-bit corruptions
of the committed constants still compile, build a correct table and pass every test.

**So "it works" is close to no evidence of "we generated it."** The `--check` guard is a
licensing guard, not a correctness one. Keep both.

### Why does the generator write the file itself, rather than `build.rs` or `quote`?

Redirecting stdout into `magics.rs` cannot work — the shell truncates the file before
cargo builds the library it is part of — and writing to `CARGO_MANIFEST_DIR` is what makes
`--check` possible. **`build.rs` was rejected** for the reason that outranks the others
here: the constants would no longer be visible in the tree or in a diff, which is exactly
how the "our own generator, not transcribed" claim is demonstrated. **`quote` was
rejected** as four dependencies to format 243 integers.

## The generation API

### Why a callback yielding a `MoveSet` per origin, rather than an iterator?

**Rejected: external iteration (`impl Iterator<Item = MoveSet>`).** It needs the
generator's nested state rewritten as a state machine reloaded on every `next()`, and it
does not buy back the one thing the callback costs — an iterator borrowing `&Position`
blocks `do_move` exactly as the closure does. `gen` blocks would give iterator ergonomics
with generator code but remain unstable as of 1.94. haitaka arrives at the same listener
shape independently ([`86168d2`](https://github.com/sugyan/shunsai/commit/86168d2)).

### Why is `MoveSet` 48 bytes, and why not shrink it?

The size invites the instinct that a move object should be small. **That instinct applies
to move objects that are *stored*** — what this crate stores is `shogi_core::Move`, 3
bytes. The extra 16 over haitaka's 32 buy exact O(1) counting.

Two things qualify the original argument and are the reason not to reopen it casually:

- "Nothing collects `MoveSet`s" was withdrawn once the consumer became a search — a move
  ordering collects them.
- The inlining that makes the size free holds for the **counting** listener only; the
  materializing one spills ([#41](https://github.com/sugyan/shunsai/issues/41)).

⚠️ **Do not shrink it speculatively.** That optimizes for a caller that does not exist yet
and costs the `len()`-as-two-popcounts the 48 bytes buy.

### Why does the listener return `ControlFlow<()>` rather than `bool`?

`true` meaning *stop* is unreadable at the call site, and `()` throws away the one bit the
caller most wants. **`ControlFlow<B>` was tried and rejected on inference, not taste**: the
common call is a full walk in statement position whose result is discarded, leaving `B`
unconstrained and failing with `E0282`
([`86168d2`](https://github.com/sugyan/shunsai/commit/86168d2)). A value-carrying
`find_move` can be added later.

### Why does `MoveSet` keep its public fields?

Rust has no private field on a public variant, so opacity is all-or-nothing, and a
consumer's move ordering wants to `match` `Normal` against `Drop`. The 48 bytes follow
from two destination bitboards, not from field visibility, so hiding them would buy no
freedom to shrink. What they do buy: **a caller can filter a set by a target mask today**,
which is much of what staged generation would otherwise be asked for first
([`67e454a`](https://github.com/sugyan/shunsai/commit/67e454a)).

### Why does `write_into` decide drop-versus-board once per set?

Because the expansion loop, not the allocation, is where materializing a move goes — and
nothing had ever optimized it
([`aa44f8b`](https://github.com/sugyan/shunsai/commit/aa44f8b) carries the mechanism, the
measurements and the two findings that outlived it: the gain tracks moves per `MoveSet`,
and the theory that motivated the change is still wrong about the initial position).

⚠️ **`perft/matsuri-cb/3` is not comparable across that change.** It reads high with huge σ
under whole-suite conditions but reproduces tightly in isolation at every revision tested,
and its non-allocating twin is flat throughout — so the id is measuring the allocator, not
generation. Which reading is the anomaly is undecided; treat that one id's series as broken
there.

### Why is there no `reserve` before a bulk push?

`Vec::push` checks capacity per element either way, so reserving only avoids a
reallocation — and `write_into` is for callers that own a sized buffer, where there is
none to avoid. A `reserve` was also in the first version and made small-set positions
worse, so this is a measured rejection rather than a judgement
([`aa44f8b`](https://github.com/sugyan/shunsai/commit/aa44f8b)). Sizing `legal_moves()`'s
`Vec` to 593 is **not** the same thing — one sizing per *call* removing real reallocations,
against a per-*set* cost buying nothing.

### Why does threading a reusable buffer through perft buy nothing?

Because only internal nodes allocate once leaves are bulk-counted, and the counts are tiny
— 931 for startpos-d4, 1 for maxmoves-d2. **The durable form of that bound is the count,
not a percentage**: the share of runtime it buys rises as the crate gets faster, so quote
the counts.

The `-cb-buf` ids are kept as the standing evidence, and as the baseline a copy-make driver
([#48](https://github.com/sugyan/shunsai/issues/48)) would have to beat. An earlier −7.4 %
claim for the buffer was an artifact of single-shot timing and is withdrawn; the tell was
that the claimed gains ran *inverse* to allocation density.

## Legality and check information

### Why does one danger bitboard decide the king's destinations, and why is it filtered?

The bitboard alone lost; the filter is what made it a gain
([`05b59e4`](https://github.com/sugyan/shunsai/commit/05b59e4),
[`1a41320`](https://github.com/sugyan/shunsai/commit/1a41320) for the slider half).

The general shape is the part worth keeping, because it is not unconditionally good:
**"replace a per-item test with one bulk computation" trades a per-item cost for a fixed
one, and loses wherever the item count is small** — here a king has two or three candidate
squares against a pass over ~20 enemy pieces.

⚠️ The step term of the filter applies to every enemy piece, not only the non-sliders —
`movegen.rs` states why beside the code, and deleting it as redundant is tidying that would
silently break this.

### Why does `king_danger` return a partial attack map?

Because of that filter: the map is valid only next to the king. This is the function a
search would take its full attacked-squares set from, and dropping the filter is one line —
but it costs what the filter buys. **The condition: a search that wants the full map must
re-measure this filter, not assume it.** That condition may never fire from evaluation at
all, NNUE consuming piece placement rather than an attack map, which leaves move ordering
as its only plausible claimant.

### Why doesn't `check_info` hand its slider union to `king_danger`, or `Position` cache one?

Both were measured and both lost
([`ffd4056`](https://github.com/sugyan/shunsai/commit/ffd4056) for passing the union,
[`d099c3e`](https://github.com/sugyan/shunsai/commit/d099c3e) for the cached gold union).
Two things generalize from them:

- **Losing is itself the refutation.** If a value were genuinely being built twice, deleting
  one construction could not make anything *slower*. "The source computes this twice" is
  re-derivable by reading two functions side by side, and can be wrong — the compiler was
  already sharing the loads.
- **Paying on every piece placement to save on every attack query is the wrong side of the
  trade.** That is the reference result for any future "cache it in `Position`" proposal
  ([#43](https://github.com/sugyan/shunsai/issues/43) is bounded by it).

⚠️ A per-call figure from the `internals/*` sweep **over-states** what that call costs in a
hot loop: the sweep walks 81 origins where `check_info` asks about one square per node, so
its table lines are already warm. That is the obvious explanation and it is **not
verified**.

### Why does the pawn-drop-mate simulation not clone, and what guards it?

Removing the clone is what made generation allocate nothing
([`4a17c50`](https://github.com/sugyan/shunsai/commit/4a17c50)). `with_drop` survives
rather than folding into clone-and-`do_move`: it touches the hot path, so deleting it wants
its own measurement.

⚠️ **What covers pawn-drop-mate exclusion is thinner than it looks.** No default-depth perft
value reaches it; the deep one that does is `#[ignore]`d, and `main`'s ruleset does not
require the `perft-deep` job, so that step can go red without stopping a merge. What still
blocks is `rules::pawn_drop_mate_is_excluded` and the differential oracle, both inside
`check`.

### Why does `do_move` return an `Undo`?

So `Position` owns nothing on the heap; the break was spent before v0.1.0 while it was
still free ([`e039cd7`](https://github.com/sugyan/shunsai/commit/e039cd7), which carries
the ~4 % `do_undo` cost and the rejected hand-written copy).

⚠️ **A guard was spent, not just an API changed.** `states.pop()` panicked on an empty
stack, so unwinding past the bottom announced itself; passing the wrong `Undo` is silent
unless it happens to trip `remove_from_hand`'s `hand underflow`. `do_move` is deliberately
not `#[must_use]` — replaying a game forward is a first-class use — so nothing catches a
dropped one either.

⚠️ **`movegen/maxmoves-buf` is this crate's most layout-volatile id.** The single-call
`movegen/*-buf` ids scatter in *both* directions around changes they cannot mechanically
reach, while their `perft/*-cb-buf` twins stay flat. A mechanism pushes one way; read those
ids as layout.

### Why doesn't `Position` derive `Copy`?

At 368 bytes it is small enough to copy deliberately and large enough that copying it *by
accident* in a search's hot loop would not announce itself; `.clone()` at a copy-make site
puts the cost in the source. Adding `Copy` later is a major break —
`cargo-semver-checks`' `copy_impl_added`, for the closure move-semantics reason in
[rust-lang/rust#100905](https://github.com/rust-lang/rust/issues/100905) — so it was the
freeze window's decision rather than a default, and **the item most likely to be
revisited**, copy-make ([#48](https://github.com/sugyan/shunsai/issues/48)) being exactly
the caller that would want it.

## Fixtures and guards

### Why do the fixtures come from real games rather than random playouts?

Decisively: playout *reproduction* depends on `legal_moves()` ordering, so any ordering
change would silently change the workload and corrupt the history. Committed SFEN/USI text
is stable forever. (Playout positions also skew unrealistic — scattered material, inflated
hands.)

### What have the guards actually caught, and what has slipped past them?

This is the one view no single commit holds. **Perft is a real net but a coverage-dependent
one**, and which holes it has is not guessable:

| sabotage | what caught it | where |
|---|---|---|
| `checkers \|= single(sniper)` → `=` | **nothing** — a double check by *two sliders* was reachable from no fixture and no deep perft tree. Two fixtures now close it, and the test asserts it *reached* each configuration | [`05b59e4`](https://github.com/sugyan/shunsai/commit/05b59e4) |
| a `STEP_ATTACKS` row filed under the wrong kind | the `shogi_legality_lite` differential **alone** — it moves no perft value at any of the three fixtures | [`bdf2836`](https://github.com/sugyan/shunsai/commit/bdf2836) |
| dropping `ProRook` from `king_danger`'s union | the differential **alone**; the fixture list gained its only position with promoted sliders | [`ffd4056`](https://github.com/sugyan/shunsai/commit/ffd4056) |
| giving `ROOK_RAYS` the diagonal steps | broadly, but **not** by the initial-position or matsuri default-depth values | [`ffd4056`](https://github.com/sugyan/shunsai/commit/ffd4056) |
| three sabotages of `king_danger`'s slider filter | all three deep perft values | [`1a41320`](https://github.com/sugyan/shunsai/commit/1a41320) |

**Correction, because the wrong reason is the memorable one:** a `king_danger` under-report
is *not* structurally invisible to perft. The reasoning had been that since `king_danger`
only subtracts, an omission yields an illegal king move and no change in node count — but
an extra generated move *is* an extra node. Perft reports the mistake only where some
position in the tree has the omitted piece bearing on a king destination.

**Establish a guard's worth by sabotage, and assert the fixture reached the configuration.**
A coverage claim is worth exactly what its sabotage showed.

### Why are the Zobrist keys from an inline splitmix64, and why const?

Not a seedable RNG crate: no runtime dependency, and keys stay byte-for-byte reproducible
independent of any crate's version — `rand`'s `StdRng` / `SmallRng` explicitly do not
guarantee algorithm stability. It also converts trivially to `const fn`, which
[`b3012cf`](https://github.com/sugyan/shunsai/commit/b3012cf) cashed in (with a correction
to the mechanism the open question had assumed).

⚠️ **The keys must come out byte-identical, and that is a consumer constraint, not a
correctness one.** Any distinct keys hash correctly, so renumbering the table is invisible
here and rebaselines every transposition-table result `rinsai` has recorded.

⚠️ **Endpoints do not pin an order** — the lesson that generalizes past this guard. The
first version of `the_draw_order_is_fixed` pinned witnesses at the ends of the sequence and
a full renumbering passed the whole suite; it now folds every entry in canonical index
order. Any guard over an ordered table needs the same shape.

## Packaging and releasing

### What can go wrong in cutting a release?

`release-plz.yml`'s comments carry the token rules and
[`7e569f8`](https://github.com/sugyan/shunsai/commit/7e569f8) /
[`3116208`](https://github.com/sugyan/shunsai/commit/3116208) carry how the path was made
real. Three things nothing in the repository can state or check:

- ⚠️ **The workflow's file name is part of the crates.io configuration.** Renaming
  `release-plz.yml` stops publishing, and its environment field must stay empty because the
  release job declares none. The operator-side procedure is deliberately not in this
  repository, so this is the only warning before a rename.
- ⚠️ **A hand-published version has to be tagged before the next push to `main`**, or the
  release range is computed from the whole history. v0.1.0 did exactly that and proposed
  0.2.0 while `cargo-semver-checks` reported the API compatible. **Nothing in CI notices
  this**; the release PR is the only place it shows.
- ⚠️ **A stray `!` ships a wrong version, not a bad changelog line.** release-plz takes the
  bump from the log and only ever raises it. `!` is a guard `rinsai` depends on.

`v0.1.0` is tagged at [`7e569f8`](https://github.com/sugyan/shunsai/commit/7e569f8) rather
than at what shipped: `main` allows squash merges only, so the published tarball's commit
cannot be an ancestor of `main`.

### What does the packaging guard not catch?

`ci.yml` states the step's limits beside it. The consequence worth stating once: `include`
names the `src` **directory** rather than `src/**/*.rs`, because an asset dropped by an
extension-scoped glob would go unnoticed until a consumer enabled the feature that reads it
([`f41ccee`](https://github.com/sugyan/shunsai/commit/f41ccee)).

⚠️ **Declaring `rust-version` changed dependency resolution with no artifact to notice it
by**: `Cargo.lock` is untracked, so there is no lockfile diff, and **criterion runs from
before and after it are not comparable**.

**Shipping `gen_magics` (~9 KiB) stays open** if a downstream ever needs the provenance
claim checkable from the artifact alone; today the guard is repository-level.

### Why aren't the backend flags renamed the way the bench feature was?

They are functional, API-preserving build knobs, so a reader of the feature list should see
them — and renaming would not be free
([`2da1253`](https://github.com/sugyan/shunsai/commit/2da1253)).

⚠️ **The `cargo-semver-checks` `feature_missing` exemption list constrains every future
feature name.** It exempts only names starting with `_` or matching
`^(?:unstable|nightly|bench)(?:[-_].*)?$`. A maintainer-only flag named outside it cannot
later be renamed or removed without a major bump, whatever its documentation says it is.

### What is running the override backends in CI actually worth?

Narrower than "it catches backend bugs", and settled by sabotage: a broken backend
*function* is already caught by plain `cargo test`, so what the flagged runs add is a defect
**local to the flagged configuration**
([`2da1253`](https://github.com/sugyan/shunsai/commit/2da1253)).

⚠️ **They cannot catch an `active` arm wired to the wrong backend.** Measured: point the
`slider-qugiy` arm at `magic` and every test in the repository stays green, because all
three backends are asserted equal. The guard is that a flagged build generates legal moves
at all, not that the flag selects the backend it names.

### What does the provenance scan establish, and what does it not?

It found no verbatim reuse, every hit accounted for
([`446c6b5`](https://github.com/sugyan/shunsai/commit/446c6b5)). **The scan itself lives in
the local-only benchmarks repository**, because it cannot run without the GPL corpus.

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

A standing written in two places starts disagreeing with itself the next time the harness
runs, so the README keeps a link and no numbers
([`c436403`](https://github.com/sugyan/shunsai/commit/c436403)). **`#![doc =
include_str!("../README.md")]` was rejected**: crates.io resolves a README's relative links
against the repository and rustdoc does not, so those links would break on docs.rs.

### Why is there no prose plugin, reviewer agent or CI job for the hooks?

All three were rejected on measurement rather than taste
([`3f4be3a`](https://github.com/sugyan/shunsai/commit/3f4be3a)). The rule that outlives the
particular tools: **a check that only matters because Claude Code is editing lives in a
hook and nowhere else** — the repository's CI is the project's, and a machine's own
discipline is the wrong thing to put in a job every contributor pays for. Both scripts take
`--all`, so a CI step is one line away if that ever changes.

⚠️ **What a hook costs, stated rather than glossed.** It fires only when Claude Code does
the editing. A hand-written commit, another tool and CI see none of it, and nothing obliges
anyone to run `--all`.

### Why is the backlog a GitHub issue, and the reasoning a commit message?

Because they go stale on different clocks, and because this project already writes its
reasoning into commits — a document that restates them is a lossier copy that nothing
re-checks. So: `git log` is the record, `gh issue list` is the backlog, and this file is the
index that says which one to read.

What that costs, and it is real: **an issue and a commit body are not read before touching
the code, are not in the diff, and are not seen by the hooks.** A cloud session carries no
memory outside the repository, so it must go and look. Nothing that has to be *true now*
went there — this file and CLAUDE.md keep that.

**Reopens if** a fact turns out to have no home in a commit, an issue, an assertion or these
documents — which is the shape that produced the decision log in the first place.
