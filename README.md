# shunsai

[![CI](https://github.com/sugyan/shunsai/actions/workflows/ci.yml/badge.svg)](https://github.com/sugyan/shunsai/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/shunsai.svg)](https://crates.io/crates/shunsai)
[![docs.rs](https://docs.rs/shunsai/badge.svg)](https://docs.rs/shunsai)

**Fast shogi legal move generator — SHogi's Ultra-fast Next-gen Successor, for AI.**

`shunsai` is a Rust library for high-speed generation of legal moves in [Shogi](https://en.wikipedia.org/wiki/Shogi). It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai) ("Yet Another Shogi library, for AI"), redesigned from the inside out to be one of the fastest shogi move generators in Rust.

> ⚠️ **Status: M4 largely done; M5 met.** Fully legal move generation (including pawn-drop-mate exclusion), validated against known perft values and differential-tested against `shogi_legality_lite`. Optimizations are adopted by measurement against the committed benchmark history — including the ones that measured neutral and were kept for other reasons, which [DECISIONS.md](./DECISIONS.md) records as such.
>
> Against [haitaka](https://github.com/tofutofu/haitaka) — the main rival, and the only engine measured on the same leaf-counting convention — shunsai is ahead on **all three** fixture positions; apery_rust is beaten on all three too. Against the C++ engines, read only the **materializing** convention, where shunsai is fastest of nine on the midgame and max-moves positions and second on the initial position. Full tables and the convention caveat: [BENCHMARKS.md](./BENCHMARKS.md).

## Usage

```toml
[dependencies]
shunsai = "0.1"
```

Everything generated is fully legal, pawn-drop mate (打ち歩詰め) included, so a caller never filters what it is handed.

```rust
use shunsai::Position;

let position = Position::startpos();
assert_eq!(position.legal_moves().len(), 30);
```

`legal_moves()` is the convenient path. The fast one hands you a `MoveSet` per origin square with the destinations still packed as a bitboard, so code that only needs to count them never builds a `Move` — and can stop early by returning `ControlFlow::Break`:

```rust
use core::ops::ControlFlow;
use shunsai::Position;

let position = Position::startpos();
let mut moves = 0;
let _ = position.generate_moves(|set| {
    moves += set.len();
    ControlFlow::Continue(())
});
assert_eq!(moves, 30);
```

For the shape of a real consumer, `examples/search.rs` is a fixed-depth alpha-beta built on the public API alone.

Minimum supported Rust version: **1.88**.

## Concept

- **Speed is the north star.** The goal is to beat [haitaka](https://github.com/tofutofu/haitaka) and apery_rust on perft / move-generation benchmarks.
- **What the speed is for.** shunsai is meant to be the foundation for a **search engine, and through it a strong shogi AI** — written as a separate crate on top of this one. Perft is how the foundation is measured, not what it is for.
- Scope is limited to **legal move generation + position management (the engine)**.
- Built on [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) for the fundamental types, so existing users (e.g. [`tsumeshogi-solver`](https://github.com/sugyan/tsumeshogi-solver)) can migrate by swapping the dependency.

### Non-goals

Kifu I/O (SFEN/USI/KIF/CSA), evaluation functions, search, and tsume (mate) solvers are **out of scope for this crate**. That is a layering decision, not a lack of interest — search and evaluation are what shunsai is being built to carry, and they belong in a crate that depends on this one. shunsai stays a lean, fast engine.

## The name

**shunsai = 旬菜 (しゅんさい).**

- **旬菜** means "seasonal vegetables at their peak" — the freshest produce of the season — continuing the culinary lineage of [`yasai`](https://github.com/sugyan/yasai) (野菜, "vegetables").
- At the same time, it is a homophone of **俊才/駿才 — "a swift prodigy"**: the 俊 of 俊足 (swift-footed) and the 駿 of 駿馬 (a fleet steed). Speed is built into the sound of the name.
- It ends in **"ai"**, echoing *yasai*. As a backronym: **SH**ogi's **U**ltra-fast **N**ext-gen **S**uccessor, for **AI**.

## How it will be built

Rather than committing to a particular optimization up front, shunsai starts with a **simple, correct implementation** (validated against known perft values) and then decides the optimization strategy (Qugiy / magic bitboards / SIMD, etc.) **by benchmarking**.

- [DESIGN.md](./DESIGN.md) — the design, scope and licensing policy
- [DECISIONS.md](./DECISIONS.md) — what was decided and rejected, and what is still open
- [BENCHMARKS.md](./BENCHMARKS.md) — how measurement is done, and every recorded number

## License

`MIT OR Apache-2.0` (permissive).

This project **does not reuse GPL-licensed code** (apery / apery_rust / YaneuraOu / cshogi / the old yasai are GPL-3.0). See [DESIGN.md](./DESIGN.md) and [CLAUDE.md](./CLAUDE.md) for details.
