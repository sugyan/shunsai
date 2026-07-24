# shunsai — Design Document

Design, implementation approach, benchmarking method, comparison targets, milestones, and licensing policy for `shunsai`.
Current status: **M1 complete — simple, correct implementation validated against known perft values.**

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

## 4. Benchmarking method

- **Metrics**: (1) **perft** (nodes/sec; also serves as a correctness check); (2) **movegen alone** (ns per position); (3) **do/undo throughput**.
- **Tooling**: `criterion`. Measurement assets for each library live in `../benchmarks` (see §5).
- **Conditions**: `--release` / `lto = "fat"` / `codegen-units = 1`; same machine; with warm-up; a fixed position set; multiple trials with variance recorded; CPU architecture (x86_64 / aarch64) noted.
- **Position set (fixed SFEN)**:
  - initial position
  - **"matsuri" midgame position** (指し手生成祭り, the standard movegen-benchmark position in the Japanese shogi-dev community; used by YaneuraOu's `bench` / `test genmoves`): `l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1`
  - **maximum-legal-move position** (`R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`)
  - check- and mate-adjacent positions
- **Fairness** — normalize every library to the same work before comparing:
  - **Full legal move generation** (account for pseudo-legal + validation vs fully-legal differences, callback vs `Vec`/`ArrayVec` API differences, and Python-binding boundary costs).
  - **Pawn-drop-mate (打ち歩詰め) exclusion**: legal movegen must not generate pawn drops that give immediate checkmate. Engines differ here and it is a known cause of perft mismatches (see the TalkChess thread in §6); shunsai excludes them, and comparisons must confirm each library does the same.
  - **Bulk counting**: perft harnesses must agree on leaf handling. haitaka's perft example bulk-counts at depth 1 (cozy-chess style) instead of playing out leaf moves; our cross-library perft comparisons standardize on **leaf bulk counting** (and note it in results).
  - **C++ engines' measurement method**: YaneuraOu is measured with its built-in `test genmoves` command (movegen throughput on the matsuri position) driven over USI; apery similarly via its own commands where available. cshogi ships no perft, so we write a small Python-side perft/movegen harness on its API (its Python-binding overhead is part of what the "practical stack" comparison measures — report it as such).

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
- **M2**: benchmark harness (criterion + `../benchmarks` integration); record the naive implementation as baseline.
- **M3**: refine the move-generation API into the callback form (keeping `legal_moves()` compatibility).
- **M4**: evaluate optimization candidates (Qugiy / magic / SIMD / const tables / incremental AttackInfo / layout) and **adopt by benchmark comparison**.
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
