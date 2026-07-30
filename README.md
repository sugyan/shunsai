# shunsai

**Fast shogi legal move generator — SHogi's Ultra-fast Next-gen Successor, for AI.**

`shunsai` is a Rust library for high-speed generation of legal moves in [Shogi](https://en.wikipedia.org/wiki/Shogi). It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai) ("Yet Another Shogi library, for AI"), redesigned from the inside out to be one of the fastest shogi move generators in Rust.

> ⚠️ **Status: M4 largely done; M5 partly met.** Fully legal move generation (including pawn-drop-mate exclusion), validated against known perft values and differential-tested against `shogi_legality_lite`. Magic slider attacks, callback move generation, pin-based legality, an allocation-free pawn-drop-mate test and a king-danger bitboard have each been adopted by measurement against the committed benchmark history (see [BENCHMARKS.md](./BENCHMARKS.md)). apery_rust, apery and the old yasai are beaten on every fixed position; [haitaka](https://github.com/tofutofu/haitaka) led by 1.1–1.3× when the engines were last measured side by side, which was before the king-danger change. See [DESIGN.md](./DESIGN.md).

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

Rather than committing to a particular optimization up front, shunsai starts with a **simple, correct implementation** (validated against known perft values) and then decides the optimization strategy (Qugiy / magic bitboards / SIMD, etc.) **by benchmarking**. See [DESIGN.md](./DESIGN.md).

## License

`MIT OR Apache-2.0` (permissive).

This project **does not reuse GPL-licensed code** (apery / apery_rust / YaneuraOu / cshogi / the old yasai are GPL-3.0). See [DESIGN.md](./DESIGN.md) and [CLAUDE.md](./CLAUDE.md) for details.
