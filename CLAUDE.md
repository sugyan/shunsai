# CLAUDE.md — shunsai project instructions

This file defines project rules that every future implementation session (Claude Code) **must follow**. For background and detail, see [DESIGN.md](./DESIGN.md).

## Project overview

`shunsai` is a Rust **shogi legal-move-generation engine**. It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai), rebuilt from scratch with **speed** as the goal. Fundamental types come from [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) (MIT).

- **Scope**: movegen + position only. Do not touch the **non-goals** (kifu I/O, evaluation, search, tsume solvers) — they belong in a separate crate that depends on this one.
- **Who it is for**: shunsai is the foundation for a **search engine, and through it a strong shogi AI**, written as a separate crate. **Perft is the measuring instrument, not the customer.** So when an API, layout or size question comes up, judge it against *a search using this crate*. In particular, an argument of the form "nothing collects these" or "this is free in perft" settles the question only for today's callers — say so explicitly rather than closing the issue. Do not, however, build speculatively for a search that does not exist yet: record the condition and re-measure when it does. That consumer now has a name and a repository — **`rinsai`**, developed separately and depending on *released* versions of this crate, so an API addition it needs is a shunsai release, not a git pin. Requests to add API therefore arrive with an engine phase attached (E0 needs nothing; E1 brings `attackers_to`, staged generation, `gives_check`, null move, exposed `checkers`/`pinned`). See DESIGN.md §1–§2 and the 2026-07-29, 2026-07-31 and 2026-08-04 decision-log entries.
- **Approach**: build a simple, correct implementation first (validate with known perft values), then decide the optimization strategy (Qugiy / magic / SIMD, etc.) by benchmarking. **Do not commit to a specific technique up front.**

## ⚠️ Top rule: licensing (stay permissive, no GPL reuse)

The project license is **`MIT OR Apache-2.0`**. To keep it clean, obey the following when generating code.

**May reference / reuse (MIT)**
- [haitaka](https://github.com/tofutofu/haitaka), [cozy-chess](https://github.com/analog-hors/cozy-chess), [shogi_core](https://github.com/rust-shogi-crates/shogi_core)
- Public algorithm write-ups such as the Qugiy appeal document and magic-bitboard articles
- When reusing from MIT sources, **retain the copyright notices**

**Must not reference / copy (GPL-3.0)**
- **apery / apery_rust / YaneuraOu / cshogi / rshogi / Fairy-Stockfish / the old yasai** (present under `../benchmarks/` — a **local-only, unpublished** sibling repo, not part of this repository — all GPL)
- Understanding the technique and **writing it yourself** is fine, but **do not read-and-copy or port the code verbatim** (that inherits GPLv3).
- ⚠️ The old yasai is sugyan's own work but is **GPL-3.0** (derived from apery_rust). Porting yasai's code is **also forbidden** — reimplement it to stay permissive.

**Other**
- **Generate attack tables / magic numbers with our own generator** (never paste tables from elsewhere).
- **`src/sliders/magics.rs` is generated — never edit it by hand.** It holds the magic multipliers and nothing else (mask and shifts are derived from the crate's own geometry at compile time). Regenerate with `cargo run --release --example gen_magics`; CI runs the same generator with `--check` and fails if the committed numbers are not its output.
  - That CI check is a **licensing** guard, not a correctness one, and it is *not* redundant with the compile-time check in `magic.rs`. A magic only has to avoid mapping two occupancies with *different* attacks onto one slot, which is a loose condition — measured 2026-07-28, **72.7 % of single-bit corruptions of our multipliers still satisfy it** (11299 of 15552 flips) and build and pass every test. So "it works" is no evidence of "we generated it": without `--check`, a hand-edited or pasted number would ship silently. Keep both guards.
- Run a **provenance scan** (distinctive-string search / code-similarity check) before publishing to crates.io.

See "7. Licensing policy" in [DESIGN.md](./DESIGN.md) for the rationale.

## Correctness baseline (known perft values)

- Initial position: `depth1=30, depth2=900, depth3=25470, depth4=719731, depth5=19861490, depth6=547581517`
- Max-moves position `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`: `depth1=593, depth2=105677`
- Benchmark midgame position ("matsuri" / 指し手生成祭り) `l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1`: `depth1=207, depth2=28684, depth3=4809015, depth4=516925165` (cross-confirmed 2026-07-23 across 9 independent implementations)
- These values assume **fully legal** generation: pawn-drop-mate (打ち歩詰め) moves must **not** be generated.
- Beyond fixed values, verify by **differential testing against `shogi_legality_lite`** (MIT, same `shogi_core` types — compare full legal-move sets on arbitrary positions, as a dev-dependency).

## Benchmarks

Measure perft / movegen / do-undo with `criterion`. Comparison targets are pinned submodules in `../benchmarks` — a **local-only, unpublished sibling repository** (no remote; not visible from this GitHub repo — see its README when working locally). Goal: **beat haitaka / apery_rust**.
