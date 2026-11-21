#!/bin/sh
# Writes the test report for a CI run: coverage, how many tests there are of each kind,
# and how much of the vocabulary the scenario story exercises. Run in package/ after
# `cargo llvm-cov` has produced coverage.txt. Prints Markdown.
set -eu

total=$(awk '$1 == "TOTAL" { print $(NF-3) }' coverage.txt)
unit=$(cargo test --locked --lib --bins -- --list 2>/dev/null | grep -c ': test$' || true)
cli=$(cargo test --locked --test cli -- --list 2>/dev/null | grep -c ': test$' || true)
story=$(cargo test --locked --test smoke -- --list 2>/dev/null | grep -c ': test$' || true)
subjects=$(grep -vc '^\(#\|$\)' tests/fixtures/lifecycle-story.txt || true)
entries=$(grep -c '^  emoji = ' vocabulary.toml || true)

echo "## Tests and coverage"
echo
echo "| What | Count |"
echo "|---|---|"
echo "| Line coverage of the command | $total |"
echo "| Unit tests | $unit |"
echo "| Tests that run the command as a user does | $cli |"
echo "| Tests over the scenario story | $story |"
echo "| Commit subjects in the scenario story | $subjects |"
echo "| Vocabulary entries, each used by the story | $entries |"
echo
echo "### Coverage by file"
echo
echo '```'
cat coverage.txt
echo '```'
