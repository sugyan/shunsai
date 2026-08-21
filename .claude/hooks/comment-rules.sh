#!/usr/bin/env bash
# The mechanically-checkable part of CLAUDE.md's "Prose" section, on
# artifacts only — the three documents are where a measured figure and a
# section heading legitimately live, so nothing here reads a `.md` file.
#
# A hook rather than a CI job. These rules exist because an *agent* is careless
# in ways someone writing this by hand is not, and the repository's CI is the
# project's, not a machine's discipline. The cost is that this fires only when
# Claude Code does the editing; `--all` runs the same checks with a non-zero
# exit, for running by hand.
#
# Two modes. Given a PostToolUse payload on stdin it answers `decision: block`,
# so a violation is fixed in the same turn. Given `--all [dir]` it scans every
# tracked artifact and exits 1 on a violation.
#
# Every path that cannot complete a check exits 2 with a reason on stderr.
# Silence means checked-and-clean and nothing else; a guard whose failure looks
# like a pass is worse than no guard.
set -uo pipefail

mode=hook
start=.
if [ "${1:-}" = "--all" ]; then
  mode=all
  start="${2:-.}"
fi

stop() {
  printf 'comment rules: %s\n' "$1" >&2
  exit 2
}

if [ "$mode" = hook ]; then
  command -v jq >/dev/null 2>&1 || stop "jq is not on PATH, so the payload cannot be read"
  payload=$(cat)
  # `?` on each index rather than bare `//`: `//` rescues null and false but not
  # an error, so indexing a non-object `tool_response` would abort the filter
  # before the fallback ran.
  file=$(printf '%s' "$payload" |
    jq -r '(.tool_response.filePath? // empty) // (.tool_input.file_path? // empty) // empty') ||
    stop "the payload is not JSON this script understands"
  case "$file" in
    *.rs | */Cargo.toml | Cargo.toml) ;;
    *) exit 0 ;;
  esac
  [ -f "$file" ] || exit 0
  start=$(dirname "$file")
fi

command -v git >/dev/null 2>&1 || stop "git is not on PATH"
root=$(cd "$start" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null) || exit 0

# This script polices the checkout it lives in, and no other. A sibling repo
# with a file of the same name is still someone else's project, so the gate is
# identity rather than existence: without it, editing any checkout with a `src/`
# directory would be judged against this project's rules.
self=$(cd "$(dirname "$0")" 2>/dev/null && pwd)/$(basename "$0")
[ "$self" = "$root/.claude/hooks/comment-rules.sh" ] || exit 0
cd "$root" || stop "cannot enter $root"

# Three file sets, because two of the checks below turn on where a file ends up
# rather than on what it says.
#   shipped — goes inside the published crate (`include` is src + README +
#             CHANGELOG + the licences), so a pointer at a document that stays
#             behind resolves for nobody on docs.rs.
#   other   — the rest of the artifacts. They stay in the repository, so naming
#             a document is fine; a *section number* still is not.
#   rs      — every Rust source, for the three comment rules.
shipped=""
other=""
rs=""
collect() {
  case "$1" in
    src/*.rs | Cargo.toml) shipped="${shipped}$1"$'\n' ;;
    *.rs) other="${other}$1"$'\n' ;;
    *) return 0 ;;
  esac
  case "$1" in *.rs) rs="${rs}$1"$'\n' ;; esac
}

if [ "$mode" = hook ]; then
  collect "${file#"$root/"}"
else
  while IFS= read -r f; do collect "$f"; done < <(git ls-files -- '*.rs' Cargo.toml)
  [ -n "$shipped$other" ] || stop "no tracked artifact was read"
fi

found=""
add() {
  [ -n "$2" ] || return 0
  found="${found}${found:+$'\n\n'}$1"$'\n'"$2"
}
# grep over a possibly-empty list, always prefixing the filename so a one-file
# hook run and an --all run read the same.
scan() {
  local list="$1" pat="$2"
  [ -n "$list" ] || return 0
  printf '%s' "$list" | tr '\n' '\0' | xargs -0 grep -HnE "$pat" 2>/dev/null
}

comment='(^|[[:space:]])(///|//!|//)'

# --- a measured figure in a code comment ------------------------------------
# The defect class this repository actually produced: three such comments stood
# before the rule was written. Static sizes are exempt — CLAUDE.md keeps one
# when it explains a layout choice.
add "writes a measured timing or speedup into a code comment, where nothing re-reads it — put the number in benches/history/*.json and let the sentence name the bench id" \
  "$(scan "$rs" "${comment}.*([0-9]+([.,][0-9]+)?[[:space:]]*(ns|µs|us|ms|%|×)([^a-zA-Z]|\$)|[0-9]+([.,][0-9]+)?x[[:space:]]+(faster|slower)|(single|double|triple)-digit[[:space:]]+(nanosecond|microsecond|millisecond|second))" |
    grep -vE '(KiB|MiB|GiB|bytes)')"

# --- history narration ------------------------------------------------------
# This one has never fired, before the rule or since. It is a grep, so it costs
# nothing to keep; do not read its silence as evidence that it works.
add "narrates history in a code comment (git has it)" \
  "$(scan "$rs" "${comment}.*\b(used to be|used to have|it replaces|what (it|they) replaced|as it stood before|no longer (dispatches|clones|allocates))\b")"

# --- an instruction the code cannot enforce ---------------------------------
add "leaves an instruction the code cannot enforce (that is FAQ.md's job, or an issue's)" \
  "$(scan "$rs" "${comment}.*(without re-measuring|do not add (this|it) back)")"

# --- a document that does not ship, named from something that does ----------
add "names a repository document from a file that ships inside the published crate, where that document is not — state the fact instead of pointing at it" \
  "$(scan "$shipped" '(CLAUDE|DESIGN|FAQ|BENCHMARKS)\.md')"

# --- a section number -------------------------------------------------------
# Section numbers move when a document is rewritten and nothing re-reads them.
# Documents are not scanned: this bans the citation from artifacts only.
add "cites a document by section number, which is stale as soon as a heading moves — quote the heading's own words instead" \
  "$(scan "$other" '§[0-9]')"

[ -z "$found" ] && exit 0

if [ "$mode" = all ]; then
  printf 'CLAUDE.md comment rules violated:\n\n%s\n' "$found" >&2
  exit 1
fi

jq -n --arg d "$found" --arg f "$file" '{
  decision: "block",
  reason: ("CLAUDE.md comment rules violated:\n\n" + $d + "\n\nFix these before continuing."),
  systemMessage: ("comment rules: violation in " + ($f | split("/") | last))
}'
