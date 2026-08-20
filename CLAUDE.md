# CLAUDE.md — shunsai project instructions

This file defines project rules that every future implementation session (Claude Code) **must follow**.

Background and detail live in three documents, each with one job:

| document | holds | go there when |
|---|---|---|
| [DESIGN.md](./DESIGN.md) | design, scope, milestones, licensing policy | you need to know what the crate is and is not |
| [DECISIONS.md](./DECISIONS.md) | what was decided and **rejected**, what is still open | before proposing an optimization — check it was not already tried |
| [BENCHMARKS.md](./BENCHMARKS.md) | measurement method, fixtures, every recorded number | before quoting or taking a measurement |

## Project overview

`shunsai` is a Rust **shogi legal-move-generation engine**. It is the successor to [`sugyan/yasai`](https://github.com/sugyan/yasai), rebuilt from scratch with **speed** as the goal. Fundamental types come from [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core) (MIT).

- **Scope**: movegen + position only. Do not touch the **non-goals** (kifu I/O, evaluation, search, tsume solvers) — they belong in a separate crate that depends on this one.
- **Who it is for**: shunsai is the foundation for a **search engine, and through it a strong shogi AI**, written as a separate crate. **Perft is the measuring instrument, not the customer.** So when an API, layout or size question comes up, judge it against *a search using this crate*. In particular, an argument of the form "nothing collects these" or "this is free in perft" settles the question only for today's callers — say so explicitly rather than closing the issue. Do not, however, build speculatively for a search that does not exist yet: record the condition and re-measure when it does. That consumer now has a name and a repository — **`rinsai`**, developed separately and depending on *released* versions of this crate, so an API addition it needs is a shunsai release, not a git pin. Requests to add API therefore arrive with an engine phase attached (E0 needs nothing; E1 brings `attackers_to`, staged generation, `gives_check`, null move, exposed `checkers`/`pinned`). See DESIGN.md §1–§2 and the 2026-07-29, 2026-07-31 and 2026-08-04 entries in DECISIONS.md.
- **Approach**: build a simple, correct implementation first (validate with known perft values), then decide the optimization strategy (Qugiy / magic / SIMD, etc.) by benchmarking. **Do not commit to a specific technique up front.**

## ⚠️ Top rule: licensing (stay permissive, no GPL reuse)

The project license is **`MIT OR Apache-2.0`**. To keep it clean, obey the following when generating code.

**May reference / reuse (MIT)**
- [haitaka](https://github.com/tofutofu/haitaka), [cozy-chess](https://github.com/analog-hors/cozy-chess), [shogi_core](https://github.com/rust-shogi-crates/shogi_core)
- Public algorithm write-ups such as the Qugiy appeal document and magic-bitboard articles
- When reusing from MIT sources, **retain the copyright notices**

**Must not reference / copy (GPL-3.0)**
- **apery / apery_rust / YaneuraOu / cshogi / rshogi / Fairy-Stockfish / the old yasai** (checked out in a **local-only, unpublished** sibling repository, not part of this one — all GPL)
- Understanding the technique and **writing it yourself** is fine, but **do not read-and-copy or port the code verbatim** (that inherits GPLv3).
- ⚠️ The old yasai is sugyan's own work but is **GPL-3.0** (derived from apery_rust). Porting yasai's code is **also forbidden** — reimplement it to stay permissive.

**Other**
- **Generate attack tables / magic numbers with our own generator** (never paste tables from elsewhere).
- **`src/sliders/magics.rs` is generated — never edit it by hand.** It holds the magic multipliers and nothing else (mask and shifts are derived from the crate's own geometry at compile time). Regenerate with `cargo run --release --example gen_magics`; CI runs the same generator with `--check` and fails if the committed numbers are not its output.
  - That CI check is a **licensing** guard, not a correctness one, and it is *not* redundant with the compile-time check in `magic.rs`. A magic only has to avoid mapping two occupancies with *different* attacks onto one slot, which is a loose condition — **most single-bit corruptions of our multipliers still satisfy it**, building a correct table and passing every test, and [DECISIONS.md](./DECISIONS.md) measured how many (2026-07-28). So "it works" is no evidence of "we generated it": without `--check`, a hand-edited or pasted number would ship silently. Keep both guards.
- Run a **provenance scan** (distinctive-string search / code-similarity check) before publishing to crates.io.

See "7. Licensing policy" in [DESIGN.md](./DESIGN.md) for the rationale.

## Correctness baseline (known perft values)

- Initial position: `depth1=30, depth2=900, depth3=25470, depth4=719731, depth5=19861490, depth6=547581517`
- Max-moves position `R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1`: `depth1=593, depth2=105677`
- Benchmark midgame position ("matsuri" / 指し手生成祭り) `l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1`: `depth1=207, depth2=28684, depth3=4809015, depth4=516925165` (cross-confirmed 2026-07-23 across 9 independent implementations)
- These values assume **fully legal** generation: pawn-drop-mate (打ち歩詰め) moves must **not** be generated.
- Beyond fixed values, verify by **differential testing against `shogi_legality_lite`** (MIT, same `shogi_core` types — compare full legal-move sets on arbitrary positions, as a dev-dependency).

## Benchmarks

Measure perft / movegen / do-undo with `criterion`. Comparison targets are pinned submodules in a **local-only, unpublished sibling repository** (no remote; not visible from this GitHub repo — see its README when working locally). Goal: **beat haitaka / apery_rust**.

⚠️ **Nothing in this repository may point at that checkout.** Not a path in a document, and above all not a script or CI step that needs it to run — it exists on one machine, so such a thing is unrunnable for everyone else and rots unwatched. Apparatus that needs the corpus belongs *in* that repository; this one keeps the result. **This is not a CI step, deliberately**: only a checkout that *has* the corpus can introduce such a path, and an agent is the only actor that plausibly writes one, so a job every contributor pays for buys nothing. [`corpus-path.sh`](./.claude/hooks/corpus-path.sh) does it on edit; `--all` runs the same check by hand.

Method, fixtures, how to quiet the machine, and what makes a run recordable are in [BENCHMARKS.md](./BENCHMARKS.md). Read it before trusting a measurement — the development machine's single-shot timings scatter far enough to invent a result.

## What runs where

The development machine is an Apple Silicon Mac; sessions also run in the cloud,
where the checkout is all there is.

**Available anywhere**: `cargo fmt` / `clippy` / `test` / `doc`, the deep perft
tests (`cargo test --release -- --ignored`), `cargo run --release --example
gen_magics -- --check`, and `cargo bench --no-run`. What makes these portable is
that they are **decided by a count, not by a clock** — a perft total is the same
number on any machine, so it is a valid result from any of them.

**Local only, and a cloud session must not claim otherwise**:

- **Any timing measurement, and any entry in `benches/history/`.** Two reasons,
  either sufficient: BENCHMARKS.md's recordability rules assume a quiet machine,
  and every committed entry is Apple Silicon — a row measured on another CPU is
  not comparable with the series it would be appended to, so adding one corrupts
  the record rather than extending it. ⚠️ `examples/perft` reports both, and only
  half of it travels: the node count is deterministic, the nodes/sec is not.
- **The cross-engine standing.** The harness and the pinned targets are in the
  local-only benchmarks repository.
- **The provenance scan** required before publishing, which needs the same GPL
  corpus.
- **Reading the sibling `rinsai` or benchmarks checkouts.** `rinsai` depends on
  *released* versions of this crate, so what it needs from here is a shunsai
  release, never a look at its tree.

A cloud session that wants one of these asks for it, and says which measurement
and against what base; it does not estimate one and it does not quote a figure
from the history as if it had re-run it.

## Documentation

Each fact lives in **one** place that can be checked. Keep it that way — prose that nothing verifies goes stale silently, and reviewing it costs the same as reviewing code.

| where | what belongs there |
|---|---|
| **Public API doc comments** (`Bitboard`, `MoveSet`, `Position`, ...) | the **contract**: what it returns, what it guarantees, what it does not. This is what `rinsai` reads on docs.rs. No rationale, no history. `src/position.rs` is the model. |
| **Private implementation comments** | an **invariant you would break by accident**, or a genuinely non-obvious trick (`src/sliders/qugiy.rs`'s `o - 2r` derivation). Put it next to the code it constrains — nobody opens a separate document while editing. |
| **Test comments** | what configuration this fixture or assertion **uniquely** covers, in a line or two. Prefer a liveness assertion over a paragraph: an `assert!(reached > 0)` enforces coverage where prose only claims it. |
| **DECISIONS.md / BENCHMARKS.md / `benches/history/*.json`** | measured figures, what was tried and rejected, open questions, and the story of how a number was obtained. |

**Never put a measured timing or speedup in a code comment.** It is true of one machine on one day, nothing in CI checks it, and it will be wrong before anyone notices. Static sizes are different — keep one when it explains a layout choice (`2.3 KiB, so it stays L1-resident`), drop it when it is just accounting.

Do not narrate history in comments (`used to be`, `it replaces ...`) — git has it. Do not leave instructions for future maintainers that the code cannot enforce (`do not add this back without re-measuring`) — that is DECISIONS.md's job.

### Keep the documents small on purpose

DESIGN.md reached 143 KB by appending to a decision log forever, and the volume is
what let errors in: restating a measurement in prose is how a figure ends up
contradicting the file it was copied from. Two rules, both cheap:

- **A number appears in prose only if a future decision depends on the number itself.** Otherwise cite the bench id and let `benches/history/*.json` hold it. A figure you cannot check against that file should not be written down.
- **Compress, do not append a correction.** When an entry's conclusion is superseded or generalized, rewrite that entry — git holds the old text. A log where later bullets qualify earlier ones is how a document comes to contradict itself.
- **Do not retell a commit; link it.** An adopted change already has a primary source written against the tree it describes. A DECISIONS.md entry for one is a heading, a link, and then *only* what the commit does not hold — what was rejected, what guard covers it, what is still open, what a later entry corrected. Most of that log was retelling when this rule was adopted, and the retellings had already drifted — [DECISIONS.md](./DECISIONS.md) opens with both measurements.
- **Write out what has no primary source.** A rejected candidate was never committed, so nothing but this file records it — and "check it was not already tried" is the reason this file exists. Same for coverage holes, corrections spanning two commits, and decisions with no code to point at. Compress these last, not first.

Before adding to any of the three documents, ask which one owns the fact
(the table above), and whether it is already stated somewhere else. It usually is.

## Commit messages

Releases are cut by [release-plz](https://release-plz.dev), which reads the log to
build `CHANGELOG.md`. It expects **Conventional Commits**, so a subject starts with a
type — but the type is a *prefix on this project's existing style*, not a replacement
for it. Keep writing the subject that says what changed, and what it bought:

```
perf: filter king_danger's sliders by where they could bear on the king (-16% on the initial position)
feat!: return an Undo from do_move, so Position owns nothing on the heap
docs: split the design from the decision log
```

- Types in use here: `perf` (an adopted optimization), `feat` / `fix`, `docs` (including
  the three documents and `examples/`), `test`, `refactor`, `chore` (CI, manifest, tooling).
  Append `!` when the change breaks the public API.
- **A measured figure belongs in a commit subject.** The ban in the table above is on
  *code comments*, which nothing re-checks; git holds a subject against the tree it
  described, so it stays true.
- ⚠️ **The prefix decides the version, and `cargo-semver-checks` can only raise it.**
  release-plz takes the bump from the log and asks semver-checks whether the compiled
  API needs a larger one; it does not lower one the log asked for. So a mistyped prefix
  ships a wrong version rather than a poor changelog line, and `!` is a guard `rinsai`
  depends on. What it looks like when that goes wrong is the 2026-08-14 entry in
  [DECISIONS.md](./DECISIONS.md).
