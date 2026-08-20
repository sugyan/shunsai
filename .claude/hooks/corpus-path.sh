#!/usr/bin/env bash
# Enforces CLAUDE.md's rule that nothing in this repository may point at the
# local-only benchmarks checkout: it has no remote and exists on one machine,
# so a path to it in a tracked file points somewhere nobody else can follow,
# and anything that *runs* against it could not run for anyone else at all.
#
# A hook rather than a CI job, deliberately. Only a working copy that *has* the
# corpus can introduce such a path, so the population at risk is one machine,
# and an agent is the only actor that plausibly writes one. The cost is that
# this fires only when Claude Code does the editing; `--all` runs the same
# check with a non-zero exit, for running by hand.
#
# Two modes. Given a PostToolUse payload on stdin it answers `decision: block`,
# so a violation is fixed in the same turn. Given `--all [dir]` it scans every
# tracked file and exits 1 on a violation.
#
# Every path that cannot complete the check exits 2 with a reason on stderr.
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
  printf 'corpus path: %s\n' "$1" >&2
  exit 2
}

# Both forms that name the corpus: the sibling-relative one used in prose, and
# an absolute path to it. Plain "benchmarks" is fine — the documents describe
# the repository constantly, and only a *path* is the problem.
pattern='\.\./benchmarks|/[A-Za-z0-9._-]+/shogi/benchmarks'

if [ "$mode" = hook ]; then
  command -v jq >/dev/null 2>&1 || stop "jq is not on PATH, so the payload cannot be read"
  payload=$(cat)
  # `?` on each index rather than bare `//`: `//` rescues null and false but not
  # an error, so indexing a non-object `tool_response` would abort the filter
  # before the fallback ran.
  file=$(printf '%s' "$payload" |
    jq -r '(.tool_response.filePath? // empty) // (.tool_input.file_path? // empty) // empty') ||
    stop "the payload is not JSON this script understands"
  [ -n "$file" ] || exit 0
  [ -f "$file" ] || exit 0
  start=$(dirname "$file")
fi

command -v git >/dev/null 2>&1 || stop "git is not on PATH"
# Outside any git repository (the scratchpad) is not this rule's business.
root=$(cd "$start" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null) || exit 0

# This script polices the checkout it lives in, and no other. A sibling repo
# with a file of the same name is still someone else's project, so the gate is
# identity rather than existence.
self=$(cd "$(dirname "$0")" 2>/dev/null && pwd)/$(basename "$0")
[ "$self" = "$root/.claude/hooks/corpus-path.sh" ] || exit 0

# `.claude/` is exempt in both modes, and this file is why: a checker has to
# carry the pattern it looks for. The personal settings beside it are gitignored
# and may name the corpus freely.
if [ "$mode" = hook ]; then
  case "${file#"$root/"}" in .claude/*) exit 0 ;; esac
  # Anything else git ignores is not published either.
  git -C "$root" check-ignore -q "$file" 2>/dev/null && exit 0
  hits=$(grep -nE "$pattern" "$file" 2>/dev/null)
  [ -n "$hits" ] || exit 0
  where="$file"
else
  cd "$root" || stop "cannot enter $root"
  git rev-parse --is-inside-work-tree >/dev/null 2>&1 || stop "$root is not a work tree"
  [ "$(git ls-files | grep -c .)" -gt 0 ] || stop "no tracked file was read"
  hits=$(git grep -nE "$pattern" -- . ':(exclude).claude' 2>/dev/null)
  [ -n "$hits" ] || exit 0
  where="the tracked tree"
fi

reason="Path into the local-only benchmarks checkout in ${where}:
${hits}

CLAUDE.md forbids one in this repository: it resolves on one machine only.
Describe the repository instead of naming a path, and put anything that needs
the corpus in that repository rather than this one."

if [ "$mode" = all ]; then
  printf '%s\n' "$reason" >&2
  exit 1
fi

jq -n --arg r "$reason" --arg f "$file" '{
  decision: "block",
  reason: $r,
  systemMessage: ("corpus path: violation in " + ($f | split("/") | last))
}'
