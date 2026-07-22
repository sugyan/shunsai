# zenmai

**Fast shogi legal move generator — the mainspring of your shogi AI.**

`zenmai` is a Rust library for high-speed generation of legal moves in [Shogi](https://en.wikipedia.org/wiki/Shogi). It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai) ("Yet Another Shogi library, for AI"), redesigned from the inside out to be one of the fastest shogi move generators in Rust.

> ⚠️ **Status: design stage (no code yet).** This repository currently contains design documents only; implementation has not started. See [DESIGN.md](./DESIGN.md).

## Concept

- **Speed is the north star.** The goal is to beat [haitaka](https://github.com/tofutofu/haitaka) and apery_rust on perft / move-generation benchmarks.
- Scope is limited to **legal move generation + position management (the engine)**.
- Built on [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) for the fundamental types, so existing users (e.g. [`tsumeshogi-solver`](https://github.com/sugyan/tsumeshogi-solver)) can migrate by swapping the dependency.

### Non-goals

Kifu I/O (SFEN/USI/KIF/CSA), evaluation functions, search, and tsume (mate) solvers are **out of scope**. zenmai stays a lean, fast engine.

## The name

**zenmai = 薇 (ぜんまい).**

- **薇** is a wild vegetable (royal fern, a *sansai*), continuing the culinary lineage of [`yasai`](https://github.com/sugyan/yasai) (野菜, "vegetables").
- At the same time, **ゼンマイ means "mainspring"** — the coiled spring that drives a clockwork mechanism, i.e. the *engine* that powers it ("the mainspring of your shogi AI").
- It ends in **"AI"**, echoing *yasai*. As a loose backronym: **Z**ippy **E**ngine, **N**ext-gen **M**ovegen, for **AI**.

## How it will be built

Rather than committing to a particular optimization up front, zenmai starts with a **simple, correct implementation** (validated against known perft values) and then decides the optimization strategy (Qugiy / magic bitboards / SIMD, etc.) **by benchmarking**. See [DESIGN.md](./DESIGN.md).

## License

`MIT OR Apache-2.0` (permissive).

This project **does not reuse GPL-licensed code** (apery / apery_rust / YaneuraOu / cshogi / the old yasai are GPL-3.0). See [DESIGN.md](./DESIGN.md) and [CLAUDE.md](./CLAUDE.md) for details.
