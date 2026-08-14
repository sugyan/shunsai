# shunsai

[![CI](https://github.com/sugyan/shunsai/actions/workflows/ci.yml/badge.svg)](https://github.com/sugyan/shunsai/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/shunsai.svg)](https://crates.io/crates/shunsai)
[![docs.rs](https://docs.rs/shunsai/badge.svg)](https://docs.rs/shunsai)

**Fast shogi legal move generator — SHogi's Ultra-fast Next-gen Successor, for AI.**

`shunsai` generates the legal moves of a [shogi](https://en.wikipedia.org/wiki/Shogi)
position, as fast as it can be made to. It is built on
[`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) for the fundamental
types and re-exports it, so a consumer needs no separate version of it.

Everything it generates is **fully legal**, pawn-drop mate (打ち歩詰め) included, so a
caller never filters what it is handed. That is checked against the known perft values
through depth 6 on the initial position, and differentially against
`shogi_legality_lite` on random playouts.

> The API is pre-1.0 and can still change between minor versions.

## Usage

```toml
[dependencies]
shunsai = "0.1"
```

```rust
use shunsai::Position;

let position = Position::startpos();
assert_eq!(position.legal_moves().len(), 30);
```

`legal_moves()` allocates. The fast path is `generate_moves`, which yields one
`MoveSet` per origin with the destinations still packed as a bitboard — so code that
only counts them never builds a `Move`, and can stop early by returning
`ControlFlow::Break`. Moves are played and taken back with `do_move` and `undo_move`,
which maintain a Zobrist `key()` incrementally; `do_move` returns an `Undo` the caller
holds and hands back, so a `Position` owns nothing on the heap.

The [API documentation](https://docs.rs/shunsai) has the rest, and
[`examples/search.rs`](./examples/search.rs) is a fixed-depth alpha-beta written on the
public API alone.

Minimum supported Rust version: **1.88**.

## Scope

Legal move generation and position management. That is the whole crate.

Kifu I/O (SFEN/USI/KIF/CSA), evaluation functions, search, and tsume (mate) solvers are
**out of scope** and belong in a crate that depends on this one — which is what shunsai
is for. It is the foundation for a **search engine, and through it a strong shogi AI**.

Speed is the point, and it is settled by measurement rather than by argument: the
method, the fixtures and where the crate currently stands are in
[BENCHMARKS.md](./BENCHMARKS.md).

## The name

**shunsai = 旬菜 (しゅんさい).**

- **旬菜** means "seasonal vegetables at their peak" — the freshest produce of the
  season — continuing the culinary line of [`yasai`](https://github.com/sugyan/yasai)
  (野菜, "vegetables").
- At the same time it is a homophone of **俊才/駿才 — "a swift prodigy"**: the 俊 of
  俊足 (swift-footed) and the 駿 of 駿馬 (a fleet steed). Speed is built into the sound
  of the name.
- It ends in **"ai"**. As a backronym: **SH**ogi's **U**ltra-fast **N**ext-gen
  **S**uccessor, for **AI**.

## The project's record

- [DESIGN.md](./DESIGN.md) — scope, milestones, and the licensing policy
- [DECISIONS.md](./DECISIONS.md) — what was decided and rejected, and what is still open
- [BENCHMARKS.md](./BENCHMARKS.md) — how measurement is done, and every recorded number

## License

`MIT OR Apache-2.0`, at your option.

No GPL-licensed code is reused; [DESIGN.md](./DESIGN.md) states that policy and the
provenance scan it requires before each release.
