# CLAUDE.md — shunsai project instructions

Rules an implementation session **must follow**. The plan is [DESIGN.md](./DESIGN.md),
why the code is the way it is is [FAQ.md](./FAQ.md), and how measurement is done is
[BENCHMARKS.md](./BENCHMARKS.md). **What is next, deferred or broken is a GitHub
issue** — `gh issue list`. What landed is `git log` and the pull requests.

## What shunsai is

A Rust **shogi legal-move-generation engine**, the successor to
[`yasai`](https://github.com/sugyan/yasai), rebuilt from scratch with **speed** as the
goal. Fundamental types come from [`shogi_core`](https://github.com/rust-shogi-crates/shogi_core)
(MIT).

- **Movegen and position only.** Kifu I/O, evaluation, search and tsume solvers are
  non-goals; they belong in a crate that depends on this one.
- **Judge an API, layout or size question against a search using this crate**, not only
  against perft — but do not build speculatively for one. DESIGN.md states both halves;
  the consumer is [`rinsai`](https://github.com/sugyan/rinsai), which depends on
  *released* versions, so an API addition is a release and carries semver.
- **Correct first, then benchmark.** Do not commit to a technique (Qugiy / magic / SIMD)
  up front; adopt by measurement.

## ⚠️ Top rule: licensing — stay permissive, no GPL reuse

The project license is **`MIT OR Apache-2.0`**. Copyright protects **expression**, not
ideas or algorithms: implementing a technique yourself from a public write-up is free,
**copying or line-by-line porting is not** — that creates a derivative work and inherits
GPLv3.

**May reference / reuse (MIT)** — [haitaka](https://github.com/tofutofu/haitaka),
[cozy-chess](https://github.com/analog-hors/cozy-chess),
[shogi_core](https://github.com/rust-shogi-crates/shogi_core), plus public algorithm
write-ups (the Qugiy appeal document, magic-bitboard articles). **Retain the copyright
notices** when reusing.

**Must not read or copy (GPL-3.0)** — apery, apery_rust, YaneuraOu, cshogi, rshogi,
Fairy-Stockfish, **and the old yasai**. They are checked out in a local-only,
unpublished sibling repository that is not part of this one. ⚠️ yasai is sugyan's own
work but is GPL-3.0 (derived from apery_rust), so porting its code is forbidden too —
reimplement it.

- **Generate attack tables and magic numbers with our own generator.** Never paste them
  from elsewhere.
- **`src/sliders/magics.rs` is generated — never edit it by hand.** Regenerate with
  `cargo run --release --example gen_magics`; CI runs the same generator with `--check`.
  That check is a **licensing** guard, not a correctness one, and it is not redundant
  with the compile-time validation in `magic.rs` — see FAQ.md for the measurement that
  settles it. Keep both.
- **Run the provenance scan before publishing to crates.io**, and re-run it before each
  release. It lives in the local-only benchmarks repository, because it needs the corpus.

This is a summary of how licensing works, not legal advice.

## Correctness baseline

- **Everything generated must be fully legal**, pawn-drop-mate (打ち歩詰め) exclusion
  included, so a caller never filters what it is handed.
- **Known perft values live in [`tests/perft.rs`](./tests/perft.rs)**, which asserts them
  and records where each came from. Do not restate them in prose; an executable assertion
  that pins a bench workload is the one exception.
- **Beyond fixed values, verify differentially against `shogi_legality_lite`** (MIT, same
  `shogi_core` types): compare full legal-move *sets* on arbitrary positions.
- **Establish a guard's worth by sabotage.** Break the code, watch which tests fail, and
  write only what that run showed. A claim that a rare configuration is covered is worth
  what its sabotage showed; assert that the fixture *reached* it, so removing one fails
  loudly instead of silently reducing coverage.
- **One optimization per change.** Batching destroys the attribution that makes the
  committed history readable.

## Measurement, and what runs where

Measure perft / movegen / do-undo with `criterion`. Comparison targets are pinned
submodules in a **local-only, unpublished** sibling repository. Goal: **beat haitaka and
apery_rust**. Read [BENCHMARKS.md](./BENCHMARKS.md) before trusting or taking a
measurement — the development machine's single-shot timings scatter far enough to invent
a result.

⚠️ **Nothing in this repository may point at that checkout** — not a path in a document,
and above all not a script or CI step that needs it to run. It exists on one machine, so
such a thing is unrunnable for everyone else and rots unwatched. Apparatus that needs the
corpus belongs *in* that repository; this one keeps the result.
[`corpus-path.sh`](./.claude/hooks/corpus-path.sh) checks this on edit; `--all` runs it
by hand.

The development machine is an Apple Silicon Mac; sessions also run in the cloud, where
the checkout is all there is.

**Available anywhere** — `cargo fmt` / `clippy` / `test` / `doc`, the deep perft tests
(`cargo test --release -- --ignored`), `cargo run --release --example gen_magics --
--check`, and `cargo bench --no-run`. What makes these portable is that they are
**decided by a count, not by a clock**.

**Local only, and a cloud session must not claim otherwise**:

- **Any timing measurement, and any entry in `benches/history/`.** Two reasons, either
  sufficient: BENCHMARKS.md's recordability rules assume a quiet machine, and every
  committed entry is Apple Silicon, so a row measured on another CPU corrupts the series
  rather than extending it. ⚠️ `examples/perft` reports both a node count and a
  nodes/sec, and only the count travels.
- **The cross-engine standing**, the harness and targets being in that repository.
- **The provenance scan**, which needs the same corpus.
- **Reading the sibling `rinsai` or benchmarks checkouts.** What `rinsai` needs from here
  is a shunsai release, never a look at its tree.

A cloud session that wants one of these asks for it, and says which measurement and
against what base. It does not estimate one, and it does not quote a figure from the
history as if it had re-run it.

## Prose

**Write less.** No compiler and no test reads these sentences, so the lever is volume,
not care.

- **One copy, or none.** A fact worth stating twice was worth stating once. Before adding
  a sentence, grep for its twin — two sentences that disagree are worse than either.
- **A doc comment states the contract**: what an item returns, what it guarantees, what
  it does not, and what breaks silently (mark that ⚠️). That is what `rinsai` reads on
  docs.rs. `src/position.rs` is the model.
- **A private comment states an invariant you would break by accident**, or a genuinely
  non-obvious trick. Put it next to the code it constrains.
- **Never put a measured timing or speedup in a code comment.** It is true of one machine
  on one day, nothing in CI checks it, and it will be wrong before anyone notices. Static
  sizes are different — keep one when it explains a layout choice, drop it when it is
  just accounting.
- **Do not narrate history** (`used to be`, `it replaces …`) — git has it. **Do not leave
  instructions the code cannot enforce** (`do not add this back without re-measuring`) —
  that is FAQ.md's job, or an issue's.
- **Name what you point at, never its position.** Quote a heading's own words or a `fn`
  name in backticks. `FILE.md §N` breaks the moment anything is inserted above it.
- **A false claim is deleted, not rewritten.** Try in order: delete the sentence; replace
  it with a pointer; write the test that checks it.
- **A test's comment says what that fixture or assertion uniquely covers.** Prefer a
  liveness assertion over a paragraph — `assert!(reached > 0)` enforces coverage where
  prose only claims it.

[`comment-rules.sh`](./.claude/hooks/comment-rules.sh) mechanically checks part of this,
on `.rs` and manifests only. It fires only when Claude Code does the editing; `--all`
runs it by hand. **An unenforced rule is still a rule.**

### Which file owns a fact

| where | what belongs there |
|---|---|
| the item's own doc comment | the contract, and anything a caller must guarantee |
| [FAQ.md](./FAQ.md) | why the code is this way, what was rejected, what a guard covers, what reopens a decision |
| [BENCHMARKS.md](./BENCHMARKS.md) / `benches/history/*.json` | how a figure was obtained, and every recorded figure |
| [DESIGN.md](./DESIGN.md) | what the crate is and is not |
| a GitHub issue | what is next, deferred, or broken |

Before adding to a document, ask which one owns the fact and whether it is already stated
somewhere else — it usually is. Then:

- **A number appears in prose only if a future decision depends on the number itself.**
  Otherwise cite the bench id and let `benches/history/*.json` hold it.
- **Compress, do not append a correction.** When a conclusion is superseded, rewrite it
  where it stands; git holds the old text.
- **Do not retell a commit; link it.** Write out only what the commit does not hold —
  what was rejected, what guard covers it, what is still open.
- **Write out what has no primary source.** A rejected candidate was never committed, so
  nothing but FAQ.md records it. Same for coverage holes and corrections spanning two
  commits. Compress these last, not first.

## Commit messages

Releases are cut by [release-plz](https://release-plz.dev), which reads the log to build
`CHANGELOG.md`. It expects **Conventional Commits** — but the type is a *prefix on this
project's existing style*, not a replacement. Keep writing the subject that says what
changed, and what it bought:

```
perf: filter king_danger's sliders by where they could bear on the king (-16% on the initial position)
feat!: return an Undo from do_move, so Position owns nothing on the heap
docs: split the design from the decision log
```

- Types in use: `perf` (an adopted optimization), `feat` / `fix`, `docs` (documents and
  `examples/`), `test`, `refactor`, `chore` (CI, manifest, tooling). Append `!` when the
  change breaks the public API.
- **A measured figure belongs in a commit subject.** The ban above is on *code comments*,
  which nothing re-checks; git holds a subject against the tree it described.
- ⚠️ **The prefix decides the version, and `cargo-semver-checks` can only raise it.**
  release-plz takes the bump from the log and asks semver-checks whether the compiled API
  needs a larger one; it never lowers one the log asked for. So a mistyped prefix ships a
  wrong version rather than a poor changelog line, and `!` is a guard `rinsai` depends on.
