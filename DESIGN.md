# shunsai — design

What this crate is, what it is not, and how it is held to that.

Why the code is the way it is is [FAQ.md](./FAQ.md); how measurement is done is
[BENCHMARKS.md](./BENCHMARKS.md); the rules a session follows are
[CLAUDE.md](./CLAUDE.md). What is next is `gh issue list`.

## Goal

Rebuild the internals of [`yasai`](https://github.com/sugyan/yasai) from scratch to be
one of the fastest shogi move generators in Rust, while staying compatible with
[`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) — and permissively
licensed, which yasai (GPL-3.0, derived from apery_rust) is not.

**What the speed is for**: shunsai is the foundation for a **search engine, and through
it a strong shogi AI**, built as a separate crate. That consumer is
[`rinsai`](https://github.com/sugyan/rinsai), and it depends on *released* versions of
this crate, so an API addition it needs is a shunsai release and carries semver.

**Perft is the measuring instrument, not the customer.** An API, layout or size question
is judged against *a search using this crate*, not only against perft. An argument of the
form "nothing collects these" or "this is free in perft" settles the question for today's
callers only — say so rather than closing it. Equally, do not build speculatively for a
search that does not exist yet: record the condition and re-measure when it does.

## Scope

| | |
|---|---|
| **In scope** | Legal move generation; position management (do/undo, Zobrist, check and pin information) |
| **Out of scope** | Kifu I/O (SFEN/USI/KIF/CSA), evaluation, search, tsume solvers |

Fundamental types stay on **`shogi_core` (MIT)** — `Color` / `Piece` / `Square` / `Move` /
`Hand` / `PartialPosition` — and it is re-exported, so a consumer needs no separate
version of it.

**The non-goals are a layering decision, not a lack of interest.** Search and evaluation
are exactly what shunsai is built to carry; they belong in a crate that *depends* on this
one, so this crate stays lean, policy-free and `no_std`-friendly.

## How correctness is held

**Known perft values live in [`tests/perft.rs`](./tests/perft.rs)**, which asserts them
and records where each came from. They assume fully legal generation, **pawn-drop-mate
(打ち歩詰め) exclusion included** — a documented source of cross-engine perft
disagreement. Provenance that is not in that file:

- Initial-position values through depth 5–6 are cross-confirmed by multiple independent
  engines ([shogi-l](https://groups.google.com/g/shogi-l/c/U7hmtThbk1k),
  [TalkChess "Shogi Perft numbers"](https://www.talkchess.com/forum3/viewtopic.php?t=71550));
  the max-moves values come from
  [this Qiita article](https://qiita.com/ak11/items/8bd5f2bb0f5b014143c8).
- Fairy-Stockfish is excluded from the max-moves consensus by convention: it *generates*
  pawn-drop-mate moves and enforces the rule as a game result, so its counts run high on
  drop-heavy trees.

**Fixed values alone cover a handful of positions.** The closing net is **differential
testing against [`shogi_legality_lite`](https://github.com/rust-shogi-crates/shogi_legality_lite)**
(MIT, same `shogi_core` types), which compares full legal-move *sets* on random playouts
rather than counts — plus the in-crate oracle tests, which hold every slider backend to
`naive`.

> What each guard has and has not caught, established by sabotage, is in
> [FAQ.md](./FAQ.md).

## How speed is decided

By measurement, never by argument. The method, the fixtures, the criterion suite, the
recordability rules and every recorded figure are in
[BENCHMARKS.md](./BENCHMARKS.md); `benches/history/*.json` is the record.

The targets to beat are **haitaka** (MIT) and **apery_rust** (GPL-3.0), with YaneuraOu
and apery as a reference ceiling. The harness and the pinned checkouts live in a
**local-only, unpublished** sibling repository — see its README when working there.
Comparison numbers are only meaningful against a recorded pin.

One rule from BENCHMARKS.md constrains design work rather than only measurement: **never
compare numbers produced under different leaf conventions.**

## Module layout

`src/lib.rs` names the modules; `src/sliders.rs` documents the backend swap boundary and
what each backend may assume. The boundary is the attack-function signatures, not a trait
over `Bitboard` — see [FAQ.md](./FAQ.md).

## Risks

- **`shogi_core` is dormant** (0.1.5, published 2022-08). The public API is built on it,
  so treat its API as frozen: depend only on what 0.1.5 provides, and do not plan around
  hoped-for upstream changes. If a blocking bug or missing capability appears, it is MIT
  — forking or vendoring the needed types is an acceptable fallback that preserves the
  "swap the dependency" migration story.
- **Every recorded win over haitaka is on Apple Silicon**, which is the family haitaka
  was tuned for; state that caveat whenever the result is quoted outside this repository.
  The x86-64 re-run is [#50](https://github.com/sugyan/shunsai/issues/50).
- **Perft-convention mismatches can fake regressions or wins.** Every cross-library
  number must state the convention used.
