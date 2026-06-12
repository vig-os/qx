#!/usr/bin/env bash
# Regenerate examples/ SVGs from the Rust codec renderer.
#
# Usage:
#   tools/regen-examples.sh          # check mode — fails if SVGs differ
#   tools/regen-examples.sh --update # overwrite committed SVGs
#
# The underlying test lives at crates/codec/tests/examples_regression.rs.
# CI runs this in check mode; developers run with --update after
# intentional rendering changes.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if [[ "${1:-}" == "--update" ]]; then
    echo "Regenerating examples/ SVGs..."
    REGEN_EXAMPLES=1 cargo test -p part-registry-codec \
        --test examples_regression -- --nocapture 2>&1
    echo "Done. Review changes with 'git diff examples/'."
else
    echo "Checking examples/ SVGs match Rust renderer output..."
    cargo test -p part-registry-codec --test examples_regression 2>&1
    echo "All examples up to date."
fi
