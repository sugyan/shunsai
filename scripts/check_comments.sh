#!/usr/bin/env bash
# Enforce the two CLAUDE.md documentation rules that are mechanically
# checkable. Prose that nothing verifies is how a figure ends up contradicting
# the file it was copied from, so these are checked rather than trusted.
#
#   1. no measured timing or speedup in a code comment
#   2. no history narration in a code comment
#
# Static sizes (KiB / MiB) are exempt: CLAUDE.md keeps one when it explains a
# layout choice. Percentages and times are not exempt at all.
set -uo pipefail
cd "$(dirname "$0")/.."

comment='^[[:space:]]*(///|//!|//)'
status=0

report() {
  if [ -n "$2" ]; then
    printf '\n%s\n%s\n' "$1" "$2"
    status=1
  fi
}

# 1. Measured timings and speedups. Matches "12 ns", "3.4%", "1.5x faster" and
#    the spelled-out forms ("single-digit milliseconds") a digit-only regex
#    would miss. Lines carrying a static size are exempt, so CLAUDE.md's own
#    example — `9 × 128 × 2 bytes = 2.3 KiB, so it stays L1-resident` — passes.
hits=$(grep -rnE "${comment}.*([0-9]+([.,][0-9]+)?[[:space:]]*(ns|µs|us|ms|%|×)([^a-zA-Z]|\$)|[0-9]+([.,][0-9]+)?x[[:space:]]+(faster|slower)|(single|double|triple)-digit[[:space:]]+(nanosecond|microsecond|millisecond|second))" \
  --include='*.rs' src benches examples tests 2>/dev/null \
  | grep -vE '(KiB|MiB|GiB|bytes)' || true)
report "error: measured timing or speedup in a code comment (CLAUDE.md: put it in benches/history/*.json and cite the bench id)" "$hits"

# 2. History narration. CLAUDE.md names the first two verbatim as anti-examples.
hits=$(grep -rnE "${comment}.*\b(used to be|used to have|it replaces|what (it|they) replaced|as it stood before|no longer (dispatches|clones|allocates))\b" \
  --include='*.rs' src benches examples tests 2>/dev/null || true)
report "error: history narration in a code comment (CLAUDE.md: git has it)" "$hits"

# 3. Instructions the code cannot enforce.
hits=$(grep -rnE "${comment}.*(without re-measuring|do not add (this|it) back)" \
  --include='*.rs' src benches examples tests 2>/dev/null || true)
report "error: instruction a maintainer must obey by hand (CLAUDE.md: that is DECISIONS.md's job)" "$hits"

if [ "$status" = 0 ]; then
  echo "comment rules: clean"
fi
exit "$status"
