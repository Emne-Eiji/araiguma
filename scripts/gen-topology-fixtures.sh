#!/usr/bin/env bash
# Generate the committed topology golden documents (fixtures/topology/*.json)
# by running the Rust projection over every snapshot fixture. The goldens are
# consumed by the frontend (mock data) and stability-checked by
# scripts/test-all.sh. Deterministic: the projection is pure and snapshot
# timestamps are fixed.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/env.sh disable=SC1091
source scripts/env.sh

cargo build -q -p araiguma-cli
BIN="$CARGO_TARGET_DIR/debug/araiguma"

mkdir -p fixtures/topology
for snapshot in fixtures/snapshots/*.json; do
  name="$(basename "$snapshot" .json)"
  "$BIN" topology --input "$snapshot" --pretty > "fixtures/topology/$name.json"
  echo "fixtures/topology/$name.json"
done
