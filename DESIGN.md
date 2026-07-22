# zenmai — Design Document

Design, implementation approach, benchmarking method, comparison targets, milestones, and licensing policy for `zenmai`.
Current status: **design stage (no code yet).**

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
- Measure perft / movegen / do-undo with `criterion`. Wire it into [`../benchmarks`](../benchmarks) and compare side-by-side against the old yasai, haitaka, and apery_rust.
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

## 4. Benchmarking method

- **Metrics**: (1) **perft** (nodes/sec; also serves as a correctness check); (2) **movegen alone** (ns per position); (3) **do/undo throughput**.
- **Tooling**: `criterion`. Measurement assets for each library live in [`../benchmarks`](../benchmarks).
- **Conditions**: `--release` / `lto = "fat"` / `codegen-units = 1`; same machine; with warm-up; a fixed position set; multiple trials with variance recorded; CPU architecture (x86_64 / aarch64) noted.
- **Position set (fixed SFEN)**: initial position / a representative midgame position / **maximum-legal-move position** (`R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`) / check- and mate-adjacent positions.
- **Fairness**: normalize every library to "full legal move generation" (account for pseudo-legal + validation vs fully-legal differences, callback vs `Vec`/`ArrayVec` API differences, and Python-binding boundary costs).

## 5. Comparison targets

(submodules under [`../benchmarks`](../benchmarks))

| Category | Library | Role | License |
|---|---|---|---|
| **Main rivals (Rust)** | **haitaka** / **apery_rust** | Targets to **beat directly** on perft/movegen | MIT / GPL-3.0 |
| Self-baseline | **yasai (old 0.5.0)** | Basis for measuring the improvement | GPL-3.0 |
| Reference (Rust) | rustshogi | Implementation-variation comparison | MIT |
| Reference ceiling (C++) | YaneuraOu / apery | A sense of "how close can we get" | GPL-3.0 |
| Reference (practical) | cshogi | Comparison with a practical Python stack | GPL-3.0 |

## 6. Milestones

- **M0 (current)**: name & concept fixed; design documents. **No code.** Licensing policy decided.
- **M1**: **simple, correct implementation** (Position + naive movegen) matching known perft values.
- **M2**: benchmark harness (criterion + `../benchmarks` integration); record the naive implementation as baseline.
- **M3**: refine the move-generation API into the callback form (keeping `legal_moves()` compatibility).
- **M4**: evaluate optimization candidates (Qugiy / magic / SIMD / const tables / incremental AttackInfo / layout) and **adopt by benchmark comparison**.
- **M5**: numerically confirm we **beat** haitaka / apery_rust.
- **M6**: switch `tsumeshogi-solver` to depend on `zenmai`; validate the migration.
- **Later**: publish v0.1.0 on crates.io (`keywords = ["shogi","move-generation","bitboard","game","usi"]`, `categories = ["game-development","algorithms"]`).

### Known perft values (correctness checks for M1/M4)

| Position | depth 1 | depth 2 | depth 3 | depth 4 |
|---|---|---|---|---|
| Initial position | 30 | 900 | 25470 | 719731 |
| Max-moves position ※ | 593 | 105677 | — | — |

※ `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`

## 7. Licensing policy (important)

**Chosen license: `MIT OR Apache-2.0` (permissive).**

### Actual licenses of the compared libraries (verified)

| permissive (MIT) | copyleft (GPL-3.0) |
|---|---|
| haitaka, rustshogi, cozy-chess, shogi_core | yasai, apery_rust, apery, YaneuraOu, cshogi |

### Principle

Copyright protects **expression (the actual code)**, not ideas, algorithms, or techniques.

- **Adopting only a technique** (implementing Qugiy, magic bitboards, etc. yourself from public write-ups) → not bound by the source's license.
- **Copying / line-by-line porting of code** → creates a derivative work and **inherits GPLv3**.
- ⚠️ The old yasai is itself GPLv3 (derived from apery_rust). **Porting yasai's code would make zenmai GPLv3 too**, so to stay permissive we reimplement yasai as well.

### Rule: do not reuse GPL code

To keep the permissive license clean, both human and AI contributors must:

- **May reference**: haitaka / cozy-chess / rustshogi / shogi_core (MIT), plus **public algorithm write-ups** (the Qugiy appeal document, magic-bitboard articles, etc.).
- **Must not copy**: apery / apery_rust / YaneuraOu / cshogi / the old yasai (GPL-3.0). Understanding the technique is fine; copying the code is not.
- **Generate tables / magic numbers with our own generator** (never paste them from elsewhere).
- When reusing from MIT sources, **retain the copyright notices** (approximate / partial reuse is fine).

### Provenance of AI-generated code

Most of the code will be written by AI (Claude Code). The question is not "was GPL in the training data" but "**does the shipped code substantially reproduce the *expression* of a specific GPL work.**" In an infringement claim the burden is on the **rights holder** (to show substantial similarity + access); we are not required to prove non-copying in advance. In this domain the core is **algorithms (not copyrightable)** plus **tables (which we generate ourselves)**, and boilerplate expression is thinly protected. In practice:

- Keep primary sources restricted to **MIT code + public write-ups** (per the rule above).
- **Run a provenance scan before release** (distinctive-string search / code-similarity scanner to check for verbatim copies).
- Keep a **git history** that shows incremental, original development.

> This section is a general summary of how licensing works, not legal advice. The copyright status of AI training data and generated output is an evolving area. For commercial or high-stakes releases, consult a professional.
