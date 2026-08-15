# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/sugyan/shunsai/compare/v0.1.1...v0.1.2) - 2026-08-15

### Internal

- rename bench-internals to _bench-internals, which docs.rs hides, and run the override backends in CI ([#35](https://github.com/sugyan/shunsai/pull/35))

## [0.1.1](https://github.com/sugyan/shunsai/compare/v0.1.0...v0.1.1) - 2026-08-14

### Documentation

- correct what bounds a release's range and what decides the version, and prune the log ([#34](https://github.com/sugyan/shunsai/pull/34))
- cut the README to what a consumer needs, and put each release fact in the file that owns it ([#31](https://github.com/sugyan/shunsai/pull/31))

## [0.1.0] - 2026-08-13

First release: fully legal shogi move generation on `shogi_core` types.

Everything generated is legal, pawn-drop mate (打ち歩詰め) included, so a caller
never filters what it is handed. Checked against the known perft values through
depth 6 on the initial position, and differential-tested against
`shogi_legality_lite` on random playouts.

### Added

- `Position` — board state with an incrementally maintained Zobrist `key()`, and
  `do_move` / `undo_move`. `do_move` returns an `Undo` that the caller stores and
  hands back, so a `Position` owns nothing on the heap.
- `Position::generate_moves` — the fast path. Yields one `MoveSet` per origin
  square with the destinations still packed as a `Bitboard`, so a caller that only
  counts them never builds a `Move`; returning `ControlFlow::Break` stops the walk.
- `Position::legal_moves` — the allocating convenience wrapper, alongside
  `has_legal_moves` and `in_check`.
- `MoveSet` and `MoveSetIter` — the moves sharing one origin, `Normal` or `Drop`.
  The fields are public, so a caller can filter a set against a target mask
  without materializing it.
- `Bitboard` — the crate's 81-square board set.
- Position accessors: `piece_at`, `side_to_move`, `ply`, `hand`, `king_square`,
  `occupied`, `player_bb`, `piece_kind_bb`.
- `shogi_core` is re-exported, so a consumer needs no separate version of it.

### Notes

- Minimum supported Rust version: **1.88**, set by one let-chain rather than by
  edition 2024.
- `bench-internals`, `slider-naive` and `slider-qugiy` are maintainer-only feature
  flags for A/B measurement and debugging. They are **not** public API and carry no
  stability guarantee; the two backend flags are mutually exclusive.
