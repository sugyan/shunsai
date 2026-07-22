# CLAUDE.md — zenmai project instructions

This file defines project rules that every future implementation session (Claude Code) **must follow**. For background and detail, see [DESIGN.md](./DESIGN.md).

## Project overview

`zenmai` is a Rust **shogi legal-move-generation engine**. It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai), rebuilt from scratch with **speed** as the goal. Fundamental types come from [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) (MIT).

- **Scope**: movegen + position only. Do not touch the **non-goals** (kifu I/O, evaluation, search, tsume solvers).
- **Approach**: build a simple, correct implementation first (validate with known perft values), then decide the optimization strategy (Qugiy / magic / SIMD, etc.) by benchmarking. **Do not commit to a specific technique up front.**

## ⚠️ Top rule: licensing (stay permissive, no GPL reuse)

The project license is **`MIT OR Apache-2.0`**. To keep it clean, obey the following when generating code.

**May reference / reuse (MIT)**
- [haitaka](https://github.com/tofutofu/haitaka), [cozy-chess](https://github.com/analog-hors/cozy-chess), [rustshogi](https://github.com/applyuser160/rustshogi), [shogi_core](https://github.com/rust-shogi-crates/shogi_core)
- Public algorithm write-ups such as the Qugiy appeal document and magic-bitboard articles
- When reusing from MIT sources, **retain the copyright notices**

**Must not reference / copy (GPL-3.0)**
- **apery / apery_rust / YaneuraOu / cshogi / the old yasai** (present under `../benchmarks/`, but GPL)
- Understanding the technique and **writing it yourself** is fine, but **do not read-and-copy or port the code verbatim** (that inherits GPLv3).
- ⚠️ The old yasai is sugyan's own work but is **GPL-3.0** (derived from apery_rust). Porting yasai's code is **also forbidden** — reimplement it to stay permissive.

**Other**
- **Generate attack tables / magic numbers with our own generator** (never paste tables from elsewhere).
- Run a **provenance scan** (distinctive-string search / code-similarity check) before publishing to crates.io.

See "7. Licensing policy" in [DESIGN.md](./DESIGN.md) for the rationale.

## Correctness baseline (known perft values)

- Initial position: `depth1=30, depth2=900, depth3=25470, depth4=719731`
- Max-moves position `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`: `depth1=593, depth2=105677`

## Benchmarks

Measure perft / movegen / do-undo with `criterion`. Comparison targets are in [`../benchmarks`](../benchmarks) (goal: **beat haitaka / apery_rust**).
