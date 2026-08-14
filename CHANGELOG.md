# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/sugyan/shunsai/compare/v0.1.0...v0.2.0) - 2026-08-14

### Documentation

- cut the README to what a consumer needs, and put each release fact in the file that owns it ([#31](https://github.com/sugyan/shunsai/pull/31))
- give the crate root a worked example and enforce doc coverage ([#27](https://github.com/sugyan/shunsai/pull/27))
- adopt Conventional Commit prefixes, keeping the descriptive subject ([#24](https://github.com/sugyan/shunsai/pull/24))

### Internal

- stop pointing at the local benchmarks checkout ([#28](https://github.com/sugyan/shunsai/pull/28))
- run the provenance scan required before publishing ([#26](https://github.com/sugyan/shunsai/pull/26))
- add a public-API-only search example, so the surface is checked the way a search uses it ([#21](https://github.com/sugyan/shunsai/pull/21))
- [**breaking**] set the MSRV, trim the published tarball, and gate the build that ships ([#25](https://github.com/sugyan/shunsai/pull/25))

### Other

- Make the release path real, and the packaging record true ([#30](https://github.com/sugyan/shunsai/pull/30))
- Link the commit instead of retelling it, and state the rule ([#29](https://github.com/sugyan/shunsai/pull/29))
- Record what v0.1.0 freezes and what it leaves out ([#23](https://github.com/sugyan/shunsai/pull/23))
- Return an Undo from do_move, so Position owns nothing on the heap ([#22](https://github.com/sugyan/shunsai/pull/22))
- Cut the comments that nothing verifies ([#20](https://github.com/sugyan/shunsai/pull/20))
- Split the design from the decision log, and give measurement one home ([#19](https://github.com/sugyan/shunsai/pull/19))
- Filter king_danger's sliders by where they could bear on the king (-16% on the initial position) ([#17](https://github.com/sugyan/shunsai/pull/17))
- Serve check_info's empty-board sniper scan from ray tables (-3.4% on real positions) ([#16](https://github.com/sugyan/shunsai/pull/16))
- Take the per-set path: piece-indexed attack dispatch and per-origin promotion (M5 met) ([#15](https://github.com/sugyan/shunsai/pull/15))
- Materialize a MoveSet with one decision per set, and route legal_moves through it ([#13](https://github.com/sugyan/shunsai/pull/13))
- Name the consumer, and make its dependency a release rather than a git pin ([#14](https://github.com/sugyan/shunsai/pull/14))
- Measure the leaf convention the cross-engine table had been mixing ([#12](https://github.com/sugyan/shunsai/pull/12))
- Record the engine roadmap (E0-E6, NNUE+ab first) and the schedule it imposes on the recorded re-measurements ([#11](https://github.com/sugyan/shunsai/pull/11))
- Decide king moves with a danger bitboard, and fuse the checker and pin scans (maxmoves now beats haitaka 1.67x) ([#10](https://github.com/sugyan/shunsai/pull/10))
- Record that the consumer is a search engine, not perft ([#9](https://github.com/sugyan/shunsai/pull/9))
- Record two rejected candidates and the M5 cross-engine re-measurement ([#7](https://github.com/sugyan/shunsai/pull/7))
- Simulate pawn drops without cloning the position ([#8](https://github.com/sugyan/shunsai/pull/8))
- decide legality per position instead of per move ([#6](https://github.com/sugyan/shunsai/pull/6))
- callback move generation, plus bitboard drop filtering ([#5](https://github.com/sugyan/shunsai/pull/5))
- fast slider attacks (magic bitboards, adopted by bake-off) ([#4](https://github.com/sugyan/shunsai/pull/4))
- criterion micro-benchmark suite with committed history baseline ([#3](https://github.com/sugyan/shunsai/pull/3))
- Fix ../benchmarks references; record cross-confirmed perft values ([#2](https://github.com/sugyan/shunsai/pull/2))
- Never generate captures of the opponent king
- Record implementation decision log in DESIGN.md
- Update status in README and DESIGN to M1 complete
- Add perft, differential and rule-specific tests
- Add fully legal move generation and perft example
- Add Position with do/undo and incremental Zobrist key
- Add Zobrist hashing keys
- Add u128 bitboard and naive attack tables
- Scaffold cargo project with licenses and CI
- Rename project: zenmai -> shunsai
- Strengthen benchmark and correctness docs from feasibility investigation
- Add initial design docs for zenmai (yasai successor)

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
