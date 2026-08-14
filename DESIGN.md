# shunsai — Design Document

What `shunsai` is, what it is not, how it is built, and the rules that constrain it.

Companion documents, each with one job:

| document | holds |
|---|---|
| [DECISIONS.md](./DECISIONS.md) | what was decided and rejected, and what is still open |
| [BENCHMARKS.md](./BENCHMARKS.md) | how measurement is done, and every recorded number |

**Status**: M3 complete; M4 largely done; M5 met (2026-08-06). See §6.

## 1. Background & goal

- [`yasai`](https://github.com/sugyan/yasai) ("Yet Another Shogi library, for AI") is a Rust library for fast legal move generation and position management. It is based on `apery_rust`, uses `shogi_core` types, and implements bitboards with hand-written per-platform SIMD. It has been **dormant since v0.5.0 (October 2023)**. Its main downstream user is `tsumeshogi-solver` (a DFPN mate solver).
- Since 2024, libraries such as [`haitaka`](https://github.com/tofutofu/haitaka) (based on [`cozy-chess`](https://github.com/analog-hors/cozy-chess): magic/Qugiy sliders, `no_std`, zero-allocation callback move generation, mostly-const tables) have moved the design state of the art ahead.
- **Goal**: rebuild yasai's internals from scratch to be one of the fastest shogi move generators in Rust, while staying compatible with `shogi_core`.
- **What the speed is for** (stated 2026-07-29): shunsai is meant to be the **foundation for a search engine, and through it a strong shogi AI**, built as a separate crate on top of this one. That consumer is what decides whether the API and the layout are right — perft is the measuring instrument, not the customer. Migrating `tsumeshogi-solver` (M6) remains a useful real-world check but is no longer the reason the project exists.

## 2. Scope

| | |
|---|---|
| **In scope** | Legal move generation (movegen); position management (do/undo, Zobrist, check/pin information) |
| **Out of scope (non-goals)** | Kifu I/O (SFEN/USI/KIF/CSA), evaluation, search, tsume solvers |

- Fundamental types stay on **`shogi_core` (MIT)** (`Color/Piece/Square/Move/Hand/PartialPosition`), so `tsumeshogi-solver` and others can migrate by swapping the dependency.
- **The non-goals are a layering decision, not a lack of interest.** Search and evaluation are exactly what shunsai is being built to carry (§1); they belong in a crate that *depends* on this one, so this crate stays lean, policy-free and `no_std`-friendly. The consequence for design work here is a standard of evidence, not a change of scope: an API or layout question must be judged against **a search using it**, not only against perft. See the 2026-07-29 entry in [DECISIONS.md](./DECISIONS.md).

## 3. Implementation approach — simple first, then benchmark-driven optimization

Don't pick Qugiy or SIMD up front. First get a **simple, correct implementation** working, then compare candidates **while benchmarking** to decide the optimization strategy.

### Phase 1: correctness-first, naive implementation
- `u128` (or a straightforward multi-word) bitboard; naive occupancy-loop slider attacks.
- Position (do/undo, incremental Zobrist) on `shogi_core` types.
- Correctness is guaranteed by matching the known perft values (see §6).

### Phase 2: benchmark harness
- Measure perft / movegen / do-undo with `criterion`. Wire it into the local benchmarks checkout (see §5) and compare side-by-side against the old yasai, haitaka, and apery_rust.
- Record the naive implementation as the **baseline**.

### Phase 3: optimization candidates, adopted by benchmark comparison
- **Slider attacks**: naive → **Qugiy** / **magic bitboards** / **hand-written SIMD**, chosen by measuring which is fastest (settle on one, or keep several behind feature flags).
- **Move-generation API**: `cozy-chess`/haitaka-style **callback generation** (yield `from + destination bitboard` grouped per piece; zero-allocation, early exit). High enough value to adopt early. `legal_moves() -> Vec` is kept as a compatibility wrapper.
- **const tables** (replacing `once_cell` runtime init), **incremental AttackInfo** updates (the old yasai rebuilds it every move), and **bit layout** (e.g. file-major) are also candidates to measure.

### Module layout

```
src/lib.rs
src/bitboard.rs   # the u128 bitboard type
src/sliders.rs    # slider attacks — the M4 backend swap boundary
src/sliders/      # magic (live) / qugiy / naive (oracle) backends
src/tables.rs     # const attack, ray, between, line and zone tables
src/zobrist.rs
src/position.rs   # Position: do/undo, Zobrist
src/movegen.rs    # callback generation, legality, legal_moves wrapper
src/internals.rs  # crate internals re-exposed to benches (feature-gated)
examples/perft.rs
benches/          # movegen / perft / do_undo
```

The **swap boundary** for slider techniques is the attack-function signatures in `sliders.rs`, not a trait over `Bitboard`. `magic` and `qugiy` are always compiled; `naive` is the oracle the tests hold the others to, and is compiled under `cfg(any(test, feature = "slider-naive", feature = "bench-internals"))`. See the 2026-07-23 entry in [DECISIONS.md](./DECISIONS.md).

## 4. Benchmarking method

Metrics, tooling, measurement conditions, the fixed position set, the fairness
rules that make cross-engine numbers comparable (in particular the **two leaf
conventions**, count-only and materializing, which decide the comparison), the
in-repo criterion suite, and the recorded history all live in
**[BENCHMARKS.md](./BENCHMARKS.md)**.

One rule from there constrains design work, not just measurement: **never compare
numbers produced under different leaf conventions.** Count-only is a genuine advantage
of the callback API, but it is one perft collects and a search cannot.

## 5. Comparison targets

(submodules of a **local-only, unpublished** sibling git repository with no remote. It is not part of the shunsai repository and is never distributed, which is also why GPL projects may live there for benchmarking.)

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

**Pinning / update policy**: those (local-only) submodules are **pinned** commits recorded in that repository (see its README). Comparison numbers are only meaningful against a recorded pin. Updates are deliberate: bump a submodule intentionally, record the new pin and date, and re-run baselines — never benchmark against a silently-drifted checkout. Dormant upstreams (apery, apery_rust, yasai, haitaka) double as stable, reproducible targets.

Correctness oracle (not a speed target): [`shogi_legality_lite`](https://github.com/rust-shogi-crates/shogi_legality_lite) (MIT, same `shogi_core` types) — see §6.

## 6. Milestones

- **M0 (done)**: name & concept fixed; design documents. Licensing policy decided.
- **M1 (done)**: **simple, correct implementation** (Position + naive movegen) matching known perft values.
- **M2 (done)**: benchmark harness (criterion + cross-engine integration); record the naive implementation as baseline. In-repo suite documented in [BENCHMARKS.md](./BENCHMARKS.md); cross-engine baseline recorded 2026-07-23 in the local benchmarks repository (§5).
- **M3 (done)**: move-generation API refined into the callback form — `Position::generate_moves(|MoveSet| -> ControlFlow<()>)`, with `legal_moves()` kept as the allocating wrapper. Measured under the append-only `-cb` bench ids beside the `Vec` ones.
- **M4 (in progress)**: evaluate optimization candidates and **adopt by benchmark comparison**. Every adoption, every rejected candidate, and the open list are in [DECISIONS.md](./DECISIONS.md); the numbers are in [BENCHMARKS.md](./BENCHMARKS.md) and `benches/history/*.json`. What remains is profiling-led rather than guessed.
- **M5 (met 2026-08-06)**: numerically confirm we **beat** haitaka / apery_rust on perft. Met — both are ahead-of on all three fixture positions. The criterion is a like-for-like comparison, so it is read on the **materializing** convention against every engine except haitaka, which is the one that shares shunsai's count-only convention. Standing and tables: [BENCHMARKS.md](./BENCHMARKS.md).
- **M6 (demoted 2026-07-29)**: switch `tsumeshogi-solver` to depend on `shunsai`; validate the migration. Still worth doing as a real-world check that the API survives contact with a consumer, but it is no longer what the project is for — see §1.
- **M7 — the actual destination**: a **search engine** on top of shunsai, in its own crate, and from there a strong AI. Deliberately not scoped in this document (§2 keeps this crate movegen-only), but it is what M4 and M5 are *for*. Named **`rinsai`**, one new repository, depending on *released* versions of this crate. Its staged plan (E0–E6, NNUE + αβ first) and the API additions each phase needs are in [DECISIONS.md](./DECISIONS.md) (2026-07-31, 2026-08-04).
- **Before E0**: publish v0.1.0 on crates.io — **done 2026-08-13**. Not optional sequencing — `rinsai` is to depend on released versions rather than a git pin, so the release is what E0 must build against. It was also the **last point at which an API break was free**, so it gated more than packaging:
  - the `states: Vec<State>` move out of `Position`: **taken** — `do_move` returns an `Undo`, so `Position` owns nothing on the heap and the break is spent before it would cost `rinsai` a version (decided 2026-08-11, [DECISIONS.md](./DECISIONS.md))
  - the **provenance scan** (§7): **run, no verbatim reuse**. Re-run before each release — the corpus moves (2026-08-11, [DECISIONS.md](./DECISIONS.md))
  - what the published tarball contains: `include` ships `src/`, the README, `CHANGELOG.md` and the two licences; the documents, fixtures and `benches/history/*.json` stay in the repository (decided 2026-08-11, [DECISIONS.md](./DECISIONS.md))
  - how the release is cut: **release-plz**, publishing over Trusted Publishing rather than a registry secret — which meant **v0.1.0 itself went out by hand**, since crates.io only takes that configuration against a crate that already exists (2026-08-12 and 2026-08-13, [DECISIONS.md](./DECISIONS.md))
  - MSRV: `rust-version = "1.88"`, set by one let-chain rather than by the edition, gated by the `msrv` CI job (decided 2026-08-11, [DECISIONS.md](./DECISIONS.md))
  - `keywords = ["shogi","move-generation","bitboard","game","perft"]`, `categories = ["game-development","algorithms"]` (already in `Cargo.toml`). **`usi` was dropped**: the five-keyword cap makes the list a set of claims, and USI is a §2 non-goal — a search for it should not land here.

### Known perft values (correctness checks for M1/M4)

| Position | depth 1 | depth 2 | depth 3 | depth 4 | depth 5 | depth 6 |
|---|---|---|---|---|---|---|
| Initial position | 30 | 900 | 25470 | 719731 | 19861490 | 547581517 |
| Matsuri position | 207 | 28684 | 4809015 | 516925165 | — | — |
| Max-moves position ※ | 593 | 105677 | 53393368 | 9342410965 | — | — |

※ `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`

- Initial-position values through depth 5–6 are cross-confirmed by multiple independent engines ([shogi-l thread](https://groups.google.com/g/shogi-l/c/U7hmtThbk1k), [TalkChess "Shogi Perft numbers"](https://www.talkchess.com/forum3/viewtopic.php?t=71550)); the max-moves values come from [this Qiita article](https://qiita.com/ak11/items/8bd5f2bb0f5b014143c8) (also used in yasai's tests).
- Matsuri values confirmed 2026-07-23 via the cross-engine perft harness in the local benchmarks repository (§5): nine independent implementations agree (shunsai, haitaka, yasai, apery_rust, rshogi, YaneuraOu, apery, cshogi, Fairy-Stockfish), matching the expected values hardcoded in YaneuraOu's own test suite.
- Max-moves depth 3–4 established 2026-07-23 by 8-engine consensus (same harness; depth 3 is also asserted in yasai's upstream bench). Fairy-Stockfish is excluded there by convention: it *generates* pawn-drop-mate moves and enforces the rule as a game result, so its counts run high on drop-heavy trees (+6369 at depth 3) — a live example of the fairness warning in [BENCHMARKS.md](./BENCHMARKS.md).
- These counts assume **fully legal** generation, including **pawn-drop-mate (打ち歩詰め) exclusion** — a documented source of cross-engine perft disagreement (see the TalkChess thread).

### Correctness verification (M1)

Fixed perft values alone only cover a handful of positions. In addition:

- **Differential testing against `shogi_legality_lite`** (MIT, [rust-shogi-crates](https://github.com/rust-shogi-crates/shogi_legality_lite)): it is slow but straightforward, and it shares `shogi_core` types, so the full legal-move **sets** (not just counts) can be compared directly on arbitrary positions. Use it as a dev-dependency oracle: random playouts from the fixed position set, asserting set-equality of legal moves at every node.
- **Cross-perft against cshogi / YaneuraOu** for positions with no published values: agreement between independent implementations establishes the reference number; record it here once confirmed. *(Done for the matsuri position, 2026-07-23 — see the known-values table above. Max-moves depth 3–4 are consensus-only, recorded in the local benchmarks repository (§5).)*

> **Perft is a real net, but a coverage-dependent one.** It only reports a mistake where some position in the tree actually exercises it — a `king_danger` under-report slipped past all three deep values in 2026-08-07 and was caught by the `shogi_legality_lite` differential alone. Treat the differential and the in-crate oracle tests as the closing net, not as a formality.

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

`src/sliders/magics.rs` is generated and must never be hand-edited; CI re-runs the generator with `--check`. That guard is a **licensing** guard, not a correctness one, and it is not redundant with the compile-time magic validation — see the 2026-07-28 entry in [DECISIONS.md](./DECISIONS.md) for why "it works" is no evidence of "we generated it".

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
- **Perft-convention mismatches can fake regressions or wins.** Pawn-drop-mate handling and leaf bulk-counting differ across libraries (see [BENCHMARKS.md](./BENCHMARKS.md) and §6). Every cross-library number must state the convention used; never compare numbers produced under different conventions.
- **Benchmark-target drift.** Comparison targets are pinned submodules of the local benchmarks repository; results are only comparable against a recorded pin (see §5 pinning policy). Active upstreams (YaneuraOu, cshogi, rshogi, Fairy-Stockfish) will move — bump deliberately and re-baseline.
- **Feasibility of the "beat haitaka / apery_rust" goal** (assessed 2026-07-22): haitaka self-describes as experimental (no engine adoption yet, tuned and tested mainly on Apple M2, no hand-written SIMD); yasai demonstrated that hand-tuned SIMD is competitive with apery_rust. Combining haitaka-style const tables / Qugiy with yasai-style SIMD (reimplemented, per §7) leaves clear headroom, so the goal is considered achievable — but it is validated empirically at M5, not assumed.
  - ⚠️ **Every recorded win over haitaka is on Apple Silicon, which is the family haitaka was tuned for.** The x86-64 re-run scheduled for engine phase E4 (see [DECISIONS.md](./DECISIONS.md), 2026-07-31) is where that caveat gets tested; state it whenever the M5 result is quoted outside this repository.
